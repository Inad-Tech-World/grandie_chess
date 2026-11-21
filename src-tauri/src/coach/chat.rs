//! "Ask the coach": a conversation about the position on the board. The model
//! answers with tools that run the coach's engine and the rules, so every
//! evaluation it quotes is real, and it can play lines on the board with
//! `show_line` while it talks.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shakmaty::{Color, Position};
use specta::Type;

use super::engine::CoachDef;
use super::facts::{color_name, eval_words, hanging, line_facts, material_for, phase, pinned};
use super::script::CoachScript;
use super::MoveContext;
use crate::ai::agent::{ToolFuture, ToolHost};
use crate::ai::openrouter::Tool;
use crate::error::{Error, Result};
use crate::game::position::{fen_of, parse_fen, parse_san, parse_uci, san_of, uci_of};
use crate::state::AppState;

/// One earlier turn of the conversation.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatTurn {
    /// "user" or "coach".
    pub from: String,
    pub text: String,
}

/// What the coach knows about the board when asked.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AskContext {
    pub fen: String,
    /// Recent moves of the game, in SAN, oldest first.
    pub recent_moves: Vec<String>,
    /// e.g. "Tal vs Botvinnik, World Championship 1960" or "your game".
    pub game: Option<String>,
    /// The player the user is studying (Masters).
    pub player: Option<String>,
}

/// Callbacks from tools to the UI.
pub trait ChatSink: Send + Sync {
    /// A line to demonstrate on the board.
    fn script(&self, script: CoachScript);
}

pub struct CoachTools<'a> {
    pub state: &'a AppState,
    pub coach: CoachDef,
    pub context: AskContext,
    pub sink: Arc<dyn ChatSink>,
    pub request_id: String,
    pub cancelled: Arc<AtomicBool>,
    /// Extra tools (e.g. the masters database), added by the caller.
    pub extra: Option<Box<dyn ExtraTools + 'a>>,
    shown: Mutex<u32>,
}

/// More tools mixed into the coach's (see `masters`).
pub trait ExtraTools: Send + Sync {
    fn tools(&self) -> Vec<Tool>;
    /// `None` when the tool isn't one of these.
    fn call<'a>(&'a self, name: &'a str, args: Value) -> Option<ToolFuture<'a>>;
}

impl<'a> CoachTools<'a> {
    pub fn new(
        state: &'a AppState,
        coach: CoachDef,
        context: AskContext,
        sink: Arc<dyn ChatSink>,
        request_id: String,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        CoachTools { state, coach, context, sink, request_id, cancelled, extra: None, shown: Mutex::new(0) }
    }

    fn fen_arg(&self, args: &Value) -> Result<String> {
        let fen = args.get("fen").and_then(Value::as_str).filter(|f| !f.trim().is_empty()).unwrap_or(&self.context.fen);
        Ok(fen_of(&parse_fen(fen)?))
    }

    async fn analyse_position(&self, args: Value) -> Result<Value> {
        let fen = self.fen_arg(&args)?;
        let lines = args.get("lines").and_then(Value::as_u64).unwrap_or(3).clamp(1, 5) as u32;
        let pos = parse_fen(&fen)?;
        let an = self.state.coach.analyse(self.state, &self.coach, &fen, lines).await?;
        let mover = pos.turn();
        let pov = |w: f64| if mover == Color::White { w } else { 100.0 - w };
        Ok(json!({
            "fen": fen,
            "sideToMove": color_name(mover),
            "lines": an.lines.iter().map(|l| json!({
                "moves": l.san.iter().take(8).collect::<Vec<_>>(),
                "sideToMoveWinPercent": l.white_win.map(|w| pov(w).round()),
                "assessment": l.white_win.map(|w| eval_words(pov(w))),
                "score": l.score,
            })).collect::<Vec<_>>(),
        }))
    }

    async fn evaluate_move(&self, args: Value) -> Result<Value> {
        let fen = self.fen_arg(&args)?;
        let pos = parse_fen(&fen)?;
        let mv = args.get("move").and_then(Value::as_str).ok_or_else(|| Error::other("`move` is required"))?;
        let m = parse_san(&pos, mv).or_else(|_| parse_uci(&pos, mv)).map_err(|_| Error::Chess(format!("{mv} isn't a legal move here")))?;
        let san = san_of(&pos, m);
        let ctx = MoveContext::from_engine(self.state, &self.coach, &fen, &uci_of(m)).await?;
        let mut after = pos.clone();
        after.play_unchecked(m);
        Ok(json!({
            "move": san,
            "verdict": format!("{:?}", ctx.class).to_lowercase(),
            "moverWinPercentWithBestMove": ctx.win_before.round(),
            "moverWinPercentAfterThisMove": ctx.win_after.round(),
            "assessmentAfter": eval_words(ctx.win_after),
            "bestLine": ctx.best_line.iter().take(8).collect::<Vec<_>>(),
            "expectedReplyLine": ctx.reply_line.iter().take(8).collect::<Vec<_>>(),
            "replyLineFacts": line_facts(&after, &ctx.reply_line),
        }))
    }

    fn position_facts(&self, args: Value) -> Result<Value> {
        let fen = self.fen_arg(&args)?;
        let pos = parse_fen(&fen)?;
        let opening = crate::game::openings::OpeningBook::get().lookup(&pos).map(|o| format!("{} {}", o.eco, o.name));
        Ok(json!({
            "sideToMove": color_name(pos.turn()),
            "phase": phase(&pos),
            "materialForWhite": material_for(&pos, Color::White),
            "inCheck": pos.is_check(),
            "hanging": { "white": hanging(&pos, Color::White), "black": hanging(&pos, Color::Black) },
            "pinned": { "white": pinned(&pos, Color::White), "black": pinned(&pos, Color::Black) },
            "opening": opening,
        }))
    }

    fn show_line(&self, args: Value) -> Result<Value> {
        let fen = self.fen_arg(&args)?;
        let moves: Vec<String> = args
            .get("moves")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|m| m.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        if moves.is_empty() {
            return Err(Error::other("`moves` must list at least one move"));
        }
        let narration: Vec<String> = args
            .get("narration")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|m| m.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let title = args.get("title").and_then(Value::as_str).unwrap_or("The coach shows a line");
        let n = {
            let mut shown = self.shown.lock().unwrap();
            *shown += 1;
            *shown
        };
        let (script, played) = super::demo_script(format!("{}-{n}", self.request_id), &self.coach.id, &fen, &moves, &narration, title)?;
        if played == 0 {
            return Err(Error::Chess(format!("{} isn't legal in this position", moves[0])));
        }
        self.sink.script(script);
        Ok(json!({ "shown": true, "movesPlayed": played, "stoppedAt": moves.get(played) }))
    }
}

impl ToolHost for CoachTools<'_> {
    fn tools(&self) -> Vec<Tool> {
        let fen = json!({ "type": "string", "description": "FEN of the position; omit for the position on the board" });
        let mut tools = vec![
            Tool::function(
                "analyse_position",
                "Run the chess engine on a position. Returns the best lines with winning chances. Call this before giving any evaluation or naming a best move.",
                json!({ "type": "object", "properties": { "fen": fen, "lines": { "type": "integer", "description": "how many lines (1-5)" } } }),
            ),
            Tool::function(
                "evaluate_move",
                "Check one candidate move with the engine: how good it is, the best alternative, and the expected reply line.",
                json!({ "type": "object", "properties": { "move": { "type": "string", "description": "the move in SAN (e.g. Nxe5) or UCI" }, "fen": fen }, "required": ["move"] }),
            ),
            Tool::function(
                "position_facts",
                "Facts about a position from the rules: material, pieces left hanging, pins, phase and opening name.",
                json!({ "type": "object", "properties": { "fen": fen } }),
            ),
            Tool::function(
                "show_line",
                "Play a line on the student's board while you talk, then put the pieces back. Use it to demonstrate ideas. Only use moves you got from the engine tools.",
                json!({
                    "type": "object",
                    "properties": {
                        "moves": { "type": "array", "items": { "type": "string" }, "description": "SAN moves from the position" },
                        "narration": { "type": "array", "items": { "type": "string" }, "description": "optional: first an introduction, then one short comment per move" },
                        "title": { "type": "string" },
                        "fen": fen
                    },
                    "required": ["moves"]
                }),
            ),
        ];
        if let Some(extra) = &self.extra {
            tools.extend(extra.tools());
        }
        tools
    }

    fn call<'b>(&'b self, name: &'b str, args: Value) -> ToolFuture<'b> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Box::pin(async { Err(Error::Cancelled) });
        }
        if let Some(extra) = &self.extra {
            if let Some(fut) = extra.call(name, args.clone()) {
                return fut;
            }
        }
        Box::pin(async move {
            match name {
                "analyse_position" => self.analyse_position(args).await,
                "evaluate_move" => self.evaluate_move(args).await,
                "position_facts" => self.position_facts(args),
                "show_line" => self.show_line(args),
                other => Err(Error::other(format!("unknown tool {other}"))),
            }
        })
    }
}

/// The system prompt for the conversation.
pub fn system_prompt(coach: &CoachDef, language: &str, student: &str, extra: Option<&str>) -> String {
    format!(
        "{persona}\n\nYou are coaching {student} in a chess training app. The student sees a board; your words are also read aloud, \
         so write the way you'd talk: short paragraphs, no markdown headings, tables or bullet lists, and keep answers under about 150 words \
         unless asked for more. Answer in {language}.\n\
         Rules: never state an evaluation, a best move or a tactic without first checking it with analyse_position or evaluate_move. \
         Use show_line to demonstrate on the board whenever a line makes the point clearer (the student loves seeing the pieces move). \
         Moves are in SAN. If a tool says a move is illegal, say so rather than guessing.{extra}",
        persona = coach.persona,
        extra = extra.map(|e| format!("\n{e}")).unwrap_or_default()
    )
}

/// The user's message with the board context.
pub fn user_message(ctx: &AskContext, question: &str) -> String {
    let side = parse_fen(&ctx.fen).map(|p| color_name(p.turn()).to_string()).unwrap_or_default();
    let mut s = format!("Board position (FEN): {}\nSide to move: {side}\n", ctx.fen);
    if !ctx.recent_moves.is_empty() {
        s.push_str(&format!("Recent moves: {}\n", ctx.recent_moves.join(" ")));
    }
    if let Some(g) = &ctx.game {
        s.push_str(&format!("Game: {g}\n"));
    }
    if let Some(p) = &ctx.player {
        s.push_str(&format!("Player being studied: {p}\n"));
    }
    s.push_str(&format!("\nQuestion: {question}"));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_carries_the_board() {
        let ctx =
            AskContext { fen: crate::game::position::START_FEN.into(), recent_moves: vec![], game: Some("your game".into()), player: None };
        let m = user_message(&ctx, "What should I play?");
        assert!(m.contains("Side to move: White") && m.contains("Game: your game") && m.ends_with("What should I play?"));
        let coach = super::super::engine::coach_defs().remove(0);
        let p = system_prompt(&coach, "English", "a club player", None);
        assert!(p.contains("analyse_position") && p.contains("show_line") && p.contains("English"));
    }
}
