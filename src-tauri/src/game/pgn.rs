//! PGN import and export for [`GameTree`], including variations, comments,
//! NAGs, `[%clk]` and `[%eval]` annotations.

use std::ops::ControlFlow;

use pgn_reader::{Nag, Outcome, RawComment, RawTag, Reader, SanPlus, Skip, Visitor};
use serde::{Deserialize, Serialize};
use shakmaty::{Chess, Position};
use specta::Type;

use super::position::{fen_of, uci_of};
use super::tree::{GameTree, PgnHeader};
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PgnParseResult {
    pub games: Vec<GameTree>,
    /// Problems found, e.g. an illegal move that cut a line short.
    pub errors: Vec<String>,
}

struct TreeVisitor;

struct Movetext {
    tree: GameTree,
    /// Position after each node, indexed by node id.
    positions: Vec<Chess>,
    current: u32,
    stack: Vec<u32>,
    /// Variation depth at which an illegal move was found; moves are ignored
    /// until that variation closes.
    dead_depth: Option<usize>,
    outcome: Option<String>,
    error: Option<String>,
}

impl Movetext {
    fn dead(&self) -> bool {
        self.dead_depth.is_some_and(|d| self.stack.len() >= d)
    }
}

type ParsedGame = std::result::Result<(GameTree, Option<String>), String>;

impl Visitor for TreeVisitor {
    type Tags = Vec<PgnHeader>;
    type Movetext = Movetext;
    type Output = ParsedGame;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(Vec::new())
    }

    fn tag(&mut self, tags: &mut Self::Tags, name: &[u8], value: RawTag<'_>) -> ControlFlow<Self::Output> {
        tags.push(PgnHeader {
            name: String::from_utf8_lossy(name).into_owned(),
            value: value.decode_utf8_lossy().into_owned(),
        });
        ControlFlow::Continue(())
    }

    fn begin_movetext(&mut self, tags: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        let fen = tags.iter().find(|h| h.name.eq_ignore_ascii_case("FEN")).map(|h| h.value.clone());
        let mut tree = match GameTree::new(fen.as_deref()) {
            Ok(t) => t,
            Err(e) => return ControlFlow::Break(Err(e.to_string())),
        };
        tree.headers = tags;
        let start = tree.position(0).expect("root position is valid");
        ControlFlow::Continue(Movetext {
            tree,
            positions: vec![start],
            current: 0,
            stack: Vec::new(),
            dead_depth: None,
            outcome: None,
            error: None,
        })
    }

    fn san(&mut self, mt: &mut Self::Movetext, san_plus: SanPlus) -> ControlFlow<Self::Output> {
        if mt.dead() {
            return ControlFlow::Continue(());
        }
        let pos = &mt.positions[mt.current as usize];
        match san_plus.san.to_move(pos) {
            Ok(m) => {
                let san = SanPlus::from_move(pos.clone(), m).to_string();
                let mut next = pos.clone();
                next.play_unchecked(m);
                let id = mt.tree.add_applied(mt.current, &uci_of(m), &san, fen_of(&next));
                if id as usize == mt.positions.len() {
                    mt.positions.push(next);
                }
                mt.current = id;
            }
            Err(_) => {
                let ply = mt.tree.nodes[mt.current as usize].ply + 1;
                if mt.error.is_none() {
                    mt.error = Some(format!("illegal move {san_plus} at ply {ply}"));
                }
                mt.dead_depth = Some(mt.stack.len());
            }
        }
        ControlFlow::Continue(())
    }

    fn nag(&mut self, mt: &mut Self::Movetext, nag: Nag) -> ControlFlow<Self::Output> {
        if !mt.dead() && mt.current != 0 {
            mt.tree.nodes[mt.current as usize].nags.push(nag.0);
        }
        ControlFlow::Continue(())
    }

    fn comment(&mut self, mt: &mut Self::Movetext, comment: RawComment<'_>) -> ControlFlow<Self::Output> {
        if mt.dead() {
            return ControlFlow::Continue(());
        }
        let text = String::from_utf8_lossy(comment.as_bytes());
        let parsed = parse_comment(&text);
        let node = &mut mt.tree.nodes[mt.current as usize];
        if parsed.clock_ms.is_some() {
            node.clock_ms = parsed.clock_ms;
        }
        if parsed.eval.is_some() {
            node.eval = parsed.eval;
        }
        if !parsed.text.is_empty() {
            node.comment = Some(match node.comment.take() {
                Some(existing) => format!("{existing} {}", parsed.text),
                None => parsed.text,
            });
        }
        ControlFlow::Continue(())
    }

    fn begin_variation(&mut self, mt: &mut Self::Movetext) -> ControlFlow<Self::Output, Skip> {
        mt.stack.push(mt.current);
        if mt.dead() || mt.current == 0 {
            return ControlFlow::Continue(Skip(true));
        }
        // A variation replaces the last move, so it starts from its parent.
        mt.current = mt.tree.nodes[mt.current as usize].parent.unwrap_or(0);
        ControlFlow::Continue(Skip(false))
    }

    fn end_variation(&mut self, mt: &mut Self::Movetext) -> ControlFlow<Self::Output> {
        if let Some(back) = mt.stack.pop() {
            mt.current = back;
        }
        if mt.dead_depth.is_some_and(|d| mt.stack.len() < d) {
            mt.dead_depth = None;
        }
        ControlFlow::Continue(())
    }

    fn outcome(&mut self, mt: &mut Self::Movetext, outcome: Outcome) -> ControlFlow<Self::Output> {
        mt.outcome = Some(outcome.to_string());
        ControlFlow::Continue(())
    }

    fn end_game(&mut self, mut mt: Self::Movetext) -> Self::Output {
        if mt.tree.header("Result").is_none() {
            if let Some(o) = mt.outcome {
                mt.tree.set_header("Result", o);
            }
        }
        Ok((mt.tree, mt.error))
    }
}

/// Stream games from any reader (e.g. a large PGN file). `f` receives each
/// game, or an error message for a game that couldn't be read, and returns
/// false to stop early.
pub fn for_each_game<R: std::io::Read>(
    reader: R,
    mut f: impl FnMut(std::result::Result<(GameTree, Option<String>), String>) -> bool,
) -> std::io::Result<()> {
    let mut reader = Reader::new(reader);
    let mut visitor = TreeVisitor;
    while let Some(parsed) = reader.read_game(&mut visitor)? {
        if !f(parsed) {
            break;
        }
    }
    Ok(())
}

/// Parse every game in a PGN text.
pub fn parse_pgn(text: &str) -> PgnParseResult {
    let mut reader = Reader::new(text.as_bytes());
    let mut visitor = TreeVisitor;
    let mut result = PgnParseResult { games: Vec::new(), errors: Vec::new() };
    let mut index = 0;
    loop {
        match reader.read_game(&mut visitor) {
            Ok(Some(parsed)) => {
                index += 1;
                match parsed {
                    Ok((tree, warning)) => {
                        if let Some(w) = warning {
                            result.errors.push(format!("game {index}: {w}"));
                        }
                        result.games.push(tree);
                    }
                    Err(e) => result.errors.push(format!("game {index}: {e}")),
                }
            }
            Ok(None) => break,
            Err(e) => {
                result.errors.push(format!("read error: {e}"));
                break;
            }
        }
    }
    result
}

struct ParsedComment {
    text: String,
    clock_ms: Option<f64>,
    eval: Option<String>,
}

fn parse_comment(raw: &str) -> ParsedComment {
    let mut text = String::new();
    let mut clock_ms = None;
    let mut eval = None;
    let mut rest = raw;
    while let Some(start) = rest.find("[%") {
        text.push_str(&rest[..start]);
        let Some(end) = rest[start..].find(']') else {
            text.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let command = &rest[start + 2..start + end];
        let mut parts = command.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some("clk"), Some(v)) => clock_ms = parse_clock(v),
            (Some("eval"), Some(v)) => eval = Some(v.trim_end_matches(',').to_string()),
            // Other commands ([%csl], [%cal], ...) are dropped for now.
            _ => {}
        }
        rest = &rest[start + end + 1..];
    }
    text.push_str(rest);
    ParsedComment { text: text.split_whitespace().collect::<Vec<_>>().join(" "), clock_ms, eval }
}

fn parse_clock(v: &str) -> Option<f64> {
    let mut total = 0.0;
    for part in v.split(':') {
        total = total * 60.0 + part.parse::<f64>().ok()?;
    }
    Some((total * 1000.0).round())
}

pub fn format_clock(ms: f64) -> String {
    let tenths = (ms / 100.0).round() as u64;
    let (secs, frac) = (tenths / 10, tenths % 10);
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if frac == 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{h}:{m:02}:{s:02}.{frac}")
    }
}

const ROSTER: [(&str, &str); 7] = [
    ("Event", "?"),
    ("Site", "?"),
    ("Date", "????.??.??"),
    ("Round", "?"),
    ("White", "?"),
    ("Black", "?"),
    ("Result", "*"),
];

fn escape_tag(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Serialise a tree as PGN text.
pub fn write_pgn(tree: &GameTree) -> String {
    let mut out = String::new();
    for (name, default) in ROSTER {
        let value = tree.header(name).unwrap_or(default);
        out.push_str(&format!("[{name} \"{}\"]\n", escape_tag(value)));
    }
    for h in &tree.headers {
        if !ROSTER.iter().any(|(n, _)| *n == h.name) {
            out.push_str(&format!("[{} \"{}\"]\n", h.name, escape_tag(&h.value)));
        }
    }
    out.push('\n');

    let mut tokens: Vec<String> = Vec::new();
    if let Some(c) = comment_block(tree, 0) {
        tokens.push(c);
    }
    write_children(tree, 0, &mut tokens, true);
    tokens.push(tree.header("Result").unwrap_or("*").to_string());
    out.push_str(&wrap_tokens(&tokens, 80));
    out.push('\n');
    out
}

/// Turn and fullmove number from a FEN's last fields.
fn turn_and_number(fen: &str) -> (bool, u32) {
    let fields: Vec<&str> = fen.split_whitespace().collect();
    let white = fields.get(1).is_none_or(|t| *t == "w");
    let number = fields.get(5).and_then(|n| n.parse().ok()).unwrap_or(1);
    (white, number)
}

fn comment_block(tree: &GameTree, id: u32) -> Option<String> {
    let node = &tree.nodes[id as usize];
    let mut parts = Vec::new();
    if let Some(ms) = node.clock_ms {
        parts.push(format!("[%clk {}]", format_clock(ms)));
    }
    if let Some(e) = &node.eval {
        parts.push(format!("[%eval {e}]"));
    }
    if let Some(c) = &node.comment {
        parts.push(c.replace('}', ")"));
    }
    (!parts.is_empty()).then(|| format!("{{ {} }}", parts.join(" ")))
}

/// Writes one move with its number, NAGs and comment. Returns true when the
/// next move needs its number repeated (after a comment).
fn push_move(tree: &GameTree, id: u32, tokens: &mut Vec<String>, force_number: bool) -> bool {
    let node = &tree.nodes[id as usize];
    let parent = &tree.nodes[node.parent.unwrap_or(0) as usize];
    let (white_to_move, number) = turn_and_number(&parent.fen);
    let san = node.san.clone().unwrap_or_default();
    let mv = if white_to_move {
        format!("{number}. {san}")
    } else if force_number {
        format!("{number}... {san}")
    } else {
        san
    };
    tokens.push(mv);
    for nag in &node.nags {
        tokens.push(format!("${nag}"));
    }
    match comment_block(tree, id) {
        Some(c) => {
            tokens.push(c);
            true
        }
        None => false,
    }
}

fn write_children(tree: &GameTree, parent: u32, tokens: &mut Vec<String>, force_number: bool) {
    let children = &tree.nodes[parent as usize].children;
    let Some(&main) = children.first() else { return };
    let main_commented = push_move(tree, main, tokens, force_number);
    for &alt in &children[1..] {
        tokens.push("(".into());
        let commented = push_move(tree, alt, tokens, true);
        write_children(tree, alt, tokens, commented);
        tokens.push(")".into());
    }
    write_children(tree, main, tokens, main_commented || children.len() > 1);
}

fn wrap_tokens(tokens: &[String], width: usize) -> String {
    let mut out = String::new();
    let mut line = String::new();
    let mut glue_next = false;
    for tok in tokens {
        let attach = glue_next || tok == ")";
        glue_next = tok == "(";
        if attach && !line.is_empty() {
            line.push_str(tok);
            continue;
        }
        if !line.is_empty() && line.len() + 1 + tok.len() > width {
            out.push_str(&line);
            out.push('\n');
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(tok);
    }
    out.push_str(&line);
    out
}

/// Read a PGN file from disk.
pub fn read_pgn_file(path: &std::path::Path) -> Result<PgnParseResult> {
    let bytes = std::fs::read(path)?;
    Ok(parse_pgn(&String::from_utf8_lossy(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::START_FEN;

    const SAMPLE: &str = r#"[Event "Casual game"]
[Site "Grandie Chess"]
[Date "2026.10.10"]
[Round "-"]
[White "You"]
[Black "Maia 1500"]
[Result "1-0"]
[TimeControl "300+3"]

{ A short game } 1. e4 { [%clk 0:05:00] [%eval 0.3] Best by test } 1... e5 { [%clk 0:04:59.5] }
2. Nf3 $1 Nc6 ( 2... d6 3. d4 ( 3. Bc4 ) 3... Nf6 ) ( 2... Nf6 { Petrov } ) 3. Bc4 Nf6?
4. Ng5 d5 5. exd5 Na5 6. Bb5+ c6 7. dxc6 bxc6 8. Qf3 1-0
"#;

    #[test]
    fn parses_tree_with_variations_comments_and_annotations() {
        let parsed = parse_pgn(SAMPLE);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let tree = &parsed.games[0];
        assert_eq!(tree.header("Black"), Some("Maia 1500"));
        assert_eq!(tree.root().comment.as_deref(), Some("A short game"));
        let main = tree.mainline();
        assert_eq!(main.len(), 15);
        let e4 = tree.node(main[0]).unwrap();
        assert_eq!(e4.clock_ms, Some(300_000.0));
        assert_eq!(e4.eval.as_deref(), Some("0.3"));
        assert_eq!(e4.comment.as_deref(), Some("Best by test"));
        assert_eq!(tree.node(main[1]).unwrap().clock_ms, Some(299_500.0));
        let nf3 = tree.node(main[2]).unwrap();
        assert_eq!(nf3.nags, vec![1]);
        assert_eq!(nf3.children.len(), 3, "Nc6 plus two alternatives");
        let d6 = tree.node(nf3.children[1]).unwrap();
        assert_eq!(d6.san.as_deref(), Some("d6"));
        let d4 = tree.node(d6.children[0]).unwrap();
        assert_eq!(d4.children.len(), 1);
        let bc4_alt = tree.node(d6.children[1]).unwrap();
        assert_eq!(bc4_alt.san.as_deref(), Some("Bc4"));
        assert_eq!(tree.node(nf3.children[2]).unwrap().comment.as_deref(), Some("Petrov"));
        assert_eq!(tree.node(main[5]).unwrap().nags, vec![2]);
        assert_eq!(tree.node(main[10]).unwrap().san.as_deref(), Some("Bb5+"));
    }

    #[test]
    fn write_then_read_round_trips() {
        let tree = parse_pgn(SAMPLE).games.remove(0);
        let text = write_pgn(&tree);
        let flat = text.replace('\n', " ");
        assert!(flat.contains("2. Nf3 $1 Nc6 (2... d6 3. d4 (3. Bc4) 3... Nf6) (2... Nf6 { Petrov }) 3. Bc4"));
        assert!(flat.contains("{ [%clk 0:05:00] [%eval 0.3] Best by test }"));
        assert!(text.lines().all(|l| l.len() <= 80), "{text}");
        let again = parse_pgn(&text);
        assert!(again.errors.is_empty(), "{:?}", again.errors);
        assert_eq!(again.games[0].nodes, tree.nodes);
        assert_eq!(write_pgn(&again.games[0]), text);
    }

    #[test]
    fn handles_fen_start_and_black_first() {
        let pgn = "[FEN \"8/8/4k3/8/8/3K4/8/7R b - - 0 40\"]\n[SetUp \"1\"]\n\n40... Kd6 41. Rh6+ *";
        let parsed = parse_pgn(pgn);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let tree = &parsed.games[0];
        assert_ne!(tree.root().fen, START_FEN);
        assert_eq!(tree.mainline().len(), 2);
        let text = write_pgn(tree);
        assert!(text.contains("40... Kd6 41. Rh6+ *"), "{text}");
    }

    #[test]
    fn illegal_move_cuts_only_its_line() {
        let pgn = "1. e4 e5 ( 1... Qxh2 2. d4 ) 2. Nf3 *";
        let parsed = parse_pgn(pgn);
        assert_eq!(parsed.errors.len(), 1);
        let tree = &parsed.games[0];
        assert_eq!(tree.mainline().len(), 3);
    }

    #[test]
    fn reads_multiple_games() {
        let two = format!("{SAMPLE}\n\n[Event \"Second\"]\n\n1. d4 d5 *\n");
        let parsed = parse_pgn(&two);
        assert_eq!(parsed.games.len(), 2);
        assert_eq!(parsed.games[1].header("Event"), Some("Second"));
    }

    #[test]
    fn clock_formatting() {
        assert_eq!(format_clock(300_000.0), "0:05:00");
        assert_eq!(format_clock(3_723_400.0), "1:02:03.4");
        assert_eq!(parse_clock("1:02:03.4"), Some(3_723_400.0));
    }
}
