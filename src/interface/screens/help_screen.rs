use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, BorderType, Paragraph};
use ratatui::Frame;

use crate::interface::i18n::Translations;
use crate::interface::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, translations: &Translations, theme: &Theme) {
    let t = |k: &str| translations.t(k);

    let section_style = Style::default().fg(theme.accent).add_modifier(Modifier::BOLD);
    let key_style = Style::default().fg(theme.text);
    let muted_style = Style::default().fg(theme.text_muted);

    let help_text = vec![
        Line::from(vec![Span::styled(
            t("help_title"),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_nav"), section_style),
        ]),
        Line::from(Span::styled(t("help_nav_tab"), key_style)),
        Line::from(Span::styled(t("help_nav_updown"), key_style)),
        Line::from(Span::styled(t("help_nav_slash"), key_style)),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_playback"), section_style),
        ]),
        Line::from(Span::styled(t("help_playback_enter"), key_style)),
        Line::from(Span::styled(t("help_playback_space"), key_style)),
        Line::from(Span::styled(t("help_playback_s"), key_style)),
        Line::from(Span::styled(t("help_playback_n"), key_style)),
        Line::from(Span::styled(t("help_playback_p"), key_style)),
        Line::from(Span::styled(t("help_playback_vol"), key_style)),
        Line::from(Span::styled(t("help_playback_v"), key_style)),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_queue"), section_style),
        ]),
        Line::from(Span::styled(t("help_queue_d"), key_style)),
        Line::from(Span::styled(t("help_queue_c"), key_style)),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_search"), section_style),
        ]),
        Line::from(Span::styled(t("help_search_slash"), key_style)),
        Line::from(Span::styled(t("help_search_a"), key_style)),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_general"), section_style),
        ]),
        Line::from(Span::styled(t("help_general_esc"), key_style)),
        Line::from(Span::styled(t("help_general_question"), key_style)),
        Line::from(Span::styled(t("help_general_q"), key_style)),
        Line::from(Span::styled(t("help_general_ctrlq"), key_style)),
        Line::from(""),
        Line::from(vec![
            Span::styled(t("help_close"), muted_style),
        ]),
    ];

    let paragraph = Paragraph::new(Text::from(help_text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(" ❓ {} ", t("help_block_title")))
                .border_style(Style::default().fg(theme.accent)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, area);
}
