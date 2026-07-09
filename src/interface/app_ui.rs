use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::infrastructure::audio::AudioMode;
use crate::interface::components::status_bar::StatusBar;
use crate::interface::screens::{help_screen, player_screen, search_screen};
use crate::interface::state::{ActiveScreen, UiState};

pub fn render(frame: &mut Frame, state: &UiState, audio_mode: AudioMode) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let title_area = chunks[0];
    let main_area = chunks[1];
    let status_area = chunks[2];

    let title_text = Line::from(vec![
        Span::styled(" rgytui ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("YouTube Music Player", Style::default().fg(Color::DarkGray)),
    ]);

    let title = Paragraph::new(title_text)
        .style(Style::default().bg(Color::Rgb(20, 20, 20)));
    frame.render_widget(title, title_area);

    match state.active_screen {
        ActiveScreen::Search => {
            search_screen::render(frame, main_area, state);
        }
        ActiveScreen::Player => {
            player_screen::render(frame, main_area, state);
        }
        ActiveScreen::Help => {
            help_screen::render(frame, main_area);
        }
    }

    let status_bar = StatusBar::new()
        .player_state(state.player_state)
        .audio_mode(audio_mode)
        .volume(state.volume);

    frame.render_widget(status_bar, status_area);

    if let Some(ref err) = state.error_message {
            let err_widget = Paragraph::new(Line::from(Span::styled(
                err,
                Style::default().fg(Color::Red),
            )))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            );
            frame.render_widget(err_widget, main_area);
        }
}
