use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Widget};

use crate::domain::media::Song;

pub struct MediaList<'a> {
    block: Option<Block<'a>>,
    songs: &'a [Song],
    selected: usize,
}

impl<'a> MediaList<'a> {
    pub fn new() -> Self {
        Self {
            block: None,
            songs: &[],
            selected: 0,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn items(mut self, songs: &'a [Song]) -> Self {
        self.songs = songs;
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
}

impl Widget for MediaList<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let items: Vec<ListItem> = self
            .songs
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
                    Span::raw("  "),
                    Span::styled(format!("[{}]", duration), Style::default().fg(Color::Yellow)),
                ])];

                ListItem::new(content).style(if i == self.selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                })
            })
            .collect();

        let list = List::new(items).highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let inner = if let Some(block) = self.block {
            let inner_area = block.inner(area);
            block.render(area, buf);
            inner_area
        } else {
            area
        };

        list.render(inner, buf);
    }
}
