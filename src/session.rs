//! One drilling session: what to ask, in what order, and how it went.
//!
//! Deliberately free of terminal and filesystem concerns — a session takes a
//! deck and the saved states, and answers questions about them. That is what
//! makes the ordering rules testable.

use std::collections::VecDeque;

use rand::seq::SliceRandom;

use crate::deck::{Card, Deck};
use crate::keys::Combo;
use crate::sched::{review, CardState};
use crate::store::Store;

/// How far back a missed card goes. Far enough that you cannot answer from
/// short-term memory, close enough to still be in this session.
const REQUEUE_DISTANCE: usize = 4;

#[derive(Debug, PartialEq)]
pub enum Answer {
    Correct,
    /// Carries what the card wanted, so the UI can show it without reaching
    /// back into the deck.
    Wrong {
        expected: Vec<Combo>,
    },
}

pub struct Session {
    pub deck: Deck,
    pub states: Vec<CardState>,
    queue: VecDeque<usize>,
    /// Cards missed at least once this session; they are the summary's weak
    /// spots and the reason accuracy is scored on first attempts.
    missed: Vec<usize>,
    seen: Vec<usize>,
    pub answered: usize,
    pub correct: usize,
    pub streak: usize,
    pub best_streak: usize,
}

impl Session {
    /// Due cards first, weakest first, then new ones — so a session opens
    /// with what you are about to forget rather than what you have never met.
    pub fn build(
        deck: Deck,
        store: &Store,
        now: i64,
        limit: Option<usize>,
        category: Option<&str>,
    ) -> Session {
        let states: Vec<CardState> = deck
            .cards
            .iter()
            .map(|card| store.get(&deck.name, &card.description))
            .collect();

        let mut due: Vec<usize> = Vec::new();
        let mut fresh: Vec<usize> = Vec::new();

        for (index, card) in deck.cards.iter().enumerate() {
            if let Some(wanted) = category {
                if card.category() != wanted {
                    continue;
                }
            }
            if !states[index].is_due(now) {
                continue;
            }
            if states[index].is_new() {
                fresh.push(index);
            } else {
                due.push(index);
            }
        }

        let mut rng = rand::thread_rng();
        due.shuffle(&mut rng);
        fresh.shuffle(&mut rng);
        // Within the due pile, the cards you have failed most come first.
        due.sort_by_key(|&i| std::cmp::Reverse(states[i].lapses));

        let mut order: Vec<usize> = due;
        order.extend(fresh);
        if let Some(limit) = limit {
            order.truncate(limit);
        }

        Session {
            deck,
            states,
            queue: order.into(),
            missed: Vec::new(),
            seen: Vec::new(),
            answered: 0,
            correct: 0,
            streak: 0,
            best_streak: 0,
        }
    }

    pub fn current(&self) -> Option<&Card> {
        self.queue.front().map(|&i| &self.deck.cards[i])
    }

    pub fn is_finished(&self) -> bool {
        self.queue.is_empty()
    }

    /// Cards left in the queue, including ones waiting for a second attempt.
    pub fn remaining(&self) -> usize {
        self.queue.len()
    }

    pub fn total_seen(&self) -> usize {
        self.seen.len()
    }

    /// First-attempt accuracy: answering correctly only after being shown the
    /// answer is not knowing it.
    pub fn accuracy(&self) -> f64 {
        if self.seen.is_empty() {
            return 0.0;
        }
        let right = self
            .seen
            .iter()
            .filter(|i| !self.missed.contains(i))
            .count();
        right as f64 / self.seen.len() as f64
    }

    pub fn weak_spots(&self) -> Vec<&Card> {
        self.missed.iter().map(|&i| &self.deck.cards[i]).collect()
    }

    pub fn answer(&mut self, pressed: &Combo, now: i64) -> Answer {
        let Some(&index) = self.queue.front() else {
            return Answer::Correct;
        };
        let card = &self.deck.cards[index];
        let correct = card.accepts(pressed);

        self.answered += 1;
        if !self.seen.contains(&index) {
            self.seen.push(index);
        }

        // A card's scheduling is only updated on its first attempt. Otherwise
        // the retry after seeing the answer would overwrite the fact that you
        // did not know it.
        let first_attempt = !self.missed.contains(&index);
        if first_attempt {
            self.states[index] = review(self.states[index], correct, now);
        }

        if correct {
            self.correct += 1;
            self.streak += 1;
            self.best_streak = self.best_streak.max(self.streak);
            self.queue.pop_front();
            Answer::Correct
        } else {
            self.streak = 0;
            if !self.missed.contains(&index) {
                self.missed.push(index);
            }
            self.requeue();
            Answer::Wrong {
                expected: self.deck.cards[index].combos(),
            }
        }
    }

    /// Skipped cards are treated as missed: pressing F5 rather than the key
    /// is not knowing it either.
    pub fn skip(&mut self, now: i64) {
        let Some(&index) = self.queue.front() else {
            return;
        };
        if !self.seen.contains(&index) {
            self.seen.push(index);
        }
        if !self.missed.contains(&index) {
            self.missed.push(index);
            self.states[index] = review(self.states[index], false, now);
        }
        self.streak = 0;
        self.requeue();
    }

    fn requeue(&mut self) {
        let Some(index) = self.queue.pop_front() else {
            return;
        };
        let position = REQUEUE_DISTANCE.min(self.queue.len());
        self.queue.insert(position, index);
    }

    /// Hands the session's scheduling back to the store. Called once, at the
    /// end, so an abandoned session still saves what it learned.
    pub fn commit(&self, store: &mut Store) {
        for (index, card) in self.deck.cards.iter().enumerate() {
            if self.seen.contains(&index) {
                store.set(&self.deck.name, &card.description, self.states[index]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::Card;

    const NOW: i64 = 1_700_000_000;

    fn deck(count: usize) -> Deck {
        Deck {
            name: "test".into(),
            description: None,
            cards: (0..count)
                .map(|i| Card {
                    description: format!("card {i}"),
                    keys: vec![format!("meta+{}", (b'a' + i as u8) as char)],
                    category: Some("Navigation".into()),
                })
                .collect(),
        }
    }

    fn key(index: usize) -> Combo {
        format!("meta+{}", (b'a' + index as u8) as char)
            .parse()
            .unwrap()
    }

    fn answer_current_correctly(session: &mut Session) -> Answer {
        let wanted = session.current().unwrap().keys[0].parse().unwrap();
        session.answer(&wanted, NOW)
    }

    #[test]
    fn a_fresh_deck_queues_every_card() {
        let session = Session::build(deck(5), &Store::default(), NOW, None, None);
        assert_eq!(session.remaining(), 5);
        assert!(!session.is_finished());
    }

    #[test]
    fn the_limit_caps_the_queue() {
        let session = Session::build(deck(20), &Store::default(), NOW, Some(3), None);
        assert_eq!(session.remaining(), 3);
    }

    #[test]
    fn a_category_filter_narrows_the_deck() {
        let mut deck = deck(3);
        deck.cards[0].category = Some("Windows".into());
        let session = Session::build(deck, &Store::default(), NOW, None, Some("Windows"));
        assert_eq!(session.remaining(), 1);
    }

    #[test]
    fn a_correct_answer_retires_the_card() {
        let mut session = Session::build(deck(2), &Store::default(), NOW, None, None);
        assert_eq!(answer_current_correctly(&mut session), Answer::Correct);
        assert_eq!(session.remaining(), 1);
        assert_eq!(session.streak, 1);
    }

    #[test]
    fn a_wrong_answer_comes_back_later_in_the_same_session() {
        let mut session = Session::build(deck(6), &Store::default(), NOW, None, None);
        let missed = session.current().unwrap().description.clone();

        let answer = session.answer(&"ctrl+z".parse().unwrap(), NOW);
        assert!(matches!(answer, Answer::Wrong { .. }));

        assert_eq!(session.remaining(), 6, "the card is still owed");
        assert_ne!(session.current().unwrap().description, missed);
        assert_eq!(session.streak, 0);
    }

    #[test]
    fn accuracy_counts_the_first_attempt_only() {
        let mut session = Session::build(deck(2), &Store::default(), NOW, None, None);
        session.answer(&"ctrl+z".parse().unwrap(), NOW);
        while !session.is_finished() {
            answer_current_correctly(&mut session);
        }

        assert_eq!(session.total_seen(), 2);
        assert_eq!(session.accuracy(), 0.5);
        assert_eq!(session.weak_spots().len(), 1);
    }

    #[test]
    fn a_second_attempt_does_not_overwrite_the_miss() {
        let mut session = Session::build(deck(1), &Store::default(), NOW, None, None);
        session.answer(&"ctrl+z".parse().unwrap(), NOW);
        answer_current_correctly(&mut session);

        let mut store = Store::default();
        session.commit(&mut store);
        // Still scheduled as failed: due now, not in a day.
        assert!(store.get("test", "card 0").is_due(NOW));
    }

    #[test]
    fn skipping_counts_as_not_knowing_it() {
        let mut session = Session::build(deck(3), &Store::default(), NOW, None, None);
        session.skip(NOW);
        assert_eq!(session.weak_spots().len(), 1);
        assert_eq!(session.remaining(), 3);
    }

    #[test]
    fn only_seen_cards_are_written_back() {
        let mut session = Session::build(deck(3), &Store::default(), NOW, None, None);
        answer_current_correctly(&mut session);

        let mut store = Store::default();
        session.commit(&mut store);

        let touched = (0..3)
            .filter(|i| !store.get("test", &format!("card {i}")).is_new())
            .count();
        assert_eq!(touched, 1);
    }

    #[test]
    fn a_finished_session_answers_nothing() {
        let mut session = Session::build(deck(1), &Store::default(), NOW, None, None);
        answer_current_correctly(&mut session);
        assert!(session.is_finished());
        assert_eq!(session.answer(&key(0), NOW), Answer::Correct);
    }
}
