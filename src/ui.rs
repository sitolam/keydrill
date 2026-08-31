//! Drawing.
//!
//! Every colour here is one of the terminal's own sixteen. That is a design
//! decision, not an omission: keydrill then inherits whatever theme the
//! terminal already wears, and never fights it with a palette of its own.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Padding, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Hint};
use crate::keys::Combo;

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;

pub fn draw(frame: &mut Frame, app: &App) {
    if app.session.is_finished() {
        draw_summary(frame, app);
    } else {
        draw_session(frame, app);
    }
}

fn outer(title: &str, right: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(MUTED))
        .title(Line::from(vec![
            Span::styled(
                " keydrill ",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{title} "), Style::default().fg(MUTED)),
        ]))
        .title_top(
            Line::from(Span::styled(
                format!(" {right} "),
                Style::default().fg(MUTED),
            ))
            .right_aligned(),
        )
        .padding(Padding::symmetric(2, 1))
}

fn draw_session(frame: &mut Frame, app: &App) {
    let session = &app.session;
    let card = session.current().expect("session is not finished");

    let header = format!(
        "{}/{}  ·  {:.0}%  ·  streak {}",
        session.total_seen(),
        session.total_seen() + session.remaining(),
        session.accuracy() * 100.0,
        session.streak,
    );

    let block = outer(&session.deck.name, &header);
    let inner = block.inner(frame.area());
    frame.render_widget(block, frame.area());

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // progress
            Constraint::Length(1),
            Constraint::Min(3),    // prompt
            Constraint::Length(5), // key caps
            Constraint::Length(2), // feedback
            Constraint::Length(1), // help
        ])
        .split(inner);

    let done = session.total_seen() as f64;
    let total = (done + session.remaining() as f64).max(1.0);
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(ACCENT))
            .ratio((done / total).clamp(0.0, 1.0))
            .label(""),
        rows[0],
    );

    let prompt = Paragraph::new(vec![
        Line::from(Span::styled(
            card.description.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            card.category().to_string(),
            Style::default().fg(MUTED),
        )),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(prompt, rows[2]);

    draw_caps(frame, rows[3], app);
    draw_feedback(frame, rows[4], app);

    let help = Line::from(vec![
        Span::styled("F1", Style::default().fg(ACCENT)),
        Span::styled(" help   ", Style::default().fg(MUTED)),
        Span::styled("F2", Style::default().fg(ACCENT)),
        Span::styled(" hint   ", Style::default().fg(MUTED)),
        Span::styled("F5", Style::default().fg(ACCENT)),
        Span::styled(" show   ", Style::default().fg(MUTED)),
        Span::styled("F10", Style::default().fg(ACCENT)),
        Span::styled(" quit", Style::default().fg(MUTED)),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(Paragraph::new(help), rows[5]);
}

/// The cap row: what you are holding, or as much of the answer as the hint
/// ladder has given up.
///
/// Watching the caps light up as you reach for a combination is most of what
/// makes this feel like practice rather than a quiz, so the row is never
/// empty for long.
fn draw_caps(frame: &mut Frame, area: Rect, app: &App) {
    let expected = app.expected();
    let answer = expected.first();

    let (combo, hint, style) = match (app.hint, answer) {
        (Hint::None, _) | (_, None) => (
            (!app.held.is_empty()).then(|| Combo::new(app.held, "")),
            Hint::None,
            Style::default().fg(ACCENT),
        ),
        (Hint::Answer, Some(answer)) => (
            Some(answer.clone()),
            Hint::Answer,
            Style::default().fg(Color::Yellow),
        ),
        (level, Some(answer)) => (
            Some(answer.clone()),
            level,
            Style::default().fg(Color::Yellow),
        ),
    };

    let Some(combo) = combo else {
        let hint = Paragraph::new(Line::from(Span::styled(
            "press the combination",
            Style::default().fg(MUTED),
        )))
        .alignment(Alignment::Center);
        frame.render_widget(hint, area);
        return;
    };

    frame.render_widget(
        Paragraph::new(caps(&combo, hint, style)).alignment(Alignment::Center),
        area,
    );
}

/// A combination drawn as key caps, showing as much as the hint allows:
///
/// ```text
/// ╭───╮   ╭───╮        ╭──────╮   ╭───╮        ╭──────╮   ╭───╮
/// │ ▢ │ + │ ▢ │        │ meta │ + │ ▢ │        │ meta │ + │ h │
/// ╰───╯   ╰───╯        ╰──────╯   ╰───╯        ╰──────╯   ╰───╯
///     shape              modifiers                 answer
/// ```
pub fn caps(combo: &Combo, hint: Hint, style: Style) -> Vec<Line<'static>> {
    let blank = "▢".to_string();

    let mut labels: Vec<String> = combo
        .mods
        .names()
        .iter()
        .map(|name| match hint {
            // At the first rung the shape is the hint: how many keys, and
            // nothing about which.
            Hint::Shape => blank.clone(),
            _ => name.to_string(),
        })
        .collect();

    if !combo.key.is_empty() {
        labels.push(match hint {
            Hint::None | Hint::Answer => combo.key.clone(),
            // The key itself is the last thing given up: knowing it is meta
            // and alt is usually enough to remember the rest.
            Hint::Shape | Hint::Modifiers => blank,
        });
    }

    if labels.is_empty() {
        return vec![Line::from("")];
    }

    let (mut top, mut middle, mut bottom) = (Vec::new(), Vec::new(), Vec::new());
    for (i, label) in labels.iter().enumerate() {
        if i > 0 {
            let gap = Style::default().fg(MUTED);
            top.push(Span::styled("   ", gap));
            middle.push(Span::styled(" + ", gap));
            bottom.push(Span::styled("   ", gap));
        }
        let width = label.chars().count() + 2;
        top.push(Span::styled(format!("╭{}╮", "─".repeat(width)), style));
        middle.push(Span::styled(format!("│ {label} │"), style));
        bottom.push(Span::styled(format!("╰{}╯", "─".repeat(width)), style));
    }

    vec![Line::from(top), Line::from(middle), Line::from(bottom)]
}

fn draw_feedback(frame: &mut Frame, area: Rect, app: &App) {
    let muted = Style::default().fg(MUTED);

    let line = if app.help {
        Line::from(Span::styled(
            "press the shortcut this describes. F2 gives a hint, one step at a time.",
            muted,
        ))
    } else if app.correct {
        Line::from(Span::styled(
            "correct",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        let mut spans = Vec::new();
        if let Some(pressed) = &app.last_wrong {
            spans.push(Span::styled(
                format!("you pressed {pressed}"),
                Style::default().fg(Color::Red),
            ));
        }

        let note = match app.hint {
            Hint::None => "",
            Hint::Shape => "that many keys",
            Hint::Modifiers => "these modifiers",
            // The card does not move on until it has actually been pressed —
            // reading the answer is not the same as having typed it.
            Hint::Answer => "now press it",
        };
        if !note.is_empty() {
            if !spans.is_empty() {
                spans.push(Span::styled("   ·   ", muted));
            }
            spans.push(Span::styled(note, Style::default().fg(Color::Yellow)));
        }

        let alternatives = app.expected();
        if app.hint.shows_answer() && alternatives.len() > 1 {
            let rest: Vec<String> = alternatives.iter().skip(1).map(Combo::to_string).collect();
            spans.push(Span::styled(format!("   or {}", rest.join(" or ")), muted));
        }

        Line::from(spans)
    };

    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

fn draw_summary(frame: &mut Frame, app: &App) {
    let session = &app.session;

    let block = outer(&session.deck.name, "done");
    let inner = block.inner(frame.area());
    frame.render_widget(block, frame.area());

    let mut lines = vec![
        Line::from(Span::styled(
            "session over",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("{:.0}%", session.accuracy() * 100.0),
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                format!(
                    "  first-attempt accuracy over {} cards",
                    session.total_seen()
                ),
                Style::default().fg(MUTED),
            ),
        ]),
        Line::from(vec![
            Span::styled(session.best_streak.to_string(), Style::default().fg(ACCENT)),
            Span::styled("  best streak", Style::default().fg(MUTED)),
        ]),
    ];

    let weak = session.weak_spots();
    if !weak.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "worth another look",
            Style::default().fg(Color::Yellow),
        )));
        for card in weak.iter().take(8) {
            lines.push(Line::from(vec![
                Span::styled(format!("  {}", card.description), Style::default()),
                Span::styled(
                    format!("  {}", card.keys.join(" or ")),
                    Style::default().fg(MUTED),
                ),
            ]));
        }
        if weak.len() > 8 {
            lines.push(Line::from(Span::styled(
                format!("  … and {} more", weak.len() - 8),
                Style::default().fg(MUTED),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "any key to leave",
        Style::default().fg(MUTED),
    )));

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::Mods;

    fn row(combo: &str, hint: Hint) -> String {
        let combo: Combo = combo.parse().unwrap();
        caps(&combo, hint, Style::default())[1].to_string()
    }

    #[test]
    fn caps_draw_three_lines_whatever_the_combination() {
        let combo: Combo = "meta+shift+h".parse().unwrap();
        assert_eq!(caps(&combo, Hint::None, Style::default()).len(), 3);
    }

    #[test]
    fn caps_of_bare_modifiers_omit_the_empty_key() {
        let combo = Combo::new(Mods::META, "");
        let middle = &caps(&combo, Hint::None, Style::default())[1];
        assert!(middle.to_string().contains("meta"));
        assert!(!middle.to_string().contains("││"));
    }

    #[test]
    fn the_shape_rung_gives_away_only_the_number_of_keys() {
        let shape = row("meta+alt+3", Hint::Shape);
        assert_eq!(shape.matches('▢').count(), 3);
        assert!(!shape.contains("meta"));
        assert!(!shape.contains('3'));
    }

    #[test]
    fn the_modifier_rung_still_hides_the_key() {
        let modifiers = row("meta+alt+3", Hint::Modifiers);
        assert!(modifiers.contains("meta"));
        assert!(modifiers.contains("alt"));
        assert!(!modifiers.contains('3'));
        assert_eq!(modifiers.matches('▢').count(), 1);
    }

    #[test]
    fn the_answer_rung_shows_everything() {
        let answer = row("meta+alt+3", Hint::Answer);
        assert!(answer.contains("meta") && answer.contains("alt") && answer.contains('3'));
        assert!(!answer.contains('▢'));
    }
}
