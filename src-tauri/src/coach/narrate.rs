//! The coach's words. Every script is first written from templates, using
//! the facts in [`super::facts`], so the coach works with no AI at all. When
//! AI is on, a model rewrites each line in the coach's voice; a rewritten line
//! is only kept if every move it names is legal and appears in the lines the
//! engine gave, otherwise the template line stays.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::engine::CoachDef;
use super::script::{CoachScript, SayRole};
use crate::ai::openrouter::{ChatRequest, Message, OpenRouter, Usage};
use crate::error::Result;
use crate::settings::{CoachSettings, Verbosity};

/// Everything the model may use when writing; serialised into the prompt.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NarrationContext {
    pub side_to_move: String,
    pub phase: String,
    pub opening: Option<String>,
    /// Free-form facts: the played move, its class, eval words, line facts.
    pub facts: Value,
    /// Every SAN move the script shows or mentions; the only moves the
    /// narration may name.
    pub allowed_moves: Vec<String>,
    /// About the student, e.g. "a club player (puzzle rating about 1500)".
    pub student: String,
}

#[derive(Debug, Deserialize)]
struct NarrationReply {
    lines: Vec<NarrationLine>,
}

#[derive(Debug, Deserialize)]
struct NarrationLine {
    id: String,
    text: String,
}

fn verbosity_rule(v: Verbosity) -> &'static str {
    match v {
        Verbosity::Brief => "one short sentence per step (at most 15 words)",
        Verbosity::Normal => "one or two sentences per step (at most 35 words)",
        Verbosity::Detailed => "up to three sentences per step, explaining the idea behind each move",
    }
}

pub fn system_prompt(coach: &CoachDef, settings: &CoachSettings) -> String {
    format!(
        "{persona}\n\nYou are narrating a demonstration on a chess board: the student watches the pieces move while you speak. \
         Write in {lang}, as you would say it aloud: warm, direct, no markdown, no lists, no move numbers unless natural. \
         Use {rule}. Name moves in standard algebraic notation (e.g. Nf3, exd5, O-O). \
         Only name moves from the allowedMoves list, and only state facts given to you; never invent tactics or evaluations.",
        persona = coach.persona,
        lang = settings.language,
        rule = verbosity_rule(settings.verbosity),
    )
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "lines": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "id": { "type": "string" }, "text": { "type": "string" } },
                    "required": ["id", "text"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["lines"],
        "additionalProperties": false
    })
}

/// Ask the model to rewrite every spoken line. Returns the usage and the
/// number of lines replaced; lines that fail validation keep their template.
pub async fn polish(
    client: &OpenRouter,
    model: &str,
    coach: &CoachDef,
    settings: &CoachSettings,
    script: &mut CoachScript,
    ctx: &NarrationContext,
) -> Result<(Usage, String, usize)> {
    let steps: Vec<Value> = script.says().map(|(id, role, text)| json!({ "id": id, "role": role, "draft": text })).collect();
    if steps.is_empty() {
        return Ok((Usage::default(), model.to_string(), 0));
    }
    let user = json!({
        "task": "Rewrite each step's draft as your spoken narration for that moment of the demonstration. Keep the same ids, one line per step, in order.",
        "context": ctx,
        "steps": steps,
    });
    let req = ChatRequest {
        model: model.to_string(),
        messages: vec![Message::system(system_prompt(coach, settings)), Message::user(user.to_string())],
        temperature: Some(0.5),
        max_tokens: Some(1200),
        ..Default::default()
    }
    .json_schema("coach_narration", schema());
    let completion = client.chat(req).await?;
    let reply: NarrationReply = serde_json::from_str(strip_fences(completion.text()))?;
    let allowed = normalised_set(&ctx.allowed_moves);
    let mut replaced = 0;
    let ids: HashSet<String> = script.says().map(|(id, _, _)| id.to_string()).collect();
    for line in reply.lines {
        let text = line.text.trim();
        if ids.contains(&line.id) && !text.is_empty() && mentions_only(text, &allowed) {
            script.set_text(&line.id, text);
            replaced += 1;
        }
    }
    script.ai_narrated = replaced > 0;
    Ok((completion.usage, completion.model, replaced))
}

/// Some models wrap JSON in ``` fences even when asked for a schema.
fn strip_fences(text: &str) -> &str {
    let t = text.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    t.strip_suffix("```").unwrap_or(t).trim()
}

fn normalise(san: &str) -> String {
    san.trim_end_matches(['+', '#', '!', '?']).replace('0', "O")
}

fn normalised_set(sans: &[String]) -> HashSet<String> {
    sans.iter().map(|s| normalise(s)).collect()
}

/// Tokens that are unmistakably moves: piece moves (Nf3, Raxd1), captures
/// (exd5) and castling. Bare squares ("the e5 square") are not counted.
pub fn move_tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || "=+#-x".contains(c)))
        .map(|t| t.trim_matches('-'))
        .filter(|t| is_move_token(t))
        .map(normalise)
        .collect()
}

fn is_move_token(t: &str) -> bool {
    let t = t.trim_end_matches(['+', '#']);
    if matches!(t, "O-O" | "O-O-O" | "0-0" | "0-0-0") {
        return true;
    }
    let square_at_end = |s: &[u8]| s.len() >= 2 && (b'a'..=b'h').contains(&s[s.len() - 2]) && (b'1'..=b'8').contains(&s[s.len() - 1]);
    let core = t.split('=').next().unwrap_or(t).as_bytes();
    if core.len() >= 3 && b"KQRBN".contains(&core[0]) && square_at_end(core) {
        return core[1..].iter().all(|c| (b'a'..=b'h').contains(c) || (b'1'..=b'8').contains(c) || *c == b'x');
    }
    // Pawn capture: exd5, exd8=Q
    core.len() == 4 && (b'a'..=b'h').contains(&core[0]) && core[1] == b'x' && square_at_end(core)
}

/// True when every move named in `text` is in `allowed`.
pub fn mentions_only(text: &str, allowed: &HashSet<String>) -> bool {
    move_tokens(text).iter().all(|m| allowed.contains(m))
}

/// "a club player (puzzle rating about 1500)" and similar.
pub fn describe_student(puzzle_rating: Option<f64>) -> String {
    match puzzle_rating {
        Some(r) if r < 1100.0 => format!("a beginner (puzzle rating about {})", r.round()),
        Some(r) if r < 1700.0 => format!("a club player (puzzle rating about {})", r.round()),
        Some(r) => format!("a strong player (puzzle rating about {})", r.round()),
        None => "a club player".into(),
    }
}

/// Spoken name of a role, for template text.
pub fn class_phrase(role: SayRole) -> &'static str {
    match role {
        SayRole::Intro => "intro",
        SayRole::Played => "played",
        SayRole::Refutation => "refutation",
        SayRole::Consequence => "consequence",
        SayRole::Better => "better",
        SayRole::BetterLine => "better line",
        SayRole::Summary => "summary",
        SayRole::Idea => "idea",
        SayRole::Moment => "moment",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_moves_but_not_squares() {
        assert_eq!(move_tokens("After Nxe5, Black plays exd5 and then O-O. The e5 square is weak."), vec!["Nxe5", "exd5", "O-O"]);
        assert_eq!(move_tokens("Qh4# ends it; Raxd1+ and e8=Q are lines too"), vec!["Qh4", "Raxd1"]);
        assert!(move_tokens("Play on the kingside, then Kingside castling").is_empty());
        assert_eq!(move_tokens("castles with 0-0-0!"), vec!["O-O-O"]);
    }

    #[test]
    fn rejects_moves_outside_the_engine_lines() {
        let allowed = normalised_set(&["Nf3".into(), "Nxe5+".into(), "d4".into()]);
        assert!(mentions_only("Nf3 develops, and Nxe5 wins a pawn.", &allowed));
        assert!(mentions_only("The pawn on d4 is strong.", &allowed));
        assert!(!mentions_only("Better was Bb5, pinning the knight.", &allowed));
    }

    #[test]
    fn strips_code_fences() {
        assert_eq!(strip_fences("```json\n{\"lines\":[]}\n```"), "{\"lines\":[]}");
        assert_eq!(strip_fences("{\"a\":1}"), "{\"a\":1}");
    }
}
