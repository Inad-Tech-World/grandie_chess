//! The masters library: players, downloads, profiles and study modes.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use super::{blocking, blocking_state, emit_download};
use crate::ai::store::{self, Purpose};
use crate::analysis::review::{ReviewDepth, ReviewResult};
use crate::coach::script::CoachScript;
use crate::coach::MoveContext;
use crate::db::games::{self as dbgames};
use crate::db::import::ImportSummary;
use crate::error::{Error, Result};
use crate::events::ImportProgress;
use crate::game::position::{Side, START_FEN};
use crate::masters::profile::{self, PlayerStats, StyleMetrics};
use crate::masters::study::{self, GuessResult};
use crate::masters::{self, LegendGame, MasterPlayer};
use crate::net::download::{download_file, Expect};
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub async fn masters_players(app: AppHandle) -> Result<Vec<MasterPlayer>> {
    blocking_state(&app, |state| masters::players(&state.db)).await
}

/// The bundled famous games (imported on first use).
#[tauri::command]
#[specta::specta]
pub async fn masters_legends(app: AppHandle) -> Result<Vec<LegendGame>> {
    blocking_state(&app, |state| masters::legends(&state.db)).await
}

fn emit_import(app: &AppHandle, job_id: &str, phase: &str, s: &ImportSummary) {
    let _ = ImportProgress { job_id: job_id.into(), phase: phase.into(), seen: s.seen, imported: s.imported, bytes: 0.0 }.emit(app);
}

/// Download a player's games from PGN Mentor and add them to the library.
/// Progress: `DownloadProgress` with id `masters:<id>`, then `ImportProgress`
/// with the same job id.
#[tauri::command]
#[specta::specta]
pub async fn masters_download(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<ImportSummary> {
    let url = masters::pgnmentor_url(&id)?;
    let job = format!("masters:{id}");
    let Some(_guard) = state.begin_download(&job) else {
        return Err(Error::other("These games are already downloading"));
    };
    let zip = state.paths.data_dir.join(format!("pgnmentor-{id}.zip"));
    let downloaded = download_file(&state.http, &url, &zip, Expect::default(), |r, t| emit_download(&app, &job, r, t, false, None)).await;
    if let Err(e) = downloaded {
        emit_download(&app, &job, 0, None, true, Some(e.to_string()));
        return Err(e);
    }
    emit_download(&app, &job, 1, Some(1), true, None);
    let (app2, id2, job2, zip2) = (app.clone(), id.clone(), job.clone(), zip.clone());
    let result = blocking_state(&app, move |state| {
        masters::import_zip(&state.db, &id2, &zip2, |s| emit_import(&app2, &job2, "Adding games to the library", s))
    })
    .await;
    let _ = std::fs::remove_file(&zip);
    result
}

/// Import PGN files as one master's games (e.g. exported from ChessBase).
#[tauri::command]
#[specta::specta]
pub async fn masters_import_files(app: AppHandle, job_id: String, paths: Vec<String>, player: String) -> Result<ImportSummary> {
    let app2 = app.clone();
    blocking_state(&app, move |state| {
        let mut total = ImportSummary::default();
        for path in paths {
            let file = std::fs::File::open(&path)?;
            let s = masters::import_pgn_as_master(&state.db, std::io::BufReader::new(file), &player, |s| {
                emit_import(&app2, &job_id, "Importing", s)
            })?;
            total.merge(s);
        }
        Ok(total)
    })
    .await
}

/// Remove a player's games from the library.
#[tauri::command]
#[specta::specta]
pub async fn masters_remove(app: AppHandle, id: String) -> Result<u32> {
    blocking_state(&app, move |state| masters::remove_player(&state.db, &id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn masters_profile(app: AppHandle, id: String) -> Result<PlayerStats> {
    blocking_state(&app, move |state| profile::stats(&state.db, &id)).await
}

/// Measure a master's style with the coach's engine over a sample of their
/// decisive games. Progress: `ReviewProgress` with review id `style:<id>`
/// (one step per game); cancel with `review_cancel`.
#[tauri::command]
#[specta::specta]
pub async fn masters_analyse_style(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    games: u32,
    coach_id: Option<String>,
) -> Result<StyleMetrics> {
    let coach = state.coach.resolve(&state, coach_id.as_deref())?;
    let sample = {
        let (pool, id) = (state.db.clone(), id.clone());
        let n = games.clamp(1, 40);
        blocking(move || {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT g.id, COALESCE(g.start_fen, ''), g.moves, s.color FROM game_subjects s JOIN games g ON g.id = s.game_id
                 WHERE s.subject = ?1 AND s.color IS NOT NULL AND g.result IN ('1-0', '0-1') AND g.ply_count BETWEEN 30 AND 160
                 ORDER BY (g.id * 7919) % 1000 LIMIT ?2",
            )?;
            let rows = stmt
                .query_map(rusqlite::params![id, n], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await?
    };
    if sample.is_empty() {
        return Err(Error::other("There are no decisive games of this player to analyse yet."));
    }
    let style_id = format!("style:{id}");
    // A job under the style id makes `review_cancel("style:<id>")` work: a
    // watcher passes the cancel on to whichever game is being reviewed.
    let job = std::sync::Arc::new(crate::state::ReviewJob::default());
    state.reviews.lock().unwrap().insert(style_id.clone(), job.clone());
    let watcher = {
        let (app, job, prefix) = (app.clone(), job.clone(), format!("{style_id}:"));
        tokio::spawn(async move {
            use tauri::Manager;
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if job.cancel.load(std::sync::atomic::Ordering::SeqCst) {
                    let state = app.state::<AppState>();
                    for (key, j) in state.reviews.lock().unwrap().iter() {
                        if key.starts_with(&prefix) {
                            j.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                }
            }
        })
    };
    let mut total = StyleMetrics { engine: coach.name.clone(), ..Default::default() };
    for (i, (_, fen, moves, color)) in sample.iter().enumerate() {
        let start = if fen.is_empty() { START_FEN.to_string() } else { fen.clone() };
        let moves: Vec<String> = moves.split_whitespace().map(str::to_string).collect();
        let review = super::review::review_game(
            &app,
            &state,
            &format!("{style_id}:{i}"),
            &start,
            &moves,
            ReviewDepth::Quick,
            Some(&coach.engine_id),
        )
        .await;
        let _ = crate::events::ReviewProgress { review_id: style_id.clone(), done: i as u32 + 1, total: sample.len() as u32 }.emit(&app);
        match review {
            Ok(r) => study::add_to_style(&mut total, &r, if color == "white" { Side::White } else { Side::Black }),
            Err(e) if e.kind() == crate::error::ErrorKind::Cancelled => break,
            Err(e) => log::warn!("style analysis skipped a game: {e}"),
        }
        if job.cancel.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
    }
    watcher.abort();
    state.reviews.lock().unwrap().remove(&style_id);
    let (pool, id2, metrics) = (state.db.clone(), id.clone(), total.clone());
    blocking(move || {
        let mut p = profile::stored(&pool, &id2)?;
        p.style = Some(metrics);
        profile::save(&pool, &id2, &p)
    })
    .await?;
    Ok(total)
}

/// The coach's write-up of a master's style (needs an OpenRouter key).
/// Cached until more games are added.
#[tauri::command]
#[specta::specta]
pub async fn masters_style_report(app: AppHandle, state: State<'_, AppState>, id: String, refresh: bool) -> Result<String> {
    let stats = {
        let id = id.clone();
        blocking_state(&app, move |state| profile::stats(&state.db, &id)).await?
    };
    let stored = {
        let (pool, id) = (state.db.clone(), id.clone());
        blocking(move || profile::stored(&pool, &id)).await?
    };
    if let (Some(report), false) = (&stored.report, refresh) {
        if stored.report_games == stats.games {
            return Ok(report.clone());
        }
    }
    let client = state.openrouter().await?;
    let settings = state.settings();
    let mut for_prompt = stats.clone();
    for_prompt.report = None;
    let req = crate::ai::ChatRequest {
        model: settings.ai.explain_model.clone(),
        models: vec![settings.ai.narrate_model.clone()],
        messages: vec![
            crate::ai::Message::system("You are a chess historian and coach. You only state facts supported by the data you are given."),
            crate::ai::Message::user(study::style_report_prompt(&for_prompt, &settings.coach.language)),
        ],
        temperature: Some(0.5),
        max_tokens: Some(700),
        ..Default::default()
    };
    let completion = client.chat(req).await?;
    let report = completion.text().trim().to_string();
    let (pool, id2, report2, games, usage, model) =
        (state.db.clone(), id.clone(), report.clone(), stats.games, completion.usage, completion.model.clone());
    blocking(move || {
        let mut p = profile::stored(&pool, &id2)?;
        p.report = Some(report2);
        p.report_games = games;
        profile::save(&pool, &id2, &p)?;
        store::record(&*pool.get()?, Purpose::PlayerStyle, &model, &usage, crate::db::now_secs())
    })
    .await?;
    Ok(report)
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WalkthroughRequest {
    pub game_id: i32,
    /// The master to follow (catalogue id); their moves are the focus.
    pub subject: Option<String>,
    pub coach_id: Option<String>,
}

/// A guided replay of a master game: the coach plays through it and stops
/// at the moments that show the master's ideas. Reviews the game first if
/// needed (progress: `ReviewProgress` with id `walk:<gameId>`).
#[tauri::command]
#[specta::specta]
pub async fn masters_walkthrough(app: AppHandle, state: State<'_, AppState>, request: WalkthroughRequest) -> Result<CoachScript> {
    let coach = state.coach.resolve(&state, request.coach_id.as_deref())?;
    let game_id = request.game_id;
    let game = blocking_state(&app, move |state| dbgames::get(&state.db, game_id)).await?;
    let tree = &game.tree;
    let start_fen = tree.root().fen.clone();
    let moves: Vec<String> = tree.mainline().iter().filter_map(|&i| tree.nodes[i as usize].uci.clone()).collect();
    let review: ReviewResult = match game.review {
        Some(r) if r.moves.len() == moves.len() => r,
        _ => {
            let r = super::review::review_game(
                &app,
                &state,
                &format!("walk:{game_id}"),
                &start_fen,
                &moves,
                ReviewDepth::Quick,
                Some(&coach.engine_id),
            )
            .await?;
            let (pool, saved) = (state.db.clone(), r.clone());
            let _ = blocking(move || dbgames::save_review(&pool, game_id, &saved)).await;
            r
        }
    };
    let (master, master_name) = match &request.subject {
        Some(subject) => {
            let (pool, subject, gid) = (state.db.clone(), subject.clone(), game_id);
            let color: Option<String> = blocking(move || {
                Ok(pool
                    .get()?
                    .query_row(
                        "SELECT color FROM game_subjects WHERE game_id = ?1 AND subject = ?2",
                        rusqlite::params![gid, subject],
                        |r| r.get(0),
                    )
                    .ok()
                    .flatten())
            })
            .await?;
            let side = color.as_deref().map(|c| if c == "white" { Side::White } else { Side::Black });
            let name = match side {
                Some(Side::White) => game.row.white.clone(),
                Some(Side::Black) => game.row.black.clone(),
                None => None,
            };
            (side, name.unwrap_or_else(|| request.subject.clone().unwrap_or_default()))
        }
        None => (None, "both players".to_string()),
    };
    let year = game.row.date.as_deref().and_then(|d| d.get(..4)).unwrap_or("").trim_matches('?').to_string();
    let title = format!(
        "{} vs {}{}",
        game.row.white.clone().unwrap_or_default(),
        game.row.black.clone().unwrap_or_default(),
        if year.is_empty() { String::new() } else { format!(", {year}") }
    );
    let (script, ctx) = study::walkthrough_script(
        format!("walk-{game_id}"),
        &coach,
        &start_fen,
        &moves,
        &review,
        master,
        &master_name,
        &title,
        &game.row.result,
    )?;
    Ok(super::coach::narrate_public(&state, &coach, script, ctx).await)
}

/// Score a guess in "Guess the move" and explain the master's choice.
#[tauri::command]
#[specta::specta]
pub async fn masters_guess(
    state: State<'_, AppState>,
    fen: String,
    guess: String,
    master_move: String,
    master_name: String,
    coach_id: Option<String>,
) -> Result<GuessResult> {
    let coach = state.coach.resolve(&state, coach_id.as_deref())?;
    let master = MoveContext::from_engine(&state, &coach, &fen, &master_move).await?;
    let guess_ctx = if guess == master_move { master.clone() } else { MoveContext::from_engine(&state, &coach, &fen, &guess).await? };
    let (mut result, ctx) =
        study::guess_script(format!("guess-{}", chrono::Utc::now().timestamp_millis()), &coach, &fen, &guess_ctx, &master, &master_name)?;
    if let (Some(script), Some(ctx)) = (result.script.take(), ctx) {
        result.script = Some(super::coach::narrate_public(&state, &coach, script, ctx).await);
    }
    Ok(result)
}
