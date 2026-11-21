//! The OpenRouter key, model catalogue and spending.

use tauri::{AppHandle, State};

use super::{blocking, blocking_state, settings_changed};
use crate::ai::openrouter::{AiKeyInfo, AiModel};
use crate::ai::store::{self, AiUsageSummary};
use crate::error::{ErrorKind, Result};
use crate::secrets::{self, Secret};
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn ai_key_status(state: State<'_, AppState>) -> Result<bool> {
    Ok(state.ai_key().await?.is_some())
}

/// Save (or with an empty string, remove) the OpenRouter key in the system
/// credential store. A new key is checked with OpenRouter first; saving one
/// switches AI on, removing it switches AI off.
#[tauri::command]
#[specta::specta]
pub async fn ai_key_set(app: AppHandle, state: State<'_, AppState>, key: String) -> Result<()> {
    let key = key.trim().to_string();
    if !key.is_empty() {
        Secret::OpenRouterKey.validate(&key)?;
        let client = crate::ai::OpenRouter::new(state.http.clone(), key.clone());
        match client.key_info().await {
            Ok(_) => {}
            // Offline: keep the key; it is checked again on first use.
            Err(e) if e.kind() == ErrorKind::Network => log::warn!("could not verify the OpenRouter key: {e}"),
            Err(e) => return Err(e),
        }
    }
    let stored = key.clone();
    blocking(move || secrets::set(Secret::OpenRouterKey, &stored)).await?;
    state.set_cached_ai_key((!key.is_empty()).then_some(key.clone()));
    let settings = state.update_settings(|s| s.ai.enabled = !key.is_empty())?;
    settings_changed(&app, settings);
    Ok(())
}

/// Credits used and the key's limit.
#[tauri::command]
#[specta::specta]
pub async fn ai_key_info(state: State<'_, AppState>) -> Result<AiKeyInfo> {
    state.openrouter().await?.key_info().await
}

/// OpenRouter's models with an output modality: "text", "speech" or
/// "transcription". The catalogue is public, so this works without a key.
#[tauri::command]
#[specta::specta]
pub async fn ai_models(state: State<'_, AppState>, modality: Option<String>) -> Result<Vec<AiModel>> {
    let key = state.ai_key().await?.unwrap_or_default();
    crate::ai::OpenRouter::new(state.http.clone(), key).models(modality.as_deref()).await
}

/// Voices a text-to-speech model offers.
#[tauri::command]
#[specta::specta]
pub async fn ai_voices(state: State<'_, AppState>, model: String) -> Result<Vec<String>> {
    let key = state.ai_key().await?.unwrap_or_default();
    crate::ai::OpenRouter::new(state.http.clone(), key).voices(&model).await
}

/// What AI requests cost over the last `days` days.
#[tauri::command]
#[specta::specta]
pub async fn ai_usage(app: AppHandle, days: u32) -> Result<AiUsageSummary> {
    let since = crate::db::now_secs() - i64::from(days) * 86_400;
    blocking_state(&app, move |state| store::summary(&*state.db.get()?, since)).await
}
