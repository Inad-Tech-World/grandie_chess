//! Tactics puzzles from the Lichess puzzle database (CC0,
//! https://database.lichess.org/#puzzles), stored in a separate `puzzles.db`.
//! The user's attempts and puzzle rating live in the main database.

use std::io::Read;
use std::path::Path;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, params_from_iter, types::Value, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use shakmaty::Position;
use specta::Type;

use super::glicko::{self, Outcome, Rating};
use crate::db::DbPool;
use crate::error::{Error, Result};
use crate::game::position::{parse_fen, parse_uci, uci_of};

pub const PUZZLE_DB_URL: &str = "https://database.lichess.org/lichess_db_puzzle.csv.zst";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Puzzle {
    pub id: String,
    /// Position before the opponent's first move.
    pub fen: String,
    /// Solution in UCI; the first move is the opponent's, then the user's, alternating.
    pub moves: Vec<String>,
    pub rating: i32,
    pub deviation: i32,
    pub popularity: i32,
    pub plays: i32,
    pub themes: Vec<String>,
    pub game_url: Option<String>,
    pub opening: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PuzzleImportFilter {
    /// Lichess popularity, -100..100.
    pub min_popularity: i32,
    pub min_plays: i32,
    pub max_deviation: i32,
}

impl Default for PuzzleImportFilter {
    /// Well-tested, well-liked puzzles: roughly a third of the database.
    fn default() -> Self {
        PuzzleImportFilter { min_popularity: 85, min_plays: 300, max_deviation: 90 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ThemeCount {
    pub theme: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveVerdict {
    pub correct: bool,
    /// The opponent's answer to play next, if the puzzle continues.
    pub reply: Option<String>,
    /// The puzzle is solved.
    pub done: bool,
}

pub fn open(path: &Path) -> Result<DbPool> {
    let manager = SqliteConnectionManager::file(path)
        .with_init(|c| c.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA busy_timeout = 5000;"));
    let pool = Pool::builder().max_size(4).build(manager)?;
    pool.get()?.execute_batch(
        "CREATE TABLE IF NOT EXISTS puzzles (
            id TEXT PRIMARY KEY,
            fen TEXT NOT NULL,
            moves TEXT NOT NULL,
            rating INTEGER NOT NULL,
            deviation INTEGER NOT NULL,
            popularity INTEGER NOT NULL,
            plays INTEGER NOT NULL,
            themes TEXT NOT NULL,
            game_url TEXT,
            opening TEXT
        );
        CREATE INDEX IF NOT EXISTS puzzles_rating ON puzzles (rating);
        CREATE TABLE IF NOT EXISTS themes (theme TEXT PRIMARY KEY, count INTEGER NOT NULL);",
    )?;
    Ok(pool)
}

#[derive(Debug, Deserialize)]
struct CsvRow {
    #[serde(rename = "PuzzleId")]
    id: String,
    #[serde(rename = "FEN")]
    fen: String,
    #[serde(rename = "Moves")]
    moves: String,
    #[serde(rename = "Rating")]
    rating: i32,
    #[serde(rename = "RatingDeviation")]
    deviation: i32,
    #[serde(rename = "Popularity")]
    popularity: i32,
    #[serde(rename = "NbPlays")]
    plays: i32,
    #[serde(rename = "Themes")]
    themes: String,
    #[serde(rename = "GameUrl")]
    game_url: Option<String>,
    #[serde(rename = "OpeningTags")]
    opening: Option<String>,
}

/// Import puzzles from the (decompressed) CSV, replacing what was there.
/// `progress(read, kept)` is called every 20,000 rows. Returns the number kept.
pub fn import_csv<R: Read>(
    pool: &DbPool,
    reader: R,
    filter: Option<PuzzleImportFilter>,
    mut progress: impl FnMut(u64, u64),
) -> Result<u64> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    tx.execute_batch("DELETE FROM puzzles; DELETE FROM themes;")?;
    let mut csv = csv::ReaderBuilder::new().flexible(true).from_reader(reader);
    let (mut read, mut kept) = (0u64, 0u64);
    let mut themes: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    {
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO puzzles (id, fen, moves, rating, deviation, popularity, plays, themes, game_url, opening)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for row in csv.deserialize::<CsvRow>() {
            let Ok(row) = row else { continue };
            read += 1;
            if read % 20_000 == 0 {
                progress(read, kept);
            }
            if let Some(f) = filter {
                if row.popularity < f.min_popularity || row.plays < f.min_plays || row.deviation > f.max_deviation {
                    continue;
                }
            }
            for t in row.themes.split_whitespace() {
                *themes.entry(t.to_string()).or_default() += 1;
            }
            stmt.execute(params![
                row.id,
                row.fen,
                row.moves,
                row.rating,
                row.deviation,
                row.popularity,
                row.plays,
                // Padded with spaces so `LIKE '% theme %'` matches whole words.
                format!(" {} ", row.themes.trim()),
                row.game_url.filter(|u| !u.is_empty()),
                row.opening.filter(|o| !o.is_empty()),
            ])?;
            kept += 1;
        }
        let mut stmt = tx.prepare("INSERT INTO themes (theme, count) VALUES (?1, ?2)")?;
        for (theme, count) in &themes {
            stmt.execute(params![theme, count])?;
        }
    }
    tx.commit()?;
    progress(read, kept);
    Ok(kept)
}

pub fn count(pool: &DbPool) -> Result<u32> {
    Ok(pool.get()?.query_row("SELECT COUNT(*) FROM puzzles", [], |r| r.get::<_, i64>(0))? as u32)
}

pub fn themes(pool: &DbPool) -> Result<Vec<ThemeCount>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT theme, count FROM themes ORDER BY count DESC")?;
    let rows = stmt.query_map([], |r| Ok(ThemeCount { theme: r.get(0)?, count: r.get::<_, i64>(1)? as u32 }))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn puzzle_from(r: &rusqlite::Row) -> rusqlite::Result<Puzzle> {
    Ok(Puzzle {
        id: r.get(0)?,
        fen: r.get(1)?,
        moves: r.get::<_, String>(2)?.split_whitespace().map(str::to_string).collect(),
        rating: r.get(3)?,
        deviation: r.get(4)?,
        popularity: r.get(5)?,
        plays: r.get(6)?,
        themes: r.get::<_, String>(7)?.split_whitespace().map(str::to_string).collect(),
        game_url: r.get(8)?,
        opening: r.get(9)?,
    })
}

/// A random puzzle near `rating`, optionally with a theme, avoiding `exclude`.
/// The rating window widens until something is found.
pub fn next(pool: &DbPool, rating: f64, theme: Option<&str>, exclude: &[String]) -> Result<Option<Puzzle>> {
    let conn = pool.get()?;
    for window in [75.0, 150.0, 300.0, 600.0, 3000.0] {
        let mut sql = "SELECT id, fen, moves, rating, deviation, popularity, plays, themes, game_url, opening
                       FROM puzzles WHERE rating BETWEEN ?1 AND ?2"
            .to_string();
        let mut args: Vec<Value> = vec![Value::Integer((rating - window) as i64), Value::Integer((rating + window) as i64)];
        if let Some(t) = theme.filter(|t| !t.is_empty()) {
            sql.push_str(&format!(" AND themes LIKE ?{}", args.len() + 1));
            args.push(Value::Text(format!("% {t} %")));
        }
        if !exclude.is_empty() {
            let marks: Vec<String> = (0..exclude.len()).map(|i| format!("?{}", args.len() + 1 + i)).collect();
            sql.push_str(&format!(" AND id NOT IN ({})", marks.join(",")));
            args.extend(exclude.iter().map(|e| Value::Text(e.clone())));
        }
        sql.push_str(" ORDER BY random() LIMIT 1");
        if let Some(p) = conn.query_row(&sql, params_from_iter(args.iter()), puzzle_from).optional()? {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

pub fn get(pool: &DbPool, id: &str) -> Result<Puzzle> {
    pool.get()?
        .query_row(
            "SELECT id, fen, moves, rating, deviation, popularity, plays, themes, game_url, opening FROM puzzles WHERE id = ?1",
            [id],
            puzzle_from,
        )
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("puzzle {id} not found")))
}

/// Check the user's move at `index` in the solution (an odd index: the
/// opponent plays index 0). On the final move any checkmate is accepted.
pub fn check_move(p: &Puzzle, index: usize, uci: &str) -> Result<MoveVerdict> {
    if index == 0 || index >= p.moves.len() || index.is_multiple_of(2) {
        return Err(Error::Chess("not the solver's turn".into()));
    }
    let mut pos = parse_fen(&p.fen)?;
    for m in &p.moves[..index] {
        let mv = parse_uci(&pos, m)?;
        pos.play_unchecked(mv);
    }
    let played = parse_uci(&pos, uci)?;
    let played_uci = uci_of(played);
    let last = index == p.moves.len() - 1;
    let mates = {
        let mut after = pos.clone();
        after.play_unchecked(played);
        after.is_checkmate()
    };
    let correct = played_uci == p.moves[index] || (last && mates);
    Ok(MoveVerdict { correct, reply: if correct && !last { p.moves.get(index + 1).cloned() } else { None }, done: correct && last })
}

// ---- the user's puzzle history (main database) ----

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PuzzleResult {
    pub before: Rating,
    pub after: Rating,
}

pub fn rating(conn: &Connection) -> Result<Rating> {
    Ok(conn
        .query_row("SELECT rating, deviation, volatility FROM ratings WHERE kind = 'puzzle'", [], |r| {
            Ok(Rating { rating: r.get(0)?, deviation: r.get(1)?, volatility: r.get(2)? })
        })
        .optional()?
        .unwrap_or_default())
}

pub fn record_attempt(conn: &Connection, p: &Puzzle, solved: bool, now: i64) -> Result<PuzzleResult> {
    let before = rating(conn)?;
    let after = glicko::update(
        before,
        &[Outcome { opponent: p.rating as f64, opponent_deviation: p.deviation as f64, score: if solved { 1.0 } else { 0.0 } }],
    );
    conn.execute(
        "INSERT INTO ratings (kind, rating, deviation, volatility, updated_at) VALUES ('puzzle', ?1, ?2, ?3, ?4)
         ON CONFLICT (kind) DO UPDATE SET rating = excluded.rating, deviation = excluded.deviation,
            volatility = excluded.volatility, updated_at = excluded.updated_at",
        params![after.rating, after.deviation, after.volatility, now],
    )?;
    conn.execute(
        "INSERT INTO puzzle_attempts (puzzle_id, solved, puzzle_rating, themes, rating_before, rating_after, attempted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![p.id, solved, p.rating, p.themes.join(" "), before.rating, after.rating, now],
    )?;
    Ok(PuzzleResult { before, after })
}

/// Puzzles attempted recently, so they aren't served again yet.
pub fn recent_ids(conn: &Connection, limit: u32) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT puzzle_id FROM puzzle_attempts ORDER BY attempted_at DESC LIMIT ?1")?;
    let ids = stmt.query_map([limit], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;

    const CSV: &str = "PuzzleId,FEN,Moves,Rating,RatingDeviation,Popularity,NbPlays,Themes,GameUrl,OpeningTags,DailyDate
00008,r6k/pp2r2p/4Rp1Q/3p4/8/1N1P2R1/PqP2bPP/7K b - - 0 24,f2g3 e6e7 b2b1 b3c1 b1c1 h6c1,1811,76,95,10282,crushing hangingPiece long middlegame,https://lichess.org/787zsVup/black#48,,
0000D,5rk1/1p3ppp/pq3b2/8/8/1P1Q1N2/P4PPP/3R2K1 w - - 2 27,d3d6 f8d8 d6d8 f6d8,1529,74,96,37506,advantage endgame short,https://lichess.org/F8M8OS71#53,,
0009B,r2qr1k1/b1p2ppp/pp4n1/P1P1p3/4P1n1/B2P2Pb/3NBP1P/RN1QR1K1 b - - 1 16,b6c5 e2g4 h3g4 d1g4,1112,81,85,571,advantage middlegame short,https://lichess.org/4MWQCxQ6/black#32,Kings_Pawn_Game,
LOWPOP,8/8/8/8/8/8/8/K6k w - - 0 1,a1a2 h1h2,900,80,10,5,endgame,,,
";

    fn pool_with_puzzles() -> DbPool {
        let dir = tempfile::tempdir().unwrap();
        let pool = open(&dir.path().join("p.db")).unwrap();
        std::mem::forget(dir); // keep the file for the test's lifetime
        let kept = import_csv(&pool, CSV.as_bytes(), Some(PuzzleImportFilter::default()), |_, _| {}).unwrap();
        assert_eq!(kept, 3, "the unpopular puzzle is filtered out");
        pool
    }

    #[test]
    fn imports_filters_and_serves_near_rating() {
        let pool = pool_with_puzzles();
        assert_eq!(count(&pool).unwrap(), 3);
        let themes = themes(&pool).unwrap();
        assert!(themes.iter().any(|t| t.theme == "middlegame" && t.count == 2));
        let p = next(&pool, 1500.0, None, &[]).unwrap().unwrap();
        assert_eq!(p.id, "0000D", "closest to 1500");
        let endgame = next(&pool, 1100.0, Some("endgame"), &[]).unwrap().unwrap();
        assert_eq!(endgame.id, "0000D", "theme filter widens the rating window");
        let none = next(&pool, 1500.0, Some("endgame"), &["0000D".into()]).unwrap();
        assert!(none.is_none());
    }

    #[test]
    fn imports_from_a_zstd_stream_like_the_download() {
        let dir = tempfile::tempdir().unwrap();
        let pool = open(&dir.path().join("p.db")).unwrap();
        let compressed = zstd::encode_all(CSV.as_bytes(), 3).unwrap();
        let decoder = zstd::Decoder::new(compressed.as_slice()).unwrap();
        let mut calls = 0;
        let kept = import_csv(&pool, decoder, None, |_, _| calls += 1).unwrap();
        assert_eq!(kept, 4, "no filter keeps everything");
        assert!(calls >= 1);
        // Importing again replaces rather than duplicates.
        let again = import_csv(&pool, CSV.as_bytes(), None, |_, _| {}).unwrap();
        assert_eq!((again, count(&pool).unwrap()), (4, 4));
    }

    #[test]
    fn checks_solution_moves() {
        let pool = pool_with_puzzles();
        let p = get(&pool, "00008").unwrap();
        let first = check_move(&p, 1, "e6e7").unwrap();
        assert!(first.correct && !first.done);
        assert_eq!(first.reply.as_deref(), Some("b2b1"));
        assert!(!check_move(&p, 1, "h6h7").unwrap().correct);
        let last = check_move(&p, 5, "h6c1").unwrap();
        assert!(last.correct && last.done);
        assert!(check_move(&p, 2, "b2b1").is_err(), "even indices are the opponent's");
    }

    #[test]
    fn records_attempts_with_glicko() {
        let pool = pool_with_puzzles();
        let main = open_memory().unwrap();
        let conn = main.get().unwrap();
        let p = get(&pool, "0000D").unwrap();
        let r = record_attempt(&conn, &p, true, 100).unwrap();
        assert_eq!(r.before.rating, 1500.0);
        assert!(r.after.rating > 1500.0);
        let r2 = record_attempt(&conn, &p, false, 200).unwrap();
        assert!(r2.after.rating < r.after.rating);
        assert_eq!(recent_ids(&conn, 10).unwrap(), vec!["0000D", "0000D"]);
        assert!((rating(&conn).unwrap().rating - r2.after.rating).abs() < 1e-9);
    }
}
