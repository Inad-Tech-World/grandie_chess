//! Repertoire drills, puzzles and progress.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use super::{blocking_state, emit_download};
use crate::error::{Error, Result};
use crate::events::ImportProgress;
use crate::game::pgn::{parse_pgn, write_pgn};
use crate::game::position::Side;
use crate::net::download::{download_file, Expect};
use crate::state::AppState;
use crate::training::progress::ProgressSummary;
use crate::training::puzzles::{self, MoveVerdict, Puzzle, PuzzleImportFilter, PuzzleResult, ThemeCount};
use crate::training::repertoire::{self, DrillItem, RepMove, RepertoireStats};
use crate::training::srs::{CardState, Grade};

#[tauri::command]
#[specta::specta]
pub async fn repertoire_moves(app: AppHandle, color: Side) -> Result<Vec<RepMove>> {
    blocking_state(&app, move |state| repertoire::moves(&*state.db.get()?, color)).await
}

#[tauri::command]
#[specta::specta]
pub async fn repertoire_add_line(app: AppHandle, color: Side, moves: Vec<String>) -> Result<u32> {
    blocking_state(&app, move |state| repertoire::add_line(&mut *state.db.get()?, color, &moves, crate::db::now_secs())).await
}

#[tauri::command]
#[specta::specta]
pub async fn repertoire_remove_move(app: AppHandle, id: i32) -> Result<()> {
    blocking_state(&app, move |state| repertoire::remove_move(&mut *state.db.get()?, id, crate::db::now_secs())).await
}

#[tauri::command]
#[specta::specta]
pub async fn repertoire_set_comment(app: AppHandle, id: i32, comment: Option<String>) -> Result<()> {
    blocking_state(&app, move |state| repertoire::set_comment(&*state.db.get()?, id, comment.as_deref())).await
}

#[tauri::command]
#[specta::specta]
pub async fn repertoire_import_pgn(app: AppHandle, color: Side, pgn: String) -> Result<u32> {
    blocking_state(&app, move |state| {
        let parsed = parse_pgn(&pgn);
        let mut conn = state.db.get()?;
        let mut added = 0;
        for tree in &parsed.games {
            added += repertoire::add_tree(&mut conn, color, tree, crate::db::now_secs())?;
        }
        Ok(added)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn repertoire_export_pgn(app: AppHandle, color: Side) -> Result<String> {
    blocking_state(&app, move |state| Ok(write_pgn(&repertoire::to_tree(&*state.db.get()?, color)?))).await
}

#[tauri::command]
#[specta::specta]
pub async fn repertoire_stats(app: AppHandle, color: Side) -> Result<RepertoireStats> {
    blocking_state(&app, move |state| repertoire::stats(&*state.db.get()?, color, crate::db::now_secs())).await
}

/// Cards to drill now: due reviews first, then up to `new_limit` new ones.
#[tauri::command]
#[specta::specta]
pub async fn repertoire_due(app: AppHandle, color: Option<Side>, limit: u32, new_limit: u32) -> Result<Vec<DrillItem>> {
    blocking_state(&app, move |state| repertoire::due(&*state.db.get()?, color, crate::db::now_secs(), limit, new_limit)).await
}

#[tauri::command]
#[specta::specta]
pub async fn repertoire_grade(app: AppHandle, color: Side, key: String, grade: Grade) -> Result<CardState> {
    blocking_state(&app, move |state| repertoire::grade(&*state.db.get()?, color, &key, grade, crate::db::now_secs())).await
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PuzzleInfo {
    pub count: u32,
    pub rating: crate::training::glicko::Rating,
}

#[tauri::command]
#[specta::specta]
pub async fn puzzles_info(app: AppHandle) -> Result<PuzzleInfo> {
    blocking_state(&app, move |state| {
        Ok(PuzzleInfo { count: puzzles::count(&state.puzzles)?, rating: puzzles::rating(&*state.db.get()?)? })
    })
    .await
}

/// Download the Lichess puzzle database and import it. With `filter` set,
/// only well-tested, popular puzzles are kept (a third of the database).
#[tauri::command]
#[specta::specta]
pub async fn puzzles_download(app: AppHandle, state: State<'_, AppState>, filter: Option<PuzzleImportFilter>) -> Result<u32> {
    let id = "puzzles";
    let Some(_guard) = state.begin_download(id) else {
        return Err(Error::other("The puzzle database is already downloading"));
    };
    let archive = state.paths.data_dir.join("lichess_db_puzzle.csv.zst");
    let result: Result<u32> = async {
        download_file(&state.http, puzzles::PUZZLE_DB_URL, &archive, Expect::default(), |r, t| emit_download(&app, id, r, t, false, None))
            .await?;
        let pool = state.puzzles.clone();
        let path = archive.clone();
        let app2 = app.clone();
        let kept = super::blocking(move || -> Result<u64> {
            let file = std::fs::File::open(&path)?;
            let decoder = zstd::Decoder::new(std::io::BufReader::new(file))?;
            puzzles::import_csv(&pool, decoder, filter, |read, kept| {
                let _ = ImportProgress {
                    job_id: "puzzles".into(),
                    phase: "Importing puzzles".into(),
                    seen: read.min(u32::MAX as u64) as u32,
                    imported: kept.min(u32::MAX as u64) as u32,
                    bytes: 0.0,
                }
                .emit(&app2);
            })
        })
        .await?;
        Ok(kept as u32)
    }
    .await;
    let _ = std::fs::remove_file(&archive);
    emit_download(&app, id, 0, None, true, result.as_ref().err().map(|e| e.to_string()));
    result
}

#[tauri::command]
#[specta::specta]
pub async fn puzzles_themes(app: AppHandle) -> Result<Vec<ThemeCount>> {
    blocking_state(&app, move |state| puzzles::themes(&state.puzzles)).await
}

/// The next puzzle near the user's rating (or `rating` if given).
#[tauri::command]
#[specta::specta]
pub async fn puzzle_next(app: AppHandle, theme: Option<String>, rating: Option<f64>) -> Result<Option<Puzzle>> {
    blocking_state(&app, move |state| {
        let conn = state.db.get()?;
        let target = rating.unwrap_or(puzzles::rating(&conn)?.rating);
        let recent = puzzles::recent_ids(&conn, 300)?;
        puzzles::next(&state.puzzles, target, theme.as_deref(), &recent)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn puzzle_check(app: AppHandle, puzzle_id: String, index: u32, uci: String) -> Result<MoveVerdict> {
    blocking_state(&app, move |state| puzzles::check_move(&puzzles::get(&state.puzzles, &puzzle_id)?, index as usize, &uci)).await
}

/// Record the outcome of a puzzle and update the puzzle rating.
#[tauri::command]
#[specta::specta]
pub async fn puzzle_finish(app: AppHandle, puzzle_id: String, solved: bool) -> Result<PuzzleResult> {
    blocking_state(&app, move |state| {
        let p = puzzles::get(&state.puzzles, &puzzle_id)?;
        puzzles::record_attempt(&*state.db.get()?, &p, solved, crate::db::now_secs())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn progress_summary(app: AppHandle) -> Result<ProgressSummary> {
    blocking_state(&app, move |state| crate::training::progress::summary(&*state.db.get()?, &state.catalog, crate::db::now_secs())).await
}

// ---- learn from your mistakes ----

/// Turn the user's mistakes in a reviewed game into practice cards.
#[tauri::command]
#[specta::specta]
pub async fn mistakes_add(app: AppHandle, game_id: Option<i32>, review: crate::analysis::review::ReviewResult, user: Side) -> Result<u32> {
    blocking_state(&app, move |state| {
        crate::training::mistakes::add_from_review(&*state.db.get()?, game_id, &review, user, crate::db::now_secs())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn mistakes_due(app: AppHandle, limit: u32) -> Result<Vec<crate::training::mistakes::MistakeCard>> {
    blocking_state(&app, move |state| crate::training::mistakes::due(&*state.db.get()?, crate::db::now_secs(), limit)).await
}

#[tauri::command]
#[specta::specta]
pub async fn mistakes_stats(app: AppHandle) -> Result<crate::training::mistakes::MistakeStats> {
    blocking_state(&app, |state| crate::training::mistakes::stats(&*state.db.get()?, crate::db::now_secs())).await
}

#[tauri::command]
#[specta::specta]
pub async fn mistakes_grade(app: AppHandle, id: i32, grade: Grade) -> Result<CardState> {
    blocking_state(&app, move |state| crate::training::mistakes::grade(&*state.db.get()?, id, grade, crate::db::now_secs())).await
}

#[tauri::command]
#[specta::specta]
pub async fn mistakes_remove(app: AppHandle, id: i32) -> Result<()> {
    blocking_state(&app, move |state| crate::training::mistakes::remove(&*state.db.get()?, id)).await
}
