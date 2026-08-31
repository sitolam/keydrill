//! Turning a compositor's own config into a deck.
//!
//! An importer's job is to be honest rather than complete: it reads the file
//! you actually run, and anything it cannot describe keeps its raw action
//! text so a gap is visible in the deck instead of missing from it.

pub mod hyprland;
pub mod niri;

use std::path::PathBuf;

use anyhow::{bail, Context, Result};

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

    /// Reads a config, following whatever include mechanism the compositor
    /// has. Both of them have one, and both are used in practice — niri's
    /// own `config.kdl` is often nothing but two `include` lines.
    pub fn read(self, path: &std::path::Path) -> Result<String> {
        match self {
            Source::Niri => niri::read(path),
            Source::Hyprland => hyprland::read(path),
        }
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

/// Expands a config's include directives into one text.
///
/// `directive` returns the path an include line points at, or `None` for any
/// other line. Paths are resolved relative to the file that named them, `~`
/// is expanded, and a file that is not there is skipped — every compositor
/// treats a missing include as a warning at most, and a trainer refusing to
/// start over one would be worse.
pub(crate) fn expand(
    path: &std::path::Path,
    directive: fn(&str) -> Option<String>,
    depth: usize,
) -> Result<String> {
    // Cheap protection against a config that includes itself.
    const MAX_DEPTH: usize = 8;
    if depth > MAX_DEPTH {
        bail!(
            "includes nested more than {MAX_DEPTH} deep at {}",
            path.display()
        );
    }

    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let parent = path.parent().unwrap_or(std::path::Path::new("."));

    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let Some(target) = directive(line) else {
            out.push_str(line);
            out.push('\n');
            continue;
        };

        let target = expand_tilde(&target);
        let target = if target.is_absolute() {
            target
        } else {
            parent.join(target)
        };

        if !target.exists() {
            continue;
        }
        out.push_str(&expand(&target, directive, depth + 1)?);
    }
    Ok(out)
}

fn expand_tilde(path: &str) -> std::path::PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => std::path::PathBuf::from(home).join(rest),
            None => std::path::PathBuf::from(path),
        },
        None => std::path::PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("keydrill-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn niri_config_that_is_only_includes_still_yields_a_deck() {
        // The shape DankMaterialShell installs: config.kdl is two include
        // lines and the binds live a file away.
        let dir = scratch("include");
        fs::write(
            dir.join("config.kdl"),
            "include optional=true \"hm.kdl\"\ninclude optional=true \"missing.kdl\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("hm.kdl"),
            "binds {\n    Mod+H { focus-column-left; }\n}\n",
        )
        .unwrap();

        let text = Source::Niri.read(&dir.join("config.kdl")).unwrap();
        let deck = Source::Niri.parse(&text).unwrap();
        assert_eq!(deck.cards.len(), 1);
        assert_eq!(deck.cards[0].keys, vec!["meta+h"]);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hyprland_follows_source_lines() {
        let dir = scratch("source");
        fs::write(
            dir.join("hyprland.conf"),
            "$mainMod = SUPER\nsource = binds.conf\nsource = gone.conf\n",
        )
        .unwrap();
        fs::write(dir.join("binds.conf"), "bind = $mainMod, Q, killactive,\n").unwrap();

        let text = Source::Hyprland.read(&dir.join("hyprland.conf")).unwrap();
        let deck = Source::Hyprland.parse(&text).unwrap();
        assert_eq!(deck.cards[0].keys, vec!["meta+q"]);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_config_that_includes_itself_does_not_hang() {
        let dir = scratch("cycle");
        fs::write(dir.join("loop.kdl"), "include \"loop.kdl\"\n").unwrap();

        let err = Source::Niri.read(&dir.join("loop.kdl")).unwrap_err();
        assert!(format!("{err:#}").contains("nested"));

        fs::remove_dir_all(&dir).ok();
    }
}
