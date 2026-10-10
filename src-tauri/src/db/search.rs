//! Position search. Exact search uses the Zobrist index; partial search
//! replays candidate games in parallel and looks for a set of pieces.

use rayon::prelude::*;
use rusqlite::{params_from_iter, types::Value, Connection};
use serde::{Deserialize, Serialize};
use shakmaty::{Board, Chess, Piece, Position, Square};
use specta::Type;

use crate::error::{Error, Result};
use crate::game::position::{hash_of, parse_fen, parse_uci, START_FEN};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PositionQuery {
    /// For exact search a full FEN; for partial search only the piece
    /// placement matters (empty squares are "anything").
    pub fen: String,
    pub exact: bool,
}

fn board_of(fen: &str) -> Result<Board> {
    let placement = fen.split_whitespace().next().unwrap_or("");
    Board::from_ascii_board_fen(placement.as_bytes()).map_err(|e| Error::Chess(format!("invalid position: {e}")))
}

/// True if every required piece stands on its square.
fn contains(board: &Board, required: &[(Square, Piece)]) -> bool {
    required.iter().all(|(sq, piece)| board.piece_at(*sq) == Some(*piece))
}

/// Does a game (start position + UCI moves) ever reach a position containing
/// all `required` pieces? Returns the first such ply.
pub fn game_reaches(start_fen: Option<&str>, moves: &str, required: &[(Square, Piece)]) -> Option<u32> {
    let mut pos: Chess = parse_fen(start_fen.unwrap_or(START_FEN)).ok()?;
    if contains(pos.board(), required) {
        return Some(0);
    }
    for (ply, uci) in moves.split_whitespace().enumerate() {
        let m = parse_uci(&pos, uci).ok()?;
        pos.play_unchecked(m);
        if contains(pos.board(), required) {
            return Some(ply as u32 + 1);
        }
    }
    None
}

/// Ids of candidate games (selected by `candidates_sql`, which must return
/// `id, start_fen, moves`) that match the position query.
pub fn position_hits(conn: &Connection, pq: &PositionQuery, candidates_sql: &str, args: &[Value]) -> Result<Vec<i64>> {
    if pq.exact {
        let hash = hash_of(&parse_fen(&pq.fen)?) as i64;
        let sql = format!(
            "SELECT c.id FROM ({candidates_sql}) c WHERE c.id IN (SELECT game_id FROM positions WHERE hash = ?{})",
            args.len() + 1
        );
        let mut all: Vec<Value> = args.to_vec();
        all.push(Value::Integer(hash));
        let mut stmt = conn.prepare(&sql)?;
        let ids = stmt.query_map(params_from_iter(all.iter()), |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        return Ok(ids);
    }
    let required: Vec<(Square, Piece)> = board_of(&pq.fen)?.iter().collect();
    let mut stmt = conn.prepare(candidates_sql)?;
    let games: Vec<(i64, Option<String>, String)> = stmt
        .query_map(params_from_iter(args.iter()), |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if required.is_empty() {
        return Ok(games.into_iter().map(|g| g.0).collect());
    }
    Ok(games
        .par_iter()
        .filter(|(_, fen, moves)| game_reaches(fen.as_deref(), moves, &required).is_some())
        .map(|(id, _, _)| *id)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::games::{query, GameQuery};
    use crate::db::import::{import_reader, UserIdentity};
    use crate::db::open_memory;

    const PGN: &str = include_str!("../../tests/fixtures/sample_games.pgn");

    #[test]
    fn exact_and_partial_search() {
        let pool = open_memory().unwrap();
        import_reader(&pool, PGN.as_bytes(), "pgn", &UserIdentity::default(), |_| {}).unwrap();
        // After 1. e4 e5 2. Nf3 Nc6: reached in the Italian and the Ruy Lopez.
        let fen = "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3";
        let exact = query(&pool, &GameQuery { position: Some(PositionQuery { fen: fen.into(), exact: true }), ..Default::default() }).unwrap();
        assert_eq!(exact.total, 2);
        // Partial: a white bishop on b5 and a black knight on c6 (Ruy Lopez only).
        let partial = PositionQuery { fen: "8/8/2n5/1B6/8/8/8/8".into(), exact: false };
        let found = query(&pool, &GameQuery { position: Some(partial), ..Default::default() }).unwrap();
        assert_eq!(found.total, 1);
        assert_eq!(found.rows[0].opening.as_deref(), Some("Ruy Lopez"));
        // Combined with another filter.
        let none = query(
            &pool,
            &GameQuery { position: Some(PositionQuery { fen: fen.into(), exact: true }), eco: Some("D".into()), ..Default::default() },
        )
        .unwrap();
        assert_eq!(none.total, 0);
    }

    #[test]
    fn reaches_reports_the_first_ply() {
        let required: Vec<(Square, Piece)> = board_of("8/8/8/8/4P3/8/8/8").unwrap().iter().collect();
        assert_eq!(game_reaches(None, "e2e4 e7e5", &required), Some(1));
        assert_eq!(game_reaches(None, "d2d4 d7d5", &required), None);
    }
}
