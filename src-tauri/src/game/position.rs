//! Chess rules for the app, built on shakmaty. The UI never computes legal
//! moves itself: it asks for a [`PositionInfo`] and draws what it gets.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use shakmaty::fen::Fen;
use shakmaty::san::SanPlus;
use shakmaty::uci::UciMove;
use shakmaty::zobrist::Zobrist64;
use shakmaty::{CastlingMode, Chess, Color, EnPassantMode, Move, Position, Role, Square};
use specta::Type;

use crate::error::{Error, Result};

pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::White => Side::Black,
            Side::Black => Side::White,
        }
    }
}

impl From<Color> for Side {
    fn from(c: Color) -> Self {
        match c {
            Color::White => Side::White,
            Color::Black => Side::Black,
        }
    }
}

impl From<Side> for Color {
    fn from(s: Side) -> Self {
        match s {
            Side::White => Color::White,
            Side::Black => Color::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum EndReason {
    Checkmate,
    Stalemate,
    InsufficientMaterial,
    ThreefoldRepetition,
    FiftyMoveRule,
    Timeout,
    TimeoutVsInsufficientMaterial,
    Resignation,
    Agreement,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GameOutcome {
    /// "1-0", "0-1" or "1/2-1/2".
    pub result: String,
    pub winner: Option<Side>,
    pub reason: EndReason,
}

impl GameOutcome {
    pub fn win(winner: Side, reason: EndReason) -> Self {
        let result = match winner {
            Side::White => "1-0",
            Side::Black => "0-1",
        };
        GameOutcome { result: result.into(), winner: Some(winner), reason }
    }

    pub fn draw(reason: EndReason) -> Self {
        GameOutcome { result: "1/2-1/2".into(), winner: None, reason }
    }

    pub fn aborted() -> Self {
        GameOutcome { result: "*".into(), winner: None, reason: EndReason::Aborted }
    }
}

/// Everything the board needs to draw a position.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PositionInfo {
    pub fen: String,
    pub turn: Side,
    /// Legal moves as origin square -> destination squares (chessground's `dests`).
    pub dests: BTreeMap<String, Vec<String>>,
    pub check: bool,
    pub fullmoves: u32,
    pub halfmoves: u32,
    /// Set when the game is over by the rules (mate, stalemate, material).
    pub outcome: Option<GameOutcome>,
}

/// A move applied to a position.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppliedMove {
    /// Normalised UCI with standard castling notation (e1g1).
    pub uci: String,
    pub san: String,
    pub position: PositionInfo,
}

pub fn parse_fen(fen: &str) -> Result<Chess> {
    let fen: Fen = fen.trim().parse().map_err(|e| Error::Chess(format!("invalid FEN: {e}")))?;
    fen.into_position(CastlingMode::Standard).map_err(|e| Error::Chess(format!("illegal position: {e}")))
}

pub fn fen_of(pos: &Chess) -> String {
    Fen::from_position(pos, EnPassantMode::Legal).to_string()
}

pub fn hash_of(pos: &Chess) -> u64 {
    pos.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0
}

pub fn uci_of(m: Move) -> String {
    UciMove::from_standard(m).to_string()
}

pub fn parse_uci(pos: &Chess, uci: &str) -> Result<Move> {
    let parsed: UciMove = uci.parse().map_err(|_| Error::Chess(format!("bad move: {uci}")))?;
    parsed.to_move(pos).map_err(|_| Error::Chess(format!("illegal move: {uci}")))
}

pub fn parse_san(pos: &Chess, san: &str) -> Result<Move> {
    let parsed: SanPlus = san.parse().map_err(|_| Error::Chess(format!("bad move: {san}")))?;
    parsed.san.to_move(pos).map_err(|_| Error::Chess(format!("illegal move: {san}")))
}

pub fn san_of(pos: &Chess, m: Move) -> String {
    SanPlus::from_move(pos.clone(), m).to_string()
}

/// Legal destinations per origin square. Castling is offered both as the
/// king's two-square move (e1g1) and as king-onto-rook (e1h1), like lichess.
pub fn dests(pos: &Chess) -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in pos.legal_moves() {
        let Some(from) = m.from() else { continue };
        let entry = map.entry(from.to_string()).or_default();
        let mut push = |sq: Square| {
            let s = sq.to_string();
            if !entry.contains(&s) {
                entry.push(s);
            }
        };
        match m {
            Move::Castle { rook, .. } => {
                let side = m.castling_side().expect("castle has a side");
                push(side.king_to(pos.turn()));
                push(rook);
            }
            _ => push(m.to()),
        }
    }
    map
}

/// Outcome decided by the position alone (no history or clocks).
pub fn rules_outcome(pos: &Chess) -> Option<GameOutcome> {
    if pos.is_checkmate() {
        Some(GameOutcome::win(Side::from(!pos.turn()), EndReason::Checkmate))
    } else if pos.is_stalemate() {
        Some(GameOutcome::draw(EndReason::Stalemate))
    } else if pos.is_insufficient_material() {
        Some(GameOutcome::draw(EndReason::InsufficientMaterial))
    } else {
        None
    }
}

pub fn position_info(pos: &Chess) -> PositionInfo {
    PositionInfo {
        fen: fen_of(pos),
        turn: pos.turn().into(),
        dests: dests(pos),
        check: pos.is_check(),
        fullmoves: pos.fullmoves().get(),
        halfmoves: pos.halfmoves(),
        outcome: rules_outcome(pos),
    }
}

pub fn apply_uci(pos: &Chess, uci: &str) -> Result<(Chess, AppliedMove)> {
    let m = parse_uci(pos, uci)?;
    let san = san_of(pos, m);
    let mut next = pos.clone();
    next.play_unchecked(m);
    let applied = AppliedMove { uci: uci_of(m), san, position: position_info(&next) };
    Ok((next, applied))
}

/// True when a side has no pieces that could ever deliver mate. Used when a
/// flag falls: running out of time against a lone king is a draw.
pub fn cannot_mate(pos: &Chess, side: Side) -> bool {
    pos.has_insufficient_material(side.into())
}

/// Material from White's point of view, in pawns (P=1, N=B=3, R=5, Q=9).
pub fn material_balance(pos: &Chess) -> i32 {
    let board = pos.board();
    let value = |role: Role| match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0,
    };
    let mut total = 0;
    for (_, piece) in board.iter() {
        let v = value(piece.role);
        total += if piece.color == Color::White { v } else { -v };
    }
    total
}

/// Tracks positions since the last capture or pawn move, to detect
/// threefold repetition and the fifty-move rule.
#[derive(Debug, Clone, Default)]
pub struct RepetitionHistory {
    hashes: Vec<u64>,
}

impl RepetitionHistory {
    pub fn new(start: &Chess) -> Self {
        RepetitionHistory { hashes: vec![hash_of(start)] }
    }

    /// Record the position reached after `m` was played.
    pub fn push(&mut self, m: Move, after: &Chess) {
        if m.is_zeroing() {
            self.hashes.clear();
        }
        self.hashes.push(hash_of(after));
    }

    pub fn is_threefold(&self) -> bool {
        match self.hashes.last() {
            Some(last) => self.hashes.iter().filter(|h| *h == last).count() >= 3,
            None => false,
        }
    }

    /// Outcome including rules that need history.
    pub fn outcome(&self, pos: &Chess) -> Option<GameOutcome> {
        rules_outcome(pos).or_else(|| {
            if self.is_threefold() {
                Some(GameOutcome::draw(EndReason::ThreefoldRepetition))
            } else if pos.halfmoves() >= 100 {
                Some(GameOutcome::draw(EndReason::FiftyMoveRule))
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(pos: &Chess, ucis: &[&str]) -> Chess {
        let mut p = pos.clone();
        for u in ucis {
            p = apply_uci(&p, u).unwrap().0;
        }
        p
    }

    #[test]
    fn start_position_has_twenty_moves() {
        let pos = parse_fen(START_FEN).unwrap();
        let d = dests(&pos);
        assert_eq!(d.values().map(Vec::len).sum::<usize>(), 20);
        assert_eq!(d["g1"], vec!["f3", "h3"]);
    }

    #[test]
    fn castling_offers_both_targets_and_normalises_uci() {
        let pos = parse_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
        let d = dests(&pos);
        let king = &d["e1"];
        for sq in ["g1", "h1", "c1", "a1"] {
            assert!(king.contains(&sq.to_string()), "missing {sq} in {king:?}");
        }
        let (_, via_rook) = apply_uci(&pos, "e1h1").unwrap();
        assert_eq!(via_rook.uci, "e1g1");
        assert_eq!(via_rook.san, "O-O");
        let (_, two_step) = apply_uci(&pos, "e1c1").unwrap();
        assert_eq!(two_step.san, "O-O-O");
    }

    #[test]
    fn rejects_illegal_moves() {
        let pos = parse_fen(START_FEN).unwrap();
        assert!(apply_uci(&pos, "e2e5").is_err());
        assert!(apply_uci(&pos, "zz").is_err());
        assert!(parse_fen("not a fen").is_err());
    }

    #[test]
    fn detects_checkmate_and_san_suffix() {
        let start = parse_fen(START_FEN).unwrap();
        let p = play(&start, &["f2f3", "e7e5", "g2g4"]);
        let (after, applied) = apply_uci(&p, "d8h4").unwrap();
        assert_eq!(applied.san, "Qh4#");
        let outcome = rules_outcome(&after).unwrap();
        assert_eq!(outcome.result, "0-1");
        assert_eq!(outcome.reason, EndReason::Checkmate);
        assert!(applied.position.dests.is_empty());
    }

    #[test]
    fn detects_stalemate_and_insufficient_material() {
        let stalemate = parse_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1").unwrap();
        assert_eq!(rules_outcome(&stalemate).unwrap().reason, EndReason::Stalemate);
        let bare = parse_fen("8/8/4k3/8/8/3K4/8/8 w - - 0 1").unwrap();
        assert_eq!(rules_outcome(&bare).unwrap().reason, EndReason::InsufficientMaterial);
    }

    #[test]
    fn threefold_repetition_with_knight_shuffle() {
        let start = parse_fen(START_FEN).unwrap();
        let mut hist = RepetitionHistory::new(&start);
        let mut pos = start.clone();
        let shuffle = ["g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6", "f3g1", "f6g8"];
        for (i, u) in shuffle.iter().enumerate() {
            let m = parse_uci(&pos, u).unwrap();
            pos.play_unchecked(m);
            hist.push(m, &pos);
            let expect = i == shuffle.len() - 1;
            assert_eq!(hist.is_threefold(), expect, "after move {i}");
        }
        assert_eq!(hist.outcome(&pos).unwrap().reason, EndReason::ThreefoldRepetition);
    }

    #[test]
    fn pawn_move_resets_repetition() {
        let start = parse_fen(START_FEN).unwrap();
        let mut hist = RepetitionHistory::new(&start);
        let mut pos = start;
        for u in ["g1f3", "g8f6", "f3g1", "f6g8", "e2e4"] {
            let m = parse_uci(&pos, u).unwrap();
            pos.play_unchecked(m);
            hist.push(m, &pos);
        }
        assert_eq!(hist.hashes.len(), 1);
    }

    #[test]
    fn fifty_move_rule() {
        let pos = parse_fen("8/8/4k3/8/8/3K4/8/7R w - - 100 80").unwrap();
        let hist = RepetitionHistory::new(&pos);
        assert_eq!(hist.outcome(&pos).unwrap().reason, EndReason::FiftyMoveRule);
    }

    #[test]
    fn material_and_mating_material() {
        let pos = parse_fen("8/8/4k3/8/8/3K4/8/7R w - - 0 1").unwrap();
        assert_eq!(material_balance(&pos), 5);
        assert!(cannot_mate(&pos, Side::Black));
        assert!(!cannot_mate(&pos, Side::White));
    }
}
