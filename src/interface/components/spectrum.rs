use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, BorderType, Paragraph, Widget};

use crate::infrastructure::audio::spectrum::BANDS;

/// Vertical gradient overlay: colors shift towards warm tones near the top.
const HEIGHT_COLORS: [(f32, Color); 3] = [
    (0.85, Color::Rgb(255, 60, 60)),   // top: red (clipping zone)
    (0.60, Color::Rgb(255, 190, 40)),   // upper-mid: amber
    (0.00, Color::Rgb(0, 0, 0)),        // base: use per-band color
];

const PARTIAL_BLOCKS: [&str; 7] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇"];
const FULL_BLOCK: &str = "█";
const PEAK_CHAR: &str = "▔";

pub struct SpectrumWidget {
    bands: [f32; BANDS],
    peaks: [f32; BANDS],
    accent: Color,
    show_block: bool,
}

impl SpectrumWidget {
    pub fn new(bands: [f32; BANDS], peaks: [f32; BANDS], accent: Color) -> Self {
        Self { bands, peaks, accent, show_block: true }
    }

    pub fn no_block(mut self) -> Self {
        self.show_block = false;
        self
    }
}

/// Interpolate between two RGB colors. `t` is clamped to [0, 1].
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let t = t.clamp(0.0, 1.0);
            Color::Rgb(
                (ar as f32 + (br as f32 - ar as f32) * t) as u8,
                (ag as f32 + (bg as f32 - ag as f32) * t) as u8,
                (ab as f32 + (bb as f32 - ab as f32) * t) as u8,
            )
        }
        _ => b,
    }
}

/// Dynamic color generator for any number of bands.
/// Blends from deep blue → cyan → green → yellow → orange → red → magenta.
fn color_for_index(idx: usize, total: usize) -> Color {
    let t = idx as f32 / (total - 1).max(1) as f32;
    let anchors = [
        Color::Rgb(60, 120, 255),   // Deep Blue
        Color::Rgb(0, 220, 210),    // Cyan
        Color::Rgb(50, 245, 100),   // Green
        Color::Rgb(245, 200, 0),    // Yellow
        Color::Rgb(255, 120, 0),    // Orange
        Color::Rgb(255, 30, 90),    // Red
        Color::Rgb(240, 20, 140),   // Magenta
    ];
    let num_anchors = anchors.len();
    let pos = t * (num_anchors - 1) as f32;
    let anchor_idx = (pos.floor() as usize).min(num_anchors - 2);
    let frac = pos - anchor_idx as f32;
    let next_idx = (anchor_idx + 1).min(num_anchors - 1);
    lerp_color(anchors[anchor_idx], anchors[next_idx], frac)
}

/// Apply vertical gradient: blend the base bar color with height-based warm tones.
fn color_for_position(base: Color, height_ratio: f32) -> Color {
    for &(threshold, warm) in &HEIGHT_COLORS[..2] {
        if height_ratio > threshold {
            let blend = ((height_ratio - threshold) / (1.0 - threshold)).clamp(0.0, 0.7);
            return lerp_color(base, warm, blend);
        }
    }
    base
}

impl Widget for SpectrumWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let inner = if self.show_block {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
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

        // Calculate dynamic layout based on actual widget width:
        // Each bar is 1 column wide, with 1 column gap between them.
        // N * (bar_width + gap) - gap <= width => N * 2 - 1 <= width => N <= (width + 1) / 2.
        let gap = 1usize;
        let bar_width = 1usize;
        let group_width = bar_width + gap;
        let num_bars = ((width + gap) / group_width).max(1);

        // Precalculate linearly resampled bands and peaks from the 32 FFT bands for the actual display columns.
        let mut resampled_bands = vec![0.0f32; num_bars];
        let mut resampled_peaks = vec![0.0f32; num_bars];

        for i in 0..num_bars {
            if num_bars == 1 {
                resampled_bands[i] = self.bands[BANDS / 2];
                resampled_peaks[i] = self.peaks[BANDS / 2];
            } else {
                let t = i as f32 / (num_bars - 1) as f32;
                let band_pos = t * (BANDS - 1) as f32;
                let idx = (band_pos.floor() as usize).min(BANDS - 1);
                let frac = band_pos - idx as f32;
                let next = (idx + 1).min(BANDS - 1);

                resampled_bands[i] = self.bands[idx] * (1.0 - frac) + self.bands[next] * frac;
                resampled_peaks[i] = self.peaks[idx] * (1.0 - frac) + self.peaks[next] * frac;
            }
        }

        let lines: Vec<Line> = (0..rows)
            .rev()
            .map(|row| {
                let mut spans = Vec::with_capacity(width);
                let row_f = row as f32;
                for x in 0..width {
                    let bar_idx = x / group_width;
                    let within_group = x % group_width;

                    // If gap column or beyond the columns we can paint, render empty space
                    if within_group >= bar_width || bar_idx >= num_bars {
                        spans.push(Span::styled(" ", Style::default()));
                        continue;
                    }

                    let value = resampled_bands[bar_idx];
                    let peak_val = resampled_peaks[bar_idx];

                    // Generate smooth continuous gradient based on this bar's fraction of the total width
                    let base_color = color_for_index(bar_idx, num_bars);
                    let height_ratio = row_f / rows as f32;
                    let color = color_for_position(base_color, height_ratio);

                    let pixel_top = value * rows as f32;
                    let fill = pixel_top - row_f;
                    let peak_top = peak_val * rows as f32;

                    if fill >= 1.0 {
                        spans.push(Span::styled(FULL_BLOCK, Style::default().fg(color)));
                    } else if fill > 0.0 {
                        let idx = ((fill * 8.0).floor() as usize).min(6);
                        spans.push(Span::styled(PARTIAL_BLOCKS[idx], Style::default().fg(color)));
                    } else if peak_top >= row_f && peak_top < row_f + 1.0 {
                        spans.push(Span::styled(PEAK_CHAR, Style::default().fg(Color::Rgb(180, 180, 220))));
                    } else {
                        spans.push(Span::styled(" ", Style::default()));
                    }
                }
                Line::from(spans)
            })
            .collect();

        Paragraph::new(lines).render(inner, buf);
    }
}
