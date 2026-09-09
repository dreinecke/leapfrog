use std::{fs, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn mix(self, other: Self, self_weight: u8) -> Self {
        let own = self_weight as u32;
        let theirs = 255 - own;
        Self::rgb(
            ((self.r as u32 * own + other.r as u32 * theirs) / 255) as u8,
            ((self.g as u32 * own + other.g as u32 * theirs) / 255) as u8,
            ((self.b as u32 * own + other.b as u32 * theirs) / 255) as u8,
        )
    }

    pub fn relative_luminance(self) -> f32 {
        let linear = |channel: u8| {
            let value = channel as f32 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    pub fn contrast_ratio(self, other: Self) -> f32 {
        let first = self.relative_luminance();
        let second = other.relative_luminance();
        (first.max(second) + 0.05) / (first.min(second) + 0.05)
    }

    fn from_hex(value: &str) -> Option<Self> {
        let hex = value.trim().trim_matches('"').trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        Some(Self::rgb(
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Theme {
    pub name: String,
    pub accent: Color,
    pub selection: Color,
    pub muted: Color,
    pub background: Color,
    pub dark_background: Color,
    pub darker_background: Color,
    pub lighter_background: Color,
    pub foreground: Color,
    pub dark_foreground: Color,
    pub light_foreground: Color,
    pub bright_foreground: Color,
    pub red: Color,
    pub yellow: Color,
    pub orange: Color,
    pub green: Color,
    pub cyan: Color,
    pub blue: Color,
    pub magenta: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "Omarchy".into(),
            accent: Color::rgb(0x50, 0x94, 0x75),
            selection: Color::rgb(0x32, 0x47, 0x3b),
            muted: Color::rgb(0x53, 0x68, 0x5b),
            background: Color::rgb(0x11, 0x1c, 0x18),
            dark_background: Color::rgb(0x0c, 0x15, 0x12),
            darker_background: Color::rgb(0x09, 0x0f, 0x0d),
            lighter_background: Color::rgb(0x23, 0x37, 0x2b),
            foreground: Color::rgb(0xc1, 0xc4, 0x97),
            dark_foreground: Color::rgb(0x81, 0xb8, 0xa8),
            light_foreground: Color::rgb(0xd6, 0xd5, 0xbc),
            bright_foreground: Color::rgb(0xf7, 0xe8, 0xb2),
            red: Color::rgb(0xff, 0x53, 0x45),
            yellow: Color::rgb(0x45, 0x94, 0x51),
            orange: Color::rgb(0xa2, 0x73, 0x4b),
            green: Color::rgb(0x54, 0x9e, 0x6a),
            cyan: Color::rgb(0x2d, 0xd5, 0xb7),
            blue: Color::rgb(0x50, 0x94, 0x75),
            magenta: Color::rgb(0xd2, 0x68, 0x9c),
        }
    }
}

impl Theme {
    pub fn load(current_dir: &Path) -> Self {
        let mut theme = Self::default();
        if let Ok(name) = fs::read_to_string(current_dir.join("theme.name")) {
            theme.name = pretty_name(name.trim());
        } else if let Ok(target) = fs::canonicalize(current_dir.join("theme"))
            && let Some(name) = target.file_name().and_then(|value| value.to_str())
        {
            theme.name = pretty_name(name);
        }

        let Ok(colors) = fs::read_to_string(current_dir.join("theme/colors.toml")) else {
            return theme;
        };
        for line in colors.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let Some(color) = Color::from_hex(value) else {
                continue;
            };
            match key.trim() {
                "accent" => theme.accent = color,
                "selection" => theme.selection = color,
                "muted" => theme.muted = color,
                "background" => theme.background = color,
                "dark_background" => theme.dark_background = color,
                "darker_background" => theme.darker_background = color,
                "lighter_background" => theme.lighter_background = color,
                "foreground" => theme.foreground = color,
                "dark_foreground" => theme.dark_foreground = color,
                "light_foreground" => theme.light_foreground = color,
                "bright_foreground" => theme.bright_foreground = color,
                "red" => theme.red = color,
                "yellow" => theme.yellow = color,
                "orange" => theme.orange = color,
                "green" => theme.green = color,
                "cyan" => theme.cyan = color,
                "blue" => theme.blue = color,
                "magenta" => theme.magenta = color,
                _ => {}
            }
        }
        theme
    }
}

fn pretty_name(value: &str) -> String {
    value
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_theme_slug() {
        assert_eq!(pretty_name("osaka-jade"), "Osaka Jade");
    }

    #[test]
    fn computes_standard_black_white_contrast() {
        let black = Color::rgb(0, 0, 0);
        let white = Color::rgb(255, 255, 255);
        assert!((black.contrast_ratio(white) - 21.0).abs() < 0.01);
    }
}
