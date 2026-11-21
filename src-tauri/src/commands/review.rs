//! Full-game review and checking candidate moves.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::analysis::live::to_white_line;
use crate::analysis::review::{self, ReviewDepth, ReviewResult};
use crate::engine::registry::EngineKind;
use crate::error::{Error, Result};
use crate::events::ReviewProgress;
use crate::game::position::{apply_uci, parse_fen};
use crate::state::{AppState, ReviewJob};

/// Review a whole game. `engine_id` defaults to the review engine from
/// settings (Stockfish when installed). Cancel with [`review_cancel`], which
/// works from the moment this is called, even while the engine starts.
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
    review_game(&app, &state, &review_id, &start_fen, &moves, depth, engine_id.as_deref()).await
}

/// Review a game as a cancellable job (see [`review_cancel`]), reporting
/// progress under `review_id`.
pub async fn review_game(
    app: &AppHandle,
    state: &AppState,
    review_id: &str,
    start_fen: &str,
    moves: &[String],
    depth: ReviewDepth,
    engine_id: Option<&str>,
) -> Result<ReviewResult> {
    let job = Arc::new(ReviewJob::default());
    {
        let mut reviews = state.reviews.lock().unwrap();
        if reviews.contains_key(review_id) {
            return Err(Error::other("this game is already being reviewed"));
        }
        reviews.insert(review_id.to_string(), job.clone());
    }
    let result = run_review(app, state, review_id, &job, start_fen, moves, depth, engine_id).await;
    state.reviews.lock().unwrap().remove(review_id);
    if let Some(engine) = job.engine.lock().unwrap().take() {
        tokio::spawn(async move { engine.quit().await });
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn run_review(
    app: &AppHandle,
    state: &AppState,
    review_id: &str,
    job: &Arc<ReviewJob>,
    start_fen: &str,
    moves: &[String],
    depth: ReviewDepth,
    engine_id: Option<&str>,
) -> Result<ReviewResult> {
    let config = state.registry.get(&state.review_engine_id(engine_id))?;
    let launch = state.launch_for(&config)?;
    let engine = crate::engine::launch(&launch.path, &launch.options).await?;
    *job.engine.lock().unwrap() = Some(engine.clone());
    if job.cancel.load(Ordering::SeqCst) {
        return Err(Error::Cancelled);
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let watcher = {
        // Mirror the job's flag into the one `review::run` polls.
        let (job, cancel) = (job.clone(), cancel.clone());
        tokio::spawn(async move {
            while !job.cancel.load(Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            cancel.store(true, Ordering::SeqCst);
        })
    };
    let lc0 = config.kind == EngineKind::Lc0;
    let result = review::run(&engine, lc0, start_fen, moves, depth, cancel, |done, total| {
        let _ = ReviewProgress { review_id: review_id.to_string(), done, total }.emit(app);
    })
    .await;
    watcher.abort();
    result
}

#[tauri::command]
#[specta::specta]
pub fn review_cancel(state: State<'_, AppState>, review_id: String) {
    if let Some(job) = state.reviews.lock().unwrap().get(&review_id) {
        job.cancel.store(true, Ordering::SeqCst);
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
