use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::application::ports::I18nPort;
use crate::domain::player_state::PlayerState;
use crate::domain::audio_mode::AudioMode;
use crate::interface::i18n::Translations;
use crate::interface::state::Focus;
use crate::interface::theme::Theme;

pub struct StatusBar {
    player_state: PlayerState,
    audio_mode: AudioMode,
    volume: f32,
    focus: Focus,
    translations: Arc<dyn I18nPort>,
    theme: Theme,
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            player_state: PlayerState::Idle,
            audio_mode: AudioMode::Audio,
            volume: 0.8,
            focus: Focus::SearchInput,
            translations: Arc::new(Translations::load("es")) as Arc<dyn I18nPort>,
            theme: Theme::default(),
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

    pub fn translations(mut self, t: Arc<dyn I18nPort>) -> Self {
        self.translations = t;
        self
    }

    pub fn theme(mut self, t: Theme) -> Self {
        self.theme = t;
        self
    }
}

impl Widget for StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let t = |k: &str| self.translations.t(k);
        let th = &self.theme;
        let sep = Span::styled("  ◆  ", Style::default().fg(th.separator));

        let state_label = match self.player_state {
            PlayerState::Idle => Span::styled(t("status_idle"), Style::default().fg(th.text_muted)),
            PlayerState::Loading => Span::styled(t("status_loading"), Style::default().fg(th.warning)),
            PlayerState::Playing => Span::styled(t("status_playing"), Style::default().fg(th.success)),
            PlayerState::Paused => Span::styled(t("status_paused"), Style::default().fg(th.warning)),
            PlayerState::Stopped => Span::styled(t("status_stopped"), Style::default().fg(th.error)),
        };

        let mode_label = match self.audio_mode {
            AudioMode::Audio => Span::styled(t("status_audio"), Style::default().fg(th.accent)),
            AudioMode::Video => Span::styled(t("status_video"), Style::default().fg(Color::Rgb(200, 120, 255))),
        };

        let vol = (self.volume * 100.0) as u8;
        let vol_icon = if vol > 50 {
            "🔊"
        } else if vol > 0 {
            "🔉"
        } else {
            "🔇"
        };

        let line = Line::from(vec![
            Span::raw("  "),
            state_label,
            sep.clone(),
            mode_label,
            sep.clone(),
            Span::styled(
                format!("{} {:3}%", vol_icon, vol),
                Style::default().fg(th.warning),
            ),
            sep.clone(),
            Span::styled(
                match self.focus {
                    Focus::SearchInput => t("hint_search_input"),
                    Focus::SearchResults => t("hint_search_results"),
                    Focus::QueueList => t("hint_queue"),
                },
                Style::default().fg(th.accent),
            ),
            sep,
            Span::styled(t("hint_general"),
                Style::default().fg(th.text_muted),
            ),
        ]);

        let block = Block::default().style(
            Style::default()
                .fg(th.text)
                .bg(th.panel_bg),
        );

        let paragraph = Paragraph::new(line).block(block);
        paragraph.render(area, buf);
    }
}
