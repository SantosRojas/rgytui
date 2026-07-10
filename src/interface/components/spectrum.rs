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

const PARTIAL_BLOCKS: [&str; 7] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇"];
const FULL_BLOCK: &str = "█";
const PEAK_DOT: &str = "·";

pub struct SpectrumWidget {
    bands: [f32; BARS],
    peaks: [f32; BARS],
    accent: Color,
    show_block: bool,
}

impl SpectrumWidget {
    pub fn new(bands: [f32; BARS], peaks: [f32; BARS], accent: Color) -> Self {
        Self { bands, peaks, accent, show_block: true }
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

        if inner.width < 2 || inner.height < 1 {
            return;
        }

        let width = inner.width as usize;
        let rows = inner.height as usize;
        let w_denom = (width - 1).max(1);
        let bands_f = (BARS - 1) as f32;

        let lines: Vec<Line> = (0..rows)
            .rev()
            .map(|row| {
                let mut spans = Vec::with_capacity(width);
                let row_f = row as f32;
                for x in 0..width {
                    let band_pos = x as f32 * bands_f / w_denom as f32;
                    let band_idx = (band_pos.floor() as usize).min(BARS - 1);
                    let frac = band_pos - band_idx as f32;
                    let next = (band_idx + 1).min(BARS - 1);

                    let value = self.bands[band_idx] * (1.0 - frac) + self.bands[next] * frac;
                    let peak_val = self.peaks[band_idx] * (1.0 - frac) + self.peaks[next] * frac;

                    let color_idx = (x * (BAR_COLORS.len() - 1) / w_denom).min(BAR_COLORS.len() - 1);
                    let color = BAR_COLORS[color_idx];

                    let pixel_top = value * rows as f32;
                    let fill = pixel_top - row_f;
                    let peak_top = peak_val * rows as f32;

                    if fill >= 1.0 {
                        spans.push(Span::styled(FULL_BLOCK, Style::default().fg(color)));
                    } else if fill > 0.0 {
                        let idx = ((fill * 8.0).floor() as usize).min(6);
                        spans.push(Span::styled(PARTIAL_BLOCKS[idx], Style::default().fg(color)));
                    } else if peak_top >= row_f && peak_top < row_f + 1.0 {
                        spans.push(Span::styled(PEAK_DOT, Style::default().fg(Color::White)));
                    } else {
                        spans.push(Span::styled(" ", Style::default().fg(color)));
                    }
                }
                Line::from(spans)
            })
            .collect();

        Paragraph::new(lines).render(inner, buf);
    }
}
