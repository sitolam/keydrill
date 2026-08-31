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

use crate::app::{App, Feedback};
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
        Span::styled("F5", Style::default().fg(ACCENT)),
        Span::styled(" skip   ", Style::default().fg(MUTED)),
        Span::styled("F10", Style::default().fg(ACCENT)),
        Span::styled(" quit", Style::default().fg(MUTED)),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(Paragraph::new(help), rows[5]);
}

/// The live row: what you are holding, or what the card wanted once you have
/// missed it. Watching the caps light up as you reach for a combination is
/// most of what makes this feel like practice rather than a quiz.
fn draw_caps(frame: &mut Frame, area: Rect, app: &App) {
    let (combo, style) = match &app.feedback {
        Some(Feedback::Wrong { expected, .. }) => {
            (expected.first().cloned(), Style::default().fg(Color::Red))
        }
        _ => (
            (!app.held.is_empty()).then(|| Combo::new(app.held, "")),
            Style::default().fg(ACCENT),
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
        Paragraph::new(caps(&combo, style)).alignment(Alignment::Center),
        area,
    );
}

/// A combination drawn as key caps:
///
/// ```text
/// ╭──────╮   ╭───╮
/// │ meta │ + │ h │
/// ╰──────╯   ╰───╯
/// ```
pub fn caps(combo: &Combo, style: Style) -> Vec<Line<'static>> {
    let mut labels: Vec<String> = combo.mods.names().iter().map(|s| s.to_string()).collect();
    if !combo.key.is_empty() {
        labels.push(combo.key.clone());
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
    let line = match &app.feedback {
        Some(Feedback::Correct) => Line::from(Span::styled(
            "correct",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Some(Feedback::Wrong { pressed, expected }) => {
            let alternatives: Vec<String> = expected.iter().map(Combo::to_string).collect();
            Line::from(vec![
                Span::styled(
                    match pressed {
                        Some(pressed) => format!("you pressed {pressed}"),
                        None => "skipped".to_string(),
                    },
                    Style::default().fg(Color::Red),
                ),
                Span::styled("   ·   ", Style::default().fg(MUTED)),
                Span::styled(
                    alternatives.join(" or "),
                    Style::default().fg(Color::Yellow),
                ),
            ])
        }
        Some(Feedback::Help) => Line::from(Span::styled(
            "press the shortcut this describes. F5 skips, F10 quits and saves.",
            Style::default().fg(MUTED),
        )),
        None => Line::from(""),
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

    #[test]
    fn caps_draw_three_lines_whatever_the_combination() {
        let combo: Combo = "meta+shift+h".parse().unwrap();
        assert_eq!(caps(&combo, Style::default()).len(), 3);
    }

    #[test]
    fn caps_of_bare_modifiers_omit_the_empty_key() {
        let combo = Combo::new(Mods::META, "");
        let middle = &caps(&combo, Style::default())[1];
        assert!(middle.to_string().contains("meta"));
        assert!(!middle.to_string().contains("││"));
    }
}
