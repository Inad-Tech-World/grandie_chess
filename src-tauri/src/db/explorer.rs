//! Opening explorer over the local database: the moves played from a
//! position, with results, taken from the position index.

use rusqlite::{params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use shakmaty::Position;
use specta::Type;

use super::DbPool;
use crate::error::Result;
use crate::game::openings::{OpeningBook, OpeningName};
use crate::game::position::{hash_of, parse_fen, parse_uci, san_of, Side};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerMove {
    pub uci: String,
    pub san: String,
    pub games: u32,
    pub white: u32,
    pub draws: u32,
    pub black: u32,
    /// Average rating of the players who chose the move.
    pub avg_rating: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerResult {
    pub source: String,
    pub fen: String,
    pub total: u32,
    pub white: u32,
    pub draws: u32,
    pub black: u32,
    pub moves: Vec<ExplorerMove>,
    pub opening: Option<OpeningName>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ExplorerFilter {
    /// Only games the user played.
    pub only_mine: bool,
    /// Only games where the user had this colour.
    pub color: Option<Side>,
}

pub fn explore(pool: &DbPool, fen: &str, filter: &ExplorerFilter) -> Result<ExplorerResult> {
    let pos = parse_fen(fen)?;
    let conn = pool.get()?;
    let mut clauses = vec!["p.hash = ?1".to_string(), "p.next_move IS NOT NULL".to_string()];
    let mut args: Vec<Value> = vec![Value::Integer(hash_of(&pos) as i64)];
    if filter.only_mine || filter.color.is_some() {
        clauses.push("g.user_color IS NOT NULL".into());
    }
    if let Some(c) = filter.color {
        clauses.push("g.user_color = ?2".into());
        args.push(Value::Text(if c == Side::White { "white" } else { "black" }.into()));
    }
    // The mover's rating: White's when White is to move, otherwise Black's.
    let mover_elo = if pos.turn().is_white() { "g.white_elo" } else { "g.black_elo" };
    let sql = format!(
        "SELECT p.next_move, COUNT(*),
                SUM(g.result = '1-0'), SUM(g.result = '1/2-1/2'), SUM(g.result = '0-1'),
                AVG({mover_elo})
         FROM positions p JOIN games g ON g.id = p.game_id
         WHERE {}
         GROUP BY p.next_move
         ORDER BY COUNT(*) DESC
         LIMIT 40",
        clauses.join(" AND ")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args.iter()), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)? as u32,
            r.get::<_, i64>(2)? as u32,
            r.get::<_, i64>(3)? as u32,
            r.get::<_, i64>(4)? as u32,
            r.get::<_, Option<f64>>(5)?,
        ))
    })?;
    let mut result = ExplorerResult {
        source: "database".into(),
        fen: fen.to_string(),
        total: 0,
        white: 0,
        draws: 0,
        black: 0,
        moves: Vec::new(),
        opening: OpeningBook::get().lookup(&pos).cloned(),
    };
    for row in rows {
        let (uci, games, white, draws, black, avg) = row?;
        let Ok(m) = parse_uci(&pos, &uci) else { continue };
        result.total += games;
        result.white += white;
        result.draws += draws;
        result.black += black;
        result.moves.push(ExplorerMove { san: san_of(&pos, m), uci, games, white, draws, black, avg_rating: avg });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::import::{import_reader, UserIdentity};
    use crate::db::open_memory;
    use crate::game::position::START_FEN;

    #[test]
    fn groups_moves_with_results() {
        let pool = open_memory().unwrap();
        let pgn = include_str!("../../tests/fixtures/sample_games.pgn");
        import_reader(&pool, pgn.as_bytes(), "pgn", &UserIdentity::new([Some("Hero".to_string())]), |_| {}).unwrap();
        let start = explore(&pool, START_FEN, &ExplorerFilter::default()).unwrap();
        assert_eq!(start.total, 4);
        assert_eq!(start.moves[0].san, "e4");
        assert_eq!(start.moves[0].games, 3);
        assert_eq!(start.moves[0].white, 2, "Italian and Ruy Lopez were won by White");
        assert_eq!(start.moves[0].black, 1);
        let mine = explore(&pool, START_FEN, &ExplorerFilter { color: Some(Side::White), ..Default::default() }).unwrap();
        assert_eq!(mine.total, 2, "Hero played White twice");
        let after_e4 = explore(&pool, "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1", &ExplorerFilter::default()).unwrap();
        assert_eq!(after_e4.moves.iter().map(|m| m.san.as_str()).collect::<Vec<_>>(), vec!["e5", "c5"]);
        assert!(after_e4.opening.is_some());
    }
}
