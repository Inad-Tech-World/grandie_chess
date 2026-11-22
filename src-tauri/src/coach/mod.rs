//! The talking coach. It shows ideas on the board, not just in words: it
//! plays the reply that punishes a mistake, takes the moves back, plays the
//! better line, and narrates as it goes. The chess always comes from an
//! engine (Stockfish, or Leela in the AlphaZero style); an AI model, when
//! enabled, only puts the engine's lines into words.

pub mod chat;
pub mod engine;
pub mod facts;
pub mod narrate;
pub mod script;

use serde::{Deserialize, Serialize};
use shakmaty::{Color, Position};
use specta::Type;

use crate::analysis::classify::{classify, MoveClass, MoveFacts};
use crate::error::Result;
use crate::game::position::{fen_of, parse_fen, parse_uci, san_of};
use engine::{CoachAnalysis, CoachDef};
use facts::{color_name, eval_words, line_facts, LineFacts};
use script::{CoachScript, CoachShape, SayRole, ScriptBuilder};

/// How many moves of a line the coach plays on the board.
const LINE_MOVES: usize = 4;

/// A move to explain, with the engine's view of it. Win percentages are from
/// the mover's point of view.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveContext {
    pub fen_before: String,
    /// The move that led to `fen_before`, to highlight on the board.
    pub last_move_before: Option<String>,
    /// The move played (UCI).
    pub played: String,
    /// The engine's best line from `fen_before`, in SAN (first = best move).
    pub best_line: Vec<String>,
    /// The engine's line after the played move, in SAN (the reply first).
    pub reply_line: Vec<String>,
    pub win_before: f64,
    pub win_after: f64,
    pub class: MoveClass,
}

impl MoveContext {
    /// Work out the context with the coach's engine.
    pub async fn from_engine(state: &crate::state::AppState, coach: &CoachDef, fen_before: &str, played: &str) -> Result<MoveContext> {
        let pos = parse_fen(fen_before)?;
        let m = parse_uci(&pos, played)?;
        let mover = pos.turn();
        let mut after = pos.clone();
        after.play_unchecked(m);
        let before = state.coach.analyse(state, coach, fen_before, 2).await?;
        let after_an = state.coach.analyse(state, coach, &fen_of(&after), 1).await?;
        let pov = |white_win: f64| if mover == Color::White { white_win } else { 100.0 - white_win };
        let best_win = pov(before.white_win(0));
        let played_win = terminal_win(&after, mover).unwrap_or_else(|| pov(after_an.white_win(0)));
        let played_best = before.best_uci() == Some(played);
        let facts = MoveFacts {
            best_win,
            played_win,
            second_win: before.lines.get(1).and_then(|l| l.white_win).map(pov),
            played_best,
            sacrificed: 0,
        };
        Ok(MoveContext {
            fen_before: fen_of(&pos),
            last_move_before: None,
            played: played.to_string(),
            best_line: before.lines.first().map(|l| l.san.clone()).unwrap_or_default(),
            reply_line: after_an.lines.first().map(|l| l.san.clone()).unwrap_or_default(),
            win_before: best_win,
            win_after: played_win,
            class: classify(&facts),
        })
    }
}

/// The mover's win% if the move ended the game.
fn terminal_win(after: &shakmaty::Chess, mover: Color) -> Option<f64> {
    if after.is_checkmate() {
        return Some(if after.turn() == mover { 0.0 } else { 100.0 });
    }
    after.is_game_over().then_some(50.0)
}

fn class_words(class: MoveClass) -> &'static str {
    match class {
        MoveClass::Brilliant => "a brilliant move",
        MoveClass::Great => "a great move",
        MoveClass::Best => "the best move",
        MoveClass::Good => "a good move",
        MoveClass::Inaccuracy => "an inaccuracy",
        MoveClass::Mistake => "a mistake",
        MoveClass::Blunder => "a blunder",
    }
}

/// "14. Nf3" or "14... Nf6".
pub fn numbered(pos: &shakmaty::Chess, san: &str) -> String {
    let n = pos.fullmoves().get();
    if pos.turn() == Color::White {
        format!("{n}. {san}")
    } else {
        format!("{n}... {san}")
    }
}

fn plural(n: i32, word: &str) -> String {
    if n.abs() == 1 {
        format!("{} {word}", n.abs())
    } else {
        format!("{} {word}s", n.abs())
    }
}

/// What a move cost, in words: the change of verdict, or, when the verdict
/// stays the same ("about equal" both times), how much of the winning
/// chances went.
fn what_it_costs(win_before: f64, win_after: f64) -> String {
    let (before, after) = (eval_words(win_before), eval_words(win_after));
    if before != after {
        return format!("Your position goes from {before} to {after}.");
    }
    let lost = (win_before - win_after).round().max(1.0);
    format!("Your position is still {before}, but your winning chances drop by about {lost:.0}%.")
}

/// Material won, in the words a coach would use ("a piece", "the queen").
fn material_words(n: i32) -> String {
    match n.abs() {
        1 => "a pawn".into(),
        3 => "a piece".into(),
        4 => "a piece and a pawn".into(),
        5 => "a rook".into(),
        9 => "the queen".into(),
        n => format!("{} of material", plural(n, "pawn")),
    }
}

/// What a line achieves, in one sentence, from its first mover's view.
fn line_outcome(f: &LineFacts, mover: Color) -> Option<String> {
    let them = color_name(!mover);
    if f.mate {
        return Some("and that's checkmate.".into());
    }
    if f.material_change >= 1 {
        return Some(format!("winning {}.", material_words(f.material_change)));
    }
    if f.material_change <= -2 {
        return Some(format!("giving up material, but {them} has to be careful."));
    }
    if f.first_move_attacks.len() >= 2 {
        let names: Vec<String> = f.first_move_attacks.iter().take(2).map(|t| format!("the {} on {}", t.role, t.square)).collect();
        return Some(format!("attacking {} at the same time.", names.join(" and ")));
    }
    if f.checks > 0 {
        return Some("keeping the initiative with checks.".into());
    }
    None
}

/// Facts for the narrator about a move and its lines.
fn context_facts(ctx: &MoveContext, played_san: &str, reply: &LineFacts, better: &LineFacts) -> serde_json::Value {
    serde_json::json!({
        "played": played_san,
        "class": class_words(ctx.class),
        "beforeMove": eval_words(ctx.win_before),
        "afterMove": eval_words(ctx.win_after),
        "winBefore": ctx.win_before.round(),
        "winAfter": ctx.win_after.round(),
        "replyLine": reply,
        "betterLine": better,
    })
}

/// Script for one move: why it fails (or works) and what was better, shown
/// on the board. Ends back on `return_fen`.
pub fn explain_move_script(
    id: String,
    coach: &CoachDef,
    ctx: &MoveContext,
    return_fen: &str,
    return_last_move: Option<String>,
) -> Result<(CoachScript, narrate::NarrationContext)> {
    let pos = parse_fen(&ctx.fen_before)?;
    let mover = pos.turn();
    let m = parse_uci(&pos, &ctx.played)?;
    let played_san = san_of(&pos, m);
    let label = numbered(&pos, &played_san);
    let mut after = pos.clone();
    after.play_unchecked(m);
    let reply = line_facts(&after, &ctx.reply_line);
    let better = line_facts(&pos, &ctx.best_line);
    let best_san = ctx.best_line.first().cloned();
    let them = color_name(!mover);

    let mut b = ScriptBuilder::new(&ctx.fen_before, ctx.last_move_before.clone())?;
    let error = ctx.class.is_error();
    b.shapes(CoachShape::arrow(&ctx.played, if error { "red" } else { "green" }).into_iter().collect());
    if error {
        b.say(
            SayRole::Intro,
            format!("{label} is {}. {} Let's see why.", class_words(ctx.class), what_it_costs(ctx.win_before, ctx.win_after)),
        );
        b.play_san(&played_san);
        b.clear_shapes();
        match ctx.reply_line.first() {
            Some(r) => {
                let outcome = line_outcome(&reply.clone(), !mover).map(|o| format!(", {o}")).unwrap_or_else(|| ".".into());
                b.say(SayRole::Refutation, format!("{them} answers {r}{outcome}"));
                b.play_line(&ctx.reply_line, LINE_MOVES);
                b.say(SayRole::Consequence, format!("Now {them} is {}.", eval_words(100.0 - ctx.win_after)));
            }
            None => {
                b.say(SayRole::Refutation, format!("{them} is now {}.", eval_words(100.0 - ctx.win_after)));
            }
        }
        b.back_to_start();
        if let Some(best) = &best_san {
            b.shapes(CoachShape::arrow(&san_to_uci(&pos, best).unwrap_or_default(), "green").into_iter().collect());
            let why = line_outcome(&better, mover).map(|o| format!(", {o}")).unwrap_or_else(|| ".".into());
            b.say(SayRole::Better, format!("Instead, {best} was stronger{why}"));
            b.clear_shapes();
            b.play_line(&ctx.best_line, LINE_MOVES + 1);
            b.say(SayRole::Summary, format!("With this line you'd stay {}.", eval_words(ctx.win_before)));
            b.back_to_start();
        }
    } else {
        let quality = if ctx.class == MoveClass::Good { "a good move".to_string() } else { class_words(ctx.class).to_string() };
        b.say(SayRole::Intro, format!("{label} is {quality}."));
        b.play_san(&played_san);
        b.clear_shapes();
        if !ctx.reply_line.is_empty() {
            b.say(SayRole::Idea, format!("The engine expects {} next.", ctx.reply_line.first().cloned().unwrap_or_default()));
            b.play_line(&ctx.reply_line, LINE_MOVES);
        }
        b.say(SayRole::Summary, format!("You stay {}.", eval_words(ctx.win_after)));
        b.back_to_start();
        if ctx.class == MoveClass::Good {
            if let Some(best) = best_san.as_ref().filter(|b| **b != played_san) {
                b.say(SayRole::Better, format!("{best} was a little more precise."));
                b.play_line(&ctx.best_line, LINE_MOVES);
                b.back_to_start();
            }
        }
    }
    b.clear_shapes();
    let title = format!("{label}: {}", class_words(ctx.class));
    let script = b.finish(id, &coach.id, title, return_fen.to_string(), return_last_move);
    let mut allowed: Vec<String> = vec![played_san.clone()];
    allowed.extend(reply.san.iter().cloned());
    allowed.extend(better.san.iter().cloned());
    let nctx = narrate::NarrationContext {
        side_to_move: color_name(mover).into(),
        phase: facts::phase(&pos).into(),
        opening: None,
        facts: context_facts(ctx, &played_san, &reply, &better),
        allowed_moves: allowed,
        student: String::new(),
    };
    Ok((script, nctx))
}

/// Script that plays an engine line from a position and explains where it
/// leads ("Why?" next to an engine line).
pub fn explain_line_script(
    id: String,
    coach: &CoachDef,
    fen: &str,
    last_move: Option<String>,
    line: &[String],
    white_win: Option<f64>,
) -> Result<(CoachScript, narrate::NarrationContext)> {
    let pos = parse_fen(fen)?;
    let mover = pos.turn();
    let f = line_facts(&pos, line);
    let mut b = ScriptBuilder::new(fen, last_move.clone())?;
    let first = f.san.first().cloned().unwrap_or_default();
    let outcome = line_outcome(&f, mover).map(|o| format!(", {o}")).unwrap_or_else(|| ".".into());
    if let Some(uci) = san_to_uci(&pos, &first) {
        b.shapes(CoachShape::arrow(&uci, "green").into_iter().collect());
    }
    b.say(SayRole::Intro, format!("The engine's idea here is {}{outcome}", numbered(&pos, &first)));
    b.clear_shapes();
    b.play_line(line, LINE_MOVES + 2);
    if let Some(w) = white_win {
        let mover_win = if mover == Color::White { w } else { 100.0 - w };
        b.say(SayRole::Summary, format!("{} ends up {}.", color_name(mover), eval_words(mover_win)));
    }
    b.back_to_start();
    let script = b.finish(id, &coach.id, format!("The idea behind {first}"), fen_of(&pos), last_move);
    let nctx = narrate::NarrationContext {
        side_to_move: color_name(mover).into(),
        phase: facts::phase(&pos).into(),
        opening: None,
        facts: serde_json::json!({ "line": f, "evalForSideToMove": white_win.map(|w| eval_words(if mover == Color::White { w } else { 100.0 - w })) }),
        allowed_moves: f.san.clone(),
        student: String::new(),
    };
    Ok((script, nctx))
}

/// A short script that just demonstrates moves with given narration (the
/// chat coach's `show_line` tool). Illegal moves end the line early.
pub fn demo_script(
    id: String,
    coach_id: &str,
    fen: &str,
    moves: &[String],
    narration: &[String],
    title: &str,
) -> Result<(CoachScript, usize)> {
    let mut b = ScriptBuilder::new(fen, None)?;
    if let Some(intro) = narration.first() {
        b.say(SayRole::Intro, intro.clone());
    }
    let mut played = 0;
    for (i, san) in moves.iter().enumerate().take(12) {
        if !b.play_san(san) {
            break;
        }
        played += 1;
        if let Some(line) = narration.get(i + 1).filter(|l| !l.trim().is_empty()) {
            b.say(SayRole::Idea, line.clone());
        } else {
            b.pause(script::MOVE_PAUSE_MS);
        }
    }
    b.pause(600);
    b.back_to_start();
    let return_fen = fen_of(&parse_fen(fen)?);
    Ok((b.finish(id, coach_id, title, return_fen, None), played))
}

pub fn san_to_uci(pos: &shakmaty::Chess, san: &str) -> Option<String> {
    crate::game::position::parse_san(pos, san).ok().map(crate::game::position::uci_of)
}

/// Turn a coach analysis line into SAN from its position.
pub fn top_line(an: &CoachAnalysis) -> Vec<String> {
    an.lines.first().map(|l| l.san.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coach::script::CoachStep;

    fn coach() -> CoachDef {
        engine::coach_defs().into_iter().next().unwrap()
    }

    #[test]
    fn a_mistake_shows_the_refutation_then_the_better_move() {
        // 1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4?! 4. Nxe5? Qg5 (hitting e5 and g2).
        let fen = "r1bqkbnr/pppp1ppp/8/4p3/2BnP3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4";
        let ctx = MoveContext {
            fen_before: fen.into(),
            last_move_before: Some("c6d4".into()),
            played: "f3e5".into(),
            best_line: vec!["Nxd4".into(), "exd4".into(), "c3".into()],
            reply_line: vec!["Qg5".into(), "Nxf7".into(), "Qxg2".into(), "Rf1".into()],
            win_before: 62.0,
            win_after: 30.0,
            class: MoveClass::Blunder,
        };
        let (script, nctx) = explain_move_script("t".into(), &coach(), &ctx, fen, Some("c6d4".into())).unwrap();
        let moves: Vec<&str> =
            script.steps.iter().filter_map(|s| if let CoachStep::Move { san, .. } = s { Some(san.as_str()) } else { None }).collect();
        assert_eq!(moves[0], "Nxe5", "first the move that was played");
        assert_eq!(&moves[1..5], ["Qg5", "Nxf7", "Qxg2", "Rf1"], "then the punishment");
        assert!(moves.contains(&"Nxd4"), "then the better move");
        let backs = script.steps.iter().filter(|s| matches!(s, CoachStep::Back { .. })).count();
        assert_eq!(backs, 5 + 3, "every demonstrated move is taken back");
        let texts: Vec<&str> = script.says().map(|(_, _, t)| t).collect();
        assert!(texts[0].starts_with("4. Nxe5 is a blunder"), "{}", texts[0]);
        assert!(texts.iter().any(|t| t.contains("Instead, Nxd4")), "{texts:?}");
        assert!(nctx.allowed_moves.contains(&"Qg5".to_string()));
        assert_eq!(script.return_fen, fen);
    }

    #[test]
    fn demo_lines_stop_at_illegal_moves() {
        let (script, played) = demo_script(
            "d".into(),
            "stockfish",
            crate::game::position::START_FEN,
            &["e4".into(), "e5".into(), "Ke3".into()],
            &["Watch the centre.".into()],
            "Demo",
        )
        .unwrap();
        assert_eq!(played, 2);
        assert!(matches!(script.steps.first(), Some(CoachStep::Say { .. })));
    }
}
