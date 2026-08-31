<div align="center">

# keydrill

**A terminal trainer for keyboard shortcuts. You answer by pressing them.**

</div>

```
╭─ keydrill niri ──────────────────────────────── 7/24 · 86% · streak 5 ─╮
│                                                                        │
│  ████████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │
│                                                                        │
│                                                                        │
│                  Send column to workspace 3, staying put               │
│                                                                        │
│                                Workspaces                              │
│                                                                        │
│              ╭──────╮   ╭─────╮   ╭───╮                                │
│              │ meta │ + │ alt │ + │ 3 │                                │
│              ╰──────╯   ╰─────╯   ╰───╯                                │
│                                                                        │
│                                correct                                 │
│                                                                        │
│                    F1 help   F5 skip   F10 quit                        │
╰────────────────────────────────────────────────────────────────────────╯
```

## Why

Flashcards teach you that `meta+alt+3` sends a column to workspace three.
They do not teach your left thumb where Super is. The difference matters,
because the thing you want back is a reflex, not a fact.

So keydrill asks the question and waits for the keys. Press the wrong ones
and it shows you the right ones, then asks again a few cards later. Cards you
keep missing come back sooner; cards you have owned for weeks stay out of the
way.

The tools that already do this — KeyCombiner, ShortcutFoo — are closed and
hosted. keydrill is neither, and it reads the config file you actually run,
so the deck cannot drift away from your real keybinds.

## What it needs

**A terminal that speaks the [Kitty keyboard
protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/).** This is not
a preference. An ordinary terminal cannot report Super at all — the key
simply never arrives — and a shortcut trainer that cannot see Super is a
crossword. [ghostty](https://ghostty.org), [kitty](https://sw.kovidgoyal.net/kitty/),
[foot](https://codeberg.org/dnkl/foot) and [WezTerm](https://wezterm.org)
all support it. keydrill checks at startup and tells you rather than
silently mis-grading you.

**Your compositor's own binds, switched off.** If niri or Hyprland binds
`Mod+H`, it takes the key before the terminal ever sees it — you would drill
your window manager instead of your memory. Switch the binds off for the
duration:

- **niri** — `niri msg action load-config-file --path <a config with an empty binds block>`,
  and back again afterwards. On NixOS, [sitolamix](https://github.com/sitolam/sitolamix)
  wraps exactly that as *practice mode*, bound to `Mod+Shift+Escape`.
- **Hyprland** — `hyprctl keyword submap drill` with an empty submap, then
  `hyprctl keyword submap reset`.

`keydrill doctor` reports what your terminal can do and reminds you of this.

## Install

**Nix, without installing anything:**

```sh
nix run github:sitolam/keydrill -- run --from niri
```

**As a flake input:**

```nix
{
  inputs.keydrill.url = "github:sitolam/keydrill";

  # then, in a module:
  environment.systemPackages = [ inputs.keydrill.packages.${pkgs.system}.default ];
  # or take the overlay and use pkgs.keydrill
}
```

**Cargo:**

```sh
cargo install --git https://github.com/sitolam/keydrill
```

## Use

```sh
keydrill run --from niri          # drill your live niri binds
keydrill run --from hyprland      # or your live hyprland.conf
keydrill run decks/vim-motions.toml
keydrill run --from niri -c Workspaces -n 15   # one category, fifteen cards
keydrill stats --from niri        # what is learned, due, and most forgotten
keydrill import niri -o niri.toml # freeze a deck to a file and edit it
keydrill doctor                   # can this terminal do it?
```

A session ends when the queue empties, or whenever you press `F10` — progress
is saved either way. The controls are function keys on purpose: `Esc`, `q`
and `Ctrl+C` are all plausible cards, and a trainer that quits when you
answer correctly would be a poor one.

## Decks

A deck is TOML, meant to be as easy to write by hand as it is to generate:

```toml
name = "niri"
description = "Imported from niri's config.kdl"

[[card]]
description = "Focus column left"
keys = ["meta+h", "meta+left"]      # either one is correct
category = "Navigation"
```

`keys` lists every combination that counts as right — two keys that genuinely
do the same thing, not two things to memorise. Modifiers are `meta` (the
Super/Windows key), `ctrl`, `shift`, `alt`, in any order; `super`, `win`,
`cmd` and `mod` are accepted as aliases for `meta`. Keys are lowercase:
`h`, `left`, `pageup`, `f2`, `/`, `-`. The `+` key itself is written `ctrl++`.

`name` and each `description` are the deck's and the card's identity in the
state file, so renaming either starts that progress over.

## Importers

| Source | Reads | Notes |
| --- | --- | --- |
| `niri` | `~/.config/niri/config.kdl` | actions become prompts; wheel binds and `XF86` keys are skipped |
| `hyprland` | `~/.config/hypr/hyprland.conf` | resolves `$mainMod`-style variables; `bindm` (mouse) is skipped |

An action the importer has no phrasing for keeps its raw text as the prompt,
so a gap shows up in your deck rather than going missing from it. Binds
sharing one description are merged onto a single card — arrows and `hjkl`
become one card with two accepted answers, which is how you actually think
about them.

## How scheduling works

SM-2, the algorithm behind SuperMemo and Anki's classic mode, with one
simplification: an answer is right or wrong, because a shortcut has no
"hard but correct".

- A miss puts the card back in the same session, four cards later, and drops
  its ease.
- First correct answer: due tomorrow. Second: in three days. After that the
  interval multiplies by the card's ease, capped at a year.
- Only your **first** attempt at a card counts. Getting it right after being
  shown the answer is not knowing it, and the score says so.
- Forgetting a card you had learned is a *lapse*; `keydrill stats` ranks your
  weak spots by it. Missing a brand-new card is not a lapse — you cannot
  forget what you never knew.

State lives in `$XDG_DATA_HOME/keydrill/state.json`. It is small, readable,
and safe to delete; that is the whole recovery story.

## Design notes

**Every colour is one of the terminal's own sixteen.** No palette, no theme
file, no configuration: keydrill wears whatever your terminal wears, and a
system-wide theme applies to it for free.

**The key-cap row is live.** Because the Kitty protocol reports key releases,
the caps light up as you hold modifiers and go out as you let go — which is
what makes it feel like practice rather than a quiz.

**Modules stay small and testable.** Combination parsing, scheduling, session
ordering and the importers each have their own tests and know nothing about
the terminal. `cargo test` runs the lot in well under a second.

## Licence

GPL-3.0-or-later. See [LICENSE](LICENSE).
