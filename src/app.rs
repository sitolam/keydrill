//! The event loop: terminal in, session out.

use std::io;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, terminal};

use crate::keys::{modifier_of, Combo, Mods};
use crate::session::{Answer, Session};
use crate::store::Store;

/// How long "correct" stays on screen. Long enough to register, short enough
/// that it never gates the next answer — input during the flash still counts.
const FLASH: Duration = Duration::from_millis(320);

/// How much of the answer is on screen.
///
/// A miss climbs one rung rather than handing the answer over, so the next
/// attempt is made against a bigger hint instead of a solved card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Hint {
    /// Nothing shown; the cap row follows what you are holding.
    None,
    /// How many keys the combination has.
    Shape,
    /// The modifiers, with the key still blank.
    Modifiers,
    /// All of it. From here the card only advances when you press it.
    Answer,
}

impl Hint {
    fn next(self) -> Hint {
        match self {
            Hint::None => Hint::Shape,
            Hint::Shape => Hint::Modifiers,
            Hint::Modifiers | Hint::Answer => Hint::Answer,
        }
    }

    pub fn shows_answer(self) -> bool {
        self == Hint::Answer
    }
}

pub struct App {
    pub session: Session,
    /// Modifiers currently held down, for the live key-cap row. Only
    /// populated when the terminal reports modifier keys of its own accord.
    pub held: Mods,
    pub hint: Hint,
    /// What you last got wrong on this card, so the screen can say so.
    pub last_wrong: Option<Combo>,
    pub correct: bool,
    pub help: bool,
    /// The card the hint belongs to, so a new card starts unhinted.
    card: Option<String>,
    flash_since: Option<Instant>,
    quit: bool,
}

impl App {
    pub fn new(session: Session) -> App {
        let mut app = App {
            session,
            held: Mods::empty(),
            hint: Hint::None,
            last_wrong: None,
            correct: false,
            help: false,
            card: None,
            flash_since: None,
            quit: false,
        };
        app.sync_card();
        app
    }

    /// Clears per-card state whenever the card in front changes.
    fn sync_card(&mut self) {
        let current = self.session.current().map(|c| c.description.clone());
        if current != self.card {
            self.card = current;
            self.hint = Hint::None;
            self.last_wrong = None;
        }
    }

    /// The combinations the current card accepts. The hint row shows the
    /// first; any of them still answers it.
    pub fn expected(&self) -> Vec<crate::keys::Combo> {
        self.session
            .current()
            .map(|card| card.combos())
            .unwrap_or_default()
    }

    fn raise_hint(&mut self) {
        self.hint = self.hint.next();
        if self.hint.shows_answer() {
            // From here the card counts as one you did not know, whether the
            // ladder got here through misses or through F5.
            self.session.reveal(Self::now());
        }
    }

    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn on_key(&mut self, event: KeyEvent) {
        // Track held modifiers from press/release of the modifier keys
        // themselves. Terminals without the Kitty protocol never send these,
        // and the row simply stays empty.
        if let Some(m) = modifier_of(event.code) {
            match event.kind {
                KeyEventKind::Press | KeyEventKind::Repeat => self.held.insert(m),
                KeyEventKind::Release => self.held.remove(m),
            }
            return;
        }

        if event.kind != KeyEventKind::Press && event.kind != KeyEventKind::Repeat {
            return;
        }

        // The control keys are function keys precisely because Esc, q and
        // Ctrl+C are all plausible cards.
        match event.code {
            KeyCode::F(10) => {
                self.quit = true;
                return;
            }
            KeyCode::F(1) => {
                self.help = !self.help;
                self.correct = false;
                self.flash_since = None;
                return;
            }
            KeyCode::F(2) => {
                self.raise_hint();
                self.correct = false;
                self.flash_since = None;
                return;
            }
            KeyCode::F(5) => {
                // Skip shows the answer, but it does not move on: the card
                // stays up, description and all, until you have pressed it.
                self.hint = Hint::Modifiers;
                self.raise_hint();
                self.correct = false;
                self.flash_since = None;
                return;
            }
            _ => {}
        }

        let Some(pressed) = Combo::from_event(event) else {
            return;
        };

        if self.session.is_finished() {
            self.quit = true;
            return;
        }

        self.help = false;
        match self.session.answer(&pressed, Self::now()) {
            Answer::Correct => {
                self.correct = true;
                self.last_wrong = None;
                self.flash_since = Some(Instant::now());
                self.sync_card();
            }
            Answer::Wrong { .. } => {
                self.correct = false;
                self.last_wrong = Some(pressed);
                self.flash_since = None;
                // Already showing the answer: no further rung to climb, you
                // simply have to press it.
                if !self.hint.shows_answer() {
                    self.raise_hint();
                }
            }
        }
    }
}

/// Runs a session to its end, then writes progress back. Progress is saved
/// even when you quit early — a drill you walked away from still happened.
pub fn run(session: Session, store: &mut Store) -> Result<()> {
    let mut app = App::new(session);

    let mut terminal = ratatui::try_init().context("cannot start the terminal UI")?;
    let enhanced = push_enhancements().is_ok();

    let result = event_loop(&mut terminal, &mut app);

    if enhanced {
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
    }
    ratatui::restore();

    app.session.commit(store);
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| crate::ui::draw(frame, app))?;

        if app.quit {
            return Ok(());
        }

        // The timeout is what lets the "correct" flash expire without a
        // keypress; it is not a poll for input.
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => app.on_key(key),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if let Some(since) = app.flash_since {
            if since.elapsed() >= FLASH {
                app.flash_since = None;
                app.correct = false;
            }
        }
    }
}

/// Asks the terminal for the Kitty keyboard protocol.
///
/// Without it a terminal cannot report Super at all, and half of any
/// compositor deck is unanswerable. With it we also get key releases, which
/// is what drives the held-modifier row.
fn push_enhancements() -> Result<()> {
    execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        )
    )?;
    Ok(())
}

/// True when the terminal supports the protocol keydrill needs.
pub fn terminal_reports_modifiers() -> bool {
    terminal::supports_keyboard_enhancement().unwrap_or(false)
}
