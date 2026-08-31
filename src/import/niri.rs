//! niri's `config.kdl` -> a deck.
//!
//! The binds block is read line by line rather than with a KDL parser: every
//! bind niri writes is one line, and a line-based reader cannot be broken by
//! a KDL feature used elsewhere in a config this does not otherwise care
//! about.
//!
//! Wheel binds, the `XF86` hardware keys and `Print` are skipped — a keyboard
//! trainer cannot ask you to scroll, and nothing useful is learned by drilling
//! a key that is its own label. `Print` also is not a key every keyboard has:
//! laptops that ship a Windows "snip" key emit `Super+Shift+S` in firmware and
//! never send the keycode at all, so the card would be unanswerable.

use anyhow::Result;
use regex::Regex;

use std::path::Path;

use crate::deck::Deck;
use crate::import::{expand, merge};

/// niri's `include` directive: `include optional=true "hm.kdl"`.
///
/// A config that is nothing but includes is the normal case rather than an
/// exotic one — DankMaterialShell installs exactly that, with the binds a
/// file away.
fn include(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix("include")?;
    let quoted = rest.split_once('"')?.1;
    let path = quoted.rsplit_once('"')?.0;
    Some(path.to_string())
}

pub fn read(path: &Path) -> anyhow::Result<String> {
    expand(path, include, 0)
}

/// niri keysym -> the token a deck uses.
fn key_token(keysym: &str) -> Option<String> {
    let token = match keysym {
        "Mod" => "meta",
        "Ctrl" => "ctrl",
        "Shift" => "shift",
        "Alt" => "alt",
        "Left" => "left",
        "Right" => "right",
        "Up" => "up",
        "Down" => "down",
        "Page_Up" => "pageup",
        "Page_Down" => "pagedown",
        "Home" => "home",
        "End" => "end",
        "Tab" => "tab",
        "Space" => "space",
        "BackSpace" => "backspace",
        "Return" => "enter",
        "Escape" => "esc",
        "Delete" => "del",
        "Insert" => "insert",
        "Slash" => "/",
        "Minus" => "-",
        "Equal" => "=",
        "Comma" => ",",
        "Period" => ".",
        "Semicolon" => ";",
        "Apostrophe" => "'",
        "Grave" => "`",
        "Backslash" => "\\",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        other => {
            let lower = other.to_lowercase();
            let is_function = other.starts_with('F')
                && other.len() > 1
                && other[1..].chars().all(|c| c.is_ascii_digit());
            if other.chars().count() == 1 || is_function {
                return Some(lower);
            }
            // XF86*, WheelScroll*, TouchpadScroll*, Print: not a key worth
            // a card. See the module comment.
            return None;
        }
    };
    Some(token.to_string())
}

fn combo(binding: &str) -> Option<String> {
    let mut parts = Vec::new();
    for piece in binding.split('+') {
        parts.push(key_token(piece)?);
    }
    Some(parts.join("+"))
}

/// Actions with no argument.
const ACTIONS: &[(&str, &str, &str)] = &[
    ("close-window", "Close window", "Windows"),
    ("toggle-overview", "Overview", "Windows"),
    (
        "toggle-column-tabbed-display",
        "Tabbed column display",
        "Windows",
    ),
    (
        "switch-preset-column-width",
        "Cycle preset column widths",
        "Windows",
    ),
    (
        "switch-preset-window-height",
        "Cycle preset window heights",
        "Windows",
    ),
    ("reset-window-height", "Reset window height", "Windows"),
    ("maximize-column", "Maximize column", "Windows"),
    ("fullscreen-window", "Fullscreen window", "Windows"),
    (
        "expand-column-to-available-width",
        "Expand column to available width",
        "Windows",
    ),
    ("center-column", "Center column", "Windows"),
    ("center-window", "Center window", "Windows"),
    (
        "center-visible-columns",
        "Center all visible columns",
        "Windows",
    ),
    ("toggle-window-floating", "Toggle floating", "Windows"),
    (
        "switch-focus-between-floating-and-tiling",
        "Focus across floating and tiling",
        "Windows",
    ),
    (
        "consume-or-expel-window-left",
        "Consume or expel window left",
        "Windows",
    ),
    (
        "consume-or-expel-window-right",
        "Consume or expel window right",
        "Windows",
    ),
    (
        "consume-window-into-column",
        "Consume window into column",
        "Windows",
    ),
    (
        "expel-window-from-column",
        "Expel window from column",
        "Windows",
    ),
    ("focus-column-left", "Focus column left", "Navigation"),
    ("focus-column-right", "Focus column right", "Navigation"),
    ("focus-column-first", "Focus first column", "Navigation"),
    ("focus-column-last", "Focus last column", "Navigation"),
    ("focus-window-up", "Focus window up in column", "Navigation"),
    (
        "focus-window-down",
        "Focus window down in column",
        "Navigation",
    ),
    (
        "focus-window-previous",
        "Focus previous window",
        "Navigation",
    ),
    ("move-column-left", "Move column left", "Navigation"),
    ("move-column-right", "Move column right", "Navigation"),
    ("move-column-to-first", "Move column to first", "Navigation"),
    ("move-column-to-last", "Move column to last", "Navigation"),
    ("move-window-up", "Move window up in column", "Navigation"),
    (
        "move-window-down",
        "Move window down in column",
        "Navigation",
    ),
    ("swap-window-left", "Swap window left", "Navigation"),
    ("swap-window-right", "Swap window right", "Navigation"),
    ("focus-monitor-left", "Focus monitor left", "Monitors"),
    ("focus-monitor-right", "Focus monitor right", "Monitors"),
    ("focus-monitor-up", "Focus monitor up", "Monitors"),
    ("focus-monitor-down", "Focus monitor down", "Monitors"),
    (
        "move-column-to-monitor-left",
        "Send column to monitor left",
        "Monitors",
    ),
    (
        "move-column-to-monitor-right",
        "Send column to monitor right",
        "Monitors",
    ),
    (
        "move-column-to-monitor-up",
        "Send column to monitor up",
        "Monitors",
    ),
    (
        "move-column-to-monitor-down",
        "Send column to monitor down",
        "Monitors",
    ),
    ("focus-workspace-up", "Focus workspace up", "Workspaces"),
    ("focus-workspace-down", "Focus workspace down", "Workspaces"),
    (
        "focus-workspace-previous",
        "Previous workspace",
        "Workspaces",
    ),
    (
        "move-workspace-up",
        "Move the workspace itself up",
        "Workspaces",
    ),
    (
        "move-workspace-down",
        "Move the workspace itself down",
        "Workspaces",
    ),
    ("screenshot", "Screenshot", "Capture"),
    ("screenshot-screen", "Screenshot the screen", "Capture"),
    ("screenshot-window", "Screenshot the window", "Capture"),
    (
        "toggle-keyboard-shortcuts-inhibit",
        "Toggle shortcut inhibiting",
        "Session",
    ),
    ("power-off-monitors", "Power off monitors", "Session"),
    ("quit", "Quit the compositor", "Session"),
    ("suspend", "Suspend", "Session"),
];

/// Actions whose argument belongs in the prompt.
const ARG_ACTIONS: &[(&str, &str, &str)] = &[
    ("focus-workspace", "Focus workspace {}", "Workspaces"),
    (
        "move-workspace-to-index",
        "Move the workspace itself to index {}",
        "Workspaces",
    ),
    ("set-window-width", "Window width {}", "Windows"),
    ("set-window-height", "Window height {}", "Windows"),
    ("set-column-width", "Column width {}", "Windows"),
];

/// `spawn` binds, matched on a fragment of the command line. Order matters:
/// the first fragment that appears wins.
const SPAWNS: &[(&str, &str, &str)] = &[
    ("dankMenu toggle root", "Root menu / launcher", "Shell"),
    ("clipboard toggle", "Clipboard history", "Shell"),
    ("keybinds toggleBinds", "Keybind cheat sheet", "Shell"),
    ("notepad toggle", "Notepad", "Shell"),
    ("notifications toggle", "Notification centre", "Shell"),
    ("dash toggle", "Dashboard", "Shell"),
    ("processlist toggle", "Process list", "Shell"),
    ("control-center toggle", "Control centre", "Shell"),
    ("powermenu toggle", "Power menu", "Session"),
    ("lock lock", "Lock the session", "Session"),
    ("loginctl lock-session", "Lock and suspend", "Session"),
    ("virtualKeyboard toggle", "On-screen keyboard", "Shell"),
    (
        "screenCaptureToolbar toggle",
        "Capture and record toolbar",
        "Capture",
    ),
    ("spotlight toggleQuery", "Emoji and unicode picker", "Tools"),
    (
        "niri-scratchpad",
        "Scratchpad: stash and restore the window",
        "Windows",
    ),
    ("nsticky", "Sticky: pin above every workspace", "Windows"),
    ("tesseract", "Region OCR to clipboard", "Capture"),
    ("hyprpicker", "Pick a colour", "Tools"),
    ("grim", "Region screenshot to clipboard", "Capture"),
    ("keydrill", "Drill these keybinds", "Tools"),
    ("practice-mode", "Toggle practice mode", "Tools"),
    ("lazygit", "lazygit", "Tools"),
    ("btop", "btop", "Tools"),
    ("theme toggle", "Toggle light and dark theme", "Tools"),
    ("wallpaper", "Wallpaper picker", "Tools"),
    ("night toggle", "Night light", "Tools"),
];

fn describe(body: &str) -> (String, String) {
    let (action, rest) = body.split_once(' ').unwrap_or((body, ""));
    let rest = rest.trim();

    if action == "spawn" || action == "spawn-sh" {
        // KDL renders a command as separate quoted arguments; dropping the
        // quotes turns it back into the command line the needles below are
        // written against.
        let command = rest.replace('"', "");
        for (needle, description, category) in SPAWNS {
            if command.contains(needle) {
                return (description.to_string(), category.to_string());
            }
        }
        let command = command.trim();
        // A bare program name reads better as an app than as a command line.
        if !command.contains(char::is_whitespace) {
            return (format!("Launch {command}"), "Apps".into());
        }
        return (format!("Run: {command}"), "Other".into());
    }

    if let Some((_, description, category)) = ACTIONS.iter().find(|(a, _, _)| *a == action) {
        return (description.to_string(), category.to_string());
    }

    if let Some((_, template, category)) = ARG_ACTIONS.iter().find(|(a, _, _)| *a == action) {
        let argument = rest.trim_matches('"');
        return (template.replace("{}", argument), category.to_string());
    }

    if action.starts_with("move-column-to-workspace")
        || action.starts_with("move-window-to-workspace")
    {
        let unit = if action.contains("column") {
            "column"
        } else {
            "window"
        };
        let stay = rest.contains("focus=false");
        let target = rest.replace("focus=false", "");
        let target = target.trim().trim_matches('"');
        let target = if !target.is_empty() {
            target.to_string()
        } else if action.ends_with("-up") {
            "up".into()
        } else {
            "down".into()
        };
        let suffix = if stay { ", staying put" } else { "" };
        return (
            format!("Send {unit} to workspace {target}{suffix}"),
            "Workspaces".into(),
        );
    }

    // Unmapped: the raw action, so the gap is visible in the deck rather than
    // silently missing from it.
    (body.to_string(), "Other".into())
}

pub fn parse(text: &str) -> Result<Deck> {
    // A bind is one line: optional quoting around the binding, optional
    // properties, then the action inside braces.
    let line = Regex::new(r#"^\s*"?([^"\s{]+)"?[^{]*\{\s*(.*?)\s*;?\s*\}\s*$"#)?;

    let mut entries = Vec::new();
    let mut in_binds = false;

    for raw in text.lines() {
        let trimmed = raw.trim_end();
        if trimmed.trim_start().starts_with("binds") && trimmed.ends_with('{') {
            in_binds = true;
            continue;
        }
        if in_binds && trimmed == "}" {
            break;
        }
        if !in_binds {
            continue;
        }

        let Some(caps) = line.captures(trimmed) else {
            continue;
        };
        let Some(keys) = combo(&caps[1]) else {
            continue;
        };
        let (description, category) = describe(&caps[2]);
        entries.push((description, keys, category));
    }

    let deck = Deck {
        name: "niri".into(),
        description: Some("Imported from niri's config.kdl".into()),
        cards: merge(entries),
    };
    deck.validate()?;
    Ok(deck)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
input {
    keyboard {
        xkb {
        }
    }
}

binds {
    Mod+H { focus-column-left; }
    Mod+Left { focus-column-left; }
    Mod+Shift+3 { move-column-to-workspace 3; }
    Mod+Alt+3 { move-column-to-workspace 3 focus=false; }
    "Mod+Page_Up" { focus-workspace-up; }
    Mod+T hotkey-overlay-title="Open a terminal" { spawn "ghostty"; }
    Mod+Shift+WheelScrollDown cooldown-ms=150 { move-column-to-workspace-down; }
    XF86AudioMute { spawn "wpctl" "set-mute" "@DEFAULT_AUDIO_SINK@" "toggle"; }
    Print { screenshot; }
    Mod+Print { screenshot-screen; }
    Mod+Escape allow-inhibiting=false { toggle-keyboard-shortcuts-inhibit; }
    Mod+F2 { spawn "dms" "ipc" "call" "spotlight" "toggleQuery" ":e "; }
}

window-rule {
}
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
    fn reads_only_the_binds_block() {
        assert!(!deck().cards.is_empty());
    }

    #[test]
    fn merges_two_keys_for_one_action_into_one_card() {
        let deck = deck();
        assert_eq!(
            card(&deck, "Focus column left").keys,
            vec!["meta+h", "meta+left"]
        );
    }

    #[test]
    fn distinguishes_following_from_staying_put() {
        let deck = deck();
        assert_eq!(
            card(&deck, "Send column to workspace 3").keys,
            vec!["meta+shift+3"]
        );
        assert_eq!(
            card(&deck, "Send column to workspace 3, staying put").keys,
            vec!["meta+alt+3"]
        );
    }

    #[test]
    fn handles_quoted_bindings_and_properties() {
        let deck = deck();
        assert_eq!(card(&deck, "Focus workspace up").keys, vec!["meta+pageup"]);
        assert_eq!(
            card(&deck, "Toggle shortcut inhibiting").keys,
            vec!["meta+esc"]
        );
        assert_eq!(card(&deck, "Launch ghostty").keys, vec!["meta+t"]);
    }

    #[test]
    fn keeps_function_keys() {
        let deck = deck();
        assert_eq!(
            card(&deck, "Emoji and unicode picker").keys,
            vec!["meta+f2"]
        );
    }

    #[test]
    fn drops_what_cannot_be_pressed_as_a_combination() {
        let deck = deck();
        let keys: Vec<&str> = deck
            .cards
            .iter()
            .flat_map(|c| c.keys.iter().map(String::as_str))
            .collect();
        assert!(!keys.iter().any(|k| k.contains("wheel")));
        assert!(!keys.iter().any(|k| k.contains("xf86")));
        // Print goes too, modifiers or not: some laptops never send it.
        assert!(!keys.iter().any(|k| k.contains("printscreen")));
    }
}
