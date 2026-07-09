use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::infrastructure::audio::AudioMode;
use crate::interface::components::status_bar::StatusBar;
use crate::interface::screens::{help_screen, player_screen, search_screen, settings_screen};
use crate::interface::state::{ActiveScreen, Focus, UiState};
use crate::interface::theme::Theme;

const QUEUE_VISIBLE: usize = 5;

pub fn render(frame: &mut Frame, state: &UiState, audio_mode: AudioMode) {
    let theme = Theme::from_settings(&state.theme_name, &state.accent_color);

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
        Span::styled(" rgytui ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled("YouTube Music Player", Style::default().fg(Color::DarkGray)),
    ]);
    let title = Paragraph::new(title_text).style(Style::default().bg(theme.panel_bg));
    frame.render_widget(title, title_area);

    match state.active_screen {
        ActiveScreen::Help => {
            help_screen::render(frame, main_area);
        }
        ActiveScreen::Settings => {
            settings_screen::render(frame, main_area, state, &theme);
        }
        ActiveScreen::Player => {
            player_screen::render(frame, main_area, state, &theme);
        }
        ActiveScreen::Search => {
            render_hybrid(frame, main_area, state, &theme);
        }
    }

    let status_bar = StatusBar::new()
        .player_state(state.player_state)
        .audio_mode(audio_mode)
        .volume(state.volume)
        .focus(state.focus);
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

fn render_hybrid(frame: &mut Frame, area: Rect, state: &UiState, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(35, 100), Constraint::Ratio(65, 100)])
        .split(area);

    render_left_panel(frame, chunks[0], state, theme);
    render_right_panel(frame, chunks[1], state, theme);
}

fn render_left_panel(frame: &mut Frame, area: Rect, state: &UiState, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Ratio(50, 100),
            Constraint::Ratio(50, 100),
        ])
        .split(area);

    render_now_playing(frame, chunks[0], state, theme);
    render_queue(frame, chunks[1], state, theme);
}

fn render_now_playing(frame: &mut Frame, area: Rect, state: &UiState, theme: &Theme) {
    let inner_area = {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Now Playing ")
            .border_style(Style::default().fg(theme.accent));
        let inner = block.inner(area);
        block.render(area, frame.buffer_mut());
        inner
    };

    if let Some(ref song) = state.current_song {
        let lines = vec![
            Line::from(Span::styled(
                &song.title,
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(&song.channel, Style::default().fg(Color::Gray))),
            Line::from(Span::raw("")),
            Line::from(Span::styled(
                format!("{:02}:{:02} / {:02}:{:02}",
                    state.progress as u64 / 60, state.progress as u64 % 60,
                    state.duration as u64 / 60, state.duration as u64 % 60),
                Style::default().fg(Color::Yellow),
            )),
        ];
        let paragraph = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center);

        if inner_area.height > 4 {
            let sub = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(4), Constraint::Min(1)])
                .split(inner_area);
            frame.render_widget(paragraph, sub[0]);

            let spectrum = crate::interface::components::spectrum::SpectrumWidget::new(state.spectrum.bands, state.spectrum.peaks, theme.accent).no_block();
            frame.render_widget(spectrum, sub[1]);
        } else {
            frame.render_widget(paragraph, inner_area);
        }
    } else {
        let no_song = Paragraph::new(Line::from(Span::styled(
            "No track loaded",
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(no_song, inner_area);
    }
}

fn render_queue(frame: &mut Frame, area: Rect, state: &UiState, theme: &Theme) {
    let border_color = if state.focus == Focus::QueueList {
        theme.accent
    } else {
        Color::DarkGray
    };

    let inner_area = {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Queue ({}) ", state.queue_songs.len()))
            .border_style(Style::default().fg(border_color));
        let inner = block.inner(area);
        block.render(area, frame.buffer_mut());
        inner
    };

    if state.queue_songs.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "Queue is empty — search and add songs",
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(empty, inner_area);
        return;
    }

    let items: Vec<ListItem> = state
        .queue_songs
        .iter()
        .enumerate()
        .map(|(i, song)| {
            let prefix = if i == state.queue_current { "▶ " } else { "  " };
            let content = vec![Line::from(vec![
                Span::styled(prefix, if i == state.queue_current { theme.accent } else { Color::DarkGray }),
                Span::styled(
                    &song.title,
                    Style::default().fg(if i == state.queue_selected {
                        theme.highlight_fg
                    } else if i == state.queue_current {
                        theme.accent
                    } else {
                        theme.text
                    }),
                ),
            ])];
            ListItem::new(content).style(if i == state.queue_selected {
                Style::default().bg(theme.highlight_bg)
            } else {
                Style::default()
            })
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner_area);
}

fn render_right_panel(frame: &mut Frame, area: Rect, state: &UiState, theme: &Theme) {
    let border_color = if state.focus == Focus::SearchInput || state.focus == Focus::SearchResults {
        theme.accent
    } else {
        Color::DarkGray
    };

    let inner_area = {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Browse ")
            .border_style(Style::default().fg(border_color));
        let inner = block.inner(area);
        block.render(area, frame.buffer_mut());
        inner
    };

    search_screen::render(frame, inner_area, state, theme);
}
