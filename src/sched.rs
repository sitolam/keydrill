//! Scheduling: SM-2, the algorithm behind SuperMemo and Anki's older mode.
//!
//! Pure functions over [`CardState`] — no clock, no files. `now` is passed in
//! so the tests can talk about "in three days" without waiting.

use serde::{Deserialize, Serialize};

pub const DAY: i64 = 24 * 60 * 60;

/// Ease never drops below this in SM-2; lower and a card you keep failing
/// would be scheduled more often than is useful.
const MIN_EASE: f32 = 1.3;
const START_EASE: f32 = 2.5;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct CardState {
    /// Consecutive correct answers. Reset to zero by a miss.
    pub reps: u32,
    pub ease: f32,
    /// Days until this card is due again.
    pub interval: f32,
    /// Unix seconds. A card with `due <= now` is waiting for you.
    pub due: i64,
    /// How many times this card has been forgotten after being learned. Not
    /// used for scheduling; it is what the summary screen calls a weak spot.
    pub lapses: u32,
}

impl Default for CardState {
    fn default() -> Self {
        CardState {
            reps: 0,
            ease: START_EASE,
            interval: 0.0,
            due: 0, // a new card is due immediately
            lapses: 0,
        }
    }
}

impl CardState {
    pub fn is_new(&self) -> bool {
        self.reps == 0 && self.lapses == 0
    }

    pub fn is_due(&self, now: i64) -> bool {
        self.due <= now
    }
}

/// One answer. `correct` is the only signal — a shortcut drill has no
/// meaningful "hard but right", you either pressed it or you did not.
pub fn review(state: CardState, correct: bool, now: i64) -> CardState {
    let mut next = state;

    if !correct {
        next.reps = 0;
        next.ease = (state.ease - 0.2).max(MIN_EASE);
        next.interval = 0.0;
        // Due immediately: a missed card comes back inside the same session.
        next.due = now;
        if !state.is_new() {
            next.lapses = state.lapses + 1;
        }
        return next;
    }

    next.reps = state.reps + 1;
    next.ease = (state.ease + 0.1).min(3.0);
    next.interval = match next.reps {
        1 => 1.0,
        2 => 3.0,
        _ => (state.interval.max(1.0) * state.ease).min(365.0),
    };
    next.due = now + (next.interval * DAY as f32) as i64;
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    #[test]
    fn a_new_card_is_due() {
        assert!(CardState::default().is_due(NOW));
        assert!(CardState::default().is_new());
    }

    #[test]
    fn correct_answers_push_the_card_further_out() {
        let first = review(CardState::default(), true, NOW);
        assert_eq!(first.interval, 1.0);

        let second = review(first, true, NOW);
        assert_eq!(second.interval, 3.0);

        let third = review(second, true, NOW);
        assert!(third.interval > second.interval);
        assert_eq!(third.due, NOW + (third.interval * DAY as f32) as i64);
    }

    #[test]
    fn a_miss_brings_the_card_straight_back() {
        let learned = review(review(CardState::default(), true, NOW), true, NOW);
        let missed = review(learned, false, NOW);

        assert_eq!(missed.reps, 0);
        assert_eq!(missed.interval, 0.0);
        assert!(missed.is_due(NOW));
        assert_eq!(missed.lapses, 1, "forgetting a learned card is a lapse");
        assert!(missed.ease < learned.ease);
    }

    #[test]
    fn missing_a_brand_new_card_is_not_a_lapse() {
        // You cannot forget what you never knew, and counting it as a weak
        // spot would make every first session look like a disaster.
        let missed = review(CardState::default(), false, NOW);
        assert_eq!(missed.lapses, 0);
    }

    #[test]
    fn ease_has_a_floor() {
        let mut state = CardState::default();
        for _ in 0..20 {
            state = review(state, false, NOW);
        }
        assert!(state.ease >= MIN_EASE);
    }
}
