use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect) {
    let help_text = vec![
        Line::from(vec![Span::styled(
            "Keyboard Shortcuts",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Navigation", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  Tab        Switch focus between panels"),
        Line::from("  ↑/↓        Navigate list"),
        Line::from("  /          Focus search input"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Playback", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  Enter      Play selected song"),
        Line::from("  Space      Play / Pause"),
        Line::from("  s          Stop"),
        Line::from("  n          Next track"),
        Line::from("  p          Previous track"),
        Line::from("  +/-        Volume up / down"),
        Line::from("  v          Toggle audio / video mode"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Queue", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  d          Remove selected from queue"),
        Line::from("  c          Clear queue"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Search", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  /          Search YouTube"),
        Line::from("  a          Add to queue"),
        Line::from(""),
        Line::from(vec![
            Span::styled("General", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from("  Esc        Go back"),
        Line::from("  ?          Toggle help"),
        Line::from("  q          Quit"),
        Line::from("  Ctrl+Q     Quit (from anywhere, even while typing)"),
        Line::from(""),
        Line::from(vec![
            Span::styled("Press any key to close help", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let paragraph = Paragraph::new(Text::from(help_text))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Help")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, area);
}
