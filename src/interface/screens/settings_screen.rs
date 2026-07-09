use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::interface::state::UiState;
use crate::interface::theme::Theme;



pub enum SettingsAction {
    None,
    ThemeToggle,
    AccentSelected(usize),
    VolumeUp,
    VolumeDown,
    LimitUp,
    LimitDown,
}

pub enum SettingsFocus {
    Theme,
    Accent,
    Volume,
    Limit,
}

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(area);

    let title = Paragraph::new(Line::from(Span::styled(
        "Settings",
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme.accent)));
    frame.render_widget(title, chunks[0]);

    let items = vec![
        ListItem::new(Line::from(vec![
            Span::styled("Theme: ", Style::default().fg(theme.text)),
            Span::styled(state.theme_name.clone(), Style::default().fg(theme.accent)),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled("Accent Color: ", Style::default().fg(theme.text)),
            Span::styled(state.accent_color.clone(), Style::default().fg(Color::Rgb(255, 255, 255))),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled("Default Volume: ", Style::default().fg(theme.text)),
            Span::styled(format!("{}%", (state.volume * 100.0) as u8), Style::default().fg(theme.accent)),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled("Search Limit: ", Style::default().fg(theme.text)),
            Span::styled(format!("{}", state.default_search_limit), Style::default().fg(theme.accent)),
        ])),
    ];

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Options"))
        .highlight_style(Style::default().bg(theme.highlight_bg).fg(theme.highlight_fg));
    frame.render_widget(list, chunks[1]);
}


