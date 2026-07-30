use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Widget};

pub struct InputBox<'a> {
    block: Option<Block<'a>>,
    value: &'a str,
    placeholder: &'a str,
    cursor_visible: bool,
}

impl<'a> InputBox<'a> {
    pub fn new() -> Self {
        Self {
            block: None,
            value: "",
            placeholder: "",
            cursor_visible: true,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn value(mut self, value: &'a str) -> Self {
        self.value = value;
        self
    }

    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    pub fn cursor_visible(mut self, visible: bool) -> Self {
        self.cursor_visible = visible;
        self
    }
}

impl Widget for InputBox<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = if let Some(block) = self.block {
            let inner_area = block.inner(area);
            block.render(area, buf);
            inner_area
        } else {
            area
        };

        let cursor_char = if self.cursor_visible { "█" } else { " " };

        if self.value.is_empty() {
            // Show placeholder in muted color with blinking cursor
            let paragraph = Paragraph::new(Line::from(vec![
                ratatui::text::Span::styled(cursor_char, Style::default().fg(Color::White)),
                ratatui::text::Span::styled(
                    format!(" {}", self.placeholder),
                    Style::default().fg(Color::Rgb(80, 80, 90)),
                ),
            ]));
            paragraph.render(inner, buf);
        } else {
            let display = format!("{}{}", self.value, cursor_char);
            let paragraph = Paragraph::new(Line::from(display));
            paragraph.render(inner, buf);
        }
    }
}
