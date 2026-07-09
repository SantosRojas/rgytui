use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Gauge, Widget};

pub struct ProgressBar {
    block: Option<Block<'static>>,
    progress: f32,
    position: f64,
    duration: f64,
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
        }
    }

    pub fn block(mut self, block: Block<'static>) -> Self {
        self.block = Some(block);
        self
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
}

impl Widget for ProgressBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = self.block.unwrap_or_else(|| {
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
        });

        let inner = block.inner(area);
        block.render(area, buf);

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

        let label = format!(" {} / {} ", pos_str, dur_str);

        let gauge = Gauge::default()
            .gauge_style(
                Style::default()
                    .fg(Color::Cyan)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .percent(self.progress as u16)
            .label(label);

        gauge.render(inner, buf);
    }
}
