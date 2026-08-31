//! keydrill — a terminal trainer for keyboard shortcuts.
//!
//! The point of the whole program: you are asked what a shortcut does, and
//! you answer by *pressing it*. Nothing is typed out, so what gets trained is
//! the hand rather than the recollection.

mod app;
mod deck;
mod import;
mod keys;
mod sched;
mod session;
mod store;
mod ui;

use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use crate::deck::Deck;
use crate::import::Source;
use crate::session::Session;
use crate::store::Store;

#[derive(Parser)]
#[command(
    name = "keydrill",
    version,
    about = "Practise keyboard shortcuts by pressing them"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Drill a deck.
    Run {
        /// A deck file. Omit it and `--from` supplies the deck instead.
        deck: Option<PathBuf>,

        /// Build the deck from a compositor's own config.
        #[arg(long, value_name = "SOURCE")]
        from: Option<Source>,

        /// Where that config lives, if not in the usual place.
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,

        /// Stop after this many cards.
        #[arg(long, short = 'n', value_name = "COUNT")]
        limit: Option<usize>,

        /// Drill one category only.
        #[arg(long, short = 'c', value_name = "NAME")]
        category: Option<String>,
    },

    /// Print a deck built from a compositor's config, as TOML.
    Import {
        #[arg(value_name = "SOURCE")]
        from: Source,

        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,

        /// Write here instead of standard output.
        #[arg(long, short = 'o', value_name = "PATH")]
        out: Option<PathBuf>,
    },

    /// What you know, and what is due.
    Stats {
        deck: Option<PathBuf>,

        #[arg(long, value_name = "SOURCE")]
        from: Option<Source>,

        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
    },

    /// Check that this terminal can report the keys keydrill needs.
    Doctor,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Run {
            deck,
            from,
            config,
            limit,
            category,
        } => run(deck, from, config, limit, category),
        Command::Import { from, config, out } => import(from, config, out),
        Command::Stats { deck, from, config } => stats(deck, from, config),
        Command::Doctor => doctor(),
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A deck from a file, or from a compositor's config. Exactly one of the two.
fn load_deck(deck: Option<PathBuf>, from: Option<Source>, config: Option<PathBuf>) -> Result<Deck> {
    match (deck, from) {
        (Some(_), Some(_)) => bail!("give either a deck file or --from, not both"),
        (Some(path), None) => Deck::load(&path),
        (None, Some(source)) => {
            let path = match config {
                Some(path) => path,
                None => source.default_path()?,
            };
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            source
                .parse(&text)
                .with_context(|| format!("cannot import {}", path.display()))
        }
        (None, None) => bail!("nothing to drill: pass a deck file or --from niri"),
    }
}

fn run(
    deck: Option<PathBuf>,
    from: Option<Source>,
    config: Option<PathBuf>,
    limit: Option<usize>,
    category: Option<String>,
) -> Result<()> {
    let deck = load_deck(deck, from, config)?;

    if !io::stdout().is_terminal() {
        bail!("keydrill run needs a terminal; try `keydrill import` to see the deck");
    }
    if !app::terminal_reports_modifiers() {
        bail!(
            "this terminal cannot report Super or key releases, so most \
             combinations are unanswerable.\nkeydrill needs the Kitty \
             keyboard protocol — ghostty, kitty, foot and WezTerm have it.\n\
             Run `keydrill doctor` for what this terminal does support."
        );
    }

    if let Some(wanted) = &category {
        let known = deck.categories();
        if !known.iter().any(|c| c == wanted) {
            bail!(
                "no category {wanted:?} in this deck; it has: {}",
                known.join(", ")
            );
        }
    }

    let path = Store::path()?;
    let mut store = Store::load(&path);

    let session = Session::build(deck, &store, now(), limit, category.as_deref());
    if session.remaining() == 0 {
        println!("nothing due. come back later, or drill anyway with --limit.");
        return Ok(());
    }

    app::run(session, &mut store)?;
    store.save(&path)?;
    Ok(())
}

fn import(from: Source, config: Option<PathBuf>, out: Option<PathBuf>) -> Result<()> {
    let deck = load_deck(None, Some(from), config)?;
    let toml = deck.to_toml()?;

    match out {
        Some(path) => {
            std::fs::write(&path, toml)
                .with_context(|| format!("cannot write {}", path.display()))?;
            eprintln!("{} cards -> {}", deck.cards.len(), path.display());
        }
        None => io::stdout().write_all(toml.as_bytes())?,
    }
    Ok(())
}

fn stats(deck: Option<PathBuf>, from: Option<Source>, config: Option<PathBuf>) -> Result<()> {
    let deck = load_deck(deck, from, config)?;
    let store = Store::load(&Store::path()?);
    let now = now();

    let states: Vec<_> = deck
        .cards
        .iter()
        .map(|card| store.get(&deck.name, &card.description))
        .collect();

    let new = states.iter().filter(|s| s.is_new()).count();
    let due = states
        .iter()
        .filter(|s| !s.is_new() && s.is_due(now))
        .count();
    let learned = states.len() - new - due;

    println!("{}: {} cards", deck.name, deck.cards.len());
    println!("  {learned} learned, {due} due now, {new} not started");

    let mut weak: Vec<_> = deck
        .cards
        .iter()
        .zip(&states)
        .filter(|(_, state)| state.lapses > 0)
        .collect();
    weak.sort_by_key(|(_, state)| std::cmp::Reverse(state.lapses));

    if !weak.is_empty() {
        println!("\nmost forgotten");
        for (card, state) in weak.iter().take(10) {
            println!(
                "  {:>3}x  {}  ({})",
                state.lapses,
                card.description,
                card.keys.join(" or ")
            );
        }
    }

    let orphans = store.orphans(
        &deck.name,
        &deck
            .cards
            .iter()
            .map(|c| c.description.clone())
            .collect::<Vec<_>>(),
    );
    if !orphans.is_empty() {
        println!(
            "\n{} card(s) in the state file are no longer in the deck, e.g. {:?}",
            orphans.len(),
            orphans[0]
        );
    }

    Ok(())
}

fn doctor() -> Result<()> {
    let supported = app::terminal_reports_modifiers();
    let term = std::env::var("TERM").unwrap_or_else(|_| "unset".into());
    let program = std::env::var("TERM_PROGRAM").unwrap_or_else(|_| "unknown".into());

    println!("TERM         {term}");
    println!("TERM_PROGRAM {program}");
    println!(
        "kitty keyboard protocol  {}",
        if supported { "yes" } else { "NO" }
    );

    if supported {
        println!("\nThis terminal can report Super and key releases. keydrill will work.");
    } else {
        println!(
            "\nThis terminal cannot report Super, so combinations like meta+shift+h\n\
             never arrive. Use ghostty, kitty, foot or WezTerm."
        );
    }

    println!(
        "\nIf your compositor binds the keys you are drilling, it takes them\n\
         before the terminal sees them. Switch its own binds off first —\n\
         on niri, that is what practice mode does."
    );
    Ok(())
}
