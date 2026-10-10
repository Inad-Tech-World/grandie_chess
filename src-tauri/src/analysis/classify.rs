//! Labels each move by how much it changed the mover's winning chances.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MoveClass {
    Brilliant,
    Great,
    Best,
    Good,
    Inaccuracy,
    Mistake,
    Blunder,
}

impl MoveClass {
    /// Matching PGN NAG, if any.
    pub fn nag(self) -> Option<u8> {
        match self {
            MoveClass::Brilliant => Some(3),
            MoveClass::Great => Some(1),
            MoveClass::Inaccuracy => Some(6),
            MoveClass::Mistake => Some(2),
            MoveClass::Blunder => Some(4),
            MoveClass::Best | MoveClass::Good => None,
        }
    }

    pub fn is_error(self) -> bool {
        matches!(self, MoveClass::Inaccuracy | MoveClass::Mistake | MoveClass::Blunder)
    }
}

/// Win-percentage drops (0..100 scale) at which a move becomes an inaccuracy,
/// mistake or blunder. Lichess's thresholds on its -1..1 winning-chances scale
/// (0.1 / 0.2 / 0.3) are 5 / 10 / 15 points here.
pub const INACCURACY: f64 = 5.0;
pub const MISTAKE: f64 = 10.0;
pub const BLUNDER: f64 = 15.0;

/// Everything needed to label one move, all from the mover's point of view.
#[derive(Debug, Clone, Copy)]
pub struct MoveFacts {
    /// Win% with the engine's best move.
    pub best_win: f64,
    /// Win% after the move actually played.
    pub played_win: f64,
    /// Win% of the engine's second choice, if it had one.
    pub second_win: Option<f64>,
    pub played_best: bool,
    /// Material (pawns) the mover is down after the move and the opponent's
    /// best reply, compared with before the move. Positive = a sacrifice.
    pub sacrificed: i32,
}

pub fn classify(f: &MoveFacts) -> MoveClass {
    let drop = (f.best_win - f.played_win).max(0.0);
    if drop >= BLUNDER {
        return MoveClass::Blunder;
    }
    if drop >= MISTAKE {
        return MoveClass::Mistake;
    }
    if drop >= INACCURACY {
        return MoveClass::Inaccuracy;
    }
    let near_best = f.played_best || drop < 1.0;
    if near_best {
        let only_move = f.second_win.is_some_and(|s| f.best_win - s >= 10.0);
        let not_already_won = f.best_win < 90.0;
        if f.sacrificed >= 2 && not_already_won && f.played_win >= 45.0 {
            return MoveClass::Brilliant;
        }
        if only_move && not_already_won {
            return MoveClass::Great;
        }
        return MoveClass::Best;
    }
    MoveClass::Good
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(best: f64, played: f64) -> MoveFacts {
        MoveFacts { best_win: best, played_win: played, second_win: None, played_best: false, sacrificed: 0 }
    }

    #[test]
    fn thresholds() {
        assert_eq!(classify(&facts(60.0, 59.5)), MoveClass::Best);
        assert_eq!(classify(&facts(60.0, 57.0)), MoveClass::Good);
        assert_eq!(classify(&facts(60.0, 55.0)), MoveClass::Inaccuracy);
        assert_eq!(classify(&facts(60.0, 50.0)), MoveClass::Mistake);
        assert_eq!(classify(&facts(60.0, 45.0)), MoveClass::Blunder);
        assert_eq!(classify(&facts(60.0, 70.0)), MoveClass::Best, "beating the engine's estimate isn't an error");
    }

    #[test]
    fn great_and_brilliant() {
        let only = MoveFacts { best_win: 55.0, played_win: 55.0, second_win: Some(30.0), played_best: true, sacrificed: 0 };
        assert_eq!(classify(&only), MoveClass::Great);
        let sac = MoveFacts { sacrificed: 3, ..only };
        assert_eq!(classify(&sac), MoveClass::Brilliant);
        let winning_anyway = MoveFacts { best_win: 97.0, played_win: 97.0, second_win: Some(80.0), played_best: true, sacrificed: 3 };
        assert_eq!(classify(&winning_anyway), MoveClass::Best);
        assert_eq!(MoveClass::Blunder.nag(), Some(4));
        assert!(MoveClass::Mistake.is_error());
    }
}
