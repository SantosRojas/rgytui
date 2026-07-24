use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

pub struct ProgressBar {
    block: Option<Block<'static>>,
    progress: f32,
    position: f64,
    duration: f64,
    accent: Color,
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressBar {
    pub fn new() -> Self {
        Self {
            block: None,
            progress: 0.0,
            position: 0.0,
            duration: 0.0,
            accent: Color::Cyan,
        }
    }

    pub fn progress(mut self, progress: f32) -> Self {
        self.progress = progress.clamp(0.0, 100.0);
        self
    }

    pub fn position(mut self, pos: f64) -> Self {
        self.position = pos;
        self
    }

    pub fn duration(mut self, dur: f64) -> Self {
        self.duration = dur;
        self
    }

    pub fn accent(mut self, color: Color) -> Self {
        self.accent = color;
        self
    }
}

impl Widget for ProgressBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = self.block.unwrap_or_else(|| {
            Block::default()
                .borders(Borders::NONE)
        });

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width < 10 {
            return;
        }

        let pos_str = format!(
            "{:02}:{:02}",
            self.position as u64 / 60,
            self.position as u64 % 60
        );
        let dur_str = format!(
            "{:02}:{:02}",
            self.duration as u64 / 60,
            self.duration as u64 % 60
        );

        // Layout: "  00:00 ━━━━●─────── 03:45  "
        let time_left_width = pos_str.len() + 2;  // "  00:00 "
        let time_right_width = dur_str.len() + 2; // " 03:45  "
        let bar_width = (inner.width as usize).saturating_sub(time_left_width + time_right_width);

        if bar_width < 3 {
            // Fallback: just show time
            let label = format!("{} / {}", pos_str, dur_str);
            let line = Line::from(Span::styled(label, Style::default().fg(self.accent)));
            Paragraph::new(line).render(inner, buf);
            return;
        }

        let filled = ((self.progress / 100.0) * bar_width as f32) as usize;
        let empty = bar_width.saturating_sub(filled).saturating_sub(1); // -1 for the knob

        let filled_str = "━".repeat(filled);
        let empty_str = "─".repeat(empty);

        let line = Line::from(vec![
            Span::styled(
                format!("  {} ", pos_str),
                Style::default().fg(self.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(&filled_str, Style::default().fg(self.accent)),
            Span::styled("●", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(&empty_str, Style::default().fg(Color::Rgb(55, 55, 65))),
            Span::styled(
                format!(" {}  ", dur_str),
                Style::default().fg(Color::Rgb(120, 120, 130)),
            ),
        ]);

        Paragraph::new(line).render(inner, buf);
    }
}
