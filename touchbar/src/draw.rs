use anyhow::{Context, Result};
use fontdue::Font;

use crate::{
    state::{TouchIdState, VisualState},
    theme::Color,
    ui::{Action, Button, Rect, Tone, UiStatus, function_layout, normal_layout, touch_id_layout},
};

pub struct RenderedUi {
    pub pixels: Vec<u8>,
    pub buttons: Vec<Button>,
}

const BAR_BACKGROUND: Color = Color::rgb(0, 0, 0);
// Touch Bar readability invariants: labels stay pure white and theme-derived
// button fills are adjusted as much as necessary to clear this contrast floor.
const TEXT_COLOR: Color = Color::rgb(255, 255, 255);
const MIN_TEXT_CONTRAST: f32 = 6.0;
const MIN_COMPONENT_CONTRAST: f32 = 3.4;
const BUTTON_FONT_SIZE: f32 = 21.0;
const BUTTON_ICON_SIZE: f32 = 22.0;

struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Canvas {
    fn solid(width: u32, height: u32, color: Color) -> Self {
        let mut pixels = vec![0; width as usize * height as usize * 4];
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&[color.r, color.g, color.b, 255]);
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    fn blend_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height || color.a == 0 {
            return;
        }
        let offset = ((y * self.width + x) * 4) as usize;
        let alpha = color.a as u32;
        let inverse = 255 - alpha;
        self.pixels[offset] =
            ((color.r as u32 * alpha + self.pixels[offset] as u32 * inverse) / 255) as u8;
        self.pixels[offset + 1] =
            ((color.g as u32 * alpha + self.pixels[offset + 1] as u32 * inverse) / 255) as u8;
        self.pixels[offset + 2] =
            ((color.b as u32 * alpha + self.pixels[offset + 2] as u32 * inverse) / 255) as u8;
        self.pixels[offset + 3] = 255;
    }

    fn fill_rect(&mut self, rect: Rect, color: Color) {
        let right = rect.x.saturating_add(rect.width).min(self.width);
        let bottom = rect.y.saturating_add(rect.height).min(self.height);
        for y in rect.y..bottom {
            for x in rect.x..right {
                self.blend_pixel(x, y, color);
            }
        }
    }

    fn rounded_rect(&mut self, rect: Rect, radius: u32, fill: Color, border: Color) {
        self.rounded_fill(rect, radius, border);
        if rect.width > 2 && rect.height > 2 {
            self.rounded_fill(
                Rect {
                    x: rect.x + 1,
                    y: rect.y + 1,
                    width: rect.width - 2,
                    height: rect.height - 2,
                },
                radius.saturating_sub(1),
                fill,
            );
        }
    }

    fn rounded_fill(&mut self, rect: Rect, radius: u32, color: Color) {
        let right = rect.x.saturating_add(rect.width).min(self.width);
        let bottom = rect.y.saturating_add(rect.height).min(self.height);
        for y in rect.y..bottom {
            for x in rect.x..right {
                if rounded_contains(rect, radius, x, y) {
                    self.blend_pixel(x, y, color);
                }
            }
        }
    }

    fn centered_text(
        &mut self,
        font: &Font,
        text: &str,
        size: f32,
        center_x: f32,
        center_y: f32,
        color: Color,
    ) {
        let total_width = text_width(font, text, size);
        let mut cursor = center_x - total_width / 2.0;
        for character in text.chars() {
            let (metrics, bitmap) = font.rasterize(character, size);
            let left = cursor + metrics.xmin as f32;
            let top = center_y - metrics.height as f32 / 2.0;
            for row in 0..metrics.height {
                for column in 0..metrics.width {
                    let coverage = bitmap[row * metrics.width + column] as u16;
                    if coverage == 0 {
                        continue;
                    }
                    let x = left.round() as i32 + column as i32;
                    let y = top.round() as i32 + row as i32;
                    if x >= 0 && y >= 0 {
                        self.blend_pixel(
                            x as u32,
                            y as u32,
                            Color {
                                a: ((color.a as u16 * coverage) / 255) as u8,
                                ..color
                            },
                        );
                    }
                }
            }
            cursor += metrics.advance_width;
        }
    }
}

pub fn render(
    visual: &VisualState,
    font: &Font,
    fn_mode: bool,
    touch_id: Option<&TouchIdState>,
    pressed: &[Action],
) -> RenderedUi {
    let width = visual_width(visual);
    let height = visual_height(visual);
    let theme = &visual.snapshot.theme;
    let mut canvas = Canvas::solid(width, height, BAR_BACKGROUND);

    if let Some(touch_id) = touch_id {
        let buttons = touch_id_layout(width, height, touch_id.cancellable());
        let status_tone = if touch_id.state == "success" {
            theme.green
        } else if touch_id.state == "retry" {
            theme.red
        } else {
            theme.selection.mix(theme.accent, 165)
        };
        let (status_fill, status_text) = accessible_colors(status_tone, theme, false, false);
        canvas.fill_rect(
            Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
            status_fill,
        );
        let label_right = if touch_id.cancellable() {
            width.saturating_sub(205)
        } else {
            width
        };
        canvas.centered_text(
            font,
            touch_id.label(),
            18.0,
            label_right as f32 / 2.0,
            height as f32 / 2.0 - 1.0,
            status_text,
        );
        if let Some(progress) = touch_id.progress {
            let progress_width = label_right.saturating_mul(progress as u32) / 100;
            canvas.fill_rect(
                Rect {
                    x: 0,
                    y: height.saturating_sub(3),
                    width: progress_width,
                    height: 3,
                },
                theme.accent,
            );
        }
        draw_buttons(&mut canvas, font, theme, &buttons, pressed);
        return RenderedUi {
            pixels: canvas.pixels,
            buttons,
        };
    }

    let snapshot = &visual.snapshot;
    let buttons = if fn_mode {
        function_layout(width, height)
    } else {
        normal_layout(
            width,
            height,
            UiStatus {
                workspace: &snapshot.workspace,
                playback: &snapshot.playback,
                media_available: snapshot.media_available,
                audio_muted: snapshot.audio_muted,
            },
        )
    };
    draw_buttons(&mut canvas, font, theme, &buttons, pressed);
    RenderedUi {
        pixels: canvas.pixels,
        buttons,
    }
}

pub fn save_png(path: &std::path::Path, width: u32, height: u32, pixels: Vec<u8>) -> Result<()> {
    let image = image::RgbaImage::from_raw(width, height, pixels)
        .context("rendered frame has an invalid byte length")?;
    image
        .save(path)
        .with_context(|| format!("save preview {}", path.display()))
}

fn draw_buttons(
    canvas: &mut Canvas,
    font: &Font,
    theme: &crate::theme::Theme,
    buttons: &[Button],
    pressed: &[Action],
) {
    for button in buttons {
        let is_pressed = button
            .action
            .is_some_and(|action| pressed.contains(&action));
        let base = tone_color(theme, button.tone);
        let (fill, text_color) = accessible_colors(base, theme, button.muted, is_pressed);
        let border = base.mix(theme.light_foreground, 225);
        canvas.rounded_rect(button.rect, 9, fill, border);

        let center_x = button.rect.x as f32 + button.rect.width as f32 / 2.0;
        canvas.centered_text(
            font,
            &button.label,
            if button.icon {
                BUTTON_ICON_SIZE
            } else {
                BUTTON_FONT_SIZE
            },
            center_x,
            button.rect.y as f32 + button.rect.height as f32 / 2.0,
            text_color,
        );
    }
}

fn text_width(font: &Font, text: &str, size: f32) -> f32 {
    text.chars()
        .map(|character| font.metrics(character, size).advance_width)
        .sum()
}

fn accessible_colors(
    base: Color,
    theme: &crate::theme::Theme,
    muted: bool,
    pressed: bool,
) -> (Color, Color) {
    let base = if muted {
        base.mix(theme.muted, 205)
    } else {
        base
    };
    let mut fill = if pressed {
        base.mix(theme.light_foreground, 190)
    } else {
        base.mix(BAR_BACKGROUND, 210)
    };

    fill = fit_for_white_text(fill);
    (fill, TEXT_COLOR)
}

fn fit_for_white_text(original: Color) -> Color {
    let mut best: Option<(u32, Color)> = None;
    for endpoint in [BAR_BACKGROUND, TEXT_COLOR] {
        for original_weight in 0_u16..=255 {
            let candidate = original.mix(endpoint, original_weight as u8);
            if candidate.contrast_ratio(TEXT_COLOR) < MIN_TEXT_CONTRAST
                || candidate.contrast_ratio(BAR_BACKGROUND) < MIN_COMPONENT_CONTRAST
            {
                continue;
            }
            let distance = color_distance_squared(original, candidate);
            if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                best = Some((distance, candidate));
            }
        }
    }
    best.map(|(_, color)| color)
        // This neutral fallback safely clears 6.0:1 white text and 3.4:1
        // against the black bar, including 8-bit color quantization.
        .unwrap_or(Color::rgb(98, 98, 98))
}

fn color_distance_squared(first: Color, second: Color) -> u32 {
    let red = i32::from(first.r) - i32::from(second.r);
    let green = i32::from(first.g) - i32::from(second.g);
    let blue = i32::from(first.b) - i32::from(second.b);
    (red * red + green * green + blue * blue) as u32
}

fn tone_color(theme: &crate::theme::Theme, tone: Tone) -> Color {
    match tone {
        Tone::Accent | Tone::Brightness | Tone::Audio => theme.selection,
        Tone::Info => theme.accent,
        Tone::Media | Tone::Danger => theme.accent,
    }
}

fn rounded_contains(rect: Rect, radius: u32, x: u32, y: u32) -> bool {
    if radius == 0 {
        return true;
    }
    let left = rect.x;
    let right = rect.x.saturating_add(rect.width).saturating_sub(1);
    let top = rect.y;
    let bottom = rect.y.saturating_add(rect.height).saturating_sub(1);
    let center_x = if x < left + radius {
        left + radius
    } else if x > right.saturating_sub(radius) {
        right.saturating_sub(radius)
    } else {
        x
    };
    let center_y = if y < top + radius {
        top + radius
    } else if y > bottom.saturating_sub(radius) {
        bottom.saturating_sub(radius)
    } else {
        y
    };
    let dx = x.abs_diff(center_x);
    let dy = y.abs_diff(center_y);
    dx * dx + dy * dy <= radius * radius
}

fn visual_width(visual: &VisualState) -> u32 {
    visual.width()
}

fn visual_height(visual: &VisualState) -> u32 {
    visual.height()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_text_and_minimum_contrast_are_invariants() {
        let theme = crate::theme::Theme::default();
        for base in [
            tone_color(&theme, Tone::Accent),
            tone_color(&theme, Tone::Info),
            tone_color(&theme, Tone::Media),
            tone_color(&theme, Tone::Brightness),
            tone_color(&theme, Tone::Audio),
            tone_color(&theme, Tone::Danger),
            Color::rgb(0, 0, 0),
            Color::rgb(255, 255, 255),
            Color::rgb(255, 0, 0),
            Color::rgb(0, 255, 0),
            Color::rgb(0, 0, 255),
        ] {
            for muted in [false, true] {
                for pressed in [false, true] {
                    let (fill, text) = accessible_colors(base, &theme, muted, pressed);
                    assert_eq!(text, TEXT_COLOR);
                    assert!(fill.contrast_ratio(BAR_BACKGROUND) >= MIN_COMPONENT_CONTRAST);
                    assert!(fill.contrast_ratio(text) >= MIN_TEXT_CONTRAST);
                }
            }
        }
    }

    #[test]
    fn button_tones_only_use_selection_and_accent_categories() {
        let theme = crate::theme::Theme::default();
        assert_eq!(tone_color(&theme, Tone::Accent), theme.selection);
        assert_eq!(tone_color(&theme, Tone::Audio), theme.selection);
        assert_eq!(tone_color(&theme, Tone::Info), theme.accent);
        assert_eq!(tone_color(&theme, Tone::Media), theme.accent);
        assert_eq!(tone_color(&theme, Tone::Brightness), theme.selection);
        assert_eq!(tone_color(&theme, Tone::Danger), theme.accent);
    }
}
