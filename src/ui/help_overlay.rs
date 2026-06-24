use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::theme;

/// Keybinding rows: (key label, description).
const BINDINGS: &[(&str, &str)] = &[
    // ── Navigation ──────────────────────────────────────────────────────────
    ("Navigation", ""),
    ("k / ↑", "move up"),
    ("j / ↓", "move down"),
    ("Ctrl+u / PgUp", "scroll up half-page"),
    ("Ctrl+d / PgDn", "scroll down half-page"),
    ("h / ←", "collapse"),
    ("l / →", "expand"),
    ("H", "collapse all"),
    ("L", "expand all"),
    ("g / Home", "jump to start"),
    ("G / End", "jump to end"),
    ("{ / }", "prev / next file"),
    ("[ / ]", "prev / next error"),
    ("Tab / Shift+Tab", "cycle panels"),
    ("", ""),
    // ── Actions ─────────────────────────────────────────────────────────────
    ("Actions", ""),
    ("Enter", "run selected test / suite"),
    ("a", "run filtered files (or all)"),
    ("A", "run all files"),
    ("r", "rerun failed"),
    ("w", "toggle watch mode"),
    ("e", "open in editor"),
    ("f / /", "filter files"),
    ("y", "yank file path"),
    ("Y", "yank failure location"),
    ("?", "toggle this help"),
    ("q / Ctrl+c", "quit"),
];

pub fn draw(frame: &mut Frame) {
    // Width: key col (18) + sep (3) + desc col (28) + borders (2) + padding (2) = 53
    let width: u16 = 55;
    // Each section header emits an extra blank line below it for breathing room.
    let header_count = BINDINGS
        .iter()
        .filter(|&&(k, d)| !k.is_empty() && d.is_empty())
        .count() as u16;
    let height: u16 = BINDINGS.len() as u16 + header_count + 2; // +2 for top/bottom border

    let area = frame.area();
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let popup = Rect { x, y, width: width.min(area.width), height: height.min(area.height) };

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BLUE))
        .style(Style::default().bg(theme::MANTLE))
        .title(Span::styled(" Help ", Style::default().fg(theme::BLUE).bold()))
        .title_alignment(Alignment::Center);

    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines: Vec<Line> = BINDINGS
        .iter()
        .flat_map(|&(key, desc)| {
            if desc.is_empty() && !key.is_empty() {
                // Section header — emit the title then a blank line below for padding.
                vec![
                    Line::from(Span::styled(
                        format!(" {}", key),
                        Style::default().fg(theme::OVERLAY0).bold(),
                    )),
                    Line::raw(""),
                ]
            } else if key.is_empty() {
                // Explicit spacer row between sections.
                vec![Line::raw("")]
            } else {
                vec![Line::from(vec![
                    Span::styled(
                        format!(" {:<16}", key),
                        Style::default().fg(theme::BLUE),
                    ),
                    Span::styled("  ", Style::default()),
                    Span::raw(desc),
                ])]
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines).style(Style::default().bg(theme::MANTLE));
    frame.render_widget(paragraph, inner);
}
