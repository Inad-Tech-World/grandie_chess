//! Full-game review: evaluate every position, then classify each move,
//! compute accuracy per side and pick out the key moments.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Color, Position};
use specta::Type;

use super::accuracy::{game_accuracy, move_accuracy};
use super::classify::{classify, MoveClass, MoveFacts};
use super::live::{position_after, to_white_line};
use crate::engine::process::{position_command, Engine};
use crate::engine::uci::Score;
use crate::error::{Error, Result};
use crate::game::position::{fen_of, material_balance, parse_fen, parse_uci, san_of, Side};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ReviewDepth {
    Quick,
    Standard,
    Deep,
}

impl ReviewDepth {
    /// `go` command per position. lc0 is limited by nodes; other engines by depth.
    pub fn go_command(self, lc0: bool) -> &'static str {
        match (self, lc0) {
            (ReviewDepth::Quick, true) => "go nodes 200",
            (ReviewDepth::Standard, true) => "go nodes 800",
            (ReviewDepth::Deep, true) => "go nodes 3000",
            (ReviewDepth::Quick, false) => "go depth 14",
            (ReviewDepth::Standard, false) => "go depth 18",
            (ReviewDepth::Deep, false) => "go depth 22",
        }
    }
}

/// Engine verdict on one position, White's point of view.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PositionEval {
    pub ply: u32,
    pub fen: String,
    pub white_win: f64,
    pub score: Option<Score>,
    pub best: Option<String>,
    pub best_san: Option<String>,
    /// Principal variation from this position, in SAN.
    pub line: Vec<String>,
    /// White's win% after the engine's second choice, if any.
    pub second_white_win: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveReview {
    /// Ply of the position after this move (1 = White's first move).
    pub ply: u32,
    pub side: Side,
    pub uci: String,
    pub san: String,
    pub class: MoveClass,
    /// Mover's win% with the best move, and after the move played.
    pub win_before: f64,
    pub win_after: f64,
    pub accuracy: f64,
    pub best: Option<String>,
    pub best_san: Option<String>,
    /// The engine's line starting with its best move, in SAN.
    pub best_line: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SideSummary {
    pub accuracy: Option<f64>,
    pub brilliant: u32,
    pub great: u32,
    pub best: u32,
    pub good: u32,
    pub inaccuracies: u32,
    pub mistakes: u32,
    pub blunders: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResult {
    pub start_fen: String,
    pub engine: String,
    pub depth: ReviewDepth,
    pub evals: Vec<PositionEval>,
    pub moves: Vec<MoveReview>,
    pub white: SideSummary,
    pub black: SideSummary,
    /// Plies of the moments worth revisiting, most important first.
    pub key_moments: Vec<u32>,
}

/// Evaluate one position (terminal positions are scored by the rules).
async fn evaluate(engine: &Engine, start_fen: &str, moves: &[String], pos: &Chess, ply: u32, go: &str) -> Result<PositionEval> {
    let fen = fen_of(pos);
    if pos.is_checkmate() {
        let white_win = if pos.turn() == Color::White { 0.0 } else { 100.0 };
        return Ok(PositionEval {
            ply,
            fen,
            white_win,
            score: Some(Score::Mate(0)),
            best: None,
            best_san: None,
            line: vec![],
            second_white_win: None,
        });
    }
    if pos.is_game_over() {
        return Ok(PositionEval {
            ply,
            fen,
            white_win: 50.0,
            score: Some(Score::Cp(0)),
            best: None,
            best_san: None,
            line: vec![],
            second_white_win: None,
        });
    }
    let result = engine.search(&position_command(start_fen, moves), go, |_| {}).await?;
    let top = result.lines.first().ok_or_else(|| Error::Engine("engine returned no evaluation".into()))?;
    let line = to_white_line(top, pos);
    let second = result.lines.get(1).filter(|l| !l.pv.is_empty()).map(|l| to_white_line(l, pos));
    let best = if result.best.is_empty() || result.best == "(none)" { None } else { Some(result.best.clone()) };
    let best_san = best.as_ref().and_then(|b| parse_uci(pos, b).ok()).map(|m| san_of(pos, m));
    Ok(PositionEval {
        ply,
        fen,
        white_win: line.white_win.unwrap_or(50.0),
        score: line.score,
        best,
        best_san,
        line: line.san.clone(),
        second_white_win: second.and_then(|s| s.white_win),
    })
}

/// Material the mover gives up with `uci`, counting the opponent's best reply.
fn sacrifice(before: &Chess, uci: &str, reply: Option<&str>) -> i32 {
    let mover = before.turn();
    let sign = if mover == Color::White { 1 } else { -1 };
    let start = material_balance(before) * sign;
    let Ok(m) = parse_uci(before, uci) else { return 0 };
    let mut after = before.clone();
    after.play_unchecked(m);
    if let Some(r) = reply.and_then(|r| parse_uci(&after, r).ok()) {
        after.play_unchecked(r);
    }
    start - material_balance(&after) * sign
}

pub fn summarise(start_fen: &str, moves: &[String], evals: Vec<PositionEval>, engine: String, depth: ReviewDepth) -> Result<ReviewResult> {
    let mut pos = parse_fen(start_fen)?;
    let mut reviews = Vec::with_capacity(moves.len());
    for (i, uci) in moves.iter().enumerate() {
        let before = &evals[i];
        let after = &evals[i + 1];
        let mover: Side = pos.turn().into();
        let pov = |white_win: f64| if mover == Side::White { white_win } else { 100.0 - white_win };
        let m = parse_uci(&pos, uci)?;
        let san = san_of(&pos, m);
        let played_best = before.best.as_deref() == Some(uci.as_str());
        let best_win = pov(before.white_win);
        let played_win = pov(after.white_win);
        let facts = MoveFacts {
            best_win,
            played_win,
            second_win: before.second_white_win.map(pov),
            played_best,
            sacrificed: if played_best { sacrifice(&pos, uci, after.best.as_deref()) } else { 0 },
        };
        reviews.push(MoveReview {
            ply: i as u32 + 1,
            side: mover,
            uci: uci.clone(),
            san,
            class: classify(&facts),
            win_before: best_win,
            win_after: played_win,
            accuracy: move_accuracy(best_win, played_win),
            best: before.best.clone(),
            best_san: before.best_san.clone(),
            best_line: before.line.clone(),
        });
        pos.play_unchecked(m);
    }

    let white_wins: Vec<f64> = evals.iter().map(|e| e.white_win).collect();
    let accuracies: Vec<f64> = reviews.iter().map(|r| r.accuracy).collect();
    // Accuracy maths assumes White moves first; pad when Black starts.
    let black_first = parse_fen(start_fen)?.turn() == Color::Black;
    let (wins, accs) = if black_first {
        let mut w = vec![white_wins[0]];
        w.extend(&white_wins);
        let mut a = vec![100.0];
        a.extend(&accuracies);
        (w, a)
    } else {
        (white_wins.clone(), accuracies.clone())
    };
    let summary = |side: Side| {
        let mut s = SideSummary { accuracy: game_accuracy(&wins, &accs, side == Side::White), ..Default::default() };
        for r in reviews.iter().filter(|r| r.side == side) {
            match r.class {
                MoveClass::Brilliant => s.brilliant += 1,
                MoveClass::Great => s.great += 1,
                MoveClass::Best => s.best += 1,
                MoveClass::Good => s.good += 1,
                MoveClass::Inaccuracy => s.inaccuracies += 1,
                MoveClass::Mistake => s.mistakes += 1,
                MoveClass::Blunder => s.blunders += 1,
            }
        }
        s
    };

    // Key moments: mistakes and blunders by size, then brilliant moves.
    let mut errors: Vec<&MoveReview> = reviews.iter().filter(|r| matches!(r.class, MoveClass::Mistake | MoveClass::Blunder)).collect();
    errors.sort_by(|a, b| (b.win_before - b.win_after).partial_cmp(&(a.win_before - a.win_after)).unwrap_or(std::cmp::Ordering::Equal));
    let mut key_moments: Vec<u32> = errors.iter().take(8).map(|r| r.ply).collect();
    key_moments.extend(reviews.iter().filter(|r| r.class == MoveClass::Brilliant).map(|r| r.ply));

    Ok(ReviewResult {
        start_fen: start_fen.to_string(),
        engine,
        depth,
        white: summary(Side::White),
        black: summary(Side::Black),
        evals,
        moves: reviews,
        key_moments,
    })
}

/// Evaluate every position of the game. `progress(done, total)` is called
/// after each one; setting `cancel` stops early with an error.
pub async fn run(
    engine: &Engine,
    lc0: bool,
    start_fen: &str,
    moves: &[String],
    depth: ReviewDepth,
    cancel: Arc<AtomicBool>,
    mut progress: impl FnMut(u32, u32),
) -> Result<ReviewResult> {
    position_after(start_fen, moves)?; // validate the whole game first
    engine.send("ucinewgame").await?;
    engine.set_option("MultiPV", 2).await?;
    engine.set_option("UCI_ShowWDL", "true").await?;
    let go = depth.go_command(lc0);
    let total = moves.len() as u32 + 1;
    let mut pos = parse_fen(start_fen)?;
    let mut evals = Vec::with_capacity(total as usize);
    for ply in 0..total {
        if cancel.load(Ordering::SeqCst) {
            return Err(Error::other("review cancelled"));
        }
        let played = &moves[..ply as usize];
        evals.push(evaluate(engine, start_fen, played, &pos, ply, go).await?);
        progress(ply + 1, total);
        if let Some(uci) = moves.get(ply as usize) {
            let m = parse_uci(&pos, uci)?;
            pos.play_unchecked(m);
        }
    }
    summarise(start_fen, moves, evals, engine.name.clone(), depth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::START_FEN;

    fn eval(ply: u32, white_win: f64, best: &str) -> PositionEval {
        PositionEval {
            ply,
            fen: String::new(),
            white_win,
            score: None,
            best: Some(best.into()),
            best_san: Some(best.into()),
            line: vec![best.into()],
            second_white_win: None,
        }
    }

    #[test]
    fn flags_mistakes_with_the_better_move() {
        // 1. e4 e5 2. Qh5 Ke7?? (a blunder) 3. Qxe5#
        let moves: Vec<String> = ["e2e4", "e7e5", "d1h5", "e8e7", "h5e5"].iter().map(|s| s.to_string()).collect();
        let evals = vec![
            eval(0, 55.0, "e2e4"),
            eval(1, 56.0, "e7e5"),
            eval(2, 55.0, "g1f3"),
            eval(3, 50.0, "b8c6"),
            eval(4, 99.0, "h5e5"),
            eval(5, 100.0, "a1a1"),
        ];
        let r = summarise(START_FEN, &moves, evals, "test".into(), ReviewDepth::Quick).unwrap();
        assert_eq!(r.moves.len(), 5);
        assert_eq!(r.moves[0].class, MoveClass::Best);
        let qh5 = &r.moves[2];
        assert_eq!(qh5.san, "Qh5");
        assert_eq!(qh5.class, MoveClass::Inaccuracy, "55 -> 50 for White");
        let ke7 = &r.moves[3];
        assert_eq!(ke7.side, Side::Black);
        assert_eq!(ke7.class, MoveClass::Blunder);
        assert_eq!(ke7.best_san.as_deref(), Some("b8c6"), "the better move is offered");
        assert_eq!(r.black.blunders, 1);
        assert_eq!(r.white.inaccuracies, 1);
        assert_eq!(r.key_moments[0], 4);
        assert!(r.white.accuracy.unwrap() > r.black.accuracy.unwrap());
    }

    #[test]
    fn measures_sacrifices() {
        let pos = parse_fen("r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/8/PPPP1PPP/RNBQK1NR w KQkq - 2 3").unwrap();
        // Bxf7+ Kxf7 gives up a bishop for a pawn.
        assert_eq!(sacrifice(&pos, "c4f7", Some("e8f7")), 2);
        assert_eq!(sacrifice(&pos, "b1c3", None), 0);
    }
}
