//! Networks, engines and live analysis.

use std::sync::Arc;

use tauri::{AppHandle, State};

use super::{emit_download, settings_changed};
use crate::analysis::live::AnalysisRequest;
use crate::engine::benchmark::{self, BackendResult};
use crate::engine::catalog::{network_path, NetworkEntry, NetworkKind};
use crate::engine::install::{self, STOCKFISH_ID};
use crate::engine::platform::GPU_BACKEND;
use crate::engine::registry::{EngineConfig, Registry};

use crate::error::{Error, Result};
use crate::events::TauriAnalysisSink;
use crate::net::download::{download_file, looks_like_network, Expect};
use crate::state::AppState;

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
    let Some(_guard) = state.begin_download(&id) else {
        return Err(Error::other(format!("{} is already downloading", def.name)));
    };
    let result =
        download_file(&state.http, &def.url, &dest, Expect::size(def.size as u64), |r, t| emit_download(&app, &id, r, t, false, None))
            .await;
    match result {
        Ok(size) if looks_like_network(&dest) => {
            emit_download(&app, &id, size, Some(size), true, None);
            if def.kind == NetworkKind::Leela
                && state.settings().default_network.is_none()
                && !crate::engine::catalog::is_small_network(&id)
            {
                let settings = state.update_settings(|s| s.default_network = Some(id.clone()))?;
                settings_changed(&app, settings);
            }
            Ok(())
        }
        Ok(_) => {
            let _ = std::fs::remove_file(&dest);
            let msg = format!("{} downloaded but is not a network file", def.name);
            emit_download(&app, &id, 0, None, true, Some(msg.clone()));
            Err(Error::other(msg))
        }
        Err(e) => {
            emit_download(&app, &id, 0, None, true, Some(e.to_string()));
            Err(e)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn network_delete(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let path = network_path(&state.paths.networks_dir, &id);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    if state.settings().default_network.as_deref() == Some(id.as_str()) {
        let settings = state.update_settings(|s| s.default_network = None)?;
        settings_changed(&app, settings);
    }
    Ok(())
}

/// Times every GPU and the CPU, saves the fastest backend in settings and
/// returns all results.
#[tauri::command]
#[specta::specta]
pub async fn engine_detect_backend(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<BackendResult>> {
    let lc0 = state.lc0_path()?;
    let network = state.full_network()?;
    let weights = network_path(&state.paths.networks_dir, &network);
    let mut results = Vec::new();
    let mut gpu_exhausted = false;
    for (backend, opts) in benchmark::candidates() {
        // GPU indices are contiguous: once one fails there are no more GPUs.
        if backend == GPU_BACKEND && gpu_exhausted {
            continue;
        }
        let r = benchmark::run_one(&lc0, &weights, &backend, opts.as_deref()).await;
        if backend == GPU_BACKEND && r.error.is_some() {
            gpu_exhausted = true;
            if results.iter().any(|p: &BackendResult| p.backend == GPU_BACKEND) {
                continue;
            }
        }
        results.push(r);
    }
    if let Some(best) = benchmark::pick_best(&results).cloned() {
        let settings = state.update_settings(|s| {
            s.backend = Some(best.backend.clone());
            s.backend_opts = best.backend_opts.clone();
        })?;
        settings_changed(&app, settings);
    }
    Ok(results)
}

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
    state.analysis.close(&id).await;
    state.registry.remove(&id)
}

/// Download the official Stockfish release (pinned, checksummed) and add it
/// as an engine. A running Stockfish is shut down first so it can be replaced.
#[tauri::command]
#[specta::specta]
pub async fn engine_install_stockfish(app: AppHandle, state: State<'_, AppState>) -> Result<EngineConfig> {
    let id = STOCKFISH_ID;
    let Some(_guard) = state.begin_download(id) else {
        return Err(Error::other("Stockfish is already downloading"));
    };
    state.analysis.close(id).await;
    state.close_engine_users(id).await;
    let result = install::install_stockfish(&state.http, &state.paths.engines_dir, &state.registry, |r, t| {
        emit_download(&app, id, r, t, false, None)
    })
    .await;
    match &result {
        Ok(_) => emit_download(&app, id, 1, Some(1), true, None),
        Err(e) => emit_download(&app, id, 0, None, true, Some(e.to_string())),
    }
    result
}

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
