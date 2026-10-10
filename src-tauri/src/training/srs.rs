//! Spaced repetition (FSRS) for repertoire cards and similar drills.
//! Wraps `rs-fsrs` and stores cards as plain columns.

use chrono::{DateTime, TimeZone, Utc};
use rs_fsrs::{Card, Rating, State, FSRS};
use serde::{Deserialize, Serialize};
use specta::Type;

/// How the user did on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Grade {
    /// Wrong move.
    Again,
    /// Right, but only after a wrong try or a long think.
    Hard,
    Good,
    Easy,
}

impl From<Grade> for Rating {
    fn from(g: Grade) -> Self {
        match g {
            Grade::Again => Rating::Again,
            Grade::Hard => Rating::Hard,
            Grade::Good => Rating::Good,
            Grade::Easy => Rating::Easy,
        }
    }
}

/// A card's scheduling state, as stored in the database (times in unix seconds).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CardState {
    pub due: f64,
    pub stability: f64,
    pub difficulty: f64,
    pub elapsed_days: i32,
    pub scheduled_days: i32,
    pub reps: i32,
    pub lapses: i32,
    /// 0 new, 1 learning, 2 review, 3 relearning.
    pub state: i32,
    pub last_review: Option<f64>,
}

fn ts(secs: f64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs as i64, 0).single().unwrap_or_else(Utc::now)
}

impl CardState {
    pub fn new(now: i64) -> Self {
        CardState {
            due: now as f64,
            stability: 0.0,
            difficulty: 0.0,
            elapsed_days: 0,
            scheduled_days: 0,
            reps: 0,
            lapses: 0,
            state: 0,
            last_review: None,
        }
    }

    fn to_card(self) -> Card {
        Card {
            due: ts(self.due),
            stability: self.stability,
            difficulty: self.difficulty,
            elapsed_days: self.elapsed_days as i64,
            scheduled_days: self.scheduled_days as i64,
            reps: self.reps,
            lapses: self.lapses,
            state: match self.state {
                1 => State::Learning,
                2 => State::Review,
                3 => State::Relearning,
                _ => State::New,
            },
            last_review: self.last_review.map(ts).unwrap_or_else(|| ts(self.due)),
        }
    }

    fn from_card(c: &Card) -> Self {
        CardState {
            due: c.due.timestamp() as f64,
            stability: c.stability,
            difficulty: c.difficulty,
            elapsed_days: c.elapsed_days as i32,
            scheduled_days: c.scheduled_days as i32,
            reps: c.reps,
            lapses: c.lapses,
            state: c.state as i32,
            last_review: Some(c.last_review.timestamp() as f64),
        }
    }

    pub fn is_new(&self) -> bool {
        self.state == 0
    }
}

/// Schedule the next review of a card after a grade at time `now` (unix seconds).
pub fn review(card: CardState, grade: Grade, now: i64) -> CardState {
    let info = FSRS::default().next(card.to_card(), ts(now as f64), grade.into());
    CardState::from_card(&info.card)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    #[test]
    fn good_answers_push_reviews_further_out() {
        let t0 = 1_800_000_000;
        let mut card = CardState::new(t0);
        assert!(card.is_new());
        let mut now = t0;
        let mut gaps = Vec::new();
        for _ in 0..4 {
            card = review(card, Grade::Good, now);
            gaps.push(card.due as i64 - now);
            now = card.due as i64;
        }
        assert!(!card.is_new());
        assert!(gaps.windows(2).all(|w| w[1] >= w[0]), "intervals grow: {gaps:?}");
        assert!(gaps[3] >= 3 * DAY, "after a few good answers the card waits days: {gaps:?}");
    }

    #[test]
    fn a_wrong_answer_brings_the_card_back_soon() {
        let t0 = 1_800_000_000;
        let mut card = CardState::new(t0);
        let mut now = t0;
        for _ in 0..4 {
            card = review(card, Grade::Good, now);
            now = card.due as i64;
        }
        let before_lapses = card.lapses;
        let lapsed = review(card, Grade::Again, now);
        assert_eq!(lapsed.lapses, before_lapses + 1);
        assert!((lapsed.due as i64 - now) < DAY, "relearn within a day");
        let easy = review(card, Grade::Easy, now);
        let good = review(card, Grade::Good, now);
        assert!(easy.due >= good.due);
    }
}
