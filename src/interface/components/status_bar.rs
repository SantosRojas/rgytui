use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::domain::player_state::PlayerState;
use crate::infrastructure::audio::AudioMode;
use crate::interface::i18n::Translations;
use crate::interface::state::Focus;

pub struct StatusBar {
    player_state: PlayerState,
    audio_mode: AudioMode,
    volume: f32,
    focus: Focus,
    translations: Translations,
    accent_color: Color,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            player_state: PlayerState::Idle,
            audio_mode: AudioMode::Audio,
            volume: 0.8,
            focus: Focus::SearchInput,
            translations: Translations::load("es"),
            accent_color: Color::Cyan,
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

    pub fn translations(mut self, t: Translations) -> Self {
        self.translations = t;
        self
    }

    pub fn accent_color(mut self, c: Color) -> Self {
        self.accent_color = c;
        self
    }
}

impl Widget for StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let t = |k: &str| self.translations.t(k);

        let state_label = match self.player_state {
            PlayerState::Idle => Span::styled(t("status_idle"), Style::default().fg(Color::Gray)),
            PlayerState::Loading => Span::styled(t("status_loading"), Style::default().fg(Color::Yellow)),
            PlayerState::Playing => Span::styled(t("status_playing"), Style::default().fg(Color::Green)),
            PlayerState::Paused => Span::styled(t("status_paused"), Style::default().fg(Color::Yellow)),
            PlayerState::Stopped => Span::styled(t("status_stopped"), Style::default().fg(Color::Red)),
        };

        let mode_label = match self.audio_mode {
            AudioMode::Audio => Span::styled(t("status_audio"), Style::default().fg(Color::Cyan)),
            AudioMode::Video => Span::styled(t("status_video"), Style::default().fg(Color::Magenta)),
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
                    Focus::SearchInput => t("hint_search_input"),
                    Focus::SearchResults => t("hint_search_results"),
                    Focus::QueueList => t("hint_queue"),
                },
                Style::default().fg(self.accent_color),
            ),
            Span::raw("  │  "),
            Span::styled(t("hint_general"),
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
