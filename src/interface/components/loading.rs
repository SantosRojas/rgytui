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
}

impl LoadingWidget {
    pub fn new(frame: usize, accent: Color, _style: LoadingAnimation) -> Self {
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
// Expanding concentric rings from the center with smooth fade-in/out,
// breathing background glow, and direct cell rendering for zero allocations.

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

    let ring_width = (max_r * 0.15).max(1.5);
    let speed = 0.025;

    // ── Precompute ring data ONCE per frame (not per cell) ──
    let mut radii = [0.0f32; 3];
    let mut envelopes = [0.0f32; 3];
    for r in 0..3 {
        let offset = r as f32 / 3.0;
        let phase = (frame as f32 * speed + offset) % 1.0;
        envelopes[r] = (phase * std::f32::consts::PI).sin();
        radii[r] = phase * max_r;
    }

    // Breathing background glow — slow sinusoidal ambient pulse.
    // Every cell gets a faint baseline that rises and falls, making the
    // whole area feel alive even between ring passes.
    let breath = ((frame as f32 * 0.015).sin() * 0.08 + 0.12).max(0.03);

    let base_x = area.x;
    let base_y = area.y;

    for row in 0..height {
        let dy = row as f32 - cy;
        let dy2 = dy * dy;

        for col in 0..width {
            let dx = col as f32 - cx;
            let dist = (dx * dx + dy2).sqrt();

            // Start with the breathing floor, then let rings push it up
            let mut intensity = breath;

            for r in 0..3 {
                let d = (dist - radii[r]).abs();
                if d < ring_width {
                    let t = d / ring_width;
                    let falloff = 1.0 - t * t; // quadratic
                    let ring_intensity = falloff * envelopes[r];
                    if ring_intensity > intensity {
                        intensity = ring_intensity;
                    }
                }
            }

            // Set cell directly — zero allocations in the hot loop
            if let Some(cell) = buf.cell_mut((base_x + col as u16, base_y + row as u16)) {
                if intensity > 0.02 {
                    let idx = ((intensity * 7.0).floor() as usize).min(7);
                    let color = lerp_rgb(dim, accent_rgb, intensity);
                    cell.set_symbol(PULSE_BLOCKS[idx]);
                    cell.set_style(Style::default().fg(color));
                } else {
                    cell.set_symbol(" ");
                    cell.set_style(Style::default());
                }
            }
        }
    }
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

        render_pulse(buf, anim_area, self.frame, accent_rgb);

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
