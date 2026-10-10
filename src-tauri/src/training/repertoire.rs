//! Opening repertoires (one for White, one for Black) and their drill cards.
//!
//! A repertoire is a graph of moves between positions (keyed by Zobrist hash,
//! so transpositions merge). Every position where it is the repertoire side's
//! turn and the repertoire has an answer becomes a spaced-repetition card.

use std::collections::{HashMap, HashSet, VecDeque};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Color, Position};
use specta::Type;

use super::srs::{review, CardState, Grade};
use crate::error::{Error, Result};
use crate::game::position::{apply_uci, fen_of, hash_of, parse_fen, Side, START_FEN};
use crate::game::tree::GameTree;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RepMove {
    pub id: i32,
    pub parent_fen: String,
    pub uci: String,
    pub san: String,
    pub child_fen: String,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedMove {
    pub uci: String,
    pub san: String,
}

/// One drill: reach `fen` by playing `line` from the start, then the user
/// must play one of `expected`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DrillItem {
    pub color: Side,
    /// Position key (a Zobrist hash as a string; TypeScript can't hold u64).
    pub key: String,
    pub fen: String,
    pub line: Vec<String>,
    pub expected: Vec<ExpectedMove>,
    pub is_new: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RepertoireStats {
    pub moves: u32,
    pub cards: u32,
    pub due_now: u32,
    pub new_cards: u32,
    /// Next due time (unix seconds) after the current ones.
    pub next_due: Option<f64>,
}

pub fn side_str(c: Side) -> &'static str {
    match c {
        Side::White => "white",
        Side::Black => "black",
    }
}

fn start_hash() -> i64 {
    hash_of(&Chess::default()) as i64
}

/// Add one move from `parent` (if not already there). Returns true if new.
fn add_move(conn: &Connection, color: Side, parent: &Chess, uci: &str, now: i64) -> Result<(bool, Chess)> {
    let (child, applied) = apply_uci(parent, uci)?;
    let changed = conn.execute(
        "INSERT OR IGNORE INTO repertoire_moves (color, parent_hash, parent_fen, uci, san, child_hash, child_fen, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            side_str(color),
            hash_of(parent) as i64,
            fen_of(parent),
            applied.uci,
            applied.san,
            hash_of(&child) as i64,
            applied.position.fen,
            now
        ],
    )?;
    Ok((changed > 0, child))
}

/// Add a line of UCI moves played from the standard start position.
pub fn add_line(conn: &mut Connection, color: Side, ucis: &[String], now: i64) -> Result<u32> {
    let tx = conn.transaction()?;
    let mut pos = Chess::default();
    let mut added = 0;
    for uci in ucis {
        let (new, next) = add_move(&tx, color, &pos, uci, now)?;
        added += new as u32;
        pos = next;
    }
    sync_cards(&tx, color, now)?;
    tx.commit()?;
    Ok(added)
}

/// Add every move of a game tree (main line and variations).
pub fn add_tree(conn: &mut Connection, color: Side, tree: &GameTree, now: i64) -> Result<u32> {
    if tree.root().fen != START_FEN {
        return Err(Error::Chess("repertoire lines must start from the initial position".into()));
    }
    let tx = conn.transaction()?;
    let mut added = 0;
    let mut stack: Vec<(u32, Chess)> = vec![(0, Chess::default())];
    while let Some((id, pos)) = stack.pop() {
        for &child in &tree.nodes[id as usize].children {
            let Some(uci) = tree.nodes[child as usize].uci.as_deref() else { continue };
            let (new, next) = add_move(&tx, color, &pos, uci, now)?;
            added += new as u32;
            stack.push((child, next));
        }
    }
    sync_cards(&tx, color, now)?;
    tx.commit()?;
    Ok(added)
}

/// Remove a move and everything only reachable through it.
pub fn remove_move(conn: &mut Connection, id: i32, now: i64) -> Result<()> {
    let tx = conn.transaction()?;
    let color: Option<String> = tx.query_row("SELECT color FROM repertoire_moves WHERE id = ?1", [id], |r| r.get(0)).optional()?;
    let Some(color) = color else { return Ok(()) };
    tx.execute("DELETE FROM repertoire_moves WHERE id = ?1", [id])?;
    let side = if color == "white" { Side::White } else { Side::Black };
    prune_unreachable(&tx, side)?;
    sync_cards(&tx, side, now)?;
    tx.commit()?;
    Ok(())
}

pub fn set_comment(conn: &Connection, id: i32, comment: Option<&str>) -> Result<()> {
    conn.execute("UPDATE repertoire_moves SET comment = ?2 WHERE id = ?1", params![id, comment.filter(|c| !c.trim().is_empty())])?;
    Ok(())
}

/// (id, parent_hash, child_hash, uci) for every move of a repertoire.
fn edges(conn: &Connection, color: Side) -> Result<Vec<(i64, i64, i64, String)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, parent_hash, child_hash, uci FROM repertoire_moves WHERE color = ?1 ORDER BY created_at, id",
    )?;
    let rows = stmt.query_map([side_str(color)], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn prune_unreachable(conn: &Connection, color: Side) -> Result<()> {
    let all = edges(conn, color)?;
    let mut reachable: HashSet<i64> = HashSet::from([start_hash()]);
    let mut queue = VecDeque::from([start_hash()]);
    while let Some(h) = queue.pop_front() {
        for (_, parent, child, _) in &all {
            if *parent == h && reachable.insert(*child) {
                queue.push_back(*child);
            }
        }
    }
    for (id, parent, _, _) in &all {
        if !reachable.contains(parent) {
            conn.execute("DELETE FROM repertoire_moves WHERE id = ?1", [id])?;
        }
    }
    Ok(())
}

/// One card per position where the repertoire side is to move and has an answer.
fn sync_cards(conn: &Connection, color: Side, now: i64) -> Result<()> {
    let turn_char = if color == Side::White { " w " } else { " b " };
    let mut stmt = conn.prepare_cached("SELECT DISTINCT parent_hash, parent_fen FROM repertoire_moves WHERE color = ?1")?;
    let wanted: Vec<(i64, String)> = stmt
        .query_map([side_str(color)], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<(i64, String)>>>()?
        .into_iter()
        .filter(|(_, fen)| fen.contains(turn_char))
        .collect();
    let wanted_keys: HashSet<i64> = wanted.iter().map(|w| w.0).collect();
    let new_card = CardState::new(now);
    for (hash, fen) in &wanted {
        conn.execute(
            "INSERT OR IGNORE INTO repertoire_cards (color, hash, fen, due, state) VALUES (?1, ?2, ?3, ?4, 0)",
            params![side_str(color), hash, fen, new_card.due as i64],
        )?;
    }
    let mut stmt = conn.prepare_cached("SELECT hash FROM repertoire_cards WHERE color = ?1")?;
    let existing: Vec<i64> = stmt.query_map([side_str(color)], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for h in existing.into_iter().filter(|h| !wanted_keys.contains(h)) {
        conn.execute("DELETE FROM repertoire_cards WHERE color = ?1 AND hash = ?2", params![side_str(color), h])?;
    }
    Ok(())
}

pub fn moves(conn: &Connection, color: Side) -> Result<Vec<RepMove>> {
    let mut stmt = conn.prepare(
        "SELECT id, parent_fen, uci, san, child_fen, comment FROM repertoire_moves WHERE color = ?1 ORDER BY created_at, id",
    )?;
    let rows = stmt.query_map([side_str(color)], |r| {
        Ok(RepMove {
            id: r.get::<_, i64>(0)? as i32,
            parent_fen: r.get(1)?,
            uci: r.get(2)?,
            san: r.get(3)?,
            child_fen: r.get(4)?,
            comment: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The repertoire as a game tree (first-added moves become main lines).
pub fn to_tree(conn: &Connection, color: Side) -> Result<GameTree> {
    let all = moves(conn, color)?;
    let mut by_parent: HashMap<u64, Vec<&RepMove>> = HashMap::new();
    for m in &all {
        by_parent.entry(hash_of(&parse_fen(&m.parent_fen)?)).or_default().push(m);
    }
    let mut tree = GameTree::new(None)?;
    tree.set_header("Event", format!("Grandie repertoire ({})", side_str(color)));
    let mut seen: HashSet<u64> = HashSet::new();
    let mut stack: Vec<(u32, u64)> = vec![(0, hash_of(&Chess::default()))];
    while let Some((node, hash)) = stack.pop() {
        if !seen.insert(hash) {
            continue; // transposition: write the line once
        }
        for m in by_parent.get(&hash).into_iter().flatten() {
            let child = tree.add_applied(node, &m.uci, &m.san, m.child_fen.clone());
            tree.nodes[child as usize].comment = m.comment.clone();
            stack.push((child, hash_of(&parse_fen(&m.child_fen)?)));
        }
    }
    Ok(tree)
}

/// Shortest move sequence from the start to every reachable position.
fn paths(all: &[(i64, i64, i64, String)]) -> HashMap<i64, Vec<String>> {
    let mut out: HashMap<i64, Vec<String>> = HashMap::from([(start_hash(), vec![])]);
    let mut queue = VecDeque::from([start_hash()]);
    while let Some(h) = queue.pop_front() {
        let base = out[&h].clone();
        for (_, parent, child, uci) in all {
            if *parent == h && !out.contains_key(child) {
                let mut p = base.clone();
                p.push(uci.clone());
                out.insert(*child, p);
                queue.push_back(*child);
            }
        }
    }
    out
}

/// Cards due at `now` (overdue first, then new ones), at most `limit`.
pub fn due(conn: &Connection, color: Option<Side>, now: i64, limit: u32, new_limit: u32) -> Result<Vec<DrillItem>> {
    let mut items = Vec::new();
    for side in [Side::White, Side::Black] {
        if color.is_some_and(|c| c != side) {
            continue;
        }
        let all = edges(conn, side)?;
        let paths = paths(&all);
        let mut stmt = conn.prepare(
            "SELECT c.hash, c.fen, c.state FROM repertoire_cards c
             WHERE c.color = ?1 AND c.due <= ?2
             ORDER BY c.state = 0, c.due",
        )?;
        let cards = stmt
            .query_map(params![side_str(side), now], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut new_taken = 0;
        for (hash, fen, state) in cards {
            if state == 0 {
                if new_taken >= new_limit {
                    continue;
                }
                new_taken += 1;
            }
            let Some(line) = paths.get(&hash) else { continue };
            let mut expected_stmt =
                conn.prepare_cached("SELECT uci, san FROM repertoire_moves WHERE color = ?1 AND parent_hash = ?2 ORDER BY created_at")?;
            let expected = expected_stmt
                .query_map(params![side_str(side), hash], |r| Ok(ExpectedMove { uci: r.get(0)?, san: r.get(1)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            items.push(DrillItem { color: side, key: hash.to_string(), fen, line: line.clone(), expected, is_new: state == 0 });
        }
    }
    // Reviews before new cards, and shorter lines first so openings are learned in order.
    items.sort_by_key(|i| (i.is_new, i.line.len()));
    items.truncate(limit as usize);
    Ok(items)
}

fn load_card(conn: &Connection, color: Side, hash: i64) -> Result<CardState> {
    conn.query_row(
        "SELECT due, stability, difficulty, elapsed_days, scheduled_days, reps, lapses, state, last_review
         FROM repertoire_cards WHERE color = ?1 AND hash = ?2",
        params![side_str(color), hash],
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
    )
    .optional()?
    .ok_or_else(|| Error::NotFound("that position is no longer in your repertoire".into()))
}

/// Record a drill answer and schedule the card's next review.
pub fn grade(conn: &Connection, color: Side, key: &str, g: Grade, now: i64) -> Result<CardState> {
    let hash: i64 = key.parse().map_err(|_| Error::other("bad card key"))?;
    let next = review(load_card(conn, color, hash)?, g, now);
    conn.execute(
        "UPDATE repertoire_cards SET due = ?3, stability = ?4, difficulty = ?5, elapsed_days = ?6, scheduled_days = ?7,
            reps = ?8, lapses = ?9, state = ?10, last_review = ?11
         WHERE color = ?1 AND hash = ?2",
        params![
            side_str(color),
            hash,
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
    conn.execute(
        "INSERT INTO repertoire_reviews (color, hash, grade, reviewed_at) VALUES (?1, ?2, ?3, ?4)",
        params![side_str(color), hash, format!("{g:?}").to_lowercase(), now],
    )?;
    Ok(next)
}

pub fn stats(conn: &Connection, color: Side, now: i64) -> Result<RepertoireStats> {
    let c = side_str(color);
    let count = |sql: &str| conn.query_row(sql, params![c, now], |r| r.get::<_, i64>(0)).map(|n| n as u32);
    Ok(RepertoireStats {
        moves: conn.query_row("SELECT COUNT(*) FROM repertoire_moves WHERE color = ?1", [c], |r| r.get::<_, i64>(0))? as u32,
        cards: conn.query_row("SELECT COUNT(*) FROM repertoire_cards WHERE color = ?1", [c], |r| r.get::<_, i64>(0))? as u32,
        due_now: count("SELECT COUNT(*) FROM repertoire_cards WHERE color = ?1 AND due <= ?2 AND state != 0")?,
        new_cards: count("SELECT COUNT(*) FROM repertoire_cards WHERE color = ?1 AND state = 0 AND ?2 = ?2")?,
        next_due: conn
            .query_row("SELECT MIN(due) FROM repertoire_cards WHERE color = ?1 AND due > ?2", params![c, now], |r| r.get::<_, Option<i64>>(0))?
            .map(|t| t as f64),
    })
}

/// Which side a position's turn belongs to, for the UI.
pub fn turn_of(fen: &str) -> Result<Side> {
    Ok(if parse_fen(fen)?.turn() == Color::White { Side::White } else { Side::Black })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;

    fn line(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    const T0: i64 = 1_800_000_000;

    #[test]
    fn builds_cards_for_the_repertoire_side_only() {
        let pool = open_memory().unwrap();
        let mut conn = pool.get().unwrap();
        // White repertoire: 1. e4, answering 1...e5 with 2. Nf3 and 1...c5 with 2. Nf3.
        assert_eq!(add_line(&mut conn, Side::White, &line("e2e4 e7e5 g1f3"), T0).unwrap(), 3);
        assert_eq!(add_line(&mut conn, Side::White, &line("e2e4 c7c5 g1f3"), T0).unwrap(), 2);
        let s = stats(&conn, Side::White, T0).unwrap();
        assert_eq!(s.moves, 5);
        assert_eq!(s.cards, 3, "start, after 1...e5, after 1...c5");
        let drill = due(&conn, Some(Side::White), T0, 10, 10).unwrap();
        assert_eq!(drill.len(), 3);
        assert_eq!(drill[0].line.len(), 0, "the first card is the start position");
        assert_eq!(drill[0].expected[0].san, "e4");
        assert!(drill.iter().any(|d| d.line == line("e2e4 c7c5") && d.expected[0].san == "Nf3"));
        assert_eq!(stats(&conn, Side::Black, T0).unwrap().cards, 0);
    }

    #[test]
    fn grading_schedules_and_removal_prunes() {
        let pool = open_memory().unwrap();
        let mut conn = pool.get().unwrap();
        add_line(&mut conn, Side::Black, &line("e2e4 c7c5 g1f3 d7d6"), T0).unwrap();
        let cards = due(&conn, Some(Side::Black), T0, 10, 10).unwrap();
        assert_eq!(cards.len(), 2, "after 1. e4 and after 2. Nf3");
        let first = &cards[0];
        let next = grade(&conn, Side::Black, &first.key, Grade::Good, T0).unwrap();
        assert!(next.due as i64 > T0);
        assert_eq!(due(&conn, Some(Side::Black), T0, 10, 10).unwrap().len(), 1, "graded card is no longer due");
        assert!(due(&conn, Some(Side::Black), next.due as i64 + 1, 10, 10).unwrap().len() >= 1);

        // Removing 1...c5 drops the rest of the line too.
        let c5 = moves(&conn, Side::Black).unwrap().into_iter().find(|m| m.san == "c5").unwrap();
        remove_move(&mut conn, c5.id, T0).unwrap();
        let left = moves(&conn, Side::Black).unwrap();
        assert_eq!(left.iter().map(|m| m.san.as_str()).collect::<Vec<_>>(), vec!["e4"]);
        assert_eq!(stats(&conn, Side::Black, T0).unwrap().cards, 0);
    }

    #[test]
    fn tree_round_trip_and_transpositions() {
        let pool = open_memory().unwrap();
        let mut conn = pool.get().unwrap();
        add_line(&mut conn, Side::White, &line("d2d4 g8f6 c2c4 e7e6 g1f3"), T0).unwrap();
        // Same position via 1. c4 move order: no new card for the shared position.
        add_line(&mut conn, Side::White, &line("c2c4 e7e6 d2d4 g8f6 g1f3"), T0).unwrap();
        let tree = to_tree(&conn, Side::White).unwrap();
        assert_eq!(tree.root().children.len(), 2);
        let text = crate::game::pgn::write_pgn(&tree);
        let parsed = crate::game::pgn::parse_pgn(&text);
        let pool2 = open_memory().unwrap();
        let mut conn2 = pool2.get().unwrap();
        add_tree(&mut conn2, Side::White, &parsed.games[0], T0).unwrap();
        assert_eq!(stats(&conn2, Side::White, T0).unwrap().cards, stats(&conn, Side::White, T0).unwrap().cards);
        assert_eq!(turn_of(START_FEN).unwrap(), Side::White);
    }
}
