//! Facts about positions and lines, worked out by the rules (shakmaty), never
//! guessed: material, checks and mates, hanging pieces, forks and pins. The
//! coach's narration is built on these, so its words match the board.

use serde::Serialize;
use shakmaty::{attacks, Bitboard, Chess, Color, Move, Piece, Position, Role, Square};

use crate::game::position::{parse_san, san_of};

pub fn value(role: Role) -> i32 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 0,
    }
}

pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::Pawn => "pawn",
        Role::Knight => "knight",
        Role::Bishop => "bishop",
        Role::Rook => "rook",
        Role::Queen => "queen",
        Role::King => "king",
    }
}

pub fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "White",
        Color::Black => "Black",
    }
}

/// Material for `color` minus the opponent's, in pawns.
pub fn material_for(pos: &Chess, color: Color) -> i32 {
    let mut total = 0;
    for (_, piece) in pos.board().iter() {
        let v = value(piece.role);
        total += if piece.color == color { v } else { -v };
    }
    total
}

/// A piece standing on a square, for descriptions like "the knight on f3".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PieceAt {
    pub role: &'static str,
    pub square: String,
}

fn piece_at(square: Square, role: Role) -> PieceAt {
    PieceAt { role: role_name(role), square: square.to_string() }
}

/// Pieces of `color` that the opponent can win: attacked and undefended, or
/// attacked by a cheaper piece.
pub fn hanging(pos: &Chess, color: Color) -> Vec<PieceAt> {
    let board = pos.board();
    let occupied = board.occupied();
    let mut out = Vec::new();
    for sq in board.by_color(color) {
        let Some(piece) = board.piece_at(sq) else { continue };
        if piece.role == Role::King {
            continue;
        }
        let attackers = board.attacks_to(sq, !color, occupied);
        if attackers.is_empty() {
            continue;
        }
        let defenders = board.attacks_to(sq, color, occupied);
        let cheapest = attackers.into_iter().filter_map(|a| board.role_at(a)).map(value).min().unwrap_or(99);
        if defenders.is_empty() || cheapest < value(piece.role) {
            out.push(piece_at(sq, piece.role));
        }
    }
    out
}

/// Pieces of `color` pinned to their king (they can't legally move off the
/// line) or to their queen (moving would lose it).
pub fn pinned(pos: &Chess, color: Color) -> Vec<(PieceAt, &'static str)> {
    let board = pos.board();
    let occupied = board.occupied();
    let mut out = Vec::new();
    let targets: Vec<(Square, &'static str)> = board
        .king_of(color)
        .map(|k| (k, "king"))
        .into_iter()
        .chain((board.by_color(color) & board.queens()).into_iter().map(|q| (q, "queen")))
        .collect();
    for (target, label) in targets {
        let enemy = board.by_color(!color);
        let sliders = (attacks::bishop_attacks(target, Bitboard::EMPTY) & enemy & board.bishops_and_queens())
            | (attacks::rook_attacks(target, Bitboard::EMPTY) & enemy & board.rooks_and_queens());
        for slider in sliders {
            // A queen isn't "pinned" to itself by another queen in a useful sense.
            if label == "queen" && board.role_at(slider) == Some(Role::Queen) {
                continue;
            }
            let blockers = attacks::between(target, slider) & occupied;
            if blockers.count() == 1 {
                let sq = blockers.first().expect("one blocker");
                if board.color_at(sq) == Some(color) {
                    if let Some(role) = board.role_at(sq) {
                        if role != Role::King {
                            out.push((piece_at(sq, role), label));
                        }
                    }
                }
            }
        }
    }
    out
}

/// Valuable enemy pieces the piece on `sq` attacks (a fork when two or more).
pub fn attacked_targets(pos: &Chess, sq: Square) -> Vec<PieceAt> {
    let board = pos.board();
    let Some(piece) = board.piece_at(sq) else { return vec![] };
    let mut out = Vec::new();
    for t in attacks::attacks(sq, piece, board.occupied()) & board.by_color(!piece.color) {
        let Some(role) = board.role_at(t) else { continue };
        let defended = !board.attacks_to(t, !piece.color, board.occupied()).is_empty();
        if role == Role::King || value(role) > value(piece.role) || (!defended && role != Role::Pawn) {
            out.push(piece_at(t, role));
        }
    }
    out
}

/// What happened over a line of moves, from the first mover's point of view.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LineFacts {
    /// The moves that could be played (the line stops at the first illegal one).
    pub san: Vec<String>,
    /// Material change for the first mover over the line, in pawns.
    pub material_change: i32,
    pub captures: Vec<String>,
    pub checks: u32,
    /// The line ends in checkmate.
    pub mate: bool,
    pub promotes: bool,
    /// After the first move: enemy pieces it attacks (two or more is a fork).
    pub first_move_attacks: Vec<PieceAt>,
    /// Pieces left hanging at the end of the line, by side.
    pub hanging_after: Vec<(String, PieceAt)>,
}

/// Play a SAN line from `pos` and describe it.
pub fn line_facts(pos: &Chess, sans: &[String]) -> LineFacts {
    let mover = pos.turn();
    let start = material_for(pos, mover);
    let mut p = pos.clone();
    let mut facts = LineFacts::default();
    // Material before the line's last move, in case that move is a capture
    // the line stops before answering.
    let mut before_last = start;
    let mut last_capture_on = None;
    for (i, san) in sans.iter().enumerate() {
        let Ok(m) = parse_san(&p, san) else { break };
        before_last = material_for(&p, mover);
        last_capture_on = m.capture().map(|_| m.to());
        if let Some(captured) = m.capture() {
            facts.captures.push(format!("{} takes the {}", san_of(&p, m), role_name(captured)));
        }
        if matches!(m, Move::Normal { promotion: Some(_), .. }) {
            facts.promotes = true;
        }
        facts.san.push(san_of(&p, m));
        p.play_unchecked(m);
        if p.is_check() {
            facts.checks += 1;
        }
        if i == 0 {
            facts.first_move_attacks = attacked_targets(&p, m.to());
        }
    }
    facts.mate = p.is_checkmate();
    facts.material_change = material_for(&p, mover) - start;
    // A line that stops right after a capture the other side can answer by
    // taking back stops mid-exchange, so only count what holds either way.
    let can_take_back = last_capture_on.is_some_and(|sq| !p.board().attacks_to(sq, p.turn(), p.board().occupied()).is_empty());
    if can_take_back && !facts.mate {
        let earlier = before_last - start;
        if earlier.signum() != facts.material_change.signum() {
            facts.material_change = 0;
        } else if earlier.abs() < facts.material_change.abs() {
            facts.material_change = earlier;
        }
    }
    for color in [Color::White, Color::Black] {
        for h in hanging(&p, color) {
            facts.hanging_after.push((color_name(color).to_string(), h));
        }
    }
    facts
}

/// Plain words for winning chances (0..100) from one side's point of view.
pub fn eval_words(win: f64) -> &'static str {
    match win {
        w if w >= 97.0 => "winning",
        w if w >= 80.0 => "clearly better",
        w if w >= 62.0 => "a little better",
        w if w > 38.0 => "about equal",
        w if w > 20.0 => "a little worse",
        w if w > 3.0 => "clearly worse",
        _ => "losing",
    }
}

/// Opening, middlegame or endgame, by the material left.
pub fn phase(pos: &Chess) -> &'static str {
    let board = pos.board();
    let non_pawn: i32 = board.iter().filter(|(_, p)| !matches!(p.role, Role::Pawn | Role::King)).map(|(_, p)| value(p.role)).sum();
    let fullmoves = pos.fullmoves().get();
    if fullmoves <= 10 && non_pawn >= 56 {
        "opening"
    } else if non_pawn <= 26 {
        "endgame"
    } else {
        "middlegame"
    }
}

/// A short description of a square's piece, e.g. "Black's knight on c6".
pub fn describe(piece: Piece, sq: Square) -> String {
    format!("{}'s {} on {}", color_name(piece.color), role_name(piece.role), sq)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::parse_fen;

    #[test]
    fn finds_hanging_pieces() {
        // White's knight on e5 is attacked by the d6 pawn and undefended, and
        // so is the e4 pawn (by the f6 knight).
        let pos = parse_fen("rnbqkb1r/ppp2ppp/3p1n2/4N3/4P3/8/PPPP1PPP/RNBQKB1R w KQkq - 0 4").unwrap();
        let h = hanging(&pos, Color::White);
        assert_eq!(h, vec![PieceAt { role: "pawn", square: "e4".into() }, PieceAt { role: "knight", square: "e5".into() }]);
        assert!(hanging(&pos, Color::Black).is_empty(), "f7 is attacked but defended by the king");
    }

    #[test]
    fn finds_pins_to_the_king() {
        // The Bb5 pins the c6 knight to the e8 king.
        let pos = parse_fen("r1bqkbnr/pp1ppppp/2n5/1Bp5/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3").unwrap();
        let p = pinned(&pos, Color::Black);
        assert!(p.is_empty(), "the d7 pawn blocks: c6 isn't pinned yet");
        let pos = parse_fen("r1bqkbnr/pp2pppp/2np4/1Bp5/4P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 0 4").unwrap();
        let p = pinned(&pos, Color::Black);
        assert_eq!(p, vec![(PieceAt { role: "knight", square: "c6".into() }, "king")]);
    }

    #[test]
    fn a_line_cut_mid_exchange_claims_no_material() {
        // 1.e4 d5: after exd5 Qxd5 Nc3 the line could stop at ...Qxd5, with
        // White's pawn taken back. Cut after exd5 alone it looks like a pawn
        // won, but d5 can be taken back, so it isn't claimed.
        let pos = parse_fen("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2").unwrap();
        let line = |sans: &[&str]| line_facts(&pos, &sans.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(line(&["exd5"]).material_change, 0);
        assert_eq!(line(&["exd5", "Qxd5"]).material_change, 0);
        assert_eq!(line(&["exd5", "Qxd5", "Nc3"]).material_change, 0);
        // A clean capture still counts: 2.Ba6?? Nxa6, and a6 can't be retaken.
        let f = line(&["Ba6", "Nxa6"]);
        assert_eq!(f.material_change, -3);
    }

    #[test]
    fn describes_a_line_with_a_fork() {
        // 1... Nc2+ forks the king on e1 and the rook on a1.
        let pos = parse_fen("r1bqkbnr/pppp1ppp/8/4p3/1n2P3/8/PPPP1PPP/RNBQKBNR b KQkq - 1 3").unwrap();
        let f = line_facts(&pos, &["Nxc2+".into(), "Ke2".into(), "Nxa1".into()]);
        assert_eq!(f.san, vec!["Nxc2+", "Ke2", "Nxa1"]);
        assert_eq!(f.checks, 1);
        assert_eq!(f.material_change, 6, "a pawn and a rook");
        let targets: Vec<&str> = f.first_move_attacks.iter().map(|t| t.role).collect();
        assert!(targets.contains(&"king") && targets.contains(&"rook"), "{targets:?}");
    }

    #[test]
    fn spots_mate_and_stops_at_illegal_moves() {
        let pos = parse_fen("rnbqkbnr/pppp1ppp/8/4p3/6P1/5P2/PPPPP2P/RNBQKBNR b KQkq g3 0 2").unwrap();
        let f = line_facts(&pos, &["Qh4#".into(), "e4".into()]);
        assert!(f.mate);
        assert_eq!(f.san, vec!["Qh4#"]);
    }

    #[test]
    fn words_and_phases() {
        assert_eq!(eval_words(50.0), "about equal");
        assert_eq!(eval_words(99.0), "winning");
        assert_eq!(eval_words(10.0), "clearly worse");
        assert_eq!(phase(&Chess::default()), "opening");
        assert_eq!(phase(&parse_fen("8/5k2/8/8/8/8/2K5/4R3 w - - 0 50").unwrap()), "endgame");
    }
}
