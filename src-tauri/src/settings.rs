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
    }
}
