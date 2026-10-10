//! The local game database (SQLite): games, a position index for search and
//! the opening tree, linked accounts, and stored reviews.

pub mod explorer;
pub mod games;
pub mod import;
pub mod search;

use std::path::Path;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

use crate::error::Result;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Migrations, applied in order. `PRAGMA user_version` records how many ran.
const MIGRATIONS: &[&str] = &[
    // 1: games, positions, accounts, reviews
    r#"
    CREATE TABLE games (
        id INTEGER PRIMARY KEY,
        source TEXT NOT NULL,
        source_id TEXT NOT NULL,
        event TEXT,
        site TEXT,
        date TEXT,
        round TEXT,
        white TEXT,
        black TEXT,
        white_elo INTEGER,
        black_elo INTEGER,
        result TEXT NOT NULL DEFAULT '*',
        eco TEXT,
        opening TEXT,
        time_control TEXT,
        speed TEXT,
        termination TEXT,
        user_color TEXT,
        opponent_level TEXT,
        ply_count INTEGER NOT NULL,
        start_fen TEXT,
        moves TEXT NOT NULL,
        pgn TEXT NOT NULL,
        played_at INTEGER,
        imported_at INTEGER NOT NULL,
        UNIQUE (source, source_id)
    );
    CREATE INDEX games_played_at ON games (played_at);
    CREATE INDEX games_white ON games (white COLLATE NOCASE);
    CREATE INDEX games_black ON games (black COLLATE NOCASE);
    CREATE INDEX games_eco ON games (eco);
    CREATE INDEX games_user_color ON games (user_color);

    CREATE TABLE positions (
        hash INTEGER NOT NULL,
        game_id INTEGER NOT NULL,
        ply INTEGER NOT NULL,
        next_move TEXT,
        PRIMARY KEY (hash, game_id, ply)
    ) WITHOUT ROWID;
    CREATE INDEX positions_game ON positions (game_id);

    CREATE TABLE accounts (
        source TEXT NOT NULL,
        username TEXT NOT NULL,
        last_sync INTEGER,
        PRIMARY KEY (source, username)
    );

    CREATE TABLE reviews (
        game_id INTEGER PRIMARY KEY REFERENCES games (id) ON DELETE CASCADE,
        depth TEXT NOT NULL,
        white_accuracy REAL,
        black_accuracy REAL,
        user_accuracy REAL,
        user_inaccuracies INTEGER,
        user_mistakes INTEGER,
        user_blunders INTEGER,
        data TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    "#,
    // 2: opening repertoires and their spaced-repetition cards
    r#"
    CREATE TABLE repertoire_moves (
        id INTEGER PRIMARY KEY,
        color TEXT NOT NULL,
        parent_hash INTEGER NOT NULL,
        parent_fen TEXT NOT NULL,
        uci TEXT NOT NULL,
        san TEXT NOT NULL,
        child_hash INTEGER NOT NULL,
        child_fen TEXT NOT NULL,
        comment TEXT,
        created_at INTEGER NOT NULL,
        UNIQUE (color, parent_hash, uci)
    );
    CREATE INDEX repertoire_moves_parent ON repertoire_moves (color, parent_hash);

    CREATE TABLE repertoire_cards (
        color TEXT NOT NULL,
        hash INTEGER NOT NULL,
        fen TEXT NOT NULL,
        due INTEGER NOT NULL,
        stability REAL NOT NULL DEFAULT 0,
        difficulty REAL NOT NULL DEFAULT 0,
        elapsed_days INTEGER NOT NULL DEFAULT 0,
        scheduled_days INTEGER NOT NULL DEFAULT 0,
        reps INTEGER NOT NULL DEFAULT 0,
        lapses INTEGER NOT NULL DEFAULT 0,
        state INTEGER NOT NULL DEFAULT 0,
        last_review INTEGER,
        PRIMARY KEY (color, hash)
    );

    CREATE TABLE repertoire_reviews (
        id INTEGER PRIMARY KEY,
        color TEXT NOT NULL,
        hash INTEGER NOT NULL,
        grade TEXT NOT NULL,
        reviewed_at INTEGER NOT NULL
    );
    "#,
    // 3: puzzle attempts and ratings
    r#"
    CREATE TABLE puzzle_attempts (
        id INTEGER PRIMARY KEY,
        puzzle_id TEXT NOT NULL,
        solved INTEGER NOT NULL,
        puzzle_rating INTEGER NOT NULL,
        themes TEXT NOT NULL,
        rating_before REAL NOT NULL,
        rating_after REAL NOT NULL,
        attempted_at INTEGER NOT NULL
    );
    CREATE INDEX puzzle_attempts_time ON puzzle_attempts (attempted_at);

    CREATE TABLE ratings (
        kind TEXT PRIMARY KEY,
        rating REAL NOT NULL,
        deviation REAL NOT NULL,
        volatility REAL NOT NULL,
        updated_at INTEGER NOT NULL
    );
    "#,
];

fn configure(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )
}

pub fn migrate(conn: &mut Connection) -> Result<()> {
    let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
        log::info!("database migrated to version {}", i + 1);
    }
    Ok(())
}

pub fn open(path: &Path) -> Result<DbPool> {
    let manager = SqliteConnectionManager::file(path).with_init(configure);
    let pool = Pool::builder().max_size(6).build(manager)?;
    let mut conn = pool.get()?;
    migrate(&mut conn)?;
    drop(conn);
    Ok(pool)
}

/// An in-memory database for tests.
pub fn open_memory() -> Result<DbPool> {
    let manager = SqliteConnectionManager::memory().with_init(configure);
    // A single connection: each in-memory connection is its own database.
    let pool = Pool::builder().max_size(1).build(manager)?;
    let mut conn = pool.get()?;
    migrate(&mut conn)?;
    drop(conn);
    Ok(pool)
}

pub fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_run_once() {
        let pool = open_memory().unwrap();
        let mut conn = pool.get().unwrap();
        let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
        migrate(&mut conn).unwrap();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }
}
