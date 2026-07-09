use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::domain::player_state::PlayerState;
use crate::interface::components::progress_bar::ProgressBar;
use crate::interface::components::spectrum::SpectrumWidget;
use crate::interface::state::UiState;
use crate::interface::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(4),
            Constraint::Length(2),
        ])
        .split(area);

    let header_area = chunks[0];
    let info_area = chunks[1];
    let spectrum_area = chunks[2];
    let progress_area = chunks[3];

    let status = if state.loading_status.is_some() {
        "⏳ Loading"
    } else {
        match state.player_state {
            PlayerState::Playing => "▶ Playing",
            PlayerState::Paused => "⏸ Paused",
            PlayerState::Loading => "⏳ Loading",
            PlayerState::Stopped | PlayerState::Idle => "⏹ Stopped",
        }
    };

    let header = Paragraph::new(Line::from(Span::styled(
        status,
        Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
    )))
    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme.accent)))
    .alignment(Alignment::Center);
    frame.render_widget(header, header_area);

    if let Some(ref loading) = state.loading_status {
        let loading_text = Paragraph::new(Line::from(Span::styled(
            loading,
            Style::default().fg(Color::Yellow),
        )))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow)))
        .alignment(Alignment::Center);
        frame.render_widget(loading_text, info_area);
    } else if let Some(ref song) = state.current_song {
        let info = Paragraph::new(vec![
            Line::from(Span::styled(
                &song.title,
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(&song.channel, Style::default().fg(Color::Gray))),
            Line::from(Span::styled(
                format!("{:02}:{:02} / {:02}:{:02}",
                    state.progress as u64 / 60, state.progress as u64 % 60,
                    state.duration as u64 / 60, state.duration as u64 % 60),
                Style::default().fg(Color::Yellow),
            )),
        ])
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
        .alignment(Alignment::Center);
        frame.render_widget(info, info_area);
    } else {
        let no_song = Paragraph::new("No track loaded")
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        frame.render_widget(no_song, info_area);
    }

    let spectrum = SpectrumWidget::new(state.spectrum, theme.accent).no_block();
    frame.render_widget(spectrum, spectrum_area);

    if state.loading_status.is_none() && state.current_song.is_some() {
        let progress_bar = ProgressBar::new()
            .progress(state.progress_percent() as f32)
            .position(state.progress)
            .duration(state.duration);
        frame.render_widget(progress_bar, progress_area);
    }
}
