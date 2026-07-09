use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::domain::player_state::PlayerState;
use crate::interface::components::progress_bar::ProgressBar;
use crate::interface::state::UiState;

pub fn render(frame: &mut Frame, area: Rect, state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Length(2),
            Constraint::Length(5),
        ])
        .split(area);

    let header_area = chunks[0];
    let info_area = chunks[1];
    let progress_area = chunks[2];
    let controls_area = chunks[3];

    let header = Block::default()
        .borders(Borders::ALL)
        .title("Now Playing")
        .border_style(Style::default().fg(Color::Cyan));

    let header_text = if state.loading_status.is_some() {
        "⏳ Loading"
    } else {
        match state.player_state {
            PlayerState::Playing => "▶ Playing",
            PlayerState::Paused => "⏸ Paused",
            PlayerState::Loading => "⏳ Loading",
            PlayerState::Stopped | PlayerState::Idle => "⏹ Stopped",
        }
    };

    let paragraph = Paragraph::new(Line::from(Span::styled(
        header_text,
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    )))
    .block(header)
    .alignment(Alignment::Center);

    frame.render_widget(paragraph, header_area);

    if let Some(ref status) = state.loading_status {
        let loading_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        let loading_text = Paragraph::new(Line::from(Span::styled(
            status,
            Style::default().fg(Color::Yellow),
        )))
        .block(loading_block)
        .alignment(Alignment::Center);
        frame.render_widget(loading_text, info_area);
    } else if let Some(song) = &state.current_song {
        let info_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let title_line = Line::from(Span::styled(
            &song.title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        let channel_line = Line::from(Span::styled(
            &song.channel,
            Style::default().fg(Color::Gray),
        ));

        let info = Paragraph::new(vec![title_line, channel_line])
            .block(info_block)
            .alignment(Alignment::Center);

        frame.render_widget(info, info_area);
    } else {
        let no_song = Paragraph::new("No track loaded. Search and select a song to play.")
            .block(Block::default().borders(Borders::ALL))
            .alignment(Alignment::Center);
        frame.render_widget(no_song, info_area);
    }

    if state.loading_status.is_none() {
        let progress_bar = ProgressBar::new()
            .progress(state.progress_percent() as f32)
            .position(state.progress)
            .duration(state.duration);

        frame.render_widget(progress_bar, progress_area);
    }

    let controls_block = Block::default().borders(Borders::ALL).border_style(
        Style::default().fg(Color::DarkGray),
    );
    let controls_text = vec![
        Line::from(vec![
            Span::styled("◄◄  ", Style::default().fg(Color::Cyan)),
            Span::styled("⏸/▶", Style::default().fg(Color::Green)),
            Span::styled("  ►►", Style::default().fg(Color::Cyan)),
            Span::raw("    │    "),
            Span::styled("Volume: ", Style::default().fg(Color::Gray)),
            Span::styled(
                state.volume_bar(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![Span::styled(
            format!("  {:3}%", (state.volume * 100.0) as u8),
            Style::default().fg(Color::Yellow),
        )]),
    ];

    let controls = Paragraph::new(controls_text)
        .block(controls_block)
        .alignment(Alignment::Center);

    frame.render_widget(controls, controls_area);
}
