use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

/// A modern loading animation widget that replaces the spectrum while a song is buffering.
///
/// Renders a pulsing wave of bars with a gradient that shifts over time,
/// plus a centered loading message with a spinner character.
pub struct LoadingWidget {
    frame: usize,
    accent: Color,
    message: String,
}

impl LoadingWidget {
    pub fn new(frame: usize, accent: Color) -> Self {
        Self {
            frame,
            accent,
            message: String::new(),
        }
    }

    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }
}

/// Interpolate between two RGB colors.
fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::Rgb(
        (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t) as u8,
        (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t) as u8,
        (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t) as u8,
    )
}

const WAVE_BLOCKS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

impl Widget for LoadingWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 2 || area.height < 1 {
            return;
        }

        let width = area.width as usize;
        let height = area.height as usize;

        // Reserve bottom row for spinner message if we have enough space
        let (wave_rows, msg_rows) = if height >= 3 && !self.message.is_empty() {
            (height - 1, 1usize)
        } else {
            (height, 0)
        };

        let t = self.frame as f32 * 0.35; // animation speed

        // Accent color decomposition for gradient
        let (ar, ag, ab) = match self.accent {
            Color::Rgb(r, g, b) => (r, g, b),
            _ => (0, 220, 255),
        };
        let accent_rgb = (ar, ag, ab);
        let dim_rgb = (ar / 5, ag / 5, ab / 5);
        let bright_rgb = (
            (ar as u16 + 60).min(255) as u8,
            (ag as u16 + 60).min(255) as u8,
            (ab as u16 + 60).min(255) as u8,
        );

        // Build wave animation lines
        let mut lines: Vec<Line> = Vec::with_capacity(wave_rows);
        for row in (0..wave_rows).rev() {
            let mut spans = Vec::with_capacity(width);
            for col in 0..width {
                // Single sinusoidal wave: amplitude varies per column with phase shift from animation frame
                let wave = ((col as f32 * 0.4 - t).sin() * 0.5 + 0.5).clamp(0.0, 1.0);

                let pixel_height = wave * wave_rows as f32;
                let fill = pixel_height - row as f32;

                if fill >= 1.0 {
                    let height_ratio = row as f32 / wave_rows as f32;
                    let color = lerp_rgb(accent_rgb, bright_rgb, height_ratio);
                    spans.push(Span::styled(WAVE_BLOCKS[7], Style::default().fg(color)));
                } else if fill > 0.0 {
                    let idx = ((fill * 8.0).floor() as usize).min(7);
                    let height_ratio = row as f32 / wave_rows as f32;
                    let color = lerp_rgb(dim_rgb, accent_rgb, height_ratio + fill * 0.3);
                    spans.push(Span::styled(WAVE_BLOCKS[idx], Style::default().fg(color)));
                } else {
                    spans.push(Span::styled(" ", Style::default()));
                }
            }
            lines.push(Line::from(spans));
        }

        // Render wave
        if wave_rows > 0 {
            let wave_area = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: wave_rows as u16,
            };
            Paragraph::new(lines).render(wave_area, buf);
        }

        // Render message row
        if msg_rows > 0 {
            const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let spinner = SPINNER[self.frame % SPINNER.len()];

            // Pulsing opacity for the message
            let pulse = ((self.frame as f32 * 0.3).sin() * 0.3 + 0.7).clamp(0.4, 1.0);
            let msg_color = lerp_rgb(dim_rgb, accent_rgb, pulse);

            let msg_line = Line::from(vec![
                Span::styled(
                    format!(" {} ", spinner),
                    Style::default().fg(self.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(&self.message, Style::default().fg(msg_color)),
            ]);

            let msg_area = Rect {
                x: area.x,
                y: area.y + wave_rows as u16,
                width: area.width,
                height: 1,
            };
            Paragraph::new(msg_line)
                .alignment(Alignment::Center)
                .render(msg_area, buf);
        }
    }
}
