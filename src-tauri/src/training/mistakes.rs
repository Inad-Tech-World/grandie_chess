//! "Learn from your mistakes": positions where the user went wrong in a
//! reviewed game become cards, scheduled with spaced repetition (FSRS) like
//! the opening repertoire, so the right idea comes back until it sticks.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::srs::{review, CardState, Grade};
use crate::analysis::classify::MoveClass;
use crate::analysis::review::ReviewResult;
use crate::error::Result;
use crate::game::position::Side;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MistakeCard {
    pub id: i32,
    pub game_id: Option<i32>,
    /// The position before the mistake; the user is to move.
    pub fen: String,
    pub side: Side,
    pub played: String,
    pub played_san: String,
    pub best: String,
    pub best_san: String,
    /// The better line, in SAN (first = best move).
    pub best_line: Vec<String>,
    /// What punishes the mistake, in SAN.
    pub refutation: Vec<String>,
    pub win_before: f64,
    pub win_after: f64,
    pub due: f64,
    pub reps: i32,
}

fn side_str(s: Side) -> &'static str {
    match s {
        Side::White => "white",
        Side::Black => "black",
    }
}

/// Add the user's mistakes and blunders from a review; returns how many
/// new cards were made.
pub fn add_from_review(conn: &Connection, game_id: Option<i32>, review_result: &ReviewResult, user: Side, now: i64) -> Result<u32> {
    let mut added = 0;
    for (i, m) in review_result.moves.iter().enumerate() {
        if m.side != user || !matches!(m.class, MoveClass::Mistake | MoveClass::Blunder) {
            continue;
        }
        let (Some(before), Some(after), Some(best), Some(best_san)) =
            (review_result.evals.get(i), review_result.evals.get(i + 1), m.best.as_ref(), m.best_san.as_ref())
        else {
            continue;
        };
        let card = CardState::new(now);
        added += conn.execute(
            "INSERT OR IGNORE INTO mistake_cards (game_id, fen, played, played_san, best, best_san, best_line, refutation, side,
                win_before, win_after, due, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                game_id,
                before.fen,
                m.uci,
                m.san,
                best,
                best_san,
                serde_json::to_string(&m.best_line)?,
                serde_json::to_string(&after.line)?,
                side_str(m.side),
                m.win_before,
                m.win_after,
                card.due as i64,
                now
            ],
        )? as u32;
    }
    Ok(added)
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<MistakeCard> {
    let line = |s: String| serde_json::from_str::<Vec<String>>(&s).unwrap_or_default();
    Ok(MistakeCard {
        id: r.get(0)?,
        game_id: r.get(1)?,
        fen: r.get(2)?,
        played: r.get(3)?,
        played_san: r.get(4)?,
        best: r.get(5)?,
        best_san: r.get(6)?,
        best_line: line(r.get(7)?),
        refutation: line(r.get(8)?),
        side: if r.get::<_, String>(9)? == "white" { Side::White } else { Side::Black },
        win_before: r.get(10)?,
        win_after: r.get(11)?,
        due: r.get::<_, i64>(12)? as f64,
        reps: r.get(13)?,
    })
}

const COLUMNS: &str = "id, game_id, fen, played, played_san, best, best_san, best_line, refutation, side, win_before, win_after, due, reps";

/// Cards due now, most overdue first.
pub fn due(conn: &Connection, now: i64, limit: u32) -> Result<Vec<MistakeCard>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM mistake_cards WHERE due <= ?1 ORDER BY due LIMIT ?2"))?;
    let cards = stmt.query_map(params![now, limit], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(cards)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MistakeStats {
    pub total: u32,
    pub due: u32,
}

pub fn stats(conn: &Connection, now: i64) -> Result<MistakeStats> {
    let (total, due): (i64, i64) =
        conn.query_row("SELECT COUNT(*), COALESCE(SUM(due <= ?1), 0) FROM mistake_cards", [now], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(MistakeStats { total: total as u32, due: due as u32 })
}

/// Record how the retry went.
pub fn grade(conn: &Connection, id: i32, g: Grade, now: i64) -> Result<CardState> {
    let card: CardState = conn.query_row(
        "SELECT due, stability, difficulty, elapsed_days, scheduled_days, reps, lapses, state, last_review FROM mistake_cards WHERE id = ?1",
        [id],
        |r| {
            Ok(CardState {
                due: r.get::<_, i64>(0)? as f64,
                stability: r.get(1)?,
                difficulty: r.get(2)?,
                elapsed_days: r.get(3)?,
                scheduled_days: r.get(4)?,
                reps: r.get(5)?,
                lapses: r.get(6)?,
                state: r.get(7)?,
                last_review: r.get::<_, Option<i64>>(8)?.map(|t| t as f64),
            })
        },
    )?;
    let next = review(card, g, now);
    conn.execute(
        "UPDATE mistake_cards SET due = ?2, stability = ?3, difficulty = ?4, elapsed_days = ?5, scheduled_days = ?6,
            reps = ?7, lapses = ?8, state = ?9, last_review = ?10 WHERE id = ?1",
        params![
            id,
            next.due as i64,
            next.stability,
            next.difficulty,
            next.elapsed_days,
            next.scheduled_days,
            next.reps,
            next.lapses,
            next.state,
            next.last_review.map(|t| t as i64)
        ],
    )?;
    Ok(next)
}

pub fn remove(conn: &Connection, id: i32) -> Result<()> {
    conn.execute("DELETE FROM mistake_cards WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::review::{MoveReview, PositionEval, ReviewDepth, SideSummary};

    #[test]
    fn mistakes_become_cards_once() {
        let pool = crate::db::open_memory().unwrap();
        let conn = pool.get().unwrap();
        let ev = |ply: u32, fen: &str, line: &[&str]| PositionEval {
            ply,
            fen: fen.into(),
            white_win: 50.0,
            score: None,
            best: None,
            best_san: None,
            line: line.iter().map(|s| s.to_string()).collect(),
            second_white_win: None,
        };
        let review = ReviewResult {
            start_fen: crate::game::position::START_FEN.into(),
            engine: "t".into(),
            depth: ReviewDepth::Quick,
            evals: vec![ev(0, "fen0", &["e4"]), ev(1, "fen1", &["Qh4"]), ev(2, "fen2", &[])],
            moves: vec![
                MoveReview {
                    ply: 1,
                    side: Side::White,
                    uci: "f2f3".into(),
                    san: "f3".into(),
                    class: MoveClass::Mistake,
                    win_before: 55.0,
                    win_after: 40.0,
                    accuracy: 50.0,
                    best: Some("e2e4".into()),
                    best_san: Some("e4".into()),
                    best_line: vec!["e4".into(), "e5".into()],
                },
                MoveReview {
                    ply: 2,
                    side: Side::Black,
                    uci: "e7e5".into(),
                    san: "e5".into(),
                    class: MoveClass::Blunder,
                    win_before: 60.0,
                    win_after: 20.0,
                    accuracy: 10.0,
                    best: Some("d7d5".into()),
                    best_san: Some("d5".into()),
                    best_line: vec![],
                },
            ],
            white: SideSummary::default(),
            black: SideSummary::default(),
            key_moments: vec![],
        };
        assert_eq!(add_from_review(&conn, None, &review, Side::White, 1000).unwrap(), 1, "only the user's own mistakes");
        assert_eq!(add_from_review(&conn, None, &review, Side::White, 1000).unwrap(), 0, "not twice");
        let cards = due(&conn, 1000, 10).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].best_line, vec!["e4", "e5"]);
        assert_eq!(cards[0].refutation, vec!["Qh4"]);
        let next = grade(&conn, cards[0].id, Grade::Good, 1000).unwrap();
        assert!(next.due > 1000.0);
        assert_eq!(stats(&conn, 1000).unwrap().due, 0);
    }
}
