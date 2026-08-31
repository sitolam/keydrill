//! Hyprland's `hyprland.conf` -> a deck.
//!
//! Reads `bind`, `binde`, `bindl`, `bindr` and their combinations
//! (`bindel`, …). `bindm` is skipped: it binds a mouse button.
//!
//! Variables (`$mainMod = SUPER`) are resolved, because almost every
//! Hyprland config in the wild is written with one.

use std::collections::HashMap;

use anyhow::Result;
use regex::Regex;

use std::path::Path;

use crate::deck::Deck;
use crate::import::{expand, merge};

/// Hyprland's `source = ~/.config/hypr/binds.conf`. Nearly every config of
/// any size splits its binds out this way.
fn source(line: &str) -> Option<String> {
    let (key, value) = line.split_once('=')?;
    (key.trim() == "source").then(|| value.trim().to_string())
}

pub fn read(path: &Path) -> anyhow::Result<String> {
    expand(path, source, 0)
}

fn modifier_token(name: &str) -> Option<&'static str> {
    Some(match name.to_uppercase().as_str() {
        "SUPER" | "SUPER_L" | "SUPER_R" | "MOD4" | "WIN" | "LOGO" => "meta",
        "CTRL" | "CONTROL" => "ctrl",
        "SHIFT" => "shift",
        "ALT" | "MOD1" => "alt",
        _ => return None,
    })
}

fn key_token(key: &str) -> Option<String> {
    let token = match key.to_lowercase().as_str() {
        "return" => "enter",
        "escape" => "esc",
        "space" => "space",
        "prior" | "page_up" => "pageup",
        "next" | "page_down" => "pagedown",
        "delete" => "del",
        "print" => "printscreen",
        "slash" => "/",
        "minus" => "-",
        "equal" | "plus" => "=",
        "comma" => ",",
        "period" => ".",
        "bracketleft" => "[",
        "bracketright" => "]",
        "mouse_down" | "mouse_up" | "mouse:272" | "mouse:273" => return None,
        other => {
            if other.starts_with("code:") || other.starts_with("mouse") {
                return None;
            }
            return Some(other.to_string());
        }
    };
    Some(token.to_string())
}

/// Dispatcher -> prompt. `{}` is replaced with the dispatcher's argument.
const DISPATCHERS: &[(&str, &str, &str)] = &[
    ("killactive", "Close window", "Windows"),
    ("fullscreen", "Fullscreen window", "Windows"),
    ("togglefloating", "Toggle floating", "Windows"),
    ("pseudo", "Toggle pseudo-tiling", "Windows"),
    ("togglesplit", "Toggle split direction", "Windows"),
    ("centerwindow", "Center window", "Windows"),
    ("pin", "Pin above every workspace", "Windows"),
    ("movefocus", "Focus window {}", "Navigation"),
    ("movewindow", "Move window {}", "Navigation"),
    ("swapwindow", "Swap window {}", "Navigation"),
    ("resizeactive", "Resize window {}", "Windows"),
    ("workspace", "Focus workspace {}", "Workspaces"),
    (
        "movetoworkspace",
        "Send window to workspace {}",
        "Workspaces",
    ),
    (
        "movetoworkspacesilent",
        "Send window to workspace {}, staying put",
        "Workspaces",
    ),
    (
        "togglespecialworkspace",
        "Toggle the special workspace",
        "Workspaces",
    ),
    ("focusmonitor", "Focus monitor {}", "Monitors"),
    (
        "movecurrentworkspacetomonitor",
        "Send this workspace to monitor {}",
        "Monitors",
    ),
    ("exit", "Quit the compositor", "Session"),
    ("forcerendererreload", "Reload the renderer", "Session"),
];

fn describe(dispatcher: &str, argument: &str) -> (String, String) {
    if dispatcher == "exec" {
        let command = argument.trim();
        let program = command.split_whitespace().next().unwrap_or(command);
        return (format!("Launch {program}"), "Apps".into());
    }

    if let Some((_, template, category)) = DISPATCHERS.iter().find(|(d, _, _)| *d == dispatcher) {
        let argument = match argument.trim() {
            "l" => "left",
            "r" => "right",
            "u" => "up",
            "d" => "down",
            other => other,
        };
        let text = template.replace("{}", argument).trim().to_string();
        return (text, category.to_string());
    }

    let text = if argument.is_empty() {
        dispatcher.to_string()
    } else {
        format!("{dispatcher} {argument}")
    };
    (text, "Other".into())
}

pub fn parse(text: &str) -> Result<Deck> {
    let variable = Regex::new(r"^\s*\$(\w+)\s*=\s*(.+?)\s*$")?;
    let bind = Regex::new(r"^\s*bind[elrnmtio]*\s*=\s*(.*)$")?;

    let mut variables: HashMap<String, String> = HashMap::new();
    let mut entries = Vec::new();

    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");

        if let Some(caps) = variable.captures(line) {
            variables.insert(caps[1].to_string(), caps[2].to_string());
            continue;
        }

        let Some(caps) = bind.captures(line) else {
            continue;
        };
        // A mouse bind has nothing to press on a keyboard.
        if line.trim_start().starts_with("bindm") {
            continue;
        }

        let fields: Vec<&str> = caps[1].splitn(4, ',').map(str::trim).collect();
        if fields.len() < 3 {
            continue;
        }

        let mut resolved = fields[0].to_string();
        for (name, value) in &variables {
            resolved = resolved.replace(&format!("${name}"), value);
        }

        let mut combo: Vec<String> = Vec::new();
        let mut known = true;
        for name in resolved
            .split_whitespace()
            .flat_map(|m| m.split('+'))
            .filter(|name| !name.is_empty())
        {
            match modifier_token(name) {
                Some(token) => combo.push(token.to_string()),
                // An unresolved variable or a modifier this build does not
                // know: skip the bind rather than teach a wrong combination.
                None => {
                    known = false;
                    break;
                }
            }
        }
        if !known {
            continue;
        }

        let Some(key) = key_token(fields[1]) else {
            continue;
        };
        combo.push(key);

        let (description, category) = describe(fields[2], fields.get(3).unwrap_or(&""));
        entries.push((description, combo.join("+"), category));
    }

    let deck = Deck {
        name: "hyprland".into(),
        description: Some("Imported from hyprland.conf".into()),
        cards: merge(entries),
    };
    deck.validate()?;
    Ok(deck)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
# a comment
$mainMod = SUPER

bind = $mainMod, Q, killactive,
bind = $mainMod, left, movefocus, l
bind = $mainMod SHIFT, left, movewindow, l
bind = $mainMod, 1, workspace, 1
bind = $mainMod SHIFT, 1, movetoworkspace, 1
bind = $mainMod ALT, 1, movetoworkspacesilent, 1
bind = $mainMod, RETURN, exec, ghostty
binde = $mainMod, equal, resizeactive, 10 0
bindm = $mainMod, mouse:272, movewindow
"#;

    fn deck() -> Deck {
        parse(CONFIG).unwrap()
    }

    fn card<'a>(deck: &'a Deck, description: &str) -> &'a crate::deck::Card {
        deck.cards
            .iter()
            .find(|c| c.description == description)
            .unwrap_or_else(|| panic!("no card {description:?} in {:#?}", deck.cards))
    }

    #[test]
    fn resolves_the_main_mod_variable() {
        let deck = deck();
        assert_eq!(card(&deck, "Close window").keys, vec!["meta+q"]);
    }

    #[test]
    fn expands_direction_shorthand() {
        let deck = deck();
        assert_eq!(card(&deck, "Focus window left").keys, vec!["meta+left"]);
        assert_eq!(
            card(&deck, "Move window left").keys,
            vec!["meta+shift+left"]
        );
    }

    #[test]
    fn tells_silent_moves_apart() {
        let deck = deck();
        assert_eq!(
            card(&deck, "Send window to workspace 1").keys,
            vec!["meta+shift+1"]
        );
        assert_eq!(
            card(&deck, "Send window to workspace 1, staying put").keys,
            vec!["meta+alt+1"]
        );
    }

    #[test]
    fn names_an_exec_bind_after_its_program() {
        let deck = deck();
        assert_eq!(card(&deck, "Launch ghostty").keys, vec!["meta+enter"]);
    }

    #[test]
    fn reads_binde_but_not_bindm() {
        let deck = deck();
        assert_eq!(card(&deck, "Resize window 10 0").keys, vec!["meta+="]);
        assert!(!deck
            .cards
            .iter()
            .any(|c| c.keys.iter().any(|k| k.contains("mouse"))));
    }
}
