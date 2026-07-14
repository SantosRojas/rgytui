use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, BorderType, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::infrastructure::audio::AudioMode;
use crate::interface::components::status_bar::StatusBar;
use crate::interface::screens::{help_screen, player_screen, search_screen, settings_screen};
use crate::interface::state::{ActiveScreen, Focus, NotificationLevel, UiState};
use crate::interface::theme::Theme;

const QUEUE_VISIBLE: usize = 5;

pub fn render(frame: &mut Frame, state: &UiState, audio_mode: AudioMode, theme: &Theme) {
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
        Span::styled(" 🎵 rgytui ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(state.tr("app_subtitle"), Style::default().fg(theme.text_muted)),
        Span::styled("  ◆  ", Style::default().fg(theme.separator)),
        Span::styled("Santos Rojas", Style::default().fg(theme.accent)),
    ]);
    let title = Paragraph::new(title_text).style(Style::default().bg(theme.panel_bg));
    frame.render_widget(title, title_area);

    match state.active_screen {
        ActiveScreen::Help => {
            help_screen::render(frame, main_area, &state.translations, theme);
        }
        ActiveScreen::Settings => {
            settings_screen::render(frame, main_area, state, theme);
        }
        ActiveScreen::Player => {
            player_screen::render(frame, main_area, state, theme);
        }
        ActiveScreen::Search => {
            render_hybrid(frame, main_area, state, theme);
        }
    }

    let status_bar = StatusBar::new()
        .player_state(state.player_state)
        .audio_mode(audio_mode)
        .volume(state.volume)
        .focus(state.focus)
        .translations(state.translations.clone())
        .theme(*theme);
    frame.render_widget(status_bar, status_area);

    if state.show_download_popup {
        render_download_popup(frame, frame.area(), state, theme);
    }

    render_notifications(frame, frame.area(), state, theme);
}

fn render_download_popup(frame: &mut Frame, area: Rect, state: &UiState, theme: &Theme) {
    let formats = [
        ("m4a",  state.tr("fmt_aac")),
        ("mp3",  state.tr("fmt_mp3")),
        ("opus", state.tr("fmt_opus")),
        ("flac", state.tr("fmt_flac")),
        ("wav",  state.tr("fmt_wav")),
    ];

    let items: Vec<ListItem> = formats.iter().enumerate().map(|(i, (name, desc))| {
        let selected = i == state.download_format;
        let content = Line::from(vec![
            Span::styled(
                format!(" {:4} ", name),
                Style::default().fg(if selected { theme.text } else { theme.text_muted }),
            ),
            Span::styled(
                desc.clone(),
                Style::default().fg(if selected { theme.text } else { theme.text_secondary }),
            ),
        ]);
        ListItem::new(content).style(if selected {
            Style::default().bg(Color::Rgb(45, 45, 55))
        } else {
            Style::default()
        })
    }).collect();

    let popup = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(" 💾 {} ", state.tr("download_title")))
                .border_style(Style::default().fg(theme.accent))
                .style(Style::default().bg(theme.panel_bg)),
        )
        .highlight_style(Style::default().bg(Color::Rgb(45, 45, 55)))
        .highlight_symbol("▸");

    let popup_area = centered_rect(40, formats.len() as u16 + 2, area);
    frame.render_widget(Clear, popup_area);
    frame.render_widget(popup, popup_area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect { x, y, width, height }
}

fn render_notifications(frame: &mut Frame, area: Rect, state: &UiState, theme: &Theme) {
    let notifications: Vec<&crate::interface::state::Notification> = state.active_notifications().collect();
    if notifications.is_empty() {
        return;
    }

    // Stack notifications from bottom-right upward
    let notif_width = 48u16.min(area.width.saturating_sub(4));
    let mut y_offset = area.y + area.height.saturating_sub(2);

    for notification in notifications.iter().rev() {
        let notif_height = 3u16;
        if y_offset < notif_height + 1 {
            break;
        }
        y_offset = y_offset.saturating_sub(notif_height);

        let (border_color, bg_color) = match notification.level {
            NotificationLevel::Info    => (theme.accent, Color::Rgb(20, 25, 45)),
            NotificationLevel::Success => (theme.success, Color::Rgb(20, 50, 25)),
            NotificationLevel::Warning => (theme.warning, Color::Rgb(50, 45, 20)),
            NotificationLevel::Error   => (theme.error, Color::Rgb(50, 20, 20)),
        };

        let notif_area = Rect {
            x: area.x + area.width.saturating_sub(notif_width) - 1,
            y: y_offset,
            width: notif_width,
            height: notif_height,
        };

        let text = Line::from(vec![
            Span::styled(
                format!(" {} ", notification.icon()),
                Style::default().fg(border_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                &notification.message,
                Style::default().fg(theme.text),
            ),
        ]);

        let widget = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(bg_color)),
            );

        frame.render_widget(Clear, notif_area);
        frame.render_widget(widget, notif_area);
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
    use crate::interface::components::loading::LoadingWidget;

    let inner_area = {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(" 🎵{}", state.tr("now_playing_title")))
            .border_style(Style::default().fg(theme.accent));
        let inner = block.inner(area);
        block.render(area, frame.buffer_mut());
        inner
    };

    let is_loading = state.player_state == crate::domain::player_state::PlayerState::Loading
        || state.loading_status.is_some();

    if is_loading {
        // Show modern loading animation instead of song info + spectrum
        let loading = LoadingWidget::new(state.spinner_frame, theme.accent)
            .message(state.loading_status.clone().unwrap_or_else(|| state.tr("player_loading")));
        frame.render_widget(loading, inner_area);
    } else if let Some(ref song) = state.current_song {
        let status_icon = match state.player_state {
            crate::domain::player_state::PlayerState::Playing => "▶",
            crate::domain::player_state::PlayerState::Paused  => "⏸",
            crate::domain::player_state::PlayerState::Loading => "⟳",
            _                                                   => "⏹",
        };

        let lines = vec![
            Line::from(vec![
                Span::styled(format!(" {} ", status_icon), Style::default().fg(theme.accent)),
                Span::styled(
                    &song.title,
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(Span::styled(
                "─".repeat((inner_area.width as usize).saturating_sub(2)),
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
        ];
        let paragraph = Paragraph::new(lines);

        if inner_area.height > 5 {
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
        let no_song = Paragraph::new(Line::from(vec![
            Span::styled(" ♫ ", Style::default().fg(theme.text_muted)),
            Span::styled(
                state.tr("no_track_loaded"),
                Style::default().fg(theme.text_muted),
            ),
        ]))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(no_song, inner_area);
    }
}

fn render_queue(frame: &mut Frame, area: Rect, state: &UiState, theme: &Theme) {
    let border_color = if state.focus == Focus::QueueList {
        theme.border_active
    } else {
        theme.border_inactive
    };

    let queue_title = format!(" 📋 {} ", state.tr("queue_title").replace("{}", &state.queue_songs.len().to_string()));
    let inner_area = {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(queue_title)
            .border_style(Style::default().fg(border_color));
        let inner = block.inner(area);
        block.render(area, frame.buffer_mut());
        inner
    };

    if state.queue_songs.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            state.tr("queue_empty"),
            Style::default().fg(theme.text_muted),
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
            let prefix = if i == state.queue_current {
                format!("▶ {:2}.", i + 1)
            } else if i < state.queue_current {
                format!("✓ {:2}.", i + 1)
            } else {
                format!("  {:2}.", i + 1)
            };

            let prefix_color = if i == state.queue_selected {
                theme.highlight_fg
            } else if i == state.queue_current {
                theme.accent
            } else if i < state.queue_current {
                theme.success
            } else {
                theme.text_muted
            };

            let content = vec![Line::from(vec![
                Span::styled(prefix, Style::default().fg(prefix_color)),
                Span::styled(" ", Style::default()),
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
    search_screen::render(frame, area, state, theme);
}
