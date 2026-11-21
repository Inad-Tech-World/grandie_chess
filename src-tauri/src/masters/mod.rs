//! Famous players and their games. The library has three sources:
//! - a bundled "Legends" pack of famous games (resources/masters/legends.pgn);
//! - per-player files from PGN Mentor, downloaded only when the user picks a
//!   player (the catalogue in resources/masters/players.json lists who is
//!   available);
//! - PGN files the user imports as master games (e.g. exported from their
//!   own ChessBase).
//!
//! ChessBase's online database has no public API and its terms don't allow
//! automated downloads, so it isn't used as a source.

pub mod profile;
pub mod study;

use std::io::Read;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;

use crate::ai::agent::ToolFuture;
use crate::ai::openrouter::Tool;
use crate::coach::chat::ExtraTools;
use crate::db::games::{self as dbgames, GameQuery, GameRow, UserOutcome};
use crate::db::import::{self, ImportSummary, Subject, UserIdentity};
use crate::db::players::player_key;
use crate::db::DbPool;
use crate::error::{Error, Result};
use crate::game::position::Side;
use crate::state::AppState;

/// The id the bundled legends are filed under.
pub const LEGENDS: &str = "legends";
const PGNMENTOR_BASE: &str = "https://www.pgnmentor.com/players/";

#[derive(Debug, Clone, Deserialize)]
struct CatalogFile {
    players: Vec<CatalogEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogEntry {
    file: String,
    name: String,
    games: u32,
}

/// A master in the catalogue, with what is in the local library.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MasterPlayer {
    /// Catalogue id, e.g. "Carlsen".
    pub id: String,
    pub name: String,
    /// Games in the PGN Mentor file.
    pub games_available: u32,
    /// Games of this player in the local library.
    pub games_in_library: u32,
}

fn catalog() -> Vec<CatalogEntry> {
    serde_json::from_str::<CatalogFile>(include_str!("../../resources/masters/players.json")).map(|c| c.players).unwrap_or_default()
}

/// The catalogue entry for an id ("Carlsen"), if any.
pub fn catalog_player(id: &str) -> Option<(String, String)> {
    catalog().into_iter().find(|p| p.file.eq_ignore_ascii_case(id)).map(|p| (p.file, p.name))
}

/// Everyone in the catalogue, with local counts.
pub fn players(pool: &DbPool) -> Result<Vec<MasterPlayer>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare("SELECT subject, COUNT(*) FROM game_subjects GROUP BY subject")?;
    let counts: std::collections::HashMap<String, u32> =
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32)))?.collect::<rusqlite::Result<_>>()?;
    let catalog = catalog();
    let mut list: Vec<MasterPlayer> = catalog
        .iter()
        .map(|p| MasterPlayer {
            games_in_library: counts.get(&p.file).copied().unwrap_or(0),
            id: p.file.clone(),
            name: p.name.clone(),
            games_available: p.games,
        })
        .collect();
    // Players the user imported under names that aren't in the catalogue.
    for (subject, n) in &counts {
        if subject != LEGENDS && !catalog.iter().any(|p| &p.file == subject) {
            list.push(MasterPlayer { id: subject.clone(), name: subject.clone(), games_available: *n, games_in_library: *n });
        }
    }
    Ok(list)
}

/// Import the bundled legends once.
pub fn ensure_legends(pool: &DbPool) -> Result<()> {
    let have: i64 = pool.get()?.query_row("SELECT COUNT(*) FROM game_subjects WHERE subject = ?1", [LEGENDS], |r| r.get(0))?;
    if have > 0 {
        return Ok(());
    }
    let subject = Subject { id: LEGENDS.into(), key: String::new() };
    import::import_reader_with(
        pool,
        include_str!("../../resources/masters/legends.pgn").as_bytes(),
        "legends",
        &UserIdentity::default(),
        Some(&subject),
        |_| {},
    )?;
    Ok(())
}

/// A bundled legend game, for the Legends list.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LegendGame {
    pub game: GameRow,
    pub title: String,
    pub why: String,
}

pub fn legends(pool: &DbPool) -> Result<Vec<LegendGame>> {
    ensure_legends(pool)?;
    let page = dbgames::query(
        pool,
        &GameQuery { subject: Some(LEGENDS.into()), limit: 200, sort: Some(dbgames::SortColumn::Date), ..Default::default() },
    )?;
    let conn = pool.get()?;
    let mut out = Vec::new();
    for row in page.rows {
        let pgn: String = conn.query_row("SELECT pgn FROM games WHERE id = ?1", [row.id], |r| r.get(0))?;
        let tag = |name: &str| {
            pgn.lines().find_map(|l| l.strip_prefix(&format!("[{name} \"")).and_then(|r| r.strip_suffix("\"]")).map(str::to_string))
        };
        out.push(LegendGame {
            title: tag("Legend")
                .unwrap_or_else(|| format!("{} vs {}", row.white.clone().unwrap_or_default(), row.black.clone().unwrap_or_default())),
            why: tag("Famous").unwrap_or_default(),
            game: row,
        });
    }
    out.sort_by_key(|g| g.game.date.clone());
    Ok(out)
}

/// The PGN Mentor download URL for a catalogue id.
pub fn pgnmentor_url(id: &str) -> Result<String> {
    let (file, _) = catalog_player(id).ok_or_else(|| Error::NotFound(format!("{id} isn't in the masters catalogue")))?;
    Ok(format!("{PGNMENTOR_BASE}{file}.zip"))
}

/// Import a downloaded PGN Mentor zip for player `id`.
pub fn import_zip(pool: &DbPool, id: &str, zip_path: &std::path::Path, progress: impl FnMut(&ImportSummary)) -> Result<ImportSummary> {
    let (file, name) = catalog_player(id).ok_or_else(|| Error::NotFound(format!("{id} isn't in the masters catalogue")))?;
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(zip_path)?).map_err(|e| Error::other(format!("the download isn't a zip file: {e}")))?;
    let mut pgn = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| Error::other(e.to_string()))?;
        let is_pgn = entry.name().map(|n| n.to_lowercase().ends_with(".pgn")).unwrap_or(false);
        if is_pgn {
            entry.read_to_end(&mut pgn)?;
        }
    }
    if pgn.is_empty() {
        return Err(Error::other("the download has no PGN file in it"));
    }
    let subject = Subject { id: file, key: player_key(&name) };
    import::import_reader_with(pool, pgn.as_slice(), "pgnmentor", &UserIdentity::default(), Some(&subject), progress)
}

/// Import a PGN file of master games (e.g. exported from ChessBase), filed
/// under the player `id` when it is in the catalogue, else under its name.
pub fn import_pgn_as_master(pool: &DbPool, reader: impl Read, player: &str, progress: impl FnMut(&ImportSummary)) -> Result<ImportSummary> {
    let (id, name) = catalog_player(player).unwrap_or_else(|| (player.trim().to_string(), player.trim().to_string()));
    let subject = Subject { id, key: player_key(&name) };
    import::import_reader_with(pool, reader, "masters", &UserIdentity::default(), Some(&subject), progress)
}

/// Remove a player's downloaded games (keeps games shared with other players).
pub fn remove_player(pool: &DbPool, id: &str) -> Result<u32> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM game_subjects WHERE subject = ?1", [id])?;
    let removed = tx.execute(
        "DELETE FROM games WHERE collection = 'masters' AND source != 'legends' AND id NOT IN (SELECT game_id FROM game_subjects)",
        [],
    )?;
    tx.execute("DELETE FROM positions WHERE game_id NOT IN (SELECT id FROM games)", [])?;
    tx.execute("DELETE FROM player_profiles WHERE subject = ?1", [id])?;
    tx.commit()?;
    Ok(removed as u32)
}

/// Tools that let the coach chat look into the masters library.
pub fn chat_tools(state: &AppState) -> Option<Box<dyn ExtraTools + '_>> {
    Some(Box::new(MasterTools { pool: state.db.clone() }))
}

struct MasterTools {
    pool: DbPool,
}

fn resolve_subject(pool: &DbPool, player: &str) -> Result<String> {
    if let Some((id, _)) = catalog_player(player) {
        return Ok(id);
    }
    // Match by surname against the catalogue ("Tal", "Mikhail Tal").
    let key = player_key(player);
    let surname = crate::db::players::key_surname(&key).to_string();
    if let Some(p) =
        catalog().into_iter().find(|p| crate::db::players::key_surname(&player_key(&p.name)) == surname || player_key(&p.name) == key)
    {
        return Ok(p.file);
    }
    // A player imported by name.
    let known: Option<String> =
        pool.get()?.query_row("SELECT subject FROM game_subjects WHERE lower(subject) = lower(?1) LIMIT 1", [player], |r| r.get(0)).ok();
    known.ok_or_else(|| Error::NotFound(format!("{player} isn't in the library; download their games in Masters first")))
}

impl ExtraTools for MasterTools {
    fn tools(&self) -> Vec<Tool> {
        vec![
            Tool::function(
                "player_profile",
                "Statistics about a famous player's games in the library: results, favourite openings with each colour, typical game length, best opponents.",
                json!({ "type": "object", "properties": { "player": { "type": "string", "description": "e.g. \"Tal\" or \"Magnus Carlsen\"" } }, "required": ["player"] }),
            ),
            Tool::function(
                "find_master_games",
                "Find a famous player's games in the library, optionally by opening (name or ECO), colour and result. Returns game ids, players, year, event and opening.",
                json!({
                    "type": "object",
                    "properties": {
                        "player": { "type": "string" },
                        "opening": { "type": "string", "description": "opening name words or an ECO code like B90" },
                        "color": { "type": "string", "enum": ["white", "black"] },
                        "result": { "type": "string", "enum": ["win", "loss", "draw"] },
                        "limit": { "type": "integer" }
                    },
                    "required": ["player"]
                }),
            ),
        ]
    }

    fn call<'a>(&'a self, name: &'a str, args: Value) -> Option<ToolFuture<'a>> {
        let pool = self.pool.clone();
        match name {
            "player_profile" => Some(Box::pin(async move {
                let player = args.get("player").and_then(Value::as_str).unwrap_or_default().to_string();
                crate::commands::blocking(move || {
                    let id = resolve_subject(&pool, &player)?;
                    let p = profile::stats(&pool, &id)?;
                    Ok(serde_json::to_value(p)?)
                })
                .await
            })),
            "find_master_games" => Some(Box::pin(async move {
                crate::commands::blocking(move || {
                    let player = args.get("player").and_then(Value::as_str).unwrap_or_default();
                    let id = resolve_subject(&pool, player)?;
                    let color = match args.get("color").and_then(Value::as_str) {
                        Some("white") => Some(Side::White),
                        Some("black") => Some(Side::Black),
                        _ => None,
                    };
                    let outcome = match args.get("result").and_then(Value::as_str) {
                        Some("win") => Some(UserOutcome::Win),
                        Some("loss") => Some(UserOutcome::Loss),
                        Some("draw") => Some(UserOutcome::Draw),
                        _ => None,
                    };
                    let opening = args.get("opening").and_then(Value::as_str).map(str::to_string);
                    let (eco, text) = match opening {
                        Some(o)
                            if o.len() <= 3
                                && o.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                                && o.chars().skip(1).all(|c| c.is_ascii_digit()) =>
                        {
                            (Some(o), None)
                        }
                        other => (None, other),
                    };
                    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(10).clamp(1, 25) as u32;
                    let page = dbgames::query(
                        &pool,
                        &GameQuery {
                            subject: Some(id),
                            subject_color: color,
                            subject_outcome: outcome,
                            eco,
                            text,
                            limit,
                            ..Default::default()
                        },
                    )?;
                    Ok(json!({
                        "total": page.total,
                        "games": page.rows.iter().map(|g| json!({
                            "id": g.id, "white": g.white, "black": g.black, "result": g.result, "date": g.date,
                            "opening": g.opening, "eco": g.eco, "moves": g.ply_count / 2,
                        })).collect::<Vec<_>>(),
                    }))
                })
                .await
            })),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legends_import_once_and_are_all_legal() {
        let pool = crate::db::open_memory().unwrap();
        ensure_legends(&pool).unwrap();
        ensure_legends(&pool).unwrap();
        let list = legends(&pool).unwrap();
        assert_eq!(list.len(), 20, "every bundled legend parses and imports");
        assert!(list.iter().any(|l| l.title == "The Immortal Game" && l.game.result == "1-0"));
        assert!(list.iter().all(|l| l.game.ply_count > 20 && !l.why.is_empty()));
        // Master games stay out of the user's database.
        let mine = dbgames::query(&pool, &GameQuery { limit: 10, ..Default::default() }).unwrap();
        assert_eq!(mine.total, 0);
        assert_eq!(dbgames::stats(&pool).unwrap().masters, 20);
    }

    #[test]
    fn imports_a_player_file_and_finds_their_colour() {
        let pool = crate::db::open_memory().unwrap();
        let pgn = r#"[Event "Test"]
[Date "1960.??.??"]
[White "Tal, Mihail"]
[Black "Smyslov, Vassily"]
[Result "1-0"]

1. e4 c6 2. d4 d5 1-0

[Event "Test"]
[Date "1961.??.??"]
[White "Botvinnik, Mikhail"]
[Black "Tal, Mikhail"]
[Result "0-1"]

1. c4 Nf6 0-1
"#;
        let s = import_pgn_as_master(&pool, pgn.as_bytes(), "Tal", |_| {}).unwrap();
        assert_eq!(s.imported, 2);
        let black = dbgames::query(
            &pool,
            &GameQuery { subject: Some("Tal".into()), subject_color: Some(Side::Black), limit: 10, ..Default::default() },
        )
        .unwrap();
        assert_eq!(black.total, 1);
        let wins = dbgames::query(
            &pool,
            &GameQuery { subject: Some("Tal".into()), subject_outcome: Some(UserOutcome::Win), limit: 10, ..Default::default() },
        )
        .unwrap();
        assert_eq!(wins.total, 2, "won with White and with Black");
        assert_eq!(players(&pool).unwrap().into_iter().find(|p| p.id == "Tal").unwrap().games_in_library, 2);
        // A player who isn't in the catalogue still shows up once imported.
        import_pgn_as_master(&pool, pgn.as_bytes(), "Club Hero", |_| {}).unwrap();
        assert!(players(&pool).unwrap().iter().any(|p| p.id == "Club Hero" && p.games_in_library > 0));
        remove_player(&pool, "Club Hero").unwrap();
        assert_eq!(remove_player(&pool, "Tal").unwrap(), 2);
        assert_eq!(resolve_subject(&pool, "Mikhail Tal").unwrap(), "Tal");
    }
}
