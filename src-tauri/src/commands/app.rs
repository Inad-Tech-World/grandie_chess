//! App info, chess rules and settings.

use tauri::State;

use crate::error::Result;
use crate::game::position::{apply_uci, parse_fen, position_info, AppliedMove, PositionInfo};
use crate::settings::Settings;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use specta::Type;

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

/// An error in the UI (a screen that failed to draw, an unhandled promise),
/// written to the log and to `errors.log` in the app's log folder
/// (`~/Library/Logs/com.grandie.chess` on macOS) so it can be reported.
#[tauri::command]
#[specta::specta]
pub async fn app_log_error(app: tauri::AppHandle, context: String, message: String) -> Result<()> {
    use std::io::Write;
    use tauri::Manager;
    let message: String = message.chars().take(8000).collect();
    log::error!("[ui] {context}: {message}");
    let Ok(dir) = app.path().app_log_dir() else { return Ok(()) };
    super::blocking(move || {
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("errors.log");
        // Keep it small: past 1 MB the old file is set aside and a new one begun.
        if std::fs::metadata(&path).map(|m| m.len() > 1_000_000).unwrap_or(false) {
            let _ = std::fs::rename(&path, dir.join("errors.old.log"));
        }
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(file, "{} [{}] {context}: {message}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), env!("CARGO_PKG_VERSION"))?;
        Ok(())
    })
    .await
}
