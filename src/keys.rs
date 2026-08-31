//! The combination model: parsing, canonical formatting, and comparison.
//!
//! Everything that touches a key combination goes through [`Combo`]. A deck
//! writes `"meta+shift+h"`, a terminal delivers a `KeyEvent`, and both end up
//! as the same value — that is the only reason grading a keypress against a
//! deck entry is reliable.

use std::fmt;
use std::str::FromStr;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode};

/// Modifiers, as a set. Kept as a bitmask so comparison is order-independent:
/// `meta+shift+h` and `shift+meta+h` are the same combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Mods(u8);

impl Mods {
    pub const META: Mods = Mods(1 << 0);
    pub const CTRL: Mods = Mods(1 << 1);
    pub const SHIFT: Mods = Mods(1 << 2);
    pub const ALT: Mods = Mods(1 << 3);

    pub const fn empty() -> Self {
        Mods(0)
    }

    pub fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: Mods) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Mods) {
        self.0 &= !other.0;
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Canonical order, used for both display and the key-cap row.
    pub fn names(self) -> Vec<&'static str> {
        let mut out = Vec::new();
        for (bit, name) in [
            (Mods::META, "meta"),
            (Mods::CTRL, "ctrl"),
            (Mods::SHIFT, "shift"),
            (Mods::ALT, "alt"),
        ] {
            if self.contains(bit) {
                out.push(name);
            }
        }
        out
    }

    fn from_name(name: &str) -> Option<Mods> {
        match name {
            // "meta" is the name KeyCombiner and most shortcut lists use for
            // the Super/Windows key; the rest are aliases people actually type.
            "meta" | "super" | "win" | "cmd" | "mod" => Some(Mods::META),
            "ctrl" | "control" => Some(Mods::CTRL),
            "shift" => Some(Mods::SHIFT),
            "alt" | "opt" | "option" => Some(Mods::ALT),
            _ => None,
        }
    }
}

/// One combination: a set of modifiers plus exactly one non-modifier key.
///
/// The key is stored as a lowercase token — `"h"`, `"left"`, `"pageup"`,
/// `"f2"`, `"/"` — rather than an enum, so a deck can name a key this build
/// has never heard of and still round-trip it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Combo {
    pub mods: Mods,
    pub key: String,
}

impl Combo {
    pub fn new(mods: Mods, key: impl Into<String>) -> Self {
        Combo {
            mods,
            key: key.into(),
        }
    }

    /// A terminal key event, or `None` for anything that is not a complete
    /// combination: key releases, and the modifier keys themselves.
    ///
    /// Modifier-only events matter to the UI (that is how the held-key row
    /// updates while you are still reaching for the last key), but they are
    /// not answers, so they are filtered here rather than at the call site.
    pub fn from_event(event: KeyEvent) -> Option<Combo> {
        if event.kind != KeyEventKind::Press && event.kind != KeyEventKind::Repeat {
            return None;
        }

        let key = key_token(event.code)?;
        let mut mods = Mods::empty();
        if event.modifiers.contains(KeyModifiers::SUPER) {
            mods.insert(Mods::META);
        }
        if event.modifiers.contains(KeyModifiers::CONTROL) {
            mods.insert(Mods::CTRL);
        }
        if event.modifiers.contains(KeyModifiers::SHIFT) {
            mods.insert(Mods::SHIFT);
        }
        if event.modifiers.contains(KeyModifiers::ALT) {
            mods.insert(Mods::ALT);
        }

        Some(Combo { mods, key })
    }
}

/// The modifier a bare modifier-key event stands for, for the held-key row.
pub fn modifier_of(code: KeyCode) -> Option<Mods> {
    let KeyCode::Modifier(m) = code else {
        return None;
    };
    Some(match m {
        ModifierKeyCode::LeftSuper | ModifierKeyCode::RightSuper => Mods::META,
        ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl => Mods::CTRL,
        ModifierKeyCode::LeftShift | ModifierKeyCode::RightShift => Mods::SHIFT,
        ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt => Mods::ALT,
        _ => return None,
    })
}

fn key_token(code: KeyCode) -> Option<String> {
    let token = match code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_lowercase().to_string(),
        KeyCode::F(n) => format!("f{n}"),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        KeyCode::Tab | KeyCode::BackTab => "tab".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Delete => "del".into(),
        KeyCode::Insert => "insert".into(),
        KeyCode::PrintScreen => "printscreen".into(),
        KeyCode::Menu => "menu".into(),
        _ => return None,
    };
    Some(token)
}

impl FromStr for Combo {
    type Err = ComboParseError;

    /// `"meta+shift+h"`. The key is whatever follows the last `+`, except
    /// for the `+` key itself, which is written `"ctrl++"` (or bare `"+"`).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let text = s.trim().to_lowercase();
        if text.is_empty() {
            return Err(ComboParseError::Empty);
        }

        let (modifiers, key) = if text == "+" {
            ("", "+".to_string())
        } else if let Some(rest) = text.strip_suffix("++") {
            (rest, "+".to_string())
        } else if let Some((before, after)) = text.rsplit_once('+') {
            if after.is_empty() {
                return Err(ComboParseError::NoKey(text.clone()));
            }
            (before, after.to_string())
        } else {
            // No separator at all: the whole thing is the key, even when it
            // happens to be spelled like a modifier ("shift" on its own).
            ("", text.clone())
        };

        let mut mods = Mods::empty();
        for name in modifiers.split('+').filter(|p| !p.is_empty()) {
            match Mods::from_name(name.trim()) {
                Some(m) => mods.insert(m),
                None => return Err(ComboParseError::TwoKeys(text.clone())),
            }
        }

        Ok(Combo { mods, key })
    }
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for name in self.mods.names() {
            write!(f, "{name}+")?;
        }
        write!(f, "{}", self.key)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ComboParseError {
    Empty,
    NoKey(String),
    TwoKeys(String),
}

impl fmt::Display for ComboParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComboParseError::Empty => write!(f, "empty combination"),
            ComboParseError::NoKey(s) => write!(f, "{s:?} is only modifiers, with no key"),
            ComboParseError::TwoKeys(s) => write!(f, "{s:?} has more than one non-modifier key"),
        }
    }
}

impl std::error::Error for ComboParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(s: &str) -> Combo {
        s.parse().unwrap()
    }

    #[test]
    fn parses_modifiers_in_any_order() {
        assert_eq!(combo("meta+shift+h"), combo("shift+meta+h"));
        assert_eq!(combo("META+Shift+H"), combo("meta+shift+h"));
    }

    #[test]
    fn accepts_the_usual_aliases_for_super() {
        for alias in ["meta", "super", "win", "cmd", "mod"] {
            assert_eq!(combo(&format!("{alias}+h")), combo("meta+h"));
        }
    }

    #[test]
    fn round_trips_through_display_in_canonical_order() {
        assert_eq!(
            combo("alt+shift+ctrl+meta+k").to_string(),
            "meta+ctrl+shift+alt+k"
        );
    }

    #[test]
    fn keeps_punctuation_and_named_keys() {
        assert_eq!(combo("meta+/").key, "/");
        assert_eq!(combo("meta+shift+pageup").key, "pageup");
        assert_eq!(combo("ctrl++").key, "+");
        assert_eq!(combo("+").key, "+");
    }

    #[test]
    fn a_lone_modifier_name_is_a_key_not_a_modifier() {
        // Otherwise "shift" alone would parse as a combination with no key.
        assert_eq!(combo("shift").key, "shift");
        assert!(combo("shift").mods.is_empty());
    }

    #[test]
    fn rejects_nonsense() {
        assert!("".parse::<Combo>().is_err());
        assert!("meta+".parse::<Combo>().is_err()); // dangling separator
        assert!("meta+h+j".parse::<Combo>().is_err());
    }

    #[test]
    fn builds_a_combo_from_a_terminal_event() {
        let event = KeyEvent::new(
            KeyCode::Char('h'),
            KeyModifiers::SUPER | KeyModifiers::SHIFT,
        );
        assert_eq!(Combo::from_event(event), Some(combo("meta+shift+h")));
    }

    #[test]
    fn ignores_releases_and_bare_modifiers() {
        let release = KeyEvent::new_with_kind(
            KeyCode::Char('h'),
            KeyModifiers::SUPER,
            KeyEventKind::Release,
        );
        assert_eq!(Combo::from_event(release), None);

        let bare = KeyEvent::new(
            KeyCode::Modifier(ModifierKeyCode::LeftSuper),
            KeyModifiers::SUPER,
        );
        assert_eq!(Combo::from_event(bare), None);
        assert_eq!(modifier_of(bare.code), Some(Mods::META));
    }
}
