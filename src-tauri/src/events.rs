//! Typed events sent from Rust to the UI (exported to TypeScript by tauri-specta).

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::game::session::{ClockTick, PlayObserver, PlaySnapshot};

/// The state of a game against the engine changed.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PlayUpdate(pub PlaySnapshot);

/// Clock times of a running game, ten times a second.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PlayClock(pub ClockTick);

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub id: String,
    pub received: f64,
    pub total: Option<f64>,
    pub done: bool,
    pub error: Option<String>,
}

/// A native menu item was chosen, e.g. "new_game" or "open_pgn".
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct MenuAction(pub String);

pub struct TauriPlayObserver(pub AppHandle);

impl PlayObserver for TauriPlayObserver {
    fn update(&self, snapshot: PlaySnapshot) {
        if let Err(e) = PlayUpdate(snapshot).emit(&self.0) {
            log::warn!("could not emit play update: {e}");
        }
    }

    fn clock(&self, tick: ClockTick) {
        let _ = PlayClock(tick).emit(&self.0);
    }
}

/// New lines from a live-analysis engine.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct AnalysisEvent(pub crate::analysis::live::AnalysisUpdate);

/// Progress of a full-game review.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ReviewProgress {
    pub review_id: String,
    pub done: u32,
    pub total: u32,
}

pub struct TauriAnalysisSink(pub AppHandle);

impl crate::analysis::live::AnalysisSink for TauriAnalysisSink {
    fn update(&self, update: crate::analysis::live::AnalysisUpdate) {
        let _ = AnalysisEvent(update).emit(&self.0);
    }
}

/// Progress of a database import.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub job_id: String,
    /// What is happening, e.g. "Downloading from Lichess".
    pub phase: String,
    pub seen: u32,
    pub imported: u32,
    pub bytes: f64,
}

/// The backend changed settings on its own (e.g. picked a default network
/// after a download); the UI replaces its copy.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct SettingsChanged(pub crate::settings::Settings);

/// Progress of an "Ask the coach" answer, by request id.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct CoachEvent {
    pub request_id: String,
    pub kind: CoachEventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CoachEventKind {
    /// More of the answer's text.
    Delta { text: String },
    /// The coach is looking something up ("analyse_position", ...).
    Tool { name: String },
    /// A line to play on the board.
    Script { script: crate::coach::script::CoachScript },
}

/// Delivers scripts from the chat tools to the UI as [`CoachEvent`]s.
pub struct TauriChatSink {
    pub app: AppHandle,
    pub request_id: String,
}

impl crate::coach::chat::ChatSink for TauriChatSink {
    fn script(&self, script: crate::coach::script::CoachScript) {
        let _ = CoachEvent { request_id: self.request_id.clone(), kind: CoachEventKind::Script { script } }.emit(&self.app);
    }
}

/// Microphone input level (0..1) while listening.
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct VoiceLevel(pub f64);
