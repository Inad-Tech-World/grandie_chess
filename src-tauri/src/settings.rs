use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::Result;

/// User preferences, stored as `settings.json` in the app data folder.
/// Unknown or missing fields fall back to defaults so older files keep loading.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Name shown for the user in games and PGN headers.
    pub player_name: String,
    /// Network used for full-strength play, hints and analysis.
    pub default_network: Option<String>,
    /// lc0 backend for full-strength work ("metal", "blas", "eigen"); `None` lets lc0 choose.
    pub backend: Option<String>,
    /// Extra lc0 backend options, e.g. "gpu=1" to pick the discrete GPU.
    pub backend_opts: Option<String>,
    /// lc0 search threads; `None` uses lc0's default.
    pub threads: Option<u32>,
    /// Add a short, human-like pause before low-strength opponents move.
    pub human_think_time: bool,
    /// Sample from Maia's policy in the opening so games vary.
    pub opponent_variety: bool,
    pub color_scheme: ColorScheme,
    pub board_theme: String,
    pub piece_set: String,
    pub show_coordinates: bool,
    pub animation_ms: u32,
    pub show_eval_bar: bool,
    /// Move, capture and game-end sounds.
    pub sound: bool,
    pub lichess_username: Option<String>,
    pub chesscom_username: Option<String>,
    pub onboarding_done: bool,
    /// Engine for game reviews; `None` picks Stockfish when installed, else lc0.
    pub review_engine_id: Option<String>,
    /// The talking coach.
    pub coach: CoachSettings,
    /// AI models used through OpenRouter (the key itself is in the keychain).
    pub ai: AiSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Verbosity {
    Brief,
    Normal,
    Detailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceOutput {
    /// The computer's own voices: free and offline.
    System,
    /// A neural voice from OpenRouter (needs a key; costs a little per use).
    Cloud,
    /// Captions only.
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct CoachSettings {
    /// "stockfish" or "leela"; `None` picks Stockfish when installed.
    pub coach_id: Option<String>,
    pub verbosity: Verbosity,
    pub voice_output: VoiceOutput,
    /// The system voice (its voiceURI; older versions stored its name).
    /// `None` uses the default voice.
    pub system_voice: Option<String>,
    /// Speaking speed, 0.5..2.0.
    pub speech_rate: f64,
    /// Say the opponent's moves out loud in games.
    pub announce_moves: bool,
    /// The language the coach speaks and writes, e.g. "English".
    pub language: String,
}

impl Default for CoachSettings {
    fn default() -> Self {
        CoachSettings {
            coach_id: None,
            verbosity: Verbosity::Normal,
            voice_output: VoiceOutput::System,
            system_voice: None,
            speech_rate: 1.0,
            announce_moves: false,
            language: "English".into(),
        }
    }
}

/// OpenRouter model ids. Defaults are checked against OpenRouter's catalog;
/// the Settings screen lists what is currently available.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(default, rename_all = "camelCase")]
pub struct AiSettings {
    /// Use OpenRouter for the coach's words and for voice input. Off until a
    /// key is saved; the coach falls back to built-in explanations.
    pub enabled: bool,
    /// Short narration of lines on the board (fast and cheap).
    pub narrate_model: String,
    /// Answers to questions and longer explanations.
    pub explain_model: String,
    /// Speech-to-text for voice input.
    pub stt_model: String,
    /// Text-to-speech for the cloud voice.
    pub tts_model: String,
    pub tts_voice: String,
}

pub const DEFAULT_NARRATE_MODEL: &str = "anthropic/claude-haiku-5.5";
pub const DEFAULT_EXPLAIN_MODEL: &str = "anthropic/claude-sonnet-5.5";
pub const DEFAULT_STT_MODEL: &str = "openai/gpt-4o-mini-transcribe";
pub const DEFAULT_TTS_MODEL: &str = "mistralai/voxtral-mini-tts-3-0";
pub const DEFAULT_TTS_VOICE: &str = "en_paul_confident";

impl Default for AiSettings {
    fn default() -> Self {
        AiSettings {
            enabled: false,
            narrate_model: DEFAULT_NARRATE_MODEL.into(),
            explain_model: DEFAULT_EXPLAIN_MODEL.into(),
            stt_model: DEFAULT_STT_MODEL.into(),
            tts_model: DEFAULT_TTS_MODEL.into(),
            tts_voice: DEFAULT_TTS_VOICE.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ColorScheme {
    Auto,
    Dark,
    Light,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            player_name: "You".into(),
            default_network: None,
            backend: None,
            backend_opts: None,
            threads: None,
            human_think_time: true,
            opponent_variety: true,
            color_scheme: ColorScheme::Auto,
            board_theme: "brown".into(),
            piece_set: "cburnett".into(),
            show_coordinates: true,
            animation_ms: 200,
            show_eval_bar: true,
            sound: true,
            lichess_username: None,
            chesscom_username: None,
            onboarding_done: false,
            review_engine_id: None,
            coach: CoachSettings::default(),
            ai: AiSettings::default(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                log::warn!("settings file unreadable, using defaults: {e}");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let s = Settings { player_name: "Ada".into(), backend: Some("metal".into()), ..Settings::default() };
        s.save(&path).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.player_name, "Ada");
        assert_eq!(loaded.backend.as_deref(), Some("metal"));

        std::fs::write(&path, r#"{"playerName":"Bo"}"#).unwrap();
        let partial = Settings::load(&path);
        assert_eq!(partial.player_name, "Bo");
        assert_eq!(partial.piece_set, "cburnett");
        assert_eq!(partial.ai.narrate_model, DEFAULT_NARRATE_MODEL, "new nested sections get defaults");

        std::fs::write(&path, r#"{"coach":{"verbosity":"brief"}}"#).unwrap();
        let nested = Settings::load(&path);
        assert_eq!(nested.coach.verbosity, Verbosity::Brief);
        assert_eq!(nested.coach.speech_rate, 1.0, "missing nested fields get defaults too");
    }
}
