//! Studying a master: a guided replay where the coach stops at the moments
//! that show how they play, "Guess the move", the engine pass that measures
//! their style, and the coach's write-up of it.

use serde::{Deserialize, Serialize};
use shakmaty::{Color, Position};
use specta::Type;

use crate::analysis::classify::MoveClass;
use crate::analysis::review::{MoveReview, ReviewResult};
use crate::coach::engine::CoachDef;
use crate::coach::facts::{color_name, eval_words, line_facts};
use crate::coach::narrate::NarrationContext;
use crate::coach::script::{CoachScript, CoachShape, SayRole, ScriptBuilder};
use crate::coach::{numbered, san_to_uci, MoveContext};
use crate::error::Result;
use crate::game::position::{fen_of, parse_fen, parse_uci, san_of, Side};

use super::profile::StyleMetrics;

/// Why the coach stops at a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MomentKind {
    /// A brilliant or only move by the master.
    Masterstroke,
    /// The opponent erred and the master's reply punishes it.
    Punish,
    /// Not the engine's first choice, but just as good: the human idea.
    HumanIdea,
    /// The master's own mistake (instructive too).
    Slip,
}

struct Moment {
    ply: u32,
    kind: MomentKind,
    weight: f64,
}

fn pick_moments(review: &ReviewResult, master: Option<Side>, max: usize) -> Vec<Moment> {
    let is_master = |r: &MoveReview| master.is_none_or(|m| r.side == m);
    let mut out: Vec<Moment> = Vec::new();
    for (i, r) in review.moves.iter().enumerate() {
        if !is_master(r) {
            continue;
        }
        let swing = (r.win_before - r.win_after).abs();
        let prev = i.checked_sub(1).and_then(|j| review.moves.get(j));
        if matches!(r.class, MoveClass::Brilliant | MoveClass::Great) {
            out.push(Moment { ply: r.ply, kind: MomentKind::Masterstroke, weight: 30.0 + swing });
        } else if prev.is_some_and(|p| matches!(p.class, MoveClass::Mistake | MoveClass::Blunder)) && !r.class.is_error() {
            let drop = prev.map(|p| p.win_before - p.win_after).unwrap_or(0.0);
            out.push(Moment { ply: r.ply, kind: MomentKind::Punish, weight: 20.0 + drop });
        } else if matches!(r.class, MoveClass::Mistake | MoveClass::Blunder) {
            out.push(Moment { ply: r.ply, kind: MomentKind::Slip, weight: 10.0 + swing });
        } else if r.best.as_deref() != Some(r.uci.as_str()) && r.win_before - r.win_after < 3.0 && r.ply > 16 && r.best_san.is_some() {
            out.push(Moment { ply: r.ply, kind: MomentKind::HumanIdea, weight: 5.0 });
        }
    }
    out.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));
    // Keep moments a few moves apart so the replay keeps moving.
    let mut chosen: Vec<Moment> = Vec::new();
    for m in out {
        if chosen.len() >= max {
            break;
        }
        if chosen.iter().all(|c| c.ply.abs_diff(m.ply) >= 4) {
            chosen.push(m);
        }
    }
    chosen.sort_by_key(|m| m.ply);
    chosen
}

/// What the coach calls a player: "McDonnell, Alexander" and "Alexander
/// McDonnell" are both "McDonnell".
fn surname(name: &str) -> String {
    let name = name.trim();
    match name.split_once(',') {
        Some((last, _)) if !last.trim().is_empty() => last.trim().to_string(),
        _ => name.split_whitespace().last().unwrap_or(name).to_string(),
    }
}

/// Plays moves between moments; jumps ahead instead when there are too many.
const REPLAY_GAP: usize = 14;
/// After a jump, the moves played out before the next moment (or the end),
/// so the build-up to it, like a final combination, is always shown.
const LEAD_IN: usize = 8;

/// A guided replay of a master game.
#[allow(clippy::too_many_arguments)]
pub fn walkthrough_script(
    id: String,
    coach: &CoachDef,
    start_fen: &str,
    moves: &[String],
    review: &ReviewResult,
    master: Option<Side>,
    master_name: &str,
    title: &str,
    result: &str,
) -> Result<(CoachScript, NarrationContext)> {
    let moments = pick_moments(review, master, 6);
    let mut b = ScriptBuilder::new(start_fen, None)?;
    let short = surname(master_name);
    // Who made a move, as the coach says it: the master's name, or the
    // colour when following both sides.
    let who = |side: Side| if master.is_some() && !short.is_empty() { short.clone() } else { color_name(Color::from(side)).to_string() };
    if master.is_some() && !short.is_empty() {
        b.say(SayRole::Intro, format!("{title}. Let's watch how {short} plays it; I'll stop at the moments that matter."));
    } else {
        b.say(SayRole::Intro, format!("{title}. Let's watch this game; I'll stop at the moments that matter."));
    }
    let mut ply = 0usize;
    let mut allowed: Vec<String> = Vec::new();
    let mut facts = Vec::new();
    let play_to = |b: &mut ScriptBuilder, from: usize, to: usize| -> Result<()> {
        let mut from = from;
        if to.saturating_sub(from) > REPLAY_GAP {
            let resume = to - LEAD_IN;
            let fen = review.evals.get(resume).map(|e| e.fen.clone()).unwrap_or_default();
            let last = resume.checked_sub(1).and_then(|i| moves.get(i).cloned());
            b.reset(&fen, last)?;
            let pos = parse_fen(&fen)?;
            b.say(SayRole::Moment, format!("Skipping ahead to move {}.", pos.fullmoves()));
            from = resume;
        }
        for uci in &moves[from..to] {
            let pos = b.position().clone();
            if let Ok(m) = parse_uci(&pos, uci) {
                b.play_san(&san_of(&pos, m));
                b.pause(250);
            }
        }
        Ok(())
    };
    for m in &moments {
        let i = m.ply as usize - 1;
        play_to(&mut b, ply, i)?;
        let Some(r) = review.moves.get(i) else { continue };
        let pos = b.position().clone();
        let label = numbered(&pos, &r.san);
        let reply = review.evals.get(i + 1).map(|e| e.line.clone()).unwrap_or_default();
        b.shapes(CoachShape::arrow(&r.uci, if m.kind == MomentKind::Slip { "red" } else { "green" }).into_iter().collect());
        let short = who(r.side);
        let intro = match m.kind {
            MomentKind::Masterstroke => format!(
                "{label}! {short} finds {}.",
                match (r.class, r.win_after) {
                    (MoveClass::Brilliant, _) => "a brilliant sacrifice",
                    (_, w) if w >= 62.0 => "the only move that keeps the advantage",
                    (_, w) if w > 38.0 => "the only move that holds the balance",
                    _ => "the toughest defence",
                }
            ),
            MomentKind::Punish => {
                let prev = &review.moves[i - 1];
                format!("{}'s {} was a mistake, and {short} punishes it with {label}.", color_name(Color::from(prev.side)), prev.san)
            }
            MomentKind::HumanIdea => format!(
                "{label}. The engine slightly prefers {}, but {short}'s move has a clear plan.",
                r.best_san.clone().unwrap_or_default()
            ),
            MomentKind::Slip => format!(
                "Even {short} slips here: {label} {}.",
                match r.win_before {
                    w if w >= 62.0 => "lets the advantage go",
                    w if w > 38.0 => "hands the opponent the better game",
                    _ => "makes a difficult position worse",
                }
            ),
        };
        b.say(SayRole::Moment, intro);
        b.play_san(&r.san);
        b.clear_shapes();
        let after = b.position().clone();
        let f = line_facts(&after, &reply);
        if !reply.is_empty() {
            let mover = Color::from(r.side);
            let what = if f.mate {
                "the attack ends in mate.".to_string()
            } else if -f.material_change >= 1 {
                format!("{} wins material.", color_name(mover))
            } else {
                format!("{} stays {}.", color_name(mover), eval_words(r.win_after))
            };
            b.say(SayRole::Idea, format!("The point: after {}, {what}", reply.first().cloned().unwrap_or_default()));
            let n = b.play_line(&reply, 4);
            b.back(n);
        }
        allowed.push(r.san.clone());
        allowed.extend(f.san.iter().cloned());
        if matches!(m.kind, MomentKind::HumanIdea | MomentKind::Slip) {
            if let Some(best) = r.best_san.clone() {
                b.back(1);
                let line = review.evals.get(i).map(|e| e.line.clone()).unwrap_or_default();
                b.shapes(CoachShape::arrow(&san_to_uci(b.position(), &best).unwrap_or_default(), "blue").into_iter().collect());
                b.say(SayRole::Better, format!("The engine's choice was {best}."));
                b.clear_shapes();
                let n = b.play_line(&line, 4);
                b.back(n);
                allowed.extend(line.iter().take(4).cloned());
                b.play_san(&r.san);
            }
        }
        facts.push(serde_json::json!({
            "move": label, "kind": format!("{:?}", m.kind), "class": format!("{:?}", r.class),
            "assessmentAfter": eval_words(r.win_after), "reply": f,
        }));
        ply = i + 1;
    }
    play_to(&mut b, ply, moves.len())?;
    let outcome = match (result, master) {
        ("1-0", Some(Side::White)) | ("0-1", Some(Side::Black)) => format!("And that's how {short} won this game."),
        ("1/2-1/2", _) => "The game ended in a draw.".to_string(),
        ("1-0", Some(Side::Black)) | ("0-1", Some(Side::White)) => format!("{short} lost this one, but there's a lot to learn from it."),
        _ => "That's the game.".to_string(),
    };
    let outcome = if b.position().is_checkmate() { format!("Checkmate. {outcome}") } else { outcome };
    b.say(SayRole::Summary, outcome);
    let final_fen = review.evals.last().map(|e| e.fen.clone()).unwrap_or_else(|| b.fen());
    let script = b.finish(id, &coach.id, title.to_string(), final_fen, moves.last().cloned());
    let ctx = NarrationContext {
        side_to_move: String::new(),
        phase: String::new(),
        opening: None,
        facts: serde_json::json!({ "game": title, "master": master_name, "moments": facts }),
        allowed_moves: allowed,
        student: String::new(),
    };
    Ok((script, ctx))
}

/// The score for one guess in "Guess the move".
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GuessResult {
    /// 3 for the master's move, 2 for an equally good one, 0 otherwise.
    pub points: u32,
    pub verdict: GuessVerdict,
    pub guess_san: String,
    pub master_san: String,
    /// Mover's win% after the guess and after the master's move.
    pub guess_win: f64,
    pub master_win: f64,
    /// A demonstration, when the guess wasn't the master's move.
    pub script: Option<CoachScript>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GuessVerdict {
    Exact,
    AlsoGood,
    Miss,
}

/// Score a guess against the master's move and, when they differ, build a
/// script comparing the two.
pub fn guess_script(
    id: String,
    coach: &CoachDef,
    fen: &str,
    guess: &MoveContext,
    master: &MoveContext,
    master_name: &str,
) -> Result<(GuessResult, Option<NarrationContext>)> {
    let pos = parse_fen(fen)?;
    let san = |uci: &str| parse_uci(&pos, uci).map(|m| san_of(&pos, m)).unwrap_or_else(|_| uci.to_string());
    let (guess_san, master_san) = (san(&guess.played), san(&master.played));
    let short = surname(master_name);
    if guess.played == master.played {
        return Ok((
            GuessResult {
                points: 3,
                verdict: GuessVerdict::Exact,
                guess_san,
                master_san,
                guess_win: master.win_after,
                master_win: master.win_after,
                script: None,
            },
            None,
        ));
    }
    let also_good = guess.win_after >= master.win_after - 5.0;
    let mut b = ScriptBuilder::new(fen, None)?;
    b.shapes(CoachShape::arrow(&guess.played, if also_good { "blue" } else { "red" }).into_iter().collect());
    if also_good {
        b.say(SayRole::Intro, format!("{guess_san} is just as good, by the engine. {short} chose {master_san}."));
    } else {
        b.say(
            SayRole::Intro,
            format!(
                "{guess_san} is {} here. Watch what happens.",
                if guess.class.is_error() { "a mistake" } else { "playable but weaker" }
            ),
        );
        b.play_san(&guess_san);
        b.clear_shapes();
        let n = b.play_line(&guess.reply_line, 3);
        let mover = pos.turn();
        b.say(SayRole::Consequence, format!("{} would be {}.", color_name(mover), eval_words(guess.win_after)));
        b.back(n + 1);
    }
    b.shapes(CoachShape::arrow(&master.played, "green").into_iter().collect());
    b.say(SayRole::Better, format!("{short} played {master_san}."));
    b.play_san(&master_san);
    b.clear_shapes();
    let n = b.play_line(&master.reply_line, 4);
    b.say(SayRole::Idea, format!("That keeps {} {}.", color_name(pos.turn()), eval_words(master.win_after)));
    b.back(n + 1);
    b.clear_shapes();
    let script = b.finish(id, &coach.id, format!("{short}'s choice: {master_san}"), fen_of(&pos), None);
    let mut allowed = vec![guess_san.clone(), master_san.clone()];
    allowed.extend(guess.reply_line.iter().take(3).cloned());
    allowed.extend(master.reply_line.iter().take(4).cloned());
    let ctx = NarrationContext {
        side_to_move: color_name(pos.turn()).into(),
        phase: crate::coach::facts::phase(&pos).into(),
        opening: None,
        facts: serde_json::json!({
            "master": master_name, "guess": guess_san, "masterMove": master_san,
            "guessAssessment": eval_words(guess.win_after), "masterAssessment": eval_words(master.win_after),
            "guessReply": guess.reply_line.iter().take(3).collect::<Vec<_>>(), "masterContinuation": master.reply_line.iter().take(4).collect::<Vec<_>>(),
        }),
        allowed_moves: allowed,
        student: String::new(),
    };
    let verdict = if also_good { GuessVerdict::AlsoGood } else { GuessVerdict::Miss };
    Ok((
        GuessResult {
            points: if also_good { 2 } else { 0 },
            verdict,
            guess_san,
            master_san,
            guess_win: guess.win_after,
            master_win: master.win_after,
            script: Some(script),
        },
        Some(ctx),
    ))
}

/// Add one reviewed game to running style totals.
pub fn add_to_style(total: &mut StyleMetrics, review: &ReviewResult, master: Side) {
    let mine: Vec<&MoveReview> = review.moves.iter().filter(|m| m.side == master).collect();
    if mine.is_empty() {
        return;
    }
    let n = total.games_analysed as f64;
    let acc = mine.iter().map(|m| m.accuracy).sum::<f64>() / mine.len() as f64;
    let swings: Vec<f64> = review.evals.windows(2).map(|w| (w[1].white_win - w[0].white_win).abs()).collect();
    let sharp = if swings.is_empty() { 0.0 } else { swings.iter().sum::<f64>() / swings.len() as f64 };
    let count = |c: MoveClass| mine.iter().filter(|m| m.class == c).count() as f64;
    let avg = |old: f64, new: f64| (old * n + new) / (n + 1.0);
    total.accuracy = avg(total.accuracy, acc);
    total.sharpness = avg(total.sharpness, sharp);
    total.sacrifices_per_game = avg(total.sacrifices_per_game, count(MoveClass::Brilliant));
    total.great_per_game = avg(total.great_per_game, count(MoveClass::Great));
    total.mistakes_per_game = avg(total.mistakes_per_game, count(MoveClass::Mistake));
    total.blunders_per_game = avg(total.blunders_per_game, count(MoveClass::Blunder));
    total.games_analysed += 1;
}

/// The prompt for the coach's style report.
pub fn style_report_prompt(stats: &super::profile::PlayerStats, language: &str) -> String {
    format!(
        "Write a short profile (about 180 words, {language}, plain prose, no headings or lists) of how {name} plays chess, \
         for a club player who wants to learn from them. Base every claim on these statistics from their games in the library; \
         mention concrete openings and numbers, what makes the style distinctive, and two practical lessons to take from it. \
         If the engine-measured style section is present, use it to describe how sharp and how accurate their play is.\n\n{json}",
        name = stats.name,
        json = serde_json::to_string_pretty(stats).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_players_by_surname() {
        assert_eq!(surname("McDonnell, Alexander"), "McDonnell");
        assert_eq!(surname("Alexander McDonnell"), "McDonnell");
        assert_eq!(surname("Tal"), "Tal");
        assert_eq!(surname("NN"), "NN");
    }
    use crate::analysis::review::{PositionEval, SideSummary};
    use crate::game::position::START_FEN;

    fn eval(ply: u32, fen: &str, white_win: f64, line: &[&str]) -> PositionEval {
        PositionEval {
            ply,
            fen: fen.into(),
            white_win,
            score: None,
            best: None,
            best_san: None,
            line: line.iter().map(|s| s.to_string()).collect(),
            second_white_win: None,
        }
    }

    #[test]
    fn a_replay_stops_where_the_opponent_errs() {
        // 1. e4 e5 2. Qh5 Ke7?? 3. Qxe5#: White punishes Black's blunder.
        let moves: Vec<String> = ["e2e4", "e7e5", "d1h5", "e8e7", "h5e5"].iter().map(|s| s.to_string()).collect();
        let fens = [
            START_FEN,
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
            "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
            "rnbqkbnr/pppp1ppp/8/4p2Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 1 2",
            "rnbq1bnr/ppppkppp/8/4p2Q/4P3/8/PPPP1PPP/RNB1KBNR w - - 2 3",
            "rnbq1bnr/ppppkppp/8/4Q3/4P3/8/PPPP1PPP/RNB1KBNR b - - 0 3",
        ];
        let evals: Vec<PositionEval> =
            fens.iter().enumerate().map(|(i, f)| eval(i as u32, f, [55.0, 56.0, 55.0, 50.0, 99.0, 100.0][i], &[])).collect();
        let mv = |ply: u32, side: Side, uci: &str, san: &str, class: MoveClass, before: f64, after: f64| MoveReview {
            ply,
            side,
            uci: uci.into(),
            san: san.into(),
            class,
            win_before: before,
            win_after: after,
            accuracy: 90.0,
            best: Some(uci.into()),
            best_san: Some(san.into()),
            best_line: vec![],
        };
        let review = ReviewResult {
            start_fen: START_FEN.into(),
            engine: "test".into(),
            depth: crate::analysis::review::ReviewDepth::Quick,
            evals,
            moves: vec![
                mv(1, Side::White, "e2e4", "e4", MoveClass::Best, 55.0, 55.0),
                mv(2, Side::Black, "e7e5", "e5", MoveClass::Best, 44.0, 44.0),
                mv(3, Side::White, "d1h5", "Qh5", MoveClass::Inaccuracy, 55.0, 50.0),
                mv(4, Side::Black, "e8e7", "Ke7", MoveClass::Blunder, 50.0, 1.0),
                mv(5, Side::White, "h5e5", "Qxe5#", MoveClass::Best, 99.0, 100.0),
            ],
            white: SideSummary::default(),
            black: SideSummary::default(),
            key_moments: vec![4],
        };
        let moments = pick_moments(&review, Some(Side::White), 6);
        assert_eq!(moments.len(), 1);
        assert_eq!((moments[0].ply, moments[0].kind), (5, MomentKind::Punish));
        let coach = crate::coach::engine::coach_defs().remove(0);
        let (script, _) =
            walkthrough_script("w".into(), &coach, START_FEN, &moves, &review, Some(Side::White), "Paul Morphy", "A miniature", "1-0")
                .unwrap();
        let texts: Vec<&str> = script.says().map(|(_, _, t)| t).collect();
        assert!(texts.iter().any(|t| t.contains("punishes it with 3. Qxe5#")), "{texts:?}");
        assert!(texts.last().unwrap().contains("Morphy won"));
    }
}
