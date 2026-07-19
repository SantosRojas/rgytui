use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, BorderType, List, ListItem};
use ratatui::Frame;

use crate::interface::components::input_box::InputBox;
use crate::interface::state::{Focus, RenderSnapshot, UiState};
use crate::interface::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, _snapshot: &RenderSnapshot, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let search_area = chunks[0];
    let results_area = chunks[1];

    let title = if state.search.is_searching {
        format!(" {} {} ", state.spinner_char(), state.tr("search_searching"))
    } else {
        format!(" 🔍 {} ", state.tr("search_title"))
    };

    let placeholder_text = state.tr("search_title");
    let input_box = InputBox::new()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(title)
                .border_style(if state.focus == Focus::SearchInput {
                    Style::default().fg(theme.border_active)
                } else {
                    Style::default().fg(theme.border_inactive)
                }),
        )
        .value(&state.search.search_query)
        .placeholder(&placeholder_text);

    frame.render_widget(input_box, search_area);

    let count_text = format!(" ♫ {} ", state.tr("search_results").replace("{}", &state.search.search_results.len().to_string()));
    let items: Vec<ListItem> = state
        .search
        .search_results
        .iter()
        .enumerate()
        .map(|(i, song)| {
            let duration = song.duration_formatted();
            let content = vec![Line::from(vec![
                Span::styled(
                    format!(" {:2}. ", i + 1),
                    Style::default().fg(if i == state.search.selected_index { theme.highlight_fg } else { theme.text_muted }),
                ),
                Span::styled(
                    &song.title,
                    Style::default()
                        .fg(if i == state.search.selected_index { theme.highlight_fg } else { theme.text })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" — ", Style::default().fg(if i == state.search.selected_index { theme.highlight_fg } else { theme.separator })),
                Span::styled(&song.channel, Style::default().fg(if i == state.search.selected_index { theme.highlight_fg } else { theme.text_secondary })),
                Span::raw(" "),
                Span::styled(
                    format!("[{}]", duration),
                    Style::default().fg(if i == state.search.selected_index { theme.highlight_fg } else { theme.warning }),
                ),
            ])];

            ListItem::new(content).style(if i == state.search.selected_index {
                Style::default().bg(theme.highlight_bg).fg(theme.highlight_fg)
            } else {
                Style::default()
            })
        })
        .collect();

    let border_color = if state.focus == Focus::SearchResults {
        theme.border_active
    } else {
        theme.border_inactive
    };

    let results_list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(count_text)
                .border_style(Style::default().fg(border_color)),
        );

    frame.render_widget(results_list, results_area);
}
