use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::interface::components::input_box::InputBox;
use crate::interface::state::{Focus, UiState};
use crate::interface::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, state: &UiState, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let search_area = chunks[0];
    let results_area = chunks[1];

    let title = if state.is_searching {
        state.tr("search_searching")
    } else {
        state.tr("search_title")
    };

    let input_box = InputBox::new()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(if state.focus == Focus::SearchInput {
                    Style::default().fg(theme.accent)
                } else {
                    Style::default().fg(Color::DarkGray)
                }),
        )
        .value(&state.search_query);

    frame.render_widget(input_box, search_area);

    let count_text = state.tr("search_results").replace("{}", &state.search_results.len().to_string());
    let items: Vec<ListItem> = state
        .search_results
        .iter()
        .enumerate()
        .map(|(i, song)| {
            let duration = song.duration_formatted();
            let content = vec![Line::from(vec![
                Span::styled(
                    &song.title,
                    Style::default()
                        .fg(if i == state.selected_index { theme.highlight_fg } else { theme.text })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" — "),
                Span::styled(&song.channel, Style::default().fg(Color::Gray)),
                Span::raw(" "),
                Span::styled(
                    format!("[{}]", duration),
                    Style::default().fg(Color::Yellow),
                ),
            ])];

            ListItem::new(content).style(if i == state.selected_index {
                Style::default().bg(theme.highlight_bg).fg(theme.highlight_fg)
            } else {
                Style::default()
            })
        })
        .collect();

    let border_color = if state.focus == Focus::SearchResults {
        theme.accent
    } else {
        Color::DarkGray
    };

    let results_list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(count_text).border_style(Style::default().fg(border_color)));

    frame.render_widget(results_list, results_area);
}
