use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::interface::components::input_box::InputBox;
use crate::interface::state::UiState;

pub fn render(frame: &mut Frame, area: Rect, state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let search_area = chunks[0];
    let results_area = chunks[1];

    let title = if state.is_searching {
        "Searching..."
    } else {
        "Search"
    };

    let input_box = InputBox::new()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(if state.focus_search {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                }),
        )
        .value(&state.search_query);

    frame.render_widget(input_box, search_area);

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
                        .fg(Color::White)
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
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            })
        })
        .collect();

    let results_block = Block::default()
        .borders(Borders::ALL)
        .title("Results")
        .border_style(if !state.focus_search && state.active_screen == crate::interface::state::ActiveScreen::Search { Style::default().fg(Color::Cyan) } else { Style::default() });

    let results_list = List::new(items).block(results_block).highlight_style(
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    frame.render_widget(results_list, results_area);
}
