//! Games against the engine.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::engine::catalog::OpponentLevel;
use crate::engine::process::{position_command, SearchControl};
use crate::error::{Error, Result};
use crate::events::TauriPlayObserver;
use crate::game::session::{PlayContext, PlayManager, PlaySetup, PlaySnapshot};
use crate::state::AppState;

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
    let ctl = SearchControl { timeout: Some(std::time::Duration::from_secs(60)), ..Default::default() };
    let result = engine.search_ctl(&position_command(&start_fen, &moves), "go nodes 400", &ctl, |_| {}).await?;
    // Dropped if the user moved while the engine was thinking.
    state.play.set_hint(session_id, moves.len(), Some(result.best.clone())).await?;
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
