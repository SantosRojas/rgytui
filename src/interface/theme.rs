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

/// Attenuate an RGB color by a factor (0.0 = black, 1.0 = original).
fn dim_color(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor) as u8,
            (g as f32 * factor) as u8,
            (b as f32 * factor) as u8,
        ),
        other => other,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub accent: Color,
    #[allow(dead_code)]
    pub accent_dim: Color,
    #[allow(dead_code)]
    pub bg: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub highlight_bg: Color,
    pub highlight_fg: Color,
    pub panel_bg: Color,
    pub border_active: Color,
    pub border_inactive: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub separator: Color,
}

impl Theme {
    pub fn from_settings(theme: &str, accent_hex: &str) -> Self {
        let accent = parse_hex(accent_hex);
        let accent_dim = dim_color(accent, 0.45);
        match theme {
            "light" => Self {
                accent,
                accent_dim,
                bg: Color::White,
                text: Color::Black,
                text_secondary: Color::Rgb(100, 100, 100),
                text_muted: Color::Rgb(160, 160, 160),
                highlight_bg: accent,
                highlight_fg: Color::White,
                panel_bg: Color::Rgb(240, 240, 240),
                border_active: accent,
                border_inactive: Color::Rgb(200, 200, 200),
                success: Color::Rgb(40, 180, 80),
                warning: Color::Rgb(220, 180, 0),
                error: Color::Rgb(220, 50, 50),
                separator: Color::Rgb(210, 210, 210),
            },
            _ => Self {
                accent,
                accent_dim,
                bg: Color::Reset,
                text: Color::White,
                text_secondary: Color::Rgb(160, 160, 170),
                text_muted: Color::Rgb(90, 90, 100),
                highlight_bg: accent,
                highlight_fg: Color::Black,
                panel_bg: Color::Rgb(18, 18, 24),
                border_active: accent,
                border_inactive: Color::Rgb(55, 55, 65),
                success: Color::Rgb(80, 220, 120),
                warning: Color::Rgb(255, 200, 50),
                error: Color::Rgb(255, 80, 80),
                separator: Color::Rgb(45, 45, 55),
            },
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Cyan,
            accent_dim: Color::Rgb(0, 80, 80),
            bg: Color::Reset,
            text: Color::White,
            text_secondary: Color::Rgb(160, 160, 170),
            text_muted: Color::Rgb(90, 90, 100),
            highlight_bg: Color::Cyan,
            highlight_fg: Color::Black,
            panel_bg: Color::Rgb(18, 18, 24),
            border_active: Color::Cyan,
            border_inactive: Color::Rgb(55, 55, 65),
            success: Color::Rgb(80, 220, 120),
            warning: Color::Rgb(255, 200, 50),
            error: Color::Rgb(255, 80, 80),
            separator: Color::Rgb(45, 45, 55),
        }
    }
}
