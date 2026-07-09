use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Widget};

pub struct InputBox<'a> {
    block: Option<Block<'a>>,
    value: &'a str,
}

impl<'a> InputBox<'a> {
    pub fn new() -> Self {
        Self {
            block: None,
            value: "",
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

        let cursor_visible = if self.value.is_empty() {
            " "
        } else {
            ""
        };

        let display = if self.value.is_empty() {
            format!("{}{}", self.value, "█")
        } else {
            format!("{}█{}", self.value, cursor_visible)
        };

        let paragraph = Paragraph::new(Line::from(display));
        paragraph.render(inner, buf);
    }
}
