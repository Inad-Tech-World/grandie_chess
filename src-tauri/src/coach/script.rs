//! Coach scripts: what the coach does on the board, step by step. Every step
//! carries the position to show, so the UI just plays them in order: it
//! animates moves, takes them back, draws arrows and speaks the lines.

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Position};
use specta::Type;

use crate::error::Result;
use crate::game::position::{fen_of, parse_fen, parse_san, san_of, uci_of};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CoachShape {
    pub orig: String,
    /// Arrow end; `None` draws a circle on `orig`.
    pub dest: Option<String>,
    /// "green", "red", "blue", "yellow" (chessground brushes).
    pub brush: String,
}

impl CoachShape {
    pub fn arrow(uci: &str, brush: &str) -> Option<Self> {
        (uci.len() >= 4).then(|| CoachShape { orig: uci[..2].into(), dest: Some(uci[2..4].into()), brush: brush.into() })
    }

    pub fn circle(square: &str, brush: &str) -> Self {
        CoachShape { orig: square.into(), dest: None, brush: brush.into() }
    }
}

/// What a spoken line is about; the narrator uses it to write the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SayRole {
    Intro,
    Played,
    Refutation,
    Consequence,
    Better,
    BetterLine,
    Summary,
    Idea,
    Moment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CoachStep {
    /// Speak (and caption) a line.
    #[serde(rename_all = "camelCase")]
    Say { id: String, role: SayRole, text: String },
    /// Play a move with animation.
    #[serde(rename_all = "camelCase")]
    Move { uci: String, san: String, fen: String },
    /// Take the last move back with animation.
    #[serde(rename_all = "camelCase")]
    Back { fen: String, last_move: Option<String> },
    /// Show a position directly (jumping between moments, or restoring).
    #[serde(rename_all = "camelCase")]
    Reset { fen: String, last_move: Option<String> },
    /// Replace the arrows and circles.
    #[serde(rename_all = "camelCase")]
    Shapes { shapes: Vec<CoachShape> },
    #[serde(rename_all = "camelCase")]
    Pause { ms: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CoachScript {
    pub id: String,
    pub coach_id: String,
    pub title: String,
    pub start_fen: String,
    pub start_last_move: Option<String>,
    /// Where the board goes when the script ends or is stopped.
    pub return_fen: String,
    pub return_last_move: Option<String>,
    pub steps: Vec<CoachStep>,
    /// True when an AI model wrote the words (false: built-in templates).
    pub ai_narrated: bool,
    /// Something the user should know, e.g. why AI narration wasn't used.
    pub notice: Option<String>,
}

impl CoachScript {
    pub fn says(&self) -> impl Iterator<Item = (&str, SayRole, &str)> {
        self.steps.iter().filter_map(|s| match s {
            CoachStep::Say { id, role, text } => Some((id.as_str(), *role, text.as_str())),
            _ => None,
        })
    }

    pub fn set_text(&mut self, id: &str, new_text: &str) {
        for s in &mut self.steps {
            if let CoachStep::Say { id: sid, text, .. } = s {
                if sid == id {
                    *text = new_text.to_string();
                }
            }
        }
    }
}

/// Pause after each demonstrated move, so the eye can follow.
pub const MOVE_PAUSE_MS: u32 = 450;

/// Builds a script while tracking the position shown on the board.
pub struct ScriptBuilder {
    pos: Chess,
    /// (position before, move uci) for each move played, to take moves back.
    history: Vec<(Chess, Option<String>)>,
    last_move: Option<String>,
    steps: Vec<CoachStep>,
    next_id: u32,
    start_fen: String,
    start_last_move: Option<String>,
}

impl ScriptBuilder {
    pub fn new(fen: &str, last_move: Option<String>) -> Result<Self> {
        let pos = parse_fen(fen)?;
        Ok(ScriptBuilder {
            start_fen: fen_of(&pos),
            pos,
            history: vec![],
            last_move: last_move.clone(),
            steps: vec![],
            next_id: 0,
            start_last_move: last_move,
        })
    }

    pub fn position(&self) -> &Chess {
        &self.pos
    }

    pub fn fen(&self) -> String {
        fen_of(&self.pos)
    }

    pub fn last_move(&self) -> Option<String> {
        self.last_move.clone()
    }

    pub fn say(&mut self, role: SayRole, text: impl Into<String>) -> String {
        self.next_id += 1;
        let id = format!("s{}", self.next_id);
        self.steps.push(CoachStep::Say { id: id.clone(), role, text: text.into() });
        id
    }

    pub fn shapes(&mut self, shapes: Vec<CoachShape>) {
        self.steps.push(CoachStep::Shapes { shapes });
    }

    pub fn clear_shapes(&mut self) {
        if !matches!(self.steps.last(), Some(CoachStep::Shapes { shapes }) if shapes.is_empty()) {
            self.shapes(vec![]);
        }
    }

    pub fn pause(&mut self, ms: u32) {
        self.steps.push(CoachStep::Pause { ms });
    }

    /// Play one SAN move; false (and nothing added) if it isn't legal here.
    pub fn play_san(&mut self, san: &str) -> bool {
        let Ok(m) = parse_san(&self.pos, san) else { return false };
        let san = san_of(&self.pos, m);
        let uci = uci_of(m);
        self.history.push((self.pos.clone(), self.last_move.clone()));
        self.pos.play_unchecked(m);
        self.last_move = Some(uci.clone());
        self.steps.push(CoachStep::Move { uci, san, fen: fen_of(&self.pos) });
        true
    }

    /// Play up to `max` moves of a SAN line, pausing after each; returns how
    /// many were played (the line stops at the first illegal move).
    pub fn play_line(&mut self, sans: &[String], max: usize) -> usize {
        let mut played = 0;
        for san in sans.iter().take(max) {
            if self.pos.is_game_over() || !self.play_san(san) {
                break;
            }
            played += 1;
            self.pause(MOVE_PAUSE_MS);
        }
        played
    }

    /// Take back `n` moves, one animated step at a time.
    pub fn back(&mut self, n: usize) {
        for _ in 0..n {
            let Some((prev, last)) = self.history.pop() else { break };
            self.pos = prev;
            self.last_move = last.clone();
            self.steps.push(CoachStep::Back { fen: fen_of(&self.pos), last_move: last });
            self.pause(150);
        }
    }

    /// Take back everything played so far.
    pub fn back_to_start(&mut self) {
        self.back(self.history.len());
    }

    /// Jump to another position (no animation of the moves in between).
    pub fn reset(&mut self, fen: &str, last_move: Option<String>) -> Result<()> {
        self.pos = parse_fen(fen)?;
        self.history.clear();
        self.last_move = last_move.clone();
        self.steps.push(CoachStep::Reset { fen: fen_of(&self.pos), last_move });
        Ok(())
    }

    pub fn moves_played(&self) -> usize {
        self.history.len()
    }

    pub fn finish(
        self,
        id: String,
        coach_id: &str,
        title: impl Into<String>,
        return_fen: String,
        return_last_move: Option<String>,
    ) -> CoachScript {
        CoachScript {
            id,
            coach_id: coach_id.into(),
            title: title.into(),
            start_fen: self.start_fen,
            start_last_move: self.start_last_move,
            return_fen,
            return_last_move,
            steps: self.steps,
            ai_narrated: false,
            notice: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::START_FEN;

    #[test]
    fn plays_and_takes_back_with_a_fen_for_every_step() {
        let mut b = ScriptBuilder::new(START_FEN, None).unwrap();
        b.say(SayRole::Intro, "Let's look at the centre.");
        let played = b.play_line(&["e4".into(), "e5".into(), "Qxf7".into(), "Nf3".into()], 10);
        assert_eq!(played, 2, "stops at the illegal Qxf7");
        b.back_to_start();
        assert_eq!(b.fen(), START_FEN);
        let script = b.finish("x".into(), "stockfish", "Test", START_FEN.into(), None);
        let moves: Vec<&str> =
            script.steps.iter().filter_map(|s| if let CoachStep::Move { san, .. } = s { Some(san.as_str()) } else { None }).collect();
        assert_eq!(moves, vec!["e4", "e5"]);
        let backs: Vec<&CoachStep> = script.steps.iter().filter(|s| matches!(s, CoachStep::Back { .. })).collect();
        assert_eq!(backs.len(), 2);
        if let CoachStep::Back { fen, last_move } = backs[0] {
            assert!(fen.contains("4P3"), "after taking back e5, e4 is still on the board");
            assert_eq!(last_move.as_deref(), Some("e2e4"));
        }
        let json = serde_json::to_value(&script.steps[0]).unwrap();
        assert_eq!(json["type"], "say");
        assert_eq!(json["role"], "intro");
    }
}
