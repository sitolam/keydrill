//! Where progress lives between sessions.
//!
//! One JSON file under `$XDG_DATA_HOME/keydrill/`, keyed by deck name and
//! card description. It is small, human-readable, and safe to delete — that
//! is the whole recovery story.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::sched::CardState;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Store {
    #[serde(default)]
    decks: HashMap<String, HashMap<String, CardState>>,
}

impl Store {
    pub fn path() -> Result<PathBuf> {
        let base = match std::env::var_os("XDG_DATA_HOME") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => {
                let home =
                    std::env::var_os("HOME").context("neither XDG_DATA_HOME nor HOME is set")?;
                PathBuf::from(home).join(".local/share")
            }
        };
        Ok(base.join("keydrill/state.json"))
    }

    /// A missing or unreadable file is an empty store, not an error: losing
    /// scheduling history should never stop you from drilling.
    pub fn load(path: &Path) -> Store {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(self)?;

        // Write-then-rename, so an interrupted save cannot leave a truncated
        // state file behind.
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, text).with_context(|| format!("cannot write {}", temp.display()))?;
        fs::rename(&temp, path).with_context(|| format!("cannot replace {}", path.display()))?;
        Ok(())
    }

    pub fn get(&self, deck: &str, card: &str) -> CardState {
        self.decks
            .get(deck)
            .and_then(|cards| cards.get(card))
            .copied()
            .unwrap_or_default()
    }

    pub fn set(&mut self, deck: &str, card: &str, state: CardState) {
        self.decks
            .entry(deck.to_string())
            .or_default()
            .insert(card.to_string(), state);
    }

    /// Cards the store knows about that the deck no longer contains. Reported
    /// rather than deleted — a card usually vanishes because a bind was
    /// renamed, and silently dropping its history would be worse than leaving
    /// a few stale rows in a JSON file.
    pub fn orphans(&self, deck: &str, live: &[String]) -> Vec<String> {
        self.decks
            .get(deck)
            .map(|cards| {
                cards
                    .keys()
                    .filter(|k| !live.contains(k))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sched::review;

    #[test]
    fn remembers_a_card_across_a_save_and_load() {
        let dir = std::env::temp_dir().join(format!("keydrill-test-{}", std::process::id()));
        let path = dir.join("state.json");

        let mut store = Store::default();
        store.set(
            "niri",
            "Focus column left",
            review(CardState::default(), true, 0),
        );
        store.save(&path).unwrap();

        let reloaded = Store::load(&path);
        assert_eq!(reloaded.get("niri", "Focus column left").reps, 1);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unknown_card_starts_fresh() {
        assert!(Store::default().get("niri", "nothing").is_new());
    }

    #[test]
    fn a_missing_file_is_an_empty_store_not_a_crash() {
        let store = Store::load(Path::new("/nonexistent/keydrill/state.json"));
        assert!(store.get("niri", "anything").is_new());
    }

    #[test]
    fn reports_cards_the_deck_no_longer_has() {
        let mut store = Store::default();
        store.set("niri", "Old bind", CardState::default());
        store.set("niri", "Live bind", CardState::default());

        assert_eq!(
            store.orphans("niri", &["Live bind".to_string()]),
            vec!["Old bind".to_string()]
        );
    }
}
