//! Turning a compositor's own config into a deck.
//!
//! An importer's job is to be honest rather than complete: it reads the file
//! you actually run, and anything it cannot describe keeps its raw action
//! text so a gap is visible in the deck instead of missing from it.

pub mod hyprland;
pub mod niri;

use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::deck::Deck;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Source {
    Niri,
    Hyprland,
}

impl Source {
    /// Where the compositor keeps its config, when no path is given.
    pub fn default_path(self) -> Result<PathBuf> {
        let base = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => {
                let Some(home) = std::env::var_os("HOME") else {
                    bail!("neither XDG_CONFIG_HOME nor HOME is set");
                };
                PathBuf::from(home).join(".config")
            }
        };
        Ok(match self {
            Source::Niri => base.join("niri/config.kdl"),
            Source::Hyprland => base.join("hypr/hyprland.conf"),
        })
    }

    pub fn parse(self, text: &str) -> Result<Deck> {
        match self {
            Source::Niri => niri::parse(text),
            Source::Hyprland => hyprland::parse(text),
        }
    }
}

/// Collapses binds that do the same thing onto one card.
///
/// Two keys with one description is the normal case in a scrollable
/// compositor — arrows and hjkl, `u` and `pageup` — and drilling them as two
/// cards would teach you to answer one prompt two ways.
pub(crate) fn merge(entries: Vec<(String, String, String)>) -> Vec<crate::deck::Card> {
    let mut cards: Vec<crate::deck::Card> = Vec::new();
    for (description, keys, category) in entries {
        match cards.iter_mut().find(|c| c.description == description) {
            Some(card) => {
                if !card.keys.contains(&keys) {
                    card.keys.push(keys);
                }
            }
            None => cards.push(crate::deck::Card {
                description,
                keys: vec![keys],
                category: Some(category),
            }),
        }
    }
    cards
}
