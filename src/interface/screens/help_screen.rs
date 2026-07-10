use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::interface::i18n::Translations;

pub fn render(frame: &mut Frame, area: Rect, translations: &Translations) {
    let t = |k: &str| translations.t(k);
    let help_text = vec![
        Line::from(vec![Span::styled(
            t("help_title"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_nav"), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(t("help_nav_tab")),
        Line::from(t("help_nav_updown")),
        Line::from(t("help_nav_slash")),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_playback"), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(t("help_playback_enter")),
        Line::from(t("help_playback_space")),
        Line::from(t("help_playback_s")),
        Line::from(t("help_playback_n")),
        Line::from(t("help_playback_p")),
        Line::from(t("help_playback_vol")),
        Line::from(t("help_playback_v")),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_queue"), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(t("help_queue_d")),
        Line::from(t("help_queue_c")),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_search"), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(t("help_search_slash")),
        Line::from(t("help_search_a")),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_general"), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(t("help_general_esc")),
        Line::from(t("help_general_question")),
        Line::from(t("help_general_q")),
        Line::from(t("help_general_ctrlq")),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_close"), Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let paragraph = Paragraph::new(Text::from(help_text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(t("help_block_title"))
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, area);
}
