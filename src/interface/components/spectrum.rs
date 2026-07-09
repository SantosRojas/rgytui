use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget, Borders};

const BARS: usize = 16;
const HEIGHT: usize = 4;

pub struct SpectrumWidget {
    bands: [f32; BARS],
    accent: Color,
    show_block: bool,
}

impl SpectrumWidget {
    pub fn new(bands: [f32; BARS], accent: Color) -> Self {
        Self { bands, accent, show_block: true }
    }

    pub fn no_block(mut self) -> Self {
        self.show_block = false;
        self
    }
}

impl Widget for SpectrumWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = if self.show_block {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.accent));
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        if inner.width < BARS as u16 || inner.height < 1 {
            return;
        }

        let bar_width = (inner.width as usize / BARS).max(1);
        let avail_height = inner.height as usize;

        let lines: Vec<Line> = (0..HEIGHT.min(avail_height))
            .rev()
            .map(|row| {
                let threshold = (row as f32 + 1.0) / HEIGHT as f32;
                let mut spans = Vec::new();
                for &band in self.bands.iter() {
                    let ch = if band >= threshold { "█" } else { "░" };
                    let intensity = (band * 255.0) as u8;
                    let color = Color::Rgb(
                        (intensity as f32 * 0.3) as u8,
                        (intensity as f32 * 0.8) as u8,
                        intensity,
                    );
                    let style = if band >= threshold {
                        Style::default().fg(color)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    spans.push(Span::styled(ch.repeat(bar_width), style));
                }
                Line::from(spans)
            })
            .collect();

        Paragraph::new(lines).render(inner, buf);
    }
}
