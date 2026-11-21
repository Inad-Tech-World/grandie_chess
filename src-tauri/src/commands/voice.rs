//! Talking to the app: hold-to-talk recording, transcription, and the cloud voice.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use super::blocking;
use crate::ai::store::{self, Purpose};
use crate::error::{Error, Result};
use crate::events::VoiceLevel;
use crate::state::AppState;
use crate::voice::intent::{self, VoiceIntent};
use crate::voice::recorder::{self, MIN_SECONDS};

/// Start listening. Needs an OpenRouter key (speech is transcribed there).
#[tauri::command]
#[specta::specta]
pub async fn voice_start(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    if state.ai_key().await?.is_none() {
        return Err(Error::NoApiKey("Voice input uses OpenRouter to understand speech. Add a key in Settings → AI coach.".into()));
    }
    let level_app = app.clone();
    let app2 = app.clone();
    blocking(move || {
        use tauri::Manager;
        app2.state::<AppState>().recorder.start(move |level| {
            let _ = VoiceLevel(level as f64).emit(&level_app);
        })
    })
    .await
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceResult {
    pub transcript: String,
    pub intent: VoiceIntent,
}

/// Stop listening, transcribe, and work out what was meant. `fen` is the
/// position on the board, so spoken moves can be matched to legal moves.
#[tauri::command]
#[specta::specta]
pub async fn voice_stop(app: AppHandle, state: State<'_, AppState>, fen: Option<String>) -> Result<VoiceResult> {
    let app2 = app.clone();
    let recording = blocking(move || {
        use tauri::Manager;
        app2.state::<AppState>().recorder.stop()
    })
    .await?;
    if recording.seconds() < MIN_SECONDS || recording.peak_level() < 0.01 {
        return Ok(VoiceResult { transcript: String::new(), intent: VoiceIntent::Empty });
    }
    let seconds = recording.seconds();
    let wav = blocking(move || recorder::to_wav(&recording)).await?;
    let settings = state.settings();
    let client = state.openrouter().await?;
    let language = language_code(&settings.coach.language);
    let transcript = client.transcribe(&wav, &settings.ai.stt_model, Some(intent::STT_PROMPT), language).await?;
    let (pool, model, now) = (state.db.clone(), settings.ai.stt_model.clone(), crate::db::now_secs());
    let usage = crate::ai::openrouter::Usage { prompt_tokens: seconds.round() as u64, ..Default::default() };
    let _ = blocking(move || store::record(&*pool.get()?, Purpose::Transcribe, &model, &usage, now)).await;
    let intent = intent::interpret(&transcript, fen.as_deref());
    Ok(VoiceResult { transcript, intent })
}

#[tauri::command]
#[specta::specta]
pub fn voice_cancel(state: State<'_, AppState>) {
    state.recorder.cancel();
}

/// Work out what typed or spoken text means (no microphone involved).
#[tauri::command]
#[specta::specta]
pub fn voice_interpret(text: String, fen: Option<String>) -> VoiceIntent {
    intent::interpret(&text, fen.as_deref())
}

/// Speak text with the cloud voice; returns MP3 audio as base64.
#[tauri::command]
#[specta::specta]
pub async fn voice_speak(state: State<'_, AppState>, text: String) -> Result<String> {
    let settings = state.settings();
    let client = state.openrouter().await?;
    let text: String = text.chars().take(1200).collect();
    let audio = client.speech(&text, &settings.ai.tts_model, &settings.ai.tts_voice, Some(settings.coach.speech_rate)).await?;
    let (pool, model, now) = (state.db.clone(), settings.ai.tts_model.clone(), crate::db::now_secs());
    let usage = crate::ai::openrouter::Usage { completion_tokens: text.chars().count() as u64, ..Default::default() };
    let _ = blocking(move || store::record(&*pool.get()?, Purpose::Speech, &model, &usage, now)).await;
    Ok(base64::engine::general_purpose::STANDARD.encode(audio))
}

/// ISO-639-1 code for the coach's language, when it is a common one.
fn language_code(language: &str) -> Option<&'static str> {
    Some(match language.to_lowercase().as_str() {
        "english" => "en",
        "french" | "français" => "fr",
        "german" | "deutsch" => "de",
        "spanish" | "español" => "es",
        "italian" | "italiano" => "it",
        "portuguese" | "português" => "pt",
        "dutch" | "nederlands" => "nl",
        "russian" => "ru",
        "swahili" | "kiswahili" => "sw",
        "hindi" => "hi",
        "chinese" => "zh",
        "japanese" => "ja",
        _ => return None,
    })
}
