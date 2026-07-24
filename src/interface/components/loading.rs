use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::domain::loading_animation::LoadingAnimation;

/// A modern loading animation widget that replaces the spectrum while a song is buffering.
///
/// Supports five animation styles selectable via settings:
/// - Wave: pulsing multi-harmonic sinusoidal wave (classic, default)
/// - Skeleton Sweep: placeholder blocks with a scanning highlight (modern, LinkedIn-style)
/// - Indeterminate Bar: Material Design-style bar that slides back and forth with smooth fadeout
/// - Pulse: expanding concentric rings from the center (radar/sonar style)
/// - Bounce Bars: vertical bars bouncing at different heights and speeds, centred horizontally
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
// A multi-harmonic sinusoidal wave for a richer, more organic look.

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
            // Two harmonics for a richer wave shape
            let x = col as f32 * 0.4 - t;
            let wave = (x.sin() * 0.7 + (x * 2.0 + 1.3).sin() * 0.3) * 0.5 + 0.5;
            let wave = wave.clamp(0.0, 1.0);
            let pixel_height = wave * height as f32;
            let fill = pixel_height - row as f32;

            if fill >= 1.0 {
                let ratio = row as f32 / height as f32;
                let color = lerp_rgb(accent_rgb, bright, ratio);
                spans.push(Span::styled(WAVE_BLOCKS[7], Style::default().fg(color)));
            } else if fill > 0.0 {
                let idx = ((fill * 8.0).floor() as usize).min(7);
                let ratio = row as f32 / height as f32;
                let color = lerp_rgb(dim, accent_rgb, (ratio + fill * 0.3).min(1.0));
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
// horizontally like a scanner. Uses toroidal (wrap-around) distance so the
// highlight seamlessly wraps across edges.

fn render_skeleton_sweep(buf: &mut Buffer, area: Rect, frame: usize, accent_rgb: (u8, u8, u8)) {
    let width = area.width as usize;
    let height = area.height as usize;
    let dim = dimmed_rgb(accent_rgb);

    // Sweep position in [0, width) with wrapping
    let sweep_f = (frame as f32 * 0.06 * width as f32) % width as f32;
    let sweep_center = sweep_f as usize;

    // Width of the highlight zone (about 25 % of total width)
    let highlight_width = (width / 4).max(3);

    let mut lines: Vec<Line> = Vec::with_capacity(height);
    for _row in 0..height {
        let mut spans = Vec::with_capacity(width);
        for col in 0..width {
            // Toroidal distance: shortest distance considering wrap-around
            let dist_from_center = {
                let fwd = if col >= sweep_center {
                    col - sweep_center
                } else {
                    width - sweep_center + col
                };
                let rev = if sweep_center >= col {
                    sweep_center - col
                } else {
                    width - col + sweep_center
                };
                fwd.min(rev)
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
// (about 30 % width) moves left→right, then reverses direction. Both leading
// and trailing edges have a smooth fadeout for a polished look.

const FADEOUT_WIDTH: usize = 3;

fn render_indeterminate_bar(buf: &mut Buffer, area: Rect, frame: usize, accent_rgb: (u8, u8, u8)) {
    let width = area.width as usize;
    let height = area.height as usize;
    let dim = dimmed_rgb(accent_rgb);

    // Bar width = 30 % of total, min 4
    let bar_width = (width * 30 / 100).max(8);
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
                // Within the bar: gradient from centre out with fadeout at edges
                let dist_to_edge = (col - bar_start).min(bar_end - 1 - col);
                let t = dist_to_edge as f32 / (bar_width / 2).max(1) as f32;
                if dist_to_edge < FADEOUT_WIDTH {
                    // Fadeout zone: ramp from accent down to dim
                    let fade = dist_to_edge as f32 / FADEOUT_WIDTH as f32;
                    let color = lerp_rgb(dim, accent_rgb, fade);
                    spans.push(Span::styled("█", Style::default().fg(color)));
                } else {
                    let brightness = 1.0 - t * 0.3;
                    let color = lerp_rgb(dim, accent_rgb, brightness);
                    spans.push(Span::styled("█", Style::default().fg(color)));
                }
            } else {
                spans.push(Span::styled("░", Style::default().fg(Color::Rgb(dim.0, dim.1, dim.2))));
            }
        }
        lines.push(Line::from(spans));
    }
    Paragraph::new(lines).render(area, buf);
}

// ── Pulse animation ──────────────────────────────────────────────────────────
//
// Expanding concentric rings from the center that fade in and out smoothly —
// no abrupt wrap-around jump. Uses WAVE_BLOCKS for a solid filled look instead
// of sparse dots.

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
                spans.push(Span::styled(WAVE_BLOCKS[idx], Style::default().fg(color)));
            } else {
                spans.push(Span::styled(" ", Style::default()));
            }
        }
        lines.push(Line::from(spans));
    }
    Paragraph::new(lines).render(area, buf);
}

// ── Bounce Bars animation ────────────────────────────────────────────────────
//
// Multiple vertical bars bouncing at different heights, each with its own phase
// and speed. The whole group is centred horizontally so it doesn't appear
// left-biased, and bars use `floor` so they can fully rest at 0 px.

const BOUNCE_BAR_COUNT: usize = 6;

/// (phase_offset, frequency, width_in_columns)
const BAR_DATA: [(f32, f32, usize); BOUNCE_BAR_COUNT] = [
    (0.0, 0.08, 1),
    (1.2, 0.10, 1),
    (2.8, 0.06, 2),
    (0.8, 0.12, 1),
    (3.6, 0.07, 2),
    (2.0, 0.09, 1),
];

fn render_bounce_bars(buf: &mut Buffer, area: Rect, frame: usize, accent_rgb: (u8, u8, u8)) {
    let width = area.width as usize;
    let height = area.height as usize;
    if height < 2 {
        return;
    }
    let dim = dimmed_rgb(accent_rgb);
    let bright = brightened_rgb(accent_rgb);

    // --- 1. Figure out how many bars fit and total group width ---
    let mut group_w = 0usize;
    let mut fit_count = 0usize;
    for &(_, _, bw) in &BAR_DATA {
        let next = group_w + bw + if fit_count > 0 { 1 } else { 0 };
        if next > width {
            break;
        }
        group_w = next;
        fit_count += 1;
    }
    if fit_count == 0 {
        return;
    }
    let left_margin = (width - group_w) / 2;

    // --- 2. Pre-compute bar heights for this frame ---
    // Plus build a column→bar lookup for the centred group
    let mut col_to_bar = vec![None; group_w];
    let mut bar_heights = vec![0usize; fit_count];
    {
        let mut cursor = 0usize;
        for i in 0..fit_count {
            let (phase, freq, bw) = BAR_DATA[i];
            // Fill lookup
            for ci in cursor..cursor + bw {
                if ci < group_w {
                    col_to_bar[ci] = Some(i);
                }
            }
            // Height
            let raw = (frame as f32 * freq + phase).sin().abs();
            let b = (raw * 1.2).min(1.0);
            // floor + 1 so at b=0 → height=1 (barely visible) and grows from there.
            // Use height.saturating_sub(1) so max b gives full height.
            let h = (b * height.saturating_sub(1) as f32).floor() as usize + 1;
            bar_heights[i] = h.min(height);
            cursor += bw + 1; // bar + gap
        }
    }

    // --- 3. Render ---
    let mut lines: Vec<Line> = Vec::with_capacity(height);
    for row in 0..height {
        let mut spans = Vec::with_capacity(width);
        for col in 0..width {
            // Columns outside the centred group → empty
            if col < left_margin || col >= left_margin + group_w {
                spans.push(Span::styled(" ", Style::default()));
                continue;
            }
            let gcol = col - left_margin;

            match col_to_bar[gcol] {
                None => {
                    // Gap between bars
                    spans.push(Span::styled(" ", Style::default()));
                }
                Some(bi) => {
                    let bar_h = bar_heights[bi];
                    let fill = (height - row) as f32 - (height - bar_h) as f32;

                    if fill >= 1.0 {
                        let ratio = row as f32 / height as f32;
                        let color = lerp_rgb(accent_rgb, bright, ratio);
                        spans.push(Span::styled("█", Style::default().fg(color)));
                    } else if fill > 0.0 {
                        let idx = ((fill * 8.0).floor() as usize).min(7);
                        let color = lerp_rgb(dim, accent_rgb, fill);
                        spans.push(Span::styled(WAVE_BLOCKS[idx], Style::default().fg(color)));
                    } else {
                        spans.push(Span::styled(" ", Style::default()));
                    }
                }
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
            LoadingAnimation::Pulse => {
                render_pulse(buf, anim_area, self.frame, accent_rgb);
            }
            LoadingAnimation::BounceBars => {
                render_bounce_bars(buf, anim_area, self.frame, accent_rgb);
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
