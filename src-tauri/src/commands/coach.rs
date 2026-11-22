//! The talking coach: scripts that demonstrate moves on the board, and the
//! "Ask the coach" conversation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_specta::Event;

use crate::ai::store::{self, Purpose};
use crate::analysis::classify::MoveClass;
use crate::analysis::review::ReviewResult;
use crate::coach::chat::{self, AskContext, ChatTurn, CoachTools};
use crate::coach::engine::{CoachDef, CoachInfo};
use crate::coach::narrate::{self, NarrationContext};
use crate::coach::script::{CoachScript, CoachStep, SayRole};
use crate::coach::{self, MoveContext};
use crate::error::{Error, ErrorKind, Result};
use crate::events::{CoachEvent, CoachEventKind, TauriChatSink};
use crate::game::position::{fen_of, parse_fen, parse_uci};
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn coach_list(state: State<'_, AppState>) -> Vec<CoachInfo> {
    state.coach.list(&state)
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", chrono::Utc::now().timestamp_millis())
}

/// What the student is like, for the coach's wording.
async fn student(state: &AppState) -> String {
    let pool = state.db.clone();
    let rating = crate::commands::blocking(move || crate::training::puzzles::rating(&*pool.get()?).map(|r| r.rating)).await.ok();
    narrate::describe_student(rating)
}

/// Let the AI rewrite the script's words when it's switched on. Failures
/// keep the template words and leave a notice for the UI.
async fn narrate_script(state: &AppState, coach: &CoachDef, mut script: CoachScript, mut ctx: NarrationContext) -> CoachScript {
    if !state.ai_ready().await {
        return script;
    }
    let settings = state.settings();
    ctx.student = student(state).await;
    let model = settings.ai.narrate_model.clone();
    let drafts: Vec<&str> = script.says().map(|(_, _, t)| t).chain(std::iter::once(script.start_fen.as_str())).collect();
    let verbosity = format!("{:?}", settings.coach.verbosity);
    let mut key_parts = vec!["narrate-v1", model.as_str(), coach.id.as_str(), verbosity.as_str(), settings.coach.language.as_str()];
    key_parts.extend(drafts.iter().copied());
    let key = store::cache_key(&key_parts);
    let pool = state.db.clone();
    let cached = {
        let (pool, key) = (pool.clone(), key.clone());
        crate::commands::blocking(move || store::cache_get(&*pool.get()?, &key)).await.ok().flatten()
    };
    if let Some(json) = cached {
        if let Ok(lines) = serde_json::from_str::<Vec<(String, String)>>(&json) {
            for (id, text) in lines {
                script.set_text(&id, &text);
            }
            script.ai_narrated = true;
            return script;
        }
    }
    let client = match state.openrouter().await {
        Ok(c) => c,
        Err(e) => {
            script.notice = Some(e.to_string());
            return script;
        }
    };
    match narrate::polish(&client, &model, coach, &settings.coach, &mut script, &ctx).await {
        Ok((usage, used_model, _)) => {
            let lines: Vec<(String, String)> = script.says().map(|(id, _, t)| (id.to_string(), t.to_string())).collect();
            let json = serde_json::to_string(&lines).unwrap_or_default();
            let now = crate::db::now_secs();
            let _ = crate::commands::blocking(move || {
                let conn = pool.get()?;
                store::record(&conn, Purpose::Narrate, &used_model, &usage, now)?;
                if !json.is_empty() {
                    store::cache_put(&conn, &key, &json, now)?;
                }
                Ok(())
            })
            .await;
        }
        Err(e) => {
            log::warn!("AI narration failed, using the built-in words: {e}");
            script.notice = Some(match e.kind() {
                ErrorKind::NoApiKey | ErrorKind::AiQuota => e.to_string(),
                _ => "The AI coach couldn't be reached, so the built-in explanation is used.".into(),
            });
        }
    }
    script
}

/// [`narrate_script`] for the other command modules.
pub async fn narrate_public(state: &AppState, coach: &CoachDef, script: CoachScript, ctx: NarrationContext) -> CoachScript {
    narrate_script(state, coach, script, ctx).await
}

/// Review data for a move, when the game has been reviewed (saves engine time).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KnownMove {
    pub best_line: Vec<String>,
    pub reply_line: Vec<String>,
    pub win_before: f64,
    pub win_after: f64,
    pub class: MoveClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplainMoveRequest {
    pub fen_before: String,
    pub last_move_before: Option<String>,
    /// The move to explain (UCI).
    pub played: String,
    pub known: Option<KnownMove>,
    /// Where the board goes afterwards; defaults to the position after the move.
    pub return_fen: Option<String>,
    pub return_last_move: Option<String>,
    pub coach_id: Option<String>,
}

/// Demonstrate why a move is good or bad, and what was better.
#[tauri::command]
#[specta::specta]
pub async fn coach_explain_move(state: State<'_, AppState>, request: ExplainMoveRequest) -> Result<CoachScript> {
    let coach = state.coach.resolve(&state, request.coach_id.as_deref())?;
    let ctx = match request.known {
        Some(k) if !k.best_line.is_empty() => MoveContext {
            fen_before: request.fen_before.clone(),
            last_move_before: request.last_move_before.clone(),
            played: request.played.clone(),
            best_line: k.best_line,
            reply_line: k.reply_line,
            win_before: k.win_before,
            win_after: k.win_after,
            class: k.class,
        },
        _ => {
            let mut ctx = MoveContext::from_engine(&state, &coach, &request.fen_before, &request.played).await?;
            ctx.last_move_before = request.last_move_before.clone();
            ctx
        }
    };
    let (return_fen, return_last) = match request.return_fen {
        Some(f) => (f, request.return_last_move),
        None => {
            let pos = parse_fen(&request.fen_before)?;
            let m = parse_uci(&pos, &request.played)?;
            let mut after = pos;
            shakmaty::Position::play_unchecked(&mut after, m);
            (fen_of(&after), Some(request.played.clone()))
        }
    };
    let (script, nctx) = coach::explain_move_script(new_id("move"), &coach, &ctx, &return_fen, return_last)?;
    Ok(narrate_script(&state, &coach, script, nctx).await)
}

/// The coach's verdict on a move just played (coach mode in games,
/// puzzles), with a demonstration when it was a mistake.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CoachVerdict {
    pub class: MoveClass,
    pub win_before: f64,
    pub win_after: f64,
    pub best_san: Option<String>,
    /// Present for inaccuracies, mistakes and blunders (or always, with `always_explain`).
    pub script: Option<CoachScript>,
}

#[tauri::command]
#[specta::specta]
pub async fn coach_check_move(
    state: State<'_, AppState>,
    fen_before: String,
    played: String,
    last_move_before: Option<String>,
    always_explain: bool,
    coach_id: Option<String>,
) -> Result<CoachVerdict> {
    let coach = state.coach.resolve(&state, coach_id.as_deref())?;
    let mut ctx = MoveContext::from_engine(&state, &coach, &fen_before, &played).await?;
    ctx.last_move_before = last_move_before.clone();
    let explain = always_explain || matches!(ctx.class, MoveClass::Inaccuracy | MoveClass::Mistake | MoveClass::Blunder);
    let script = if explain {
        let pos = parse_fen(&fen_before)?;
        let m = parse_uci(&pos, &played)?;
        let mut after = pos;
        shakmaty::Position::play_unchecked(&mut after, m);
        let (script, nctx) = coach::explain_move_script(new_id("check"), &coach, &ctx, &fen_of(&after), Some(played.clone()))?;
        Some(narrate_script(&state, &coach, script, nctx).await)
    } else {
        None
    };
    Ok(CoachVerdict {
        class: ctx.class,
        win_before: ctx.win_before,
        win_after: ctx.win_after,
        best_san: ctx.best_line.first().cloned(),
        script,
    })
}

/// Play an engine line on the board and explain where it leads.
#[tauri::command]
#[specta::specta]
pub async fn coach_explain_line(
    state: State<'_, AppState>,
    fen: String,
    last_move: Option<String>,
    line: Vec<String>,
    white_win: Option<f64>,
    coach_id: Option<String>,
) -> Result<CoachScript> {
    let coach = state.coach.resolve(&state, coach_id.as_deref())?;
    let (script, nctx) = coach::explain_line_script(new_id("line"), &coach, &fen, last_move, &line, white_win)?;
    Ok(narrate_script(&state, &coach, script, nctx).await)
}

/// Walk through a reviewed game's key moments, demonstrating each.
#[tauri::command]
#[specta::specta]
pub async fn coach_walk_game(
    state: State<'_, AppState>,
    review: ReviewResult,
    moves: Vec<String>,
    plies: Option<Vec<u32>>,
    coach_id: Option<String>,
) -> Result<CoachScript> {
    let coach = state.coach.resolve(&state, coach_id.as_deref())?;
    let mut plies = plies.unwrap_or_else(|| review.key_moments.clone());
    plies.sort_unstable();
    plies.dedup();
    plies.truncate(6);
    if plies.is_empty() {
        return Err(Error::other("This game has no key moments to walk through."));
    }
    let last_fen = review.evals.last().map(|e| e.fen.clone()).ok_or_else(|| Error::other("the review is empty"))?;
    let mut steps: Vec<CoachStep> = Vec::new();
    let mut allowed: Vec<String> = Vec::new();
    let mut moments = Vec::new();
    for (k, ply) in plies.iter().enumerate() {
        let i = (*ply as usize).saturating_sub(1);
        let (Some(mv), Some(before), Some(after)) = (review.moves.get(i), review.evals.get(i), review.evals.get(i + 1)) else { continue };
        let ctx = MoveContext {
            fen_before: before.fen.clone(),
            last_move_before: i.checked_sub(1).and_then(|j| moves.get(j).cloned()),
            played: mv.uci.clone(),
            best_line: before.line.clone(),
            reply_line: after.line.clone(),
            win_before: mv.win_before,
            win_after: mv.win_after,
            class: mv.class,
        };
        let (sub, nctx) = coach::explain_move_script(format!("m{k}"), &coach, &ctx, &after.fen, Some(mv.uci.clone()))?;
        steps.push(CoachStep::Reset { fen: sub.start_fen.clone(), last_move: sub.start_last_move.clone() });
        if k == 0 {
            steps.push(CoachStep::Say {
                id: "w0".into(),
                role: SayRole::Moment,
                text: if plies.len() == 1 {
                    "Let's look at the key moment of this game.".to_string()
                } else {
                    format!("Let's go through the {} key moments of this game.", plies.len())
                },
            });
        }
        for step in sub.steps {
            steps.push(match step {
                CoachStep::Say { id, role, text } => CoachStep::Say { id: format!("m{k}-{id}"), role, text },
                other => other,
            });
        }
        allowed.extend(nctx.allowed_moves.iter().cloned());
        moments.push(nctx.facts);
    }
    let first_fen = match steps.first() {
        Some(CoachStep::Reset { fen, .. }) => fen.clone(),
        _ => last_fen.clone(),
    };
    let script = CoachScript {
        id: new_id("walk"),
        coach_id: coach.id.clone(),
        title: "Key moments of the game".into(),
        start_fen: first_fen,
        start_last_move: None,
        return_fen: last_fen,
        return_last_move: moves.last().cloned(),
        steps,
        ai_narrated: false,
        notice: None,
    };
    let side =
        parse_fen(&script.start_fen).map(|p| crate::coach::facts::color_name(shakmaty::Position::turn(&p)).to_string()).unwrap_or_default();
    let nctx = NarrationContext {
        side_to_move: side,
        phase: String::new(),
        opening: None,
        facts: serde_json::json!({ "moments": moments }),
        allowed_moves: allowed,
        student: String::new(),
    };
    Ok(narrate_script(&state, &coach, script, nctx).await)
}

/// Ask the coach about the position. The answer streams as [`CoachEvent`]s
/// (text, tool use, and lines to show on the board); the full text is also
/// returned. Needs an OpenRouter key.
#[tauri::command]
#[specta::specta]
pub async fn coach_ask(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    context: AskContext,
    question: String,
    history: Vec<ChatTurn>,
    coach_id: Option<String>,
) -> Result<String> {
    let client = state.openrouter().await?;
    let coach = state.coach.resolve(&state, coach_id.as_deref())?;
    let settings = state.settings();
    let cancel = Arc::new(AtomicBool::new(false));
    state.coach_requests.lock().unwrap().insert(request_id.clone(), cancel.clone());
    let student = student(&state).await;
    let player_note = context.player.as_ref().map(|p| {
        format!("The student is studying {p}'s games; use find_master_games and player_profile to ground what you say about {p}.")
    });
    let mut messages =
        vec![crate::ai::Message::system(chat::system_prompt(&coach, &settings.coach.language, &student, player_note.as_deref()))];
    for turn in history.iter().rev().take(12).rev() {
        messages.push(if turn.from == "user" { crate::ai::Message::user(&turn.text) } else { crate::ai::Message::assistant(&turn.text) });
    }
    messages.push(crate::ai::Message::user(chat::user_message(&context, &question)));
    let sink = Arc::new(TauriChatSink { app: app.clone(), request_id: request_id.clone() });
    let mut tools = CoachTools::new(&state, coach.clone(), context.clone(), sink, request_id.clone(), cancel.clone());
    tools.extra = crate::masters::chat_tools(&state);
    let request = crate::ai::ChatRequest {
        model: settings.ai.explain_model.clone(),
        models: vec![settings.ai.narrate_model.clone()],
        messages,
        temperature: Some(0.4),
        max_tokens: Some(900),
        ..Default::default()
    };
    let emit = |kind: CoachEventKind| {
        let _ = CoachEvent { request_id: request_id.clone(), kind }.emit(&app);
    };
    let result = crate::ai::agent::run(
        &client,
        request,
        &tools,
        6,
        |e| match e {
            crate::ai::agent::AgentEvent::Text(text) => emit(CoachEventKind::Delta { text }),
            crate::ai::agent::AgentEvent::ToolStart(name) => emit(CoachEventKind::Tool { name }),
            crate::ai::agent::AgentEvent::ToolEnd(_) => {}
        },
        || !cancel.load(Ordering::SeqCst),
    )
    .await;
    state.coach_requests.lock().unwrap().remove(&request_id);
    let result = result?;
    let (pool, model, usage, now) = (state.db.clone(), result.model.clone(), result.usage, crate::db::now_secs());
    let _ = crate::commands::blocking(move || store::record(&*pool.get()?, Purpose::Explain, &model, &usage, now)).await;
    Ok(result.text)
}

#[tauri::command]
#[specta::specta]
pub fn coach_cancel(state: State<'_, AppState>, request_id: String) {
    if let Some(flag) = state.coach_requests.lock().unwrap().get(&request_id) {
        flag.store(true, Ordering::SeqCst);
    }
}
