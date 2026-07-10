use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::domain::player_state::PlayerState;
use crate::infrastructure::audio::AudioMode;
use crate::interface::state::Focus;

pub struct StatusBar {
    player_state: PlayerState,
    audio_mode: AudioMode,
    volume: f32,
    focus: Focus,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            player_state: PlayerState::Idle,
            audio_mode: AudioMode::Audio,
            volume: 0.8,
            focus: Focus::SearchInput,
        }
    }

    pub fn player_state(mut self, state: PlayerState) -> Self {
        self.player_state = state;
        self
    }

    pub fn audio_mode(mut self, mode: AudioMode) -> Self {
        self.audio_mode = mode;
        self
    }

    pub fn volume(mut self, vol: f32) -> Self {
        self.volume = vol;
        self
    }

    pub fn focus(mut self, f: Focus) -> Self {
        self.focus = f;
        self
    }
}

impl Widget for StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let state_label = match self.player_state {
            PlayerState::Idle => Span::styled("■ Idle", Style::default().fg(Color::Gray)),
            PlayerState::Loading => Span::styled("◌ Loading", Style::default().fg(Color::Yellow)),
            PlayerState::Playing => Span::styled("▶ Playing", Style::default().fg(Color::Green)),
            PlayerState::Paused => Span::styled("⏸ Paused", Style::default().fg(Color::Yellow)),
            PlayerState::Stopped => Span::styled("■ Stopped", Style::default().fg(Color::Red)),
        };

        let mode_label = match self.audio_mode {
            AudioMode::Audio => Span::styled("🎵 Audio", Style::default().fg(Color::Cyan)),
            AudioMode::Video => Span::styled("🎬 Video", Style::default().fg(Color::Magenta)),
        };

        let vol = (self.volume * 100.0) as u8;
        let vol_bar = if vol > 50 {
            "🔊"
        } else if vol > 0 {
            "🔉"
        } else {
            "🔇"
        };

        let line = Line::from(vec![
            Span::raw("  "),
            state_label,
            Span::raw("  │  "),
            mode_label,
            Span::raw("  │  "),
            Span::styled(
                format!("{} {:3}%", vol_bar, vol),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  │  "),
            Span::styled(
                match self.focus {
                    Focus::SearchInput => "Type to search — Esc:clear  Tab:focus  Enter:search",
                    Focus::SearchResults => "↑↓:navigate  Enter:play  a:add  q:quit",
                    Focus::QueueList => "↑↓:navigate  Enter:play  d:delete  c:clear  q:quit",
                },
                Style::default().fg(Color::Blue),
            ),
            Span::raw("  │  "),
            Span::styled("?:Help  t:Settings  q:Quit  Ctrl+Q:force quit",
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let block = Block::default().style(
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(30, 30, 30)),
        );

        let paragraph = Paragraph::new(line).block(block);
        paragraph.render(area, buf);
    }
}
