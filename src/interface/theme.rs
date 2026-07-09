use ratatui::style::Color;

fn parse_hex(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6
        && let Ok(r) = u8::from_str_radix(&hex[0..2], 16)
        && let Ok(g) = u8::from_str_radix(&hex[2..4], 16)
        && let Ok(b) = u8::from_str_radix(&hex[4..6], 16)
    {
        Color::Rgb(r, g, b)
    } else {
        Color::Cyan
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub accent: Color,
    pub bg: Color,
    pub text: Color,
    pub highlight_bg: Color,
    pub highlight_fg: Color,
    pub panel_bg: Color,
}

impl Theme {
    pub fn from_settings(theme: &str, accent_hex: &str) -> Self {
        let accent = parse_hex(accent_hex);
        match theme {
            "light" => Self {
                accent,
                bg: Color::White,
                text: Color::Black,
                highlight_bg: accent,
                highlight_fg: Color::White,
                panel_bg: Color::Rgb(240, 240, 240),
            },
            _ => Self {
                accent,
                bg: Color::Reset,
                text: Color::White,
                highlight_bg: accent,
                highlight_fg: Color::Black,
                panel_bg: Color::Rgb(20, 20, 20),
            },
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Cyan,
            bg: Color::Reset,
            text: Color::White,
            highlight_bg: Color::Cyan,
            highlight_fg: Color::Black,
            panel_bg: Color::Rgb(20, 20, 20),
        }
    }
}
