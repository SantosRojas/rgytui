use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, BorderType, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::domain::audio_mode::AudioMode;
use crate::domain::media::RepeatMode;
use crate::interface::state::{RenderSnapshot, UiState};
use crate::interface::theme::Theme;


pub fn render(frame: &mut Frame, area: Rect, state: &UiState, snapshot: &RenderSnapshot, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    let title = Paragraph::new(Line::from(Span::styled(
        state.tr("settings_title"),
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent)),
    );
    frame.render_widget(title, chunks[0]);

    let items = vec![
        ListItem::new(Line::from(vec![
            Span::styled(state.tr("settings_theme"), Style::default().fg(theme.text)),
            Span::styled(state.config.theme_name.clone(), Style::default().fg(theme.accent)),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(state.tr("settings_accent"), Style::default().fg(theme.text)),
            Span::styled(state.config.accent_color.clone(), Style::default().fg(theme.text)),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(state.tr("settings_volume"), Style::default().fg(theme.text)),
            Span::styled(format!("{}%", (snapshot.volume * 100.0) as u8), Style::default().fg(theme.accent)),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(state.tr("settings_search_limit"), Style::default().fg(theme.text)),
            Span::styled(format!("{}", state.config.default_search_limit), Style::default().fg(theme.accent)),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(state.tr("settings_download_path"), Style::default().fg(theme.text)),
            Span::styled(
                if state.config.download_path.len() > 40 {
                    let path = &state.config.download_path;
                    let cutoff = path.len().saturating_sub(37);
                    let start = path
                        .char_indices()
                        .find(|(i, _)| *i >= cutoff)
                        .map(|(i, _)| i)
                        .unwrap_or(path.len());
                    format!("...{}", &path[start..])
                } else {
                    state.config.download_path.clone()
                },
                Style::default().fg(theme.accent),
            ),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(state.tr("settings_language"), Style::default().fg(theme.text)),
            Span::styled(state.config.language.clone(), Style::default().fg(theme.accent)),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(state.tr("settings_audio_mode"), Style::default().fg(theme.text)),
            Span::styled(
                match snapshot.audio_mode {
                    AudioMode::Audio => state.tr("status_audio"),
                    AudioMode::Video => state.tr("status_video"),
                },
                Style::default().fg(theme.accent),
            ),
        ])),
        ListItem::new(Line::from(vec![
            Span::styled(state.tr("settings_repeat"), Style::default().fg(theme.text)),
            Span::styled(
                match snapshot.repeat_mode {
                    RepeatMode::None => state.tr("status_repeat_none"),
                    RepeatMode::All => state.tr("status_repeat_all"),
                    RepeatMode::One => state.tr("status_repeat_one"),
                },
                Style::default().fg(theme.accent),
            ),
        ])),
    ];

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(" ⚙️ {} ", state.tr("settings_options")))
                .border_style(Style::default().fg(theme.border_inactive)),
        )
        .highlight_style(Style::default().bg(theme.highlight_bg).fg(theme.highlight_fg))
        .highlight_symbol("▸ ");

    let mut list_state = ListState::default();
    list_state.select(Some(state.settings.settings_focus));
    frame.render_stateful_widget(list, chunks[1], &mut list_state);

    let hints = Line::from(Span::styled(
        state.tr("settings_hints"),
        Style::default().fg(theme.text_muted),
    ));
    frame.render_widget(Paragraph::new(hints).alignment(Alignment::Center), chunks[2]);
}
