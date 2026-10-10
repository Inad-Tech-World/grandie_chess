//! Reading games back: filtered, sorted pages for the database table, single
//! games for analysis, stored reviews and account sync state.

use rusqlite::{params, params_from_iter, types::Value, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::search::{position_hits, PositionQuery};
use super::{now_secs, DbPool};
use crate::analysis::review::ReviewResult;
use crate::error::{Error, Result};
use crate::game::pgn::parse_pgn;
use crate::game::position::Side;
use crate::game::tree::GameTree;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GameRow {
    pub id: i32,
    pub source: String,
    pub white: Option<String>,
    pub black: Option<String>,
    pub white_elo: Option<i32>,
    pub black_elo: Option<i32>,
    pub result: String,
    pub date: Option<String>,
    pub eco: Option<String>,
    pub opening: Option<String>,
    pub time_control: Option<String>,
    pub speed: Option<String>,
    pub user_color: Option<Side>,
    pub opponent_level: Option<String>,
    pub ply_count: u32,
    pub reviewed: bool,
    pub user_accuracy: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UserOutcome {
    Win,
    Loss,
    Draw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SortColumn {
    Date,
    White,
    Black,
    WhiteElo,
    BlackElo,
    Result,
    Eco,
    Moves,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct GameQuery {
    /// Matches player names, event and opening.
    pub text: Option<String>,
    /// Only games where the user played this colour.
    pub color: Option<Side>,
    pub outcome: Option<UserOutcome>,
    pub only_mine: bool,
    pub source: Option<String>,
    pub speed: Option<String>,
    /// ECO code prefix, e.g. "B" or "C50".
    pub eco: Option<String>,
    /// Inclusive bounds, "YYYY-MM-DD".
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    /// Bounds on the game's average rating (of the players with a rating).
    pub min_rating: Option<i32>,
    pub max_rating: Option<i32>,
    pub position: Option<PositionQuery>,
    pub sort: Option<SortColumn>,
    pub desc: bool,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GamePage {
    pub total: u32,
    pub rows: Vec<GameRow>,
    /// Milliseconds the query took (shown so search speed is visible).
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DbGame {
    pub row: GameRow,
    pub tree: GameTree,
    pub review: Option<ReviewResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DbStats {
    pub games: u32,
    pub mine: u32,
    pub reviewed: u32,
    pub sources: Vec<(String, u32)>,
}

const ROW_COLUMNS: &str = "g.id, g.source, g.white, g.black, g.white_elo, g.black_elo, g.result, g.date, g.eco, g.opening,
    g.time_control, g.speed, g.user_color, g.opponent_level, g.ply_count, r.game_id IS NOT NULL, r.user_accuracy";

fn parse_side(s: Option<String>) -> Option<Side> {
    match s.as_deref() {
        Some("white") => Some(Side::White),
        Some("black") => Some(Side::Black),
        _ => None,
    }
}

fn row_from(r: &rusqlite::Row) -> rusqlite::Result<GameRow> {
    Ok(GameRow {
        id: r.get::<_, i64>(0)? as i32,
        source: r.get(1)?,
        white: r.get(2)?,
        black: r.get(3)?,
        white_elo: r.get(4)?,
        black_elo: r.get(5)?,
        result: r.get(6)?,
        date: r.get(7)?,
        eco: r.get(8)?,
        opening: r.get(9)?,
        time_control: r.get(10)?,
        speed: r.get(11)?,
        user_color: parse_side(r.get(12)?),
        opponent_level: r.get(13)?,
        ply_count: r.get::<_, i64>(14)? as u32,
        reviewed: r.get(15)?,
        user_accuracy: r.get(16)?,
    })
}

fn date_to_secs(d: &str, end: bool) -> Option<i64> {
    let date = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()?;
    let t = if end { date.and_hms_opt(23, 59, 59)? } else { date.and_hms_opt(0, 0, 0)? };
    Some(t.and_utc().timestamp())
}

/// SQL `WHERE` clause and parameters for the non-position filters.
fn filters(q: &GameQuery) -> (String, Vec<Value>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut args: Vec<Value> = Vec::new();
    if let Some(text) = q.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        clauses.push(
            "(g.white LIKE ?1 OR g.black LIKE ?1 OR g.event LIKE ?1 OR g.opening LIKE ?1 OR g.eco LIKE ?1)"
                .replace("?1", &format!("?{}", args.len() + 1)),
        );
        args.push(Value::Text(format!("%{text}%")));
    }
    if q.only_mine || q.color.is_some() || q.outcome.is_some() {
        clauses.push("g.user_color IS NOT NULL".into());
    }
    if let Some(c) = q.color {
        clauses.push(format!("g.user_color = ?{}", args.len() + 1));
        args.push(Value::Text(if c == Side::White { "white" } else { "black" }.into()));
    }
    match q.outcome {
        Some(UserOutcome::Win) => {
            clauses.push("((g.user_color = 'white' AND g.result = '1-0') OR (g.user_color = 'black' AND g.result = '0-1'))".into())
        }
        Some(UserOutcome::Loss) => {
            clauses.push("((g.user_color = 'white' AND g.result = '0-1') OR (g.user_color = 'black' AND g.result = '1-0'))".into())
        }
        Some(UserOutcome::Draw) => clauses.push("g.result = '1/2-1/2'".into()),
        None => {}
    }
    if let Some(s) = &q.source {
        clauses.push(format!("g.source = ?{}", args.len() + 1));
        args.push(Value::Text(s.clone()));
    }
    if let Some(s) = &q.speed {
        clauses.push(format!("g.speed = ?{}", args.len() + 1));
        args.push(Value::Text(s.clone()));
    }
    if let Some(e) = q.eco.as_deref().filter(|e| !e.is_empty()) {
        clauses.push(format!("g.eco LIKE ?{}", args.len() + 1));
        args.push(Value::Text(format!("{}%", e.to_uppercase())));
    }
    if let Some(t) = q.date_from.as_deref().and_then(|d| date_to_secs(d, false)) {
        clauses.push(format!("g.played_at >= ?{}", args.len() + 1));
        args.push(Value::Integer(t));
    }
    if let Some(t) = q.date_to.as_deref().and_then(|d| date_to_secs(d, true)) {
        clauses.push(format!("g.played_at <= ?{}", args.len() + 1));
        args.push(Value::Integer(t));
    }
    const AVG_RATING: &str = "((COALESCE(g.white_elo, g.black_elo) + COALESCE(g.black_elo, g.white_elo)) / 2.0)";
    if let Some(r) = q.min_rating {
        clauses.push(format!("{AVG_RATING} >= ?{}", args.len() + 1));
        args.push(Value::Integer(r as i64));
    }
    if let Some(r) = q.max_rating {
        clauses.push(format!("{AVG_RATING} <= ?{}", args.len() + 1));
        args.push(Value::Integer(r as i64));
    }
    let sql = if clauses.is_empty() { String::new() } else { format!("WHERE {}", clauses.join(" AND ")) };
    (sql, args)
}

fn order_by(q: &GameQuery) -> String {
    let col = match q.sort.unwrap_or(SortColumn::Date) {
        SortColumn::Date => "g.played_at",
        SortColumn::White => "g.white COLLATE NOCASE",
        SortColumn::Black => "g.black COLLATE NOCASE",
        SortColumn::WhiteElo => "g.white_elo",
        SortColumn::BlackElo => "g.black_elo",
        SortColumn::Result => "g.result",
        SortColumn::Eco => "g.eco",
        SortColumn::Moves => "g.ply_count",
    };
    let desc = if q.sort.is_none() || q.desc { "DESC" } else { "ASC" };
    format!("ORDER BY {col} {desc} NULLS LAST, g.id {desc}")
}

pub fn query(pool: &DbPool, q: &GameQuery) -> Result<GamePage> {
    let started = std::time::Instant::now();
    let conn = pool.get()?;
    let (mut where_sql, args) = filters(q);
    if let Some(pq) = &q.position {
        // Candidate games first (other filters), then the position test.
        let candidates_sql = format!("SELECT g.id, g.start_fen, g.moves FROM games g {where_sql}");
        let hits = position_hits(&conn, pq, &candidates_sql, &args)?;
        conn.execute_batch("CREATE TEMP TABLE IF NOT EXISTS position_hits (game_id INTEGER PRIMARY KEY); DELETE FROM position_hits;")?;
        {
            let mut stmt = conn.prepare_cached("INSERT INTO position_hits (game_id) VALUES (?1)")?;
            for id in hits {
                stmt.execute([id])?;
            }
        }
        where_sql = if where_sql.is_empty() {
            "WHERE g.id IN (SELECT game_id FROM position_hits)".into()
        } else {
            format!("{where_sql} AND g.id IN (SELECT game_id FROM position_hits)")
        };
    }
    let total: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM games g {where_sql}"), params_from_iter(args.iter()), |r| r.get(0))?;
    let limit = if q.limit == 0 { 200 } else { q.limit.min(2000) };
    let sql = format!(
        "SELECT {ROW_COLUMNS} FROM games g LEFT JOIN reviews r ON r.game_id = g.id {where_sql} {} LIMIT {limit} OFFSET {}",
        order_by(q),
        q.offset
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args.iter()), row_from)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(GamePage { total: total as u32, rows, elapsed_ms: started.elapsed().as_secs_f64() * 1000.0 })
}

pub fn get(pool: &DbPool, id: i32) -> Result<DbGame> {
    let conn = pool.get()?;
    let (row, pgn): (GameRow, String) = conn
        .query_row(
            &format!("SELECT {ROW_COLUMNS}, g.pgn FROM games g LEFT JOIN reviews r ON r.game_id = g.id WHERE g.id = ?1"),
            [id],
            |r| Ok((row_from(r)?, r.get(17)?)),
        )
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("game {id} not found")))?;
    let tree = parse_pgn(&pgn).games.into_iter().next().ok_or_else(|| Error::Chess("stored game is unreadable".into()))?;
    let review: Option<String> = conn.query_row("SELECT data FROM reviews WHERE game_id = ?1", [id], |r| r.get(0)).optional()?;
    let review = review.and_then(|d| serde_json::from_str(&d).ok());
    Ok(DbGame { row, tree, review })
}

pub fn find(pool: &DbPool, source: &str, source_id: &str) -> Result<Option<i32>> {
    let conn = pool.get()?;
    Ok(conn
        .query_row("SELECT id FROM games WHERE source = ?1 AND source_id = ?2", params![source, source_id], |r| r.get::<_, i64>(0))
        .optional()?
        .map(|id| id as i32))
}

pub fn delete(pool: &DbPool, ids: &[i32]) -> Result<u32> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    let mut n = 0;
    for id in ids {
        tx.execute("DELETE FROM positions WHERE game_id = ?1", [id])?;
        n += tx.execute("DELETE FROM games WHERE id = ?1", [id])? as u32;
    }
    tx.commit()?;
    Ok(n)
}

pub fn stats(pool: &DbPool) -> Result<DbStats> {
    let conn = pool.get()?;
    let count = |sql: &str| conn.query_row(sql, [], |r| r.get::<_, i64>(0)).map(|n| n as u32);
    let mut stmt = conn.prepare("SELECT source, COUNT(*) FROM games GROUP BY source ORDER BY COUNT(*) DESC")?;
    let sources =
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(DbStats {
        games: count("SELECT COUNT(*) FROM games")?,
        mine: count("SELECT COUNT(*) FROM games WHERE user_color IS NOT NULL")?,
        reviewed: count("SELECT COUNT(*) FROM reviews")?,
        sources,
    })
}

pub fn save_review(pool: &DbPool, game_id: i32, review: &ReviewResult) -> Result<()> {
    let conn = pool.get()?;
    let user: Option<String> = conn.query_row("SELECT user_color FROM games WHERE id = ?1", [game_id], |r| r.get(0)).optional()?.flatten();
    let user_side = parse_side(user);
    let summary = match user_side {
        Some(Side::White) => Some(&review.white),
        Some(Side::Black) => Some(&review.black),
        None => None,
    };
    let depth = serde_json::to_value(review.depth)?.as_str().unwrap_or("standard").to_string();
    conn.execute(
        "INSERT OR REPLACE INTO reviews (game_id, depth, white_accuracy, black_accuracy, user_accuracy, user_inaccuracies,
            user_mistakes, user_blunders, data, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            game_id,
            depth,
            review.white.accuracy,
            review.black.accuracy,
            summary.and_then(|s| s.accuracy),
            summary.map(|s| s.inaccuracies),
            summary.map(|s| s.mistakes),
            summary.map(|s| s.blunders),
            serde_json::to_string(review)?,
            now_secs(),
        ],
    )?;
    Ok(())
}

pub fn last_sync(conn: &Connection, source: &str, username: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row("SELECT last_sync FROM accounts WHERE source = ?1 AND username = ?2 COLLATE NOCASE", params![source, username], |r| {
            r.get(0)
        })
        .optional()?
        .flatten())
}

pub fn set_last_sync(conn: &Connection, source: &str, username: &str, at: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO accounts (source, username, last_sync) VALUES (?1, ?2, ?3)
         ON CONFLICT (source, username) DO UPDATE SET last_sync = MAX(COALESCE(last_sync, 0), excluded.last_sync)",
        params![source, username.to_lowercase(), at],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::import::{import_reader, UserIdentity};
    use crate::db::open_memory;

    const PGN: &str = include_str!("../../tests/fixtures/sample_games.pgn");

    fn db() -> DbPool {
        let pool = open_memory().unwrap();
        let identity = UserIdentity::new([Some("Hero".to_string())]);
        import_reader(&pool, PGN.as_bytes(), "pgn", &identity, |_| {}).unwrap();
        pool
    }

    #[test]
    fn filters_sorts_and_pages() {
        let pool = db();
        let all = query(&pool, &GameQuery::default()).unwrap();
        assert_eq!(all.total, 4);
        let mine = query(&pool, &GameQuery { only_mine: true, ..Default::default() }).unwrap();
        assert_eq!(mine.total, 3);
        let wins = query(&pool, &GameQuery { outcome: Some(UserOutcome::Win), ..Default::default() }).unwrap();
        assert_eq!(wins.total, 2);
        let as_black = query(&pool, &GameQuery { color: Some(Side::Black), ..Default::default() }).unwrap();
        assert_eq!(as_black.total, 1);
        let text = query(&pool, &GameQuery { text: Some("sicilian".into()), ..Default::default() }).unwrap();
        assert_eq!(text.total, 1);
        let eco = query(&pool, &GameQuery { eco: Some("c".into()), ..Default::default() }).unwrap();
        assert_eq!(eco.total, 2);
        let sorted = query(&pool, &GameQuery { sort: Some(SortColumn::WhiteElo), desc: true, limit: 2, ..Default::default() }).unwrap();
        assert_eq!(sorted.rows.len(), 2);
        assert!(sorted.rows[0].white_elo >= sorted.rows[1].white_elo);
        let range =
            query(&pool, &GameQuery { date_from: Some("2026-02-01".into()), date_to: Some("2026-03-31".into()), ..Default::default() })
                .unwrap();
        assert_eq!(range.total, 2);
        let masters = query(&pool, &GameQuery { min_rating: Some(2000), ..Default::default() }).unwrap();
        assert_eq!(masters.total, 1);
        let club = query(&pool, &GameQuery { min_rating: Some(1600), max_rating: Some(1700), ..Default::default() }).unwrap();
        assert_eq!(club.total, 3);
    }

    #[test]
    fn loads_deletes_and_counts() {
        let pool = db();
        let page = query(&pool, &GameQuery::default()).unwrap();
        let id = page.rows[0].id;
        let game = get(&pool, id).unwrap();
        assert!(game.tree.nodes.len() > 1);
        assert!(game.review.is_none());
        let s = stats(&pool).unwrap();
        assert_eq!((s.games, s.mine), (4, 3));
        assert_eq!(delete(&pool, &[id]).unwrap(), 1);
        assert_eq!(stats(&pool).unwrap().games, 3);
        assert!(get(&pool, id).is_err());
    }

    #[test]
    fn account_sync_only_moves_forward() {
        let pool = db();
        let conn = pool.get().unwrap();
        assert_eq!(last_sync(&conn, "lichess", "Hero").unwrap(), None);
        set_last_sync(&conn, "lichess", "Hero", 100).unwrap();
        set_last_sync(&conn, "lichess", "hero", 50).unwrap();
        assert_eq!(last_sync(&conn, "lichess", "HERO").unwrap(), Some(100));
    }
}
