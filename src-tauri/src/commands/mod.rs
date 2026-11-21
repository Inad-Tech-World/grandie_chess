//! Commands callable from the UI, grouped by area. Types are exported to
//! `src/bindings.ts`; every command is registered in `lib.rs`.
//!
//! Commands only translate between the UI and the modules that do the work:
//! keep business logic in `engine`, `db`, `coach` and friends. Anything that
//! touches the database, files or the credential store runs through
//! [`blocking`] so it never stalls the async runtime or the UI thread.

pub mod ai;
pub mod app;
pub mod coach;
pub mod db;
pub mod engines;
pub mod files;
pub mod masters;
pub mod play;
pub mod review;
pub mod training;
pub mod voice;

use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::error::{Error, Result};
use crate::events::{DownloadProgress, SettingsChanged};
use crate::settings::Settings;
use crate::state::AppState;

/// Run blocking work (SQLite, files, the keychain) on the blocking pool.
pub async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| Error::other(format!("background task failed: {e}")))?
}

/// [`blocking`] with access to the app state.
pub async fn blocking_state<T: Send + 'static>(app: &AppHandle, f: impl FnOnce(&AppState) -> Result<T> + Send + 'static) -> Result<T> {
    let app = app.clone();
    blocking(move || f(&app.state::<AppState>())).await
}

/// Progress of a download, for the UI's progress bars.
pub fn emit_download(app: &AppHandle, id: &str, received: u64, total: Option<u64>, done: bool, error: Option<String>) {
    let _ = DownloadProgress { id: id.to_string(), received: received as f64, total: total.map(|t| t as f64), done, error }.emit(app);
}

/// Tell the UI about settings the backend changed on its own.
pub fn settings_changed(app: &AppHandle, settings: Settings) {
    let _ = SettingsChanged(settings).emit(app);
}
