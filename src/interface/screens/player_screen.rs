use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, BorderType, Paragraph};
use ratatui::Frame;

use crate::domain::player_state::PlayerState;
use crate::interface::components::loading::LoadingWidget;
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

    let status_icon = match state.player_state {
        PlayerState::Playing => "▶",
        PlayerState::Paused  => "⏸",
        PlayerState::Loading => "⟳",
        _                    => "⏹",
    };

    let status = if state.loading_status.is_some() {
        state.tr("player_loading")
    } else {
        match state.player_state {
            PlayerState::Playing => state.tr("player_playing"),
            PlayerState::Paused => state.tr("player_paused"),
            PlayerState::Loading => state.tr("player_loading"),
            PlayerState::Stopped | PlayerState::Idle => state.tr("player_stopped"),
        }
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(format!(" {} ", status_icon), Style::default().fg(theme.accent)),
        Span::styled(
            status,
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent)),
    )
    .alignment(Alignment::Center);
    frame.render_widget(header, header_area);

    if let Some(ref loading) = state.loading_status {
        let spinner = state.spinner_char();
        let loading_text = Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} ", spinner), Style::default().fg(theme.warning)),
            Span::styled(
                loading,
                Style::default().fg(theme.warning),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.warning)),
        )
        .alignment(Alignment::Center);
        frame.render_widget(loading_text, info_area);
    } else if let Some(ref song) = state.current_song {
        let info = Paragraph::new(vec![
            Line::from(Span::styled(
                &song.title,
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "─".repeat((info_area.width as usize).saturating_sub(4)),
                Style::default().fg(theme.separator),
            )),
            Line::from(vec![
                Span::styled(" 🎤 ", Style::default()),
                Span::styled(&song.channel, Style::default().fg(theme.text_secondary)),
            ]),
            Line::from(Span::styled(
                format!("  {:02}:{:02} / {:02}:{:02}",
                    state.progress as u64 / 60, state.progress as u64 % 60,
                    state.duration as u64 / 60, state.duration as u64 % 60),
                Style::default().fg(theme.warning),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(theme.border_inactive)),
        )
        .alignment(Alignment::Center);
        frame.render_widget(info, info_area);
    } else {
        let no_song = Paragraph::new(state.tr("player_no_track"))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme.border_inactive)),
            )
            .alignment(Alignment::Center);
        frame.render_widget(no_song, info_area);
    }

    if state.player_state == PlayerState::Loading || state.loading_status.is_some() {
        let loading = LoadingWidget::new(state.spinner_frame, theme.accent)
            .message(state.loading_status.clone().unwrap_or_else(|| state.tr("player_loading")));
        frame.render_widget(loading, spectrum_area);
    } else {
        let spectrum = SpectrumWidget::new(state.spectrum.bands, state.spectrum.peaks, theme.accent).no_block();
        frame.render_widget(spectrum, spectrum_area);
    }

    if state.loading_status.is_none() && state.current_song.is_some() {
        let progress_bar = ProgressBar::new()
            .progress(state.progress_percent() as f32)
            .position(state.progress)
            .duration(state.duration)
            .accent(theme.accent);
        frame.render_widget(progress_bar, progress_area);
    }
}
