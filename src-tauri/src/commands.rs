//! Commands callable from the UI. Types are exported to `src/bindings.ts`.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use std::sync::atomic::AtomicBool;

use crate::analysis::live::{to_white_line, AnalysisRequest};
use crate::analysis::review::{self, ReviewDepth, ReviewResult};
use crate::db::explorer::{ExplorerFilter, ExplorerResult};
use crate::db::games::{self as dbgames, DbGame, DbStats, GamePage, GameQuery};
use crate::db::import::{self as dbimport, ImportSummary};
use crate::engine::benchmark::{self, BackendResult};
use crate::engine::catalog::{network_path, NetworkEntry, NetworkKind, OpponentLevel};
use crate::engine::process::position_command;
use crate::engine::registry::{EngineConfig, EngineKind, OptionValue, Registry, LC0_ID};
use crate::error::{Error, Result};
use crate::events::{DownloadProgress, ImportProgress, ReviewProgress, TauriAnalysisSink, TauriPlayObserver};
use crate::game::openings::{OpeningBook, OpeningName};
use crate::game::pgn::{parse_pgn, read_pgn_file, write_pgn, PgnParseResult};
use crate::game::position::Side;
use crate::game::position::{apply_uci, parse_fen, position_info, AppliedMove, PositionInfo};
use crate::game::session::{PlayContext, PlayManager, PlaySetup, PlaySnapshot};
use crate::game::tree::GameTree;
use crate::net::download::{download_file, looks_like_network};
use crate::settings::Settings;
use crate::state::AppState;
use crate::training::progress::ProgressSummary;
use crate::training::puzzles::{self, MoveVerdict, Puzzle, PuzzleImportFilter, PuzzleResult, ThemeCount};
use crate::training::repertoire::{self, DrillItem, RepMove, RepertoireStats};
use crate::training::srs::{CardState, Grade};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub arch: String,
    pub lc0_path: Option<String>,
    pub data_dir: String,
    pub networks_dir: String,
    pub pgn_dir: String,
}

#[tauri::command]
#[specta::specta]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        arch: std::env::consts::ARCH.into(),
        lc0_path: state.lc0_path().ok().map(|p| p.to_string_lossy().into_owned()),
        data_dir: state.paths.data_dir.to_string_lossy().into_owned(),
        networks_dir: state.paths.networks_dir.to_string_lossy().into_owned(),
        pgn_dir: state.paths.pgn_dir.to_string_lossy().into_owned(),
    }
}

// ---- chess rules ----

#[tauri::command]
#[specta::specta]
pub fn chess_position_info(fen: String) -> Result<PositionInfo> {
    Ok(position_info(&parse_fen(&fen)?))
}

#[tauri::command]
#[specta::specta]
pub fn chess_apply_move(fen: String, uci: String) -> Result<AppliedMove> {
    Ok(apply_uci(&parse_fen(&fen)?, &uci)?.1)
}

/// Play a line of SAN moves from a position, e.g. an engine line to add as a variation.
#[tauri::command]
#[specta::specta]
pub fn chess_apply_san_line(fen: String, sans: Vec<String>) -> Result<Vec<AppliedMove>> {
    let mut pos = parse_fen(&fen)?;
    let mut out = Vec::with_capacity(sans.len());
    for san in &sans {
        let m = crate::game::position::parse_san(&pos, san)?;
        let (next, applied) = apply_uci(&pos, &crate::game::position::uci_of(m))?;
        out.push(applied);
        pos = next;
    }
    Ok(out)
}

// ---- settings ----

#[tauri::command]
#[specta::specta]
pub fn settings_get(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
#[specta::specta]
pub fn settings_set(state: State<'_, AppState>, settings: Settings) -> Result<()> {
    state.set_settings(settings)
}

// ---- networks and engines ----

#[tauri::command]
#[specta::specta]
pub fn networks_list(state: State<'_, AppState>) -> Vec<NetworkEntry> {
    state.catalog.entries(&state.paths.networks_dir)
}

#[tauri::command]
#[specta::specta]
pub async fn network_download(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let def = state.catalog.network(&id)?.clone();
    let dest = network_path(&state.paths.networks_dir, &id);
    if dest.exists() {
        return Ok(());
    }
    if !state.begin_download(&id) {
        return Err(Error::other(format!("{} is already downloading", def.name)));
    }
    let emit = |received: u64, total: Option<u64>, done: bool, error: Option<String>| {
        let _ = DownloadProgress { id: id.clone(), received: received as f64, total: total.map(|t| t as f64), done, error }.emit(&app);
    };
    let result = download_file(&state.http, &def.url, &dest, Some(def.size as u64), |r, t| emit(r, t, false, None)).await;
    state.end_download(&id);
    match result {
        Ok(size) if looks_like_network(&dest) => {
            emit(size, Some(size), true, None);
            if def.kind == NetworkKind::Leela && state.settings().default_network.is_none() && id != "703810" {
                state.update_settings(|s| s.default_network = Some(id.clone()))?;
            }
            Ok(())
        }
        Ok(_) => {
            let _ = std::fs::remove_file(&dest);
            let msg = format!("{} downloaded but is not a network file", def.name);
            emit(0, None, true, Some(msg.clone()));
            Err(Error::other(msg))
        }
        Err(e) => {
            emit(0, None, true, Some(e.to_string()));
            Err(e)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn network_delete(state: State<'_, AppState>, id: String) -> Result<()> {
    let path = network_path(&state.paths.networks_dir, &id);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    if state.settings().default_network.as_deref() == Some(id.as_str()) {
        state.update_settings(|s| s.default_network = None)?;
    }
    Ok(())
}

/// Times every Metal GPU and the CPU, saves the fastest backend in settings
/// and returns all results.
#[tauri::command]
#[specta::specta]
pub async fn engine_detect_backend(state: State<'_, AppState>) -> Result<Vec<BackendResult>> {
    let lc0 = state.lc0_path()?;
    let network = state.full_network()?;
    let weights = network_path(&state.paths.networks_dir, &network);
    let mut results = Vec::new();
    let mut metal_exhausted = false;
    for (backend, opts) in benchmark::candidates() {
        // GPU indices are contiguous: once one fails there are no more GPUs.
        if backend == "metal" && metal_exhausted {
            continue;
        }
        let r = benchmark::run_one(&lc0, &weights, &backend, opts.as_deref()).await;
        if backend == "metal" && r.error.is_some() {
            metal_exhausted = true;
            if results.iter().any(|p: &BackendResult| p.backend == "metal") {
                continue;
            }
        }
        results.push(r);
    }
    if let Some(best) = benchmark::pick_best(&results).cloned() {
        state.update_settings(|s| {
            s.backend = Some(best.backend.clone());
            s.backend_opts = best.backend_opts.clone();
        })?;
    }
    Ok(results)
}

// ---- play ----

#[tauri::command]
#[specta::specta]
pub fn levels_list(state: State<'_, AppState>) -> Vec<OpponentLevel> {
    state.catalog.levels.clone()
}

/// Network ids the opponent needs that are not installed.
#[tauri::command]
#[specta::specta]
pub fn play_missing_networks(state: State<'_, AppState>, opponent: String) -> Result<Vec<String>> {
    PlayManager::missing_networks(&state.catalog, &state.settings(), &state.paths.networks_dir, &opponent)
}

#[tauri::command]
#[specta::specta]
pub async fn play_start(app: AppHandle, state: State<'_, AppState>, setup: PlaySetup) -> Result<PlaySnapshot> {
    let ctx = Arc::new(PlayContext {
        observer: Arc::new(TauriPlayObserver(app.clone())),
        settings: state.settings(),
        lc0_path: state.lc0_path()?,
        networks_dir: state.paths.networks_dir.clone(),
        pgn_dir: state.paths.pgn_dir.clone(),
    });
    state.play.start(ctx, &state.catalog, setup).await
}

#[tauri::command]
#[specta::specta]
pub async fn play_snapshot(state: State<'_, AppState>, session_id: u32) -> Result<PlaySnapshot> {
    state.play.snapshot(session_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn play_move(state: State<'_, AppState>, session_id: u32, uci: String) -> Result<PlaySnapshot> {
    state.play.user_move(session_id, &uci).await
}

#[tauri::command]
#[specta::specta]
pub async fn play_takeback(state: State<'_, AppState>, session_id: u32) -> Result<PlaySnapshot> {
    state.play.takeback(session_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn play_resign(state: State<'_, AppState>, session_id: u32) -> Result<PlaySnapshot> {
    state.play.resign(session_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn play_offer_draw(state: State<'_, AppState>, session_id: u32) -> Result<bool> {
    state.play.offer_draw(session_id).await
}

/// Ask the full-strength engine for a move suggestion (shown as an arrow).
#[tauri::command]
#[specta::specta]
pub async fn play_hint(state: State<'_, AppState>, session_id: u32) -> Result<String> {
    let (start_fen, moves, users_turn) = state.play.position(session_id).await?;
    if !users_turn {
        return Err(Error::Chess("hints are only available on your move".into()));
    }
    let engine = state.helper_engine().await?;
    let result = engine.search(&position_command(&start_fen, &moves), "go nodes 400", |_| {}).await?;
    state.play.set_hint(session_id, Some(result.best.clone())).await?;
    Ok(result.best)
}

#[tauri::command]
#[specta::specta]
pub async fn play_pgn(state: State<'_, AppState>, session_id: u32) -> Result<String> {
    state.play.pgn(session_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn play_close(state: State<'_, AppState>, session_id: u32) -> Result<()> {
    state.play.close(session_id).await
}

// ---- PGN and files ----

#[tauri::command]
#[specta::specta]
pub fn pgn_parse(text: String) -> PgnParseResult {
    parse_pgn(&text)
}

#[tauri::command]
#[specta::specta]
pub fn pgn_read_file(path: String) -> Result<PgnParseResult> {
    read_pgn_file(&PathBuf::from(path))
}

#[tauri::command]
#[specta::specta]
pub fn pgn_write(tree: GameTree) -> String {
    write_pgn(&tree)
}

#[tauri::command]
#[specta::specta]
pub fn file_read_text(path: String) -> Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[tauri::command]
#[specta::specta]
pub fn file_write_text(path: String, contents: String) -> Result<()> {
    std::fs::write(path, contents)?;
    Ok(())
}

// ---- engines ----

#[tauri::command]
#[specta::specta]
pub fn engines_list(state: State<'_, AppState>) -> Vec<EngineConfig> {
    state.registry.list()
}

/// Add a UCI engine from an executable the user picked.
#[tauri::command]
#[specta::specta]
pub async fn engine_add(state: State<'_, AppState>, path: String) -> Result<EngineConfig> {
    let config = Registry::probe(std::path::Path::new(&path)).await?;
    state.registry.upsert(config.clone())?;
    Ok(config)
}

#[tauri::command]
#[specta::specta]
pub async fn engine_update(state: State<'_, AppState>, config: EngineConfig) -> Result<()> {
    state.analysis.stop(&config.id).await?;
    state.registry.upsert(config)
}

#[tauri::command]
#[specta::specta]
pub async fn engine_remove(state: State<'_, AppState>, id: String) -> Result<()> {
    state.analysis.stop(&id).await?;
    state.registry.remove(&id)
}

const STOCKFISH_URL: &str = "https://github.com/official-stockfish/Stockfish/releases/latest/download/stockfish-macos-universal.tar.gz";

/// Download the official Stockfish release for macOS and add it as an engine.
#[tauri::command]
#[specta::specta]
pub async fn engine_install_stockfish(app: AppHandle, state: State<'_, AppState>) -> Result<EngineConfig> {
    let id = "stockfish".to_string();
    if !state.begin_download(&id) {
        return Err(Error::other("Stockfish is already downloading"));
    }
    let dir = state.paths.engines_dir.join("stockfish");
    let archive = state.paths.engines_dir.join("stockfish.tar.gz");
    let emit = |received: u64, total: Option<u64>, done: bool, error: Option<String>| {
        let _ = DownloadProgress { id: id.clone(), received: received as f64, total: total.map(|t| t as f64), done, error }.emit(&app);
    };
    let result: Result<EngineConfig> = async {
        download_file(&state.http, STOCKFISH_URL, &archive, None, |r, t| emit(r, t, false, None)).await?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        let status = tokio::process::Command::new("tar").arg("-xzf").arg(&archive).arg("-C").arg(&dir).status().await?;
        let _ = std::fs::remove_file(&archive);
        if !status.success() {
            return Err(Error::other("could not unpack the Stockfish download"));
        }
        let exe = find_executable(&dir, "stockfish").ok_or_else(|| Error::other("no Stockfish binary in the download"))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))?;
        let mut config = Registry::probe(&exe).await?;
        config.id = "stockfish".into();
        let threads = std::thread::available_parallelism().map(|n| n.get().saturating_sub(2).max(1)).unwrap_or(2);
        config.options = vec![
            OptionValue { name: "Threads".into(), value: threads.to_string() },
            OptionValue { name: "Hash".into(), value: "256".into() },
        ];
        state.registry.upsert(config.clone())?;
        Ok(config)
    }
    .await;
    state.end_download(&id);
    match &result {
        Ok(_) => emit(1, Some(1), true, None),
        Err(e) => emit(0, None, true, Some(e.to_string())),
    }
    result
}

fn find_executable(dir: &std::path::Path, prefix: &str) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(prefix) && !n.contains('.')) {
                return Some(path);
            }
        }
    }
    None
}

// ---- live analysis ----

#[tauri::command]
#[specta::specta]
pub async fn analysis_start(app: AppHandle, state: State<'_, AppState>, request: AnalysisRequest) -> Result<()> {
    let config = state.registry.get(&request.engine_id)?;
    let launch = state.launch_for(&config)?;
    state.analysis.start(launch, request, Arc::new(TauriAnalysisSink(app))).await
}

#[tauri::command]
#[specta::specta]
pub async fn analysis_stop(state: State<'_, AppState>, engine_id: String) -> Result<()> {
    state.analysis.stop(&engine_id).await
}

/// The backend an engine reported, e.g. "metal (AMD Radeon Pro 560X)".
#[tauri::command]
#[specta::specta]
pub async fn analysis_backend(state: State<'_, AppState>, engine_id: String) -> Result<Option<String>> {
    Ok(state.analysis.backend_banner(&engine_id).await)
}

// ---- game review ----

#[tauri::command]
#[specta::specta]
pub async fn review_start(
    app: AppHandle,
    state: State<'_, AppState>,
    review_id: String,
    start_fen: String,
    moves: Vec<String>,
    depth: ReviewDepth,
    engine_id: Option<String>,
) -> Result<ReviewResult> {
    let config = state.registry.get(engine_id.as_deref().unwrap_or(LC0_ID))?;
    let launch = state.launch_for(&config)?;
    let engine = crate::engine::process::Engine::spawn(&launch.path, &[]).await?;
    for (name, value) in &launch.options {
        engine.set_option(name, value).await?;
    }
    engine.ready(crate::engine::lc0::READY_TIMEOUT).await?;
    let cancel = Arc::new(AtomicBool::new(false));
    state.reviews.lock().unwrap().insert(review_id.clone(), cancel.clone());
    let lc0 = config.kind == EngineKind::Lc0;
    let result = review::run(&engine, lc0, &start_fen, &moves, depth, cancel, |done, total| {
        let _ = ReviewProgress { review_id: review_id.clone(), done, total }.emit(&app);
    })
    .await;
    state.reviews.lock().unwrap().remove(&review_id);
    engine.quit().await;
    result
}

#[tauri::command]
#[specta::specta]
pub fn review_cancel(state: State<'_, AppState>, review_id: String) {
    if let Some(flag) = state.reviews.lock().unwrap().get(&review_id) {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MoveCheck {
    /// Mover's win% after the move (0..100).
    pub win_after: f64,
    pub san: String,
    /// The engine's reply and line after the move, in SAN.
    pub line: Vec<String>,
}

/// Evaluate a candidate move ("retry this position").
#[tauri::command]
#[specta::specta]
pub async fn review_check_move(state: State<'_, AppState>, fen: String, uci: String) -> Result<MoveCheck> {
    let pos = parse_fen(&fen)?;
    let (after, applied) = apply_uci(&pos, &uci)?;
    let mover_white = applied.position.turn == crate::game::position::Side::Black;
    let white_win = if let Some(outcome) = &applied.position.outcome {
        match outcome.winner {
            Some(crate::game::position::Side::White) => 100.0,
            Some(crate::game::position::Side::Black) => 0.0,
            None => 50.0,
        }
    } else {
        let engine = state.helper_engine().await?;
        let result = engine.search(&format!("position fen {}", applied.position.fen), "go nodes 400", |_| {}).await?;
        let top = result.top().ok_or_else(|| Error::Engine("no evaluation".into()))?;
        let line = to_white_line(top, &after);
        return Ok(MoveCheck {
            win_after: if mover_white { line.white_win.unwrap_or(50.0) } else { 100.0 - line.white_win.unwrap_or(50.0) },
            san: applied.san,
            line: line.san,
        });
    };
    Ok(MoveCheck { win_after: if mover_white { white_win } else { 100.0 - white_win }, san: applied.san, line: vec![] })
}

// ---- database ----

#[tauri::command]
#[specta::specta]
pub async fn db_query(state: State<'_, AppState>, query: GameQuery) -> Result<GamePage> {
    let pool = state.db.clone();
    tokio::task::spawn_blocking(move || dbgames::query(&pool, &query)).await.map_err(|e| Error::other(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub fn db_game(state: State<'_, AppState>, id: i32) -> Result<DbGame> {
    dbgames::get(&state.db, id)
}

#[tauri::command]
#[specta::specta]
pub fn db_delete(state: State<'_, AppState>, ids: Vec<i32>) -> Result<u32> {
    dbgames::delete(&state.db, &ids)
}

#[tauri::command]
#[specta::specta]
pub fn db_stats(state: State<'_, AppState>) -> Result<DbStats> {
    dbgames::stats(&state.db)
}

#[tauri::command]
#[specta::specta]
pub fn db_find(state: State<'_, AppState>, source: String, source_id: String) -> Result<Option<i32>> {
    dbgames::find(&state.db, &source, &source_id)
}

#[tauri::command]
#[specta::specta]
pub fn db_save_review(state: State<'_, AppState>, game_id: i32, review: ReviewResult) -> Result<()> {
    dbgames::save_review(&state.db, game_id, &review)
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
    tokio::task::spawn_blocking(move || {
        let mut total = ImportSummary::default();
        for path in paths {
            let file = std::fs::File::open(&path)?;
            let s = dbimport::import_reader(&pool, std::io::BufReader::new(file), "pgn", &identity, |s| {
                emit_import(&app, &job_id, "Importing", s, 0)
            })?;
            total.seen += s.seen;
            total.imported += s.imported;
            total.duplicates += s.duplicates;
            total.errors.extend(s.errors.into_iter().take(20usize.saturating_sub(total.errors.len())));
        }
        Ok(total)
    })
    .await
    .map_err(|e| Error::other(e.to_string()))?
}

async fn import_bytes(app: &AppHandle, state: &AppState, job_id: &str, bytes: Vec<u8>, source: &str) -> Result<ImportSummary> {
    let pool = state.db.clone();
    let identity = state.identity();
    let (app, job_id, source) = (app.clone(), job_id.to_string(), source.to_string());
    tokio::task::spawn_blocking(move || {
        dbimport::import_reader(&pool, bytes.as_slice(), &source, &identity, |s| emit_import(&app, &job_id, "Importing", s, 0))
    })
    .await
    .map_err(|e| Error::other(e.to_string()))?
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
        total.seen += s.seen;
        total.imported += s.imported;
        total.duplicates += s.duplicates;
        total.newest = match (total.newest, s.newest) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        total.errors.extend(s.errors.into_iter().take(20usize.saturating_sub(total.errors.len())));
    }
    if let Some(newest) = total.newest {
        dbgames::set_last_sync(&*state.db.get()?, "chesscom", &username, newest as i64)?;
    }
    Ok(total)
}

/// Add games played in Grandie (saved as PGN files) to the database.
#[tauri::command]
#[specta::specta]
pub fn db_import_grandie(state: State<'_, AppState>) -> Result<ImportSummary> {
    let identity = state.identity();
    let mut summary = ImportSummary::default();
    let mut conn = state.db.get()?;
    let tx = conn.transaction()?;
    for entry in std::fs::read_dir(&state.paths.pgn_dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pgn") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let exists: bool =
            tx.query_row("SELECT EXISTS (SELECT 1 FROM games WHERE source = 'grandie' AND source_id = ?1)", [&name], |r| r.get(0))?;
        if exists {
            continue;
        }
        summary.seen += 1;
        let parsed = read_pgn_file(&path)?;
        if let Some(tree) = parsed.games.into_iter().next() {
            let rec = dbimport::record_from_tree(&tree, "grandie", Some(name), &identity)?;
            if dbimport::insert(&tx, &rec)?.is_some() {
                summary.imported += 1;
            }
        }
    }
    tx.commit()?;
    Ok(summary)
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
    tokio::task::spawn_blocking(move || crate::db::explorer::explore(&pool, &fen, &filter))
        .await
        .map_err(|e| Error::other(e.to_string()))?
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
    let token = crate::secrets::lichess_token()?
        .ok_or_else(|| Error::NotFound("Add a Lichess API token in Settings to use the Lichess explorer.".into()))?;
    crate::net::explorer::explore(&state.http, &state.explorer_cache, &token, &db, &fen, &speeds, &ratings).await
}

#[tauri::command]
#[specta::specta]
pub fn lichess_token_status() -> Result<bool> {
    Ok(crate::secrets::lichess_token()?.is_some())
}

/// Store (or with an empty string, remove) the Lichess API token in the Keychain.
#[tauri::command]
#[specta::specta]
pub fn lichess_token_set(token: String) -> Result<()> {
    crate::secrets::set_lichess_token(token.trim())
}

// ---- repertoire ----

#[tauri::command]
#[specta::specta]
pub fn repertoire_moves(state: State<'_, AppState>, color: Side) -> Result<Vec<RepMove>> {
    repertoire::moves(&*state.db.get()?, color)
}

#[tauri::command]
#[specta::specta]
pub fn repertoire_add_line(state: State<'_, AppState>, color: Side, moves: Vec<String>) -> Result<u32> {
    repertoire::add_line(&mut *state.db.get()?, color, &moves, crate::db::now_secs())
}

#[tauri::command]
#[specta::specta]
pub fn repertoire_remove_move(state: State<'_, AppState>, id: i32) -> Result<()> {
    repertoire::remove_move(&mut *state.db.get()?, id, crate::db::now_secs())
}

#[tauri::command]
#[specta::specta]
pub fn repertoire_set_comment(state: State<'_, AppState>, id: i32, comment: Option<String>) -> Result<()> {
    repertoire::set_comment(&*state.db.get()?, id, comment.as_deref())
}

#[tauri::command]
#[specta::specta]
pub fn repertoire_import_pgn(state: State<'_, AppState>, color: Side, pgn: String) -> Result<u32> {
    let parsed = parse_pgn(&pgn);
    let mut conn = state.db.get()?;
    let mut added = 0;
    for tree in &parsed.games {
        added += repertoire::add_tree(&mut conn, color, tree, crate::db::now_secs())?;
    }
    Ok(added)
}

#[tauri::command]
#[specta::specta]
pub fn repertoire_export_pgn(state: State<'_, AppState>, color: Side) -> Result<String> {
    Ok(write_pgn(&repertoire::to_tree(&*state.db.get()?, color)?))
}

#[tauri::command]
#[specta::specta]
pub fn repertoire_stats(state: State<'_, AppState>, color: Side) -> Result<RepertoireStats> {
    repertoire::stats(&*state.db.get()?, color, crate::db::now_secs())
}

/// Cards to drill now: due reviews first, then up to `new_limit` new ones.
#[tauri::command]
#[specta::specta]
pub fn repertoire_due(state: State<'_, AppState>, color: Option<Side>, limit: u32, new_limit: u32) -> Result<Vec<DrillItem>> {
    repertoire::due(&*state.db.get()?, color, crate::db::now_secs(), limit, new_limit)
}

#[tauri::command]
#[specta::specta]
pub fn repertoire_grade(state: State<'_, AppState>, color: Side, key: String, grade: Grade) -> Result<CardState> {
    repertoire::grade(&*state.db.get()?, color, &key, grade, crate::db::now_secs())
}

// ---- puzzles ----

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PuzzleInfo {
    pub count: u32,
    pub rating: crate::training::glicko::Rating,
}

#[tauri::command]
#[specta::specta]
pub fn puzzles_info(state: State<'_, AppState>) -> Result<PuzzleInfo> {
    Ok(PuzzleInfo { count: puzzles::count(&state.puzzles)?, rating: puzzles::rating(&*state.db.get()?)? })
}

/// Download the Lichess puzzle database and import it. With `filter` set,
/// only well-tested, popular puzzles are kept (a third of the database).
#[tauri::command]
#[specta::specta]
pub async fn puzzles_download(app: AppHandle, state: State<'_, AppState>, filter: Option<PuzzleImportFilter>) -> Result<u32> {
    let id = "puzzles".to_string();
    if !state.begin_download(&id) {
        return Err(Error::other("The puzzle database is already downloading"));
    }
    let archive = state.paths.data_dir.join("lichess_db_puzzle.csv.zst");
    let result: Result<u32> = async {
        download_file(&state.http, puzzles::PUZZLE_DB_URL, &archive, None, |r, t| {
            let _ =
                DownloadProgress { id: id.clone(), received: r as f64, total: t.map(|t| t as f64), done: false, error: None }.emit(&app);
        })
        .await?;
        let pool = state.puzzles.clone();
        let path = archive.clone();
        let app2 = app.clone();
        let kept = tokio::task::spawn_blocking(move || -> Result<u64> {
            let file = std::fs::File::open(&path)?;
            let decoder = zstd::Decoder::new(std::io::BufReader::new(file))?;
            puzzles::import_csv(&pool, decoder, filter, |read, kept| {
                let _ = ImportProgress {
                    job_id: "puzzles".into(),
                    phase: "Importing puzzles".into(),
                    seen: read.min(u32::MAX as u64) as u32,
                    imported: kept.min(u32::MAX as u64) as u32,
                    bytes: 0.0,
                }
                .emit(&app2);
            })
        })
        .await
        .map_err(|e| Error::other(e.to_string()))??;
        Ok(kept as u32)
    }
    .await;
    let _ = std::fs::remove_file(&archive);
    state.end_download(&id);
    let _ =
        DownloadProgress { id: id.clone(), received: 0.0, total: None, done: true, error: result.as_ref().err().map(|e| e.to_string()) }
            .emit(&app);
    result
}

#[tauri::command]
#[specta::specta]
pub fn puzzles_themes(state: State<'_, AppState>) -> Result<Vec<ThemeCount>> {
    puzzles::themes(&state.puzzles)
}

/// The next puzzle near the user's rating (or `rating` if given).
#[tauri::command]
#[specta::specta]
pub fn puzzle_next(state: State<'_, AppState>, theme: Option<String>, rating: Option<f64>) -> Result<Option<Puzzle>> {
    let conn = state.db.get()?;
    let target = rating.unwrap_or(puzzles::rating(&conn)?.rating);
    let recent = puzzles::recent_ids(&conn, 300)?;
    puzzles::next(&state.puzzles, target, theme.as_deref(), &recent)
}

#[tauri::command]
#[specta::specta]
pub fn puzzle_check(state: State<'_, AppState>, puzzle_id: String, index: u32, uci: String) -> Result<MoveVerdict> {
    puzzles::check_move(&puzzles::get(&state.puzzles, &puzzle_id)?, index as usize, &uci)
}

/// Record the outcome of a puzzle and update the puzzle rating.
#[tauri::command]
#[specta::specta]
pub fn puzzle_finish(state: State<'_, AppState>, puzzle_id: String, solved: bool) -> Result<PuzzleResult> {
    let p = puzzles::get(&state.puzzles, &puzzle_id)?;
    puzzles::record_attempt(&*state.db.get()?, &p, solved, crate::db::now_secs())
}

// ---- progress ----

#[tauri::command]
#[specta::specta]
pub fn progress_summary(state: State<'_, AppState>) -> Result<ProgressSummary> {
    crate::training::progress::summary(&*state.db.get()?, &state.catalog, crate::db::now_secs())
}
