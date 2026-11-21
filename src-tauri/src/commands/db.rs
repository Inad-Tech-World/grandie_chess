//! The game database, imports, openings and the explorers.

use tauri::{AppHandle, State};
use tauri_specta::Event;

use super::{blocking, blocking_state};
use crate::analysis::review::ReviewResult;
use crate::db::explorer::{ExplorerFilter, ExplorerResult};
use crate::db::games::{self as dbgames, DbGame, DbStats, GamePage, GameQuery};
use crate::db::import::{self as dbimport, ImportSummary};
use crate::error::{Error, Result};
use crate::events::ImportProgress;
use crate::game::openings::{OpeningBook, OpeningName};
use crate::game::pgn::read_pgn_file;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn db_query(state: State<'_, AppState>, query: GameQuery) -> Result<GamePage> {
    let pool = state.db.clone();
    blocking(move || dbgames::query(&pool, &query)).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_game(state: State<'_, AppState>, id: i32) -> Result<DbGame> {
    let pool = state.db.clone();
    blocking(move || dbgames::get(&pool, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_delete(state: State<'_, AppState>, ids: Vec<i32>) -> Result<u32> {
    let pool = state.db.clone();
    blocking(move || dbgames::delete(&pool, &ids)).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_stats(state: State<'_, AppState>) -> Result<DbStats> {
    let pool = state.db.clone();
    blocking(move || dbgames::stats(&pool)).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_find(state: State<'_, AppState>, source: String, source_id: String) -> Result<Option<i32>> {
    let pool = state.db.clone();
    blocking(move || dbgames::find(&pool, &source, &source_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn db_save_review(state: State<'_, AppState>, game_id: i32, review: ReviewResult) -> Result<()> {
    let pool = state.db.clone();
    blocking(move || dbgames::save_review(&pool, game_id, &review)).await
}

fn emit_import(app: &AppHandle, job_id: &str, phase: &str, s: &ImportSummary, bytes: u64) {
    let _ =
        ImportProgress { job_id: job_id.into(), phase: phase.into(), seen: s.seen, imported: s.imported, bytes: bytes as f64 }.emit(app);
}

/// Import PGN files into the database.
#[tauri::command]
#[specta::specta]
pub async fn db_import_files(app: AppHandle, state: State<'_, AppState>, job_id: String, paths: Vec<String>) -> Result<ImportSummary> {
    let pool = state.db.clone();
    let identity = state.identity();
    blocking(move || {
        let mut total = ImportSummary::default();
        for path in paths {
            let file = std::fs::File::open(&path)?;
            let s = dbimport::import_reader(&pool, std::io::BufReader::new(file), "pgn", &identity, |s| {
                emit_import(&app, &job_id, "Importing", s, 0)
            })?;
            total.merge(s);
        }
        Ok(total)
    })
    .await
}

async fn import_bytes(app: &AppHandle, state: &AppState, job_id: &str, bytes: Vec<u8>, source: &str) -> Result<ImportSummary> {
    let pool = state.db.clone();
    let identity = state.identity();
    let (app, job_id, source) = (app.clone(), job_id.to_string(), source.to_string());
    blocking(move || {
        dbimport::import_reader(&pool, bytes.as_slice(), &source, &identity, |s| emit_import(&app, &job_id, "Importing", s, 0))
    })
    .await
}

/// Import (new) games of a Lichess user.
#[tauri::command]
#[specta::specta]
pub async fn db_import_lichess(app: AppHandle, state: State<'_, AppState>, job_id: String, username: String) -> Result<ImportSummary> {
    let username = username.trim().to_string();
    let since = dbgames::last_sync(&*state.db.get()?, "lichess", &username)?;
    let bytes = crate::net::lichess::fetch_user_games(&state.http, &username, since.map(|s| (s + 1) * 1000), None, |games, bytes| {
        emit_import(&app, &job_id, "Downloading from Lichess", &ImportSummary { seen: games, ..Default::default() }, bytes)
    })
    .await?;
    let summary = import_bytes(&app, &state, &job_id, bytes, "lichess").await?;
    if let Some(newest) = summary.newest {
        dbgames::set_last_sync(&*state.db.get()?, "lichess", &username, newest as i64)?;
    }
    Ok(summary)
}

/// Import (new) games of a Chess.com user, month by month.
#[tauri::command]
#[specta::specta]
pub async fn db_import_chesscom(app: AppHandle, state: State<'_, AppState>, job_id: String, username: String) -> Result<ImportSummary> {
    let username = username.trim().to_string();
    let since = dbgames::last_sync(&*state.db.get()?, "chesscom", &username)?;
    let months = crate::net::chesscom::archives_since(crate::net::chesscom::archives(&state.http, &username).await?, since);
    let mut total = ImportSummary::default();
    for (i, url) in months.iter().enumerate() {
        emit_import(&app, &job_id, &format!("Downloading month {} of {} from Chess.com", i + 1, months.len()), &total, 0);
        let pgn = crate::net::chesscom::month_pgn(&state.http, url).await?;
        let s = import_bytes(&app, &state, &job_id, pgn.into_bytes(), "chesscom").await?;
        total.merge(s);
    }
    if let Some(newest) = total.newest {
        dbgames::set_last_sync(&*state.db.get()?, "chesscom", &username, newest as i64)?;
    }
    Ok(total)
}

/// Add games played in Grandie (saved as PGN files) to the database. A file
/// that can't be read is reported and skipped; the rest are still imported.
#[tauri::command]
#[specta::specta]
pub async fn db_import_grandie(app: AppHandle) -> Result<ImportSummary> {
    blocking_state(&app, |state| {
        let identity = state.identity();
        let mut summary = ImportSummary::default();
        let mut conn = state.db.get()?;
        let tx = conn.transaction()?;
        for entry in std::fs::read_dir(&state.paths.pgn_dir)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("pgn") {
                continue;
            }
            let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            let exists: bool =
                tx.query_row("SELECT EXISTS (SELECT 1 FROM games WHERE source = 'grandie' AND source_id = ?1)", [&name], |r| r.get(0))?;
            if exists {
                continue;
            }
            summary.seen += 1;
            let record = read_pgn_file(&path).and_then(|parsed| match parsed.games.into_iter().next() {
                Some(tree) => dbimport::record_from_tree(&tree, "grandie", Some(name.clone()), &identity).map(Some),
                None => Ok(None),
            });
            match record {
                Ok(Some(rec)) => {
                    if dbimport::insert(&tx, &rec)?.is_some() {
                        summary.imported += 1;
                    }
                }
                Ok(None) => {}
                Err(e) => summary.push_error(format!("{name}: {e}")),
            }
        }
        tx.commit()?;
        Ok(summary)
    })
    .await
}

// ---- openings ----

/// Name of the opening reached by a line (deepest named position).
#[tauri::command]
#[specta::specta]
pub fn opening_name(root_fen: String, moves: Vec<String>) -> Option<OpeningName> {
    let start = (root_fen != crate::game::position::START_FEN).then_some(root_fen.as_str());
    OpeningBook::get().classify(start, &moves)
}

#[tauri::command]
#[specta::specta]
pub async fn explorer_db(state: State<'_, AppState>, fen: String, filter: ExplorerFilter) -> Result<ExplorerResult> {
    let pool = state.db.clone();
    blocking(move || crate::db::explorer::explore(&pool, &fen, &filter)).await
}

/// Moves from the Lichess explorer ("lichess" or "masters"); needs a Lichess token.
#[tauri::command]
#[specta::specta]
pub async fn explorer_lichess(
    state: State<'_, AppState>,
    db: String,
    fen: String,
    speeds: Vec<String>,
    ratings: Vec<u32>,
) -> Result<ExplorerResult> {
    let token = blocking(crate::secrets::lichess_token)
        .await?
        .ok_or_else(|| Error::NotFound("Add a Lichess API token in Settings to use the Lichess explorer.".into()))?;
    crate::net::explorer::explore(&state.http, &state.explorer_cache, &token, &db, &fen, &speeds, &ratings).await
}

#[tauri::command]
#[specta::specta]
pub async fn lichess_token_status() -> Result<bool> {
    Ok(blocking(crate::secrets::lichess_token).await?.is_some())
}

/// Store (or with an empty string, remove) the Lichess API token in the system
/// credential store (the Keychain on macOS, Credential Manager on Windows).
#[tauri::command]
#[specta::specta]
pub async fn lichess_token_set(token: String) -> Result<()> {
    blocking(move || crate::secrets::set_lichess_token(token.trim())).await
}
