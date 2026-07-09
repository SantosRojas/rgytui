use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

const BARS: usize = 16;

const BAR_COLORS: [Color; BARS] = [
    Color::Rgb(0, 200, 255),
    Color::Rgb(0, 210, 230),
    Color::Rgb(0, 220, 200),
    Color::Rgb(0, 230, 170),
    Color::Rgb(50, 240, 120),
    Color::Rgb(100, 250, 70),
    Color::Rgb(150, 255, 20),
    Color::Rgb(200, 240, 0),
    Color::Rgb(230, 210, 0),
    Color::Rgb(250, 180, 0),
    Color::Rgb(255, 150, 0),
    Color::Rgb(255, 120, 0),
    Color::Rgb(255, 90, 10),
    Color::Rgb(255, 60, 30),
    Color::Rgb(255, 30, 50),
    Color::Rgb(255, 0, 80),
];

const PARTIAL_BLOCKS: [char; 7] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇'];

pub struct SpectrumWidget {
    bands: [f32; BARS],
    peaks: [f32; BARS],
    accent: Color,
    show_block: bool,
}

impl SpectrumWidget {
    pub fn new(bands: [f32; BARS], peaks: [f32; BARS], _accent: Color) -> Self {
        Self { bands, peaks, accent: _accent, show_block: true }
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
        let rows = inner.height as usize;

        let lines: Vec<Line> = (0..rows)
            .rev()
            .map(|row| {
                let mut spans = Vec::new();
                for (i, (&band, &peak)) in self.bands.iter().zip(self.peaks.iter()).enumerate() {
                    let color = if i < BAR_COLORS.len() {
                        BAR_COLORS[i]
                    } else {
                        self.accent
                    };
                    let bar_top = band * rows as f32;
                    let row_f = row as f32;
                    let fill = bar_top - row_f;

                    let ch = if fill >= 1.0 {
                        "█"
                    } else if fill > 0.0 {
                        let idx = ((fill * 8.0).floor() as usize).min(6);
                        let c = PARTIAL_BLOCKS[idx];
                        let s: String = c.to_string().repeat(bar_width);
                        spans.push(Span::styled(s, Style::default().fg(color)));
                        continue;
                    } else {
                        let peak_pos = peak * rows as f32;
                        if peak_pos >= row_f && peak_pos < row_f + 1.0 {
                            "·"
                        } else {
                            " "
                        }
                    };
                    spans.push(Span::styled(ch.repeat(bar_width), Style::default().fg(color)));
                }
                Line::from(spans)
            })
            .collect();

        Paragraph::new(lines).render(inner, buf);
    }
}
