//! What the user said: a move ("knight takes e five"), a command ("take
//! back"), or a question for the coach. Moves are matched against the legal
//! moves of the position, so speech-recognition slips ("night f3", "e for")
//! still land on a real move, and an unclear move is never guessed.

use serde::{Deserialize, Serialize};
use shakmaty::{Chess, File, Move, Position, Role, Square};
use specta::Type;

use crate::game::position::{parse_fen, san_of, uci_of};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceCommand {
    TakeBack,
    Hint,
    Flip,
    Resign,
    OfferDraw,
    BestMove,
    Repeat,
    Stop,
    Next,
    Previous,
    Explain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum VoiceIntent {
    #[serde(rename_all = "camelCase")]
    Move {
        uci: String,
        san: String,
    },
    /// Several legal moves fit; ask which one.
    #[serde(rename_all = "camelCase")]
    Ambiguous {
        options: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    Command {
        command: VoiceCommand,
    },
    /// Anything else: a question for the coach.
    #[serde(rename_all = "camelCase")]
    Question {
        text: String,
    },
    /// Sounded like a move, but no legal move matches.
    #[serde(rename_all = "camelCase")]
    NoMatch {
        heard: String,
    },
    Empty,
}

/// Words that speech recognition produces for chess terms.
fn normalise_word(w: &str) -> Option<&'static str> {
    Some(match w {
        "knight" | "knights" | "night" | "nights" | "nite" | "knite" | "kn" | "n" => "N",
        "bishop" | "bishops" | "b1shop" => "B",
        "rook" | "rooks" | "rock" | "rocks" | "ruck" => "R",
        "queen" | "queens" | "q" => "Q",
        "king" | "kings" | "k" => "K",
        "pawn" | "pawns" | "pond" | "porn" | "prawn" => "P",
        "takes" | "take" | "captures" | "capture" | "x" | "times" | "by" => "x",
        "castle" | "castles" | "castling" | "castled" => "castle",
        "short" | "kingside" => "short",
        "long" | "queenside" => "long",
        "promote" | "promotes" | "promotion" | "equals" | "becomes" => "=",
        "one" | "won" | "1" => "1",
        "two" | "too" | "2" => "2",
        "three" | "tree" | "free" | "3" => "3",
        "four" | "for" | "fore" | "4" => "4",
        "five" | "5" => "5",
        "six" | "sex" | "sicks" | "6" => "6",
        "seven" | "7" => "7",
        "eight" | "ate" | "8" => "8",
        "a" | "ay" | "alpha" => "a",
        "be" | "bee" | "bravo" => "b",
        "c" | "see" | "sea" | "si" | "charlie" => "c",
        "d" | "dee" | "delta" => "d",
        "e" | "ee" | "echo" => "e",
        "f" | "ef" | "eff" | "foxtrot" => "f",
        "g" | "gee" | "golf" | "ji" => "g",
        "h" | "aitch" | "age" | "hotel" => "h",
        _ => return None,
    })
}

/// Tokens such as "N", "x", "e4", "castle", "short", "=" from free speech.
fn tokens(text: &str) -> Vec<String> {
    let lower = text.to_lowercase().replace(['-', '.', ',', '!', '?', ';', ':'], " ");
    let mut out: Vec<String> = Vec::new();
    for raw in lower.split_whitespace() {
        // Already written chess: "nf3", "exd5", "e4", "o-o" (handled above).
        if raw == "oo" || raw == "o" || raw == "0" {
            out.push("castle-o".into());
            continue;
        }
        if let Some(n) = normalise_word(raw) {
            out.push(n.to_string());
            continue;
        }
        if square(raw).is_some() {
            out.push(raw.to_string());
            continue;
        }
        // Written SAN: "nf3", "nxe5", "nbd2", "exd5".
        if let Some(parts) = split_written_san(raw) {
            out.extend(parts);
            continue;
        }
        out.push(format!("~{raw}"));
    }
    // Join a file letter followed by a rank digit into a square ("e" "4" -> "e4").
    let mut joined: Vec<String> = Vec::new();
    let mut i = 0;
    while i < out.len() {
        let t = &out[i];
        if t.len() == 1
            && ("a"..="h").contains(&t.as_str())
            && i + 1 < out.len()
            && out[i + 1].len() == 1
            && out[i + 1].chars().all(|c| ('1'..='8').contains(&c))
        {
            joined.push(format!("{t}{}", out[i + 1]));
            i += 2;
        } else {
            joined.push(t.clone());
            i += 1;
        }
    }
    joined
}

fn square(t: &str) -> Option<Square> {
    if t.len() != 2 {
        return None;
    }
    t.parse::<Square>().ok()
}

fn is_file(c: u8) -> bool {
    (b'a'..=b'h').contains(&c)
}

/// "nf3" -> N f3; "nxe5" -> N x e5; "nbd2" -> N b d2; "exd5" -> e x d5.
fn split_written_san(raw: &str) -> Option<Vec<String>> {
    let raw = raw.trim_end_matches(['+', '#']);
    let b = raw.as_bytes();
    let (piece, rest) =
        if !b.is_empty() && b"nbrqk".contains(&b[0]) && b.len() >= 3 { (Some(raw[..1].to_uppercase()), &raw[1..]) } else { (None, raw) };
    let r = rest.as_bytes();
    let mut out: Vec<String> = piece.into_iter().collect();
    match r.len() {
        2 if square(rest).is_some() && !out.is_empty() => out.push(rest.into()),
        3 if r[0] == b'x' && square(&rest[1..]).is_some() && !out.is_empty() => out.extend(["x".to_string(), rest[1..].to_string()]),
        3 if is_file(r[0]) && square(&rest[1..]).is_some() && !out.is_empty() => out.extend([rest[..1].to_string(), rest[1..].to_string()]),
        4 if is_file(r[0]) && r[1] == b'x' && square(&rest[2..]).is_some() => {
            out.extend([rest[..1].to_string(), "x".to_string(), rest[2..].to_string()])
        }
        _ => return None,
    }
    Some(out)
}

fn role(t: &str) -> Option<Role> {
    Some(match t {
        "N" => Role::Knight,
        "B" => Role::Bishop,
        "R" => Role::Rook,
        "Q" => Role::Queen,
        "K" => Role::King,
        "P" => Role::Pawn,
        _ => return None,
    })
}

fn command(text: &str) -> Option<VoiceCommand> {
    let t = text.to_lowercase();
    let t = t.trim().trim_end_matches(['.', '!', '?']);
    let has = |words: &[&str]| {
        words.iter().any(|w| t == *w || t.starts_with(&format!("{w} ")) || t.ends_with(&format!(" {w}")) || t.contains(&format!(" {w} ")))
    };
    if has(&["take back", "takeback", "take it back", "undo", "undo that"]) {
        return Some(VoiceCommand::TakeBack);
    }
    if has(&["hint", "give me a hint", "help me"]) {
        return Some(VoiceCommand::Hint);
    }
    if has(&["flip", "flip the board", "turn the board"]) {
        return Some(VoiceCommand::Flip);
    }
    if has(&["i resign", "resign"]) {
        return Some(VoiceCommand::Resign);
    }
    if has(&["offer a draw", "offer draw", "draw please", "draw"]) && t.split_whitespace().count() <= 4 {
        return Some(VoiceCommand::OfferDraw);
    }
    if has(&["best move", "show me the best move", "what's the best move", "what is the best move"]) {
        return Some(VoiceCommand::BestMove);
    }
    if has(&["repeat", "say that again", "again", "repeat that"]) && t.split_whitespace().count() <= 4 {
        return Some(VoiceCommand::Repeat);
    }
    if has(&["stop", "be quiet", "quiet", "shush", "pause"]) && t.split_whitespace().count() <= 3 {
        return Some(VoiceCommand::Stop);
    }
    if has(&["next", "next move", "continue", "go on"]) && t.split_whitespace().count() <= 3 {
        return Some(VoiceCommand::Next);
    }
    if has(&["previous", "go back", "back"]) && t.split_whitespace().count() <= 3 {
        return Some(VoiceCommand::Previous);
    }
    if matches!(t, "why" | "explain" | "explain that" | "explain it" | "why is that" | "why that") {
        return Some(VoiceCommand::Explain);
    }
    None
}

/// A move description pulled out of tokens.
#[derive(Debug, Default)]
struct Spoken {
    role: Option<Role>,
    capture: bool,
    squares: Vec<Square>,
    /// A lone file letter before the target, e.g. "e takes d5" or "rook a d1".
    from_file: Option<File>,
    promotion: Option<Role>,
    castle: Option<bool>, // Some(true) = kingside
    unknown_words: usize,
}

fn parse_spoken(toks: &[String]) -> Spoken {
    let mut s = Spoken::default();
    let mut i = 0;
    while i < toks.len() {
        let t = toks[i].as_str();
        if t == "castle" || t == "castle-o" {
            let side = toks.iter().skip(i + 1).find_map(|n| match n.as_str() {
                "short" => Some(true),
                "long" => Some(false),
                _ => None,
            });
            let os = toks.iter().filter(|t| *t == "castle-o").count();
            s.castle = Some(side.unwrap_or(os < 3));
        } else if t == "x" {
            s.capture = true;
        } else if t == "=" {
            if let Some(r) = toks.get(i + 1).and_then(|n| role(n)) {
                s.promotion = Some(r);
                i += 1;
            }
        } else if let Some(r) = role(t) {
            // After a target square a piece name means promotion ("e8 queen").
            if !s.squares.is_empty() && s.role.is_none_or(|x| x == Role::Pawn) && r != Role::Pawn && r != Role::King {
                s.promotion = Some(r);
            } else if s.role.is_none() {
                s.role = Some(r);
            }
        } else if let Some(sq) = square(t) {
            s.squares.push(sq);
        } else if t.len() == 1 && ("a"..="h").contains(&t) {
            s.from_file = File::from_char(t.chars().next().unwrap_or('a'));
        } else if matches!(t, "short" | "long")
            || t == "~to"
            || t == "~on"
            || t == "~the"
            || t == "~from"
            || t == "~check"
            || t == "~mate"
            || t == "~checkmate"
            || t == "~move"
            || t == "~play"
            || t == "~i"
            || t == "~and"
            || t == "~plays"
        {
            // filler
        } else if t.starts_with('~') {
            s.unknown_words += 1;
        }
        i += 1;
    }
    s
}

fn candidates(pos: &Chess, s: &Spoken) -> Vec<Move> {
    let legal = pos.legal_moves();
    if let Some(kingside) = s.castle {
        return legal.into_iter().filter(|m| matches!(m, Move::Castle { king, rook } if (rook.file() > king.file()) == kingside)).collect();
    }
    let Some(&target) = s.squares.last() else { return vec![] };
    let from_sq = if s.squares.len() >= 2 { Some(s.squares[s.squares.len() - 2]) } else { None };
    legal
        .into_iter()
        .filter(|m| {
            let to = match m {
                Move::Castle { king, rook } => {
                    // "king g1" means castling for the king.
                    let file = if rook.file() > king.file() { File::G } else { File::C };
                    Square::from_coords(file, king.rank())
                }
                _ => m.to(),
            };
            if to != target {
                return false;
            }
            if let Some(f) = from_sq {
                return m.from() == Some(f);
            }
            let role_ok = match s.role {
                Some(r) => m.role() == r,
                // Just a square: a pawn move, or the only piece that can go there.
                None => true,
            };
            let capture_ok = !s.capture || m.is_capture();
            let file_ok = s.from_file.is_none_or(|f| m.from().is_some_and(|from| from.file() == f));
            let promo_ok = match (m.promotion(), s.promotion) {
                (Some(p), Some(want)) => p == want,
                (Some(_), None) => true, // filtered to queen below
                (None, Some(_)) => false,
                (None, None) => true,
            };
            role_ok && capture_ok && file_ok && promo_ok
        })
        .collect()
}

/// Work out what was said. `fen` is the position to play in, if any.
pub fn interpret(text: &str, fen: Option<&str>) -> VoiceIntent {
    let text = text.trim();
    if text.is_empty() || text.chars().all(|c| !c.is_alphanumeric()) {
        return VoiceIntent::Empty;
    }
    if let Some(c) = command(text) {
        return VoiceIntent::Command { command: c };
    }
    let looks_like_question = {
        let t = text.to_lowercase();
        text.ends_with('?')
            || [
                "what", "why", "how", "should", "is ", "can ", "which", "where", "when", "tell me", "explain", "does", "would", "could",
                "who",
            ]
            .iter()
            .any(|w| t.starts_with(w))
    };
    let toks = tokens(text);
    let spoken = parse_spoken(&toks);
    let movey = spoken.castle.is_some() || !spoken.squares.is_empty();
    if let (Some(pos), true, false) = (fen.and_then(|f| parse_fen(f).ok()), movey, looks_like_question) {
        let mut moves = candidates(&pos, &spoken);
        // Without a named piece, prefer pawn moves ("e4" means the pawn).
        if spoken.role.is_none() && moves.iter().any(|m| m.role() == Role::Pawn) {
            moves.retain(|m| m.role() == Role::Pawn);
        }
        // Unnamed promotions default to a queen.
        if spoken.promotion.is_none() && moves.iter().any(|m| m.promotion() == Some(Role::Queen)) {
            moves.retain(|m| m.promotion().is_none_or(|p| p == Role::Queen));
        }
        match moves.len() {
            1 => {
                let m = moves[0];
                return VoiceIntent::Move { uci: uci_of(m), san: san_of(&pos, m) };
            }
            0 if spoken.unknown_words <= 2 => return VoiceIntent::NoMatch { heard: text.to_string() },
            0 => {}
            _ => {
                let mut options: Vec<String> = moves.iter().map(|m| san_of(&pos, *m)).collect();
                options.sort();
                return VoiceIntent::Ambiguous { options };
            }
        }
    }
    VoiceIntent::Question { text: text.to_string() }
}

/// Chess vocabulary to help speech-to-text.
pub const STT_PROMPT: &str =
    "Chess moves and questions. Knight, bishop, rook, queen, king, pawn, takes, castles kingside, castles queenside, \
promotes to queen, check, checkmate, e4, d5, Nf3, Nxe5, Bb5, O-O, take back, hint, flip the board, resign, offer a draw, best move. \
Why is that move bad? What should I play here?";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::START_FEN;

    fn mv(text: &str, fen: &str) -> VoiceIntent {
        interpret(text, Some(fen))
    }

    fn san(i: VoiceIntent) -> String {
        match i {
            VoiceIntent::Move { san, .. } => san,
            other => panic!("not a move: {other:?}"),
        }
    }

    #[test]
    fn understands_spoken_moves() {
        assert_eq!(san(mv("e4", START_FEN)), "e4");
        assert_eq!(san(mv("e four", START_FEN)), "e4");
        assert_eq!(san(mv("pawn to e for", START_FEN)), "e4");
        assert_eq!(san(mv("Knight f3", START_FEN)), "Nf3");
        assert_eq!(san(mv("night to f three.", START_FEN)), "Nf3");
        assert_eq!(san(mv("Nf3", START_FEN)), "Nf3");
        assert_eq!(san(mv("g1 f3", START_FEN)), "Nf3");
    }

    #[test]
    fn captures_castling_and_disambiguation() {
        // 1. e4 d5: exd5 is the only capture.
        let fen = "rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2";
        assert_eq!(san(mv("e takes d5", fen)), "exd5");
        assert_eq!(san(mv("pawn takes d five", fen)), "exd5");
        // Castling both ways.
        let fen = "r3k2r/pppqbppp/2np1n2/4p3/2B1P1b1/2NP1N2/PPPBQPPP/R3K2R w KQkq - 6 8";
        assert_eq!(san(mv("castle kingside", fen)), "O-O");
        assert_eq!(san(mv("castles long", fen)), "O-O-O");
        assert_eq!(san(mv("O-O", fen)), "O-O");
        // Both rooks can reach d1: ask which, or take the named file.
        let fen = "4k3/8/8/8/8/8/4K3/R6R w - - 0 1";
        match mv("rook d1", fen) {
            VoiceIntent::Ambiguous { options } => assert_eq!(options, vec!["Rad1", "Rhd1"]),
            other => panic!("{other:?}"),
        }
        assert_eq!(san(mv("rook a d1", fen)), "Rad1");
    }

    #[test]
    fn promotion_defaults_to_a_queen() {
        let fen = "8/4P1k1/8/8/8/8/6K1/8 w - - 0 1";
        assert_eq!(san(mv("e8", fen)), "e8=Q");
        assert_eq!(san(mv("e8 knight", fen)), "e8=N+");
        assert_eq!(san(mv("e8 promote to rook", fen)), "e8=R");
    }

    #[test]
    fn commands_questions_and_mishearings() {
        assert_eq!(interpret("Take back", Some(START_FEN)), VoiceIntent::Command { command: VoiceCommand::TakeBack });
        assert_eq!(interpret("give me a hint", Some(START_FEN)), VoiceIntent::Command { command: VoiceCommand::Hint });
        assert_eq!(interpret("why?", None), VoiceIntent::Command { command: VoiceCommand::Explain });
        assert!(matches!(interpret("Why is e5 a bad square for my knight?", Some(START_FEN)), VoiceIntent::Question { .. }));
        assert!(matches!(interpret("What should I play here", Some(START_FEN)), VoiceIntent::Question { .. }));
        assert_eq!(interpret("knight e5", Some(START_FEN)), VoiceIntent::NoMatch { heard: "knight e5".into() });
        assert_eq!(interpret("  ", Some(START_FEN)), VoiceIntent::Empty);
    }
}
