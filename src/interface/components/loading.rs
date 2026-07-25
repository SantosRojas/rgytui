use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::domain::loading_animation::LoadingAnimation;

/// A modern loading animation widget that replaces the spectrum while a song is buffering.
///
/// Uses a Pulse animation: expanding concentric rings from the center (radar/sonar style).
pub struct LoadingWidget {
    frame: usize,
    accent: Color,
    message: String,
    style: LoadingAnimation,
}

impl LoadingWidget {
    pub fn new(frame: usize, accent: Color, style: LoadingAnimation) -> Self {
        Self {
            frame,
            accent,
            message: String::new(),
            style,
        }
    }

    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }
}

// ── Colour helpers ───────────────────────────────────────────────────────────

/// Interpolate between two RGB colours.
fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::Rgb(
        (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t) as u8,
        (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t) as u8,
        (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t) as u8,
    )
}

/// Decompose a Color into (r, g, b). Falls back to cyan for non-RGB colours.
fn decompose(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 220, 255),
    }
}

fn dimmed_rgb(rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    (rgb.0 / 5, rgb.1 / 5, rgb.2 / 5)
}

// ── Pulse animation ──────────────────────────────────────────────────────────
//
// Expanding concentric rings from the center that fade in and out smoothly —
// no abrupt wrap-around jump.

const PULSE_BLOCKS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

fn render_pulse(buf: &mut Buffer, area: Rect, frame: usize, accent_rgb: (u8, u8, u8)) {
    let width = area.width as usize;
    let height = area.height as usize;
    if width < 3 || height < 2 {
        return;
    }
    let dim = dimmed_rgb(accent_rgb);

    let cx = width as f32 / 2.0;
    let cy = height as f32 / 2.0;
    let max_r = (cx * cx + cy * cy).sqrt();
    if max_r < 1.0 {
        return;
    }

    let ring_count = 3usize;
    let ring_width = (max_r * 0.15).max(1.5);
    let speed = 0.025;

    let mut lines: Vec<Line> = Vec::with_capacity(height);
    for row in 0..height {
        let mut spans = Vec::with_capacity(width);
        for col in 0..width {
            let dx = col as f32 - cx;
            let dy = row as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            // Accumulate intensity from all rings (each with smooth envelope).
            // Because the envelope peaks mid-phase and reaches 0 at both ends,
            // a ring fading out at the edge is replaced by the next ring
            // fading in at the center — no abrupt jump.
            let mut intensity = 0.0f32;
            for r in 0..ring_count {
                let offset = r as f32 / ring_count as f32;
                let phase = (frame as f32 * speed + offset) % 1.0;
                // sin(π·phase) → 0 at 0.0, 1 at 0.5, 0 at 1.0 — smooth fade both ends
                let envelope = (phase * std::f32::consts::PI).sin();
                let radius = phase * max_r;

                let d = (dist - radius).abs();
                if d < ring_width {
                    let t = d / ring_width;
                    let falloff = 1.0 - t * t; // quadratic
                    intensity = intensity.max(falloff * envelope);
                }
            }

            if intensity > 0.02 {
                // Use block characters for a solid filled look
                let idx = ((intensity * 7.0).floor() as usize).min(7);
                let color = lerp_rgb(dim, accent_rgb, intensity);
                spans.push(Span::styled(PULSE_BLOCKS[idx], Style::default().fg(color)));
            } else {
                spans.push(Span::styled(" ", Style::default()));
            }
        }
        lines.push(Line::from(spans));
    }
    Paragraph::new(lines).render(area, buf);
}

// ── Spinner message ──────────────────────────────────────────────────────────

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn render_message_bar(buf: &mut Buffer, area: Rect, frame: usize, message: &str, accent_rgb: (u8, u8, u8)) {
    if area.height == 0 || message.is_empty() {
        return;
    }
    let dim = dimmed_rgb(accent_rgb);
    let spinner = SPINNER[frame % SPINNER.len()];
    let pulse = ((frame as f32 * 0.3).sin() * 0.3 + 0.7).clamp(0.4, 1.0);
    let msg_color = lerp_rgb(dim, accent_rgb, pulse);

    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", spinner),
            Style::default().fg(Color::Rgb(accent_rgb.0, accent_rgb.1, accent_rgb.2))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(message, Style::default().fg(msg_color)),
    ]);
    Paragraph::new(line)
        .alignment(Alignment::Center)
        .render(area, buf);
}

// ── Widget impl ──────────────────────────────────────────────────────────────

impl Widget for LoadingWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 2 || area.height < 1 {
            return;
        }

        let height = area.height as usize;
        let accent_rgb = decompose(self.accent);

        // Reserve bottom row for spinner message
        let (anim_rows, msg_rows) = if height >= 3 && !self.message.is_empty() {
            (height - 1, 1usize)
        } else {
            (height, 0)
        };

        // Render animation area
        let anim_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: anim_rows as u16,
        };

        if self.style == LoadingAnimation::Pulse {
            render_pulse(buf, anim_area, self.frame, accent_rgb);
        }

        // Render message row
        if msg_rows > 0 {
            let msg_area = Rect {
                x: area.x,
                y: area.y + anim_rows as u16,
                width: area.width,
                height: 1,
            };
            render_message_bar(buf, msg_area, self.frame, &self.message, accent_rgb);
        }
    }
}
