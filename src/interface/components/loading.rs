use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::domain::loading_animation::LoadingAnimation;

/// A modern loading animation widget that replaces the spectrum while a song is buffering.
///
/// Supports three animation styles selectable via settings:
/// - Wave: pulsing sinusoidal wave (classic, default)
/// - Skeleton Sweep: placeholder blocks with a scanning highlight (modern, LinkedIn-style)
/// - Indeterminate Bar: Material Design-style bar that slides back and forth
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

fn brightened_rgb(rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    (
        (rgb.0 as u16 + 60).min(255) as u8,
        (rgb.1 as u16 + 60).min(255) as u8,
        (rgb.2 as u16 + 60).min(255) as u8,
    )
}

// ── Wave animation ───────────────────────────────────────────────────────────

const WAVE_BLOCKS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

fn render_wave(buf: &mut Buffer, area: Rect, frame: usize, accent_rgb: (u8, u8, u8)) {
    let width = area.width as usize;
    let height = area.height as usize;
    let t = frame as f32 * 0.35;
    let dim = dimmed_rgb(accent_rgb);
    let bright = brightened_rgb(accent_rgb);

    let mut lines: Vec<Line> = Vec::with_capacity(height);
    for row in 0..height {
        let mut spans = Vec::with_capacity(width);
        for col in 0..width {
            let wave = ((col as f32 * 0.4 - t).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let pixel_height = wave * height as f32;
            let fill = pixel_height - row as f32;

            if fill >= 1.0 {
                let ratio = row as f32 / height as f32;
                let color = lerp_rgb(accent_rgb, bright, ratio);
                spans.push(Span::styled(WAVE_BLOCKS[7], Style::default().fg(color)));
            } else if fill > 0.0 {
                let idx = ((fill * 8.0).floor() as usize).min(7);
                let ratio = row as f32 / height as f32;
                let color = lerp_rgb(dim, accent_rgb, ratio + fill * 0.3);
                spans.push(Span::styled(WAVE_BLOCKS[idx], Style::default().fg(color)));
            } else {
                spans.push(Span::styled(" ", Style::default()));
            }
        }
        lines.push(Line::from(spans));
    }
    Paragraph::new(lines).render(area, buf);
}

// ── Skeleton Sweep animation ─────────────────────────────────────────────────
//
// Renders placeholder █ blocks with a bright gradient highlight that sweeps
// horizontally like a scanner. The highlight has a Gaussian-like falloff so it
// looks like a light bar moving across content placeholders.

fn render_skeleton_sweep(buf: &mut Buffer, area: Rect, frame: usize, accent_rgb: (u8, u8, u8)) {
    let width = area.width as usize;
    let height = area.height as usize;
    let dim = dimmed_rgb(accent_rgb);

    // Sweep position: goes from 0..width, wraps around
    let sweep_phase = (frame as f32 * 0.06).fract();
    let sweep_center = (sweep_phase * width as f32) as usize;

    // Width of the highlight zone (about 25 % of total width)
    let highlight_width = (width / 4).max(3);

    let mut lines: Vec<Line> = Vec::with_capacity(height);
    for _row in 0..height {
        let mut spans = Vec::with_capacity(width);
        for col in 0..width {
            // Distance from sweep center (handling wrap-around)
            let dist_from_center = if col >= sweep_center {
                col - sweep_center
            } else {
                width - sweep_center + col
            };

            if dist_from_center < highlight_width {
                // Within highlight zone: gradient from bright center to dim edges
                let t = dist_from_center as f32 / highlight_width as f32;
                let falloff = 1.0 - t * t; // quadratic falloff
                let color = lerp_rgb(dim, accent_rgb, falloff);
                spans.push(Span::styled("█", Style::default().fg(color)));
            } else {
                // Outside: dimmed placeholder
                spans.push(Span::styled("▒", Style::default().fg(Color::Rgb(dim.0, dim.1, dim.2))));
            }
        }
        lines.push(Line::from(spans));
    }
    Paragraph::new(lines).render(area, buf);
}

// ── Indeterminate Bar animation ──────────────────────────────────────────────
//
// A Material Design-style bar that slides back and forth. A bright bar segment
// (about 30 % width) moves left→right, then reverses direction.

fn render_indeterminate_bar(buf: &mut Buffer, area: Rect, frame: usize, accent_rgb: (u8, u8, u8)) {
    let width = area.width as usize;
    let height = area.height as usize;
    let dim = dimmed_rgb(accent_rgb);

    // Bar width = 30 % of total, min 4
    let bar_width = (width * 30 / 100).max(4);
    let travel = width.saturating_sub(bar_width);

    // Ping-pong: triangle wave over 0..travel (or stay at 0 if no room)
    let safe_travel = travel.max(1);
    let cycle = safe_travel * 2;
    let phase = (frame as f32 * 0.08) as usize % cycle;
    let bar_start = if phase < safe_travel {
        phase
    } else {
        cycle - phase
    }
    .min(travel);

    let bar_end = (bar_start + bar_width).min(width);

    let mut lines: Vec<Line> = Vec::with_capacity(height);
    for _row in 0..height {
        let mut spans = Vec::with_capacity(width);
        for col in 0..width {
            if col >= bar_start && col < bar_end {
                // Within the bar: gradient from centre out
                let dist_to_edge = (col - bar_start).min(bar_end - 1 - col);
                let t = dist_to_edge as f32 / (bar_width / 2).max(1) as f32;
                let brightness = 1.0 - t * 0.4; // 60-100 % brightness
                let color = lerp_rgb(dim, accent_rgb, brightness);
                spans.push(Span::styled("█", Style::default().fg(color)));
            } else {
                spans.push(Span::styled("░", Style::default().fg(Color::Rgb(dim.0, dim.1, dim.2))));
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

        match self.style {
            LoadingAnimation::Wave => {
                render_wave(buf, anim_area, self.frame, accent_rgb);
            }
            LoadingAnimation::SkeletonSweep => {
                render_skeleton_sweep(buf, anim_area, self.frame, accent_rgb);
            }
            LoadingAnimation::IndeterminateBar => {
                render_indeterminate_bar(buf, anim_area, self.frame, accent_rgb);
            }
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
