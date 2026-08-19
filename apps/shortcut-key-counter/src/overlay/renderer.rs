use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fontdue::layout::{CoordinateSystem, Layout, LayoutSettings, TextStyle};
use fontdue::{Font, FontSettings};
use resvg::{tiny_skia, usvg};
use thiserror::Error;

use super::config::{OverlayConfig, OverlayKeyLabels, OverlayPaths, RepeatBadgeShape};

#[derive(Debug, Error)]
pub enum OverlayRenderError {
    #[error("overlay SVG could not be read: {0}")]
    SvgRead(#[source] std::io::Error),
    #[error("overlay SVG could not be parsed: {0}")]
    SvgParse(#[source] usvg::Error),
    #[error("overlay font could not be loaded from Windows Fonts")]
    FontUnavailable,
    #[error("overlay pixmap allocation failed")]
    PixmapAllocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub fn parse_hex_with_opacity(color: &str, opacity: f32) -> Option<Rgba> {
    let hex = color.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    let a = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(Rgba { r, g, b, a })
}

pub fn premultiply(color: Rgba) -> Rgba {
    let alpha = color.a as u16;
    Rgba {
        r: ((color.r as u16 * alpha + 127) / 255) as u8,
        g: ((color.g as u16 * alpha + 127) / 255) as u8,
        b: ((color.b as u16 * alpha + 127) / 255) as u8,
        a: color.a,
    }
}

pub struct OverlayRenderer {
    config: OverlayConfig,
    labels: OverlayKeyLabels,
    templates: SvgTemplates,
    font: Font,
    text_color: [u8; 4],
    separator_color: [u8; 4],
    cache: HashMap<String, Arc<tiny_skia::Pixmap>>,
    cache_order: VecDeque<String>,
}

impl OverlayRenderer {
    pub fn load(
        paths: &OverlayPaths,
        config: OverlayConfig,
        labels: OverlayKeyLabels,
    ) -> Result<Self, OverlayRenderError> {
        let templates = SvgTemplates::load(&paths.design_directory)?;
        let font = load_windows_font()?;
        let text_color = parse_color(&config.key.text_color, [32, 37, 43, 255]);
        let separator_color = parse_color(&config.key.separator_color, [244, 247, 250, 255]);
        Ok(Self {
            config: config.sanitize(),
            labels,
            templates,
            font,
            text_color,
            separator_color,
            cache: HashMap::new(),
            cache_order: VecDeque::new(),
        })
    }

    pub fn config(&self) -> &OverlayConfig {
        &self.config
    }

    pub fn background_rgba(&self) -> Rgba {
        parse_hex_with_opacity(
            &self.config.background.color,
            self.config.background.opacity,
        )
        .unwrap_or(Rgba {
            r: 32,
            g: 32,
            b: 32,
            a: 204,
        })
    }

    pub fn replace_assets(
        &mut self,
        paths: &OverlayPaths,
        config: OverlayConfig,
        labels: OverlayKeyLabels,
    ) -> Result<(), OverlayRenderError> {
        let templates = SvgTemplates::load(&paths.design_directory)?;
        self.text_color = parse_color(&config.key.text_color, [32, 37, 43, 255]);
        self.separator_color = parse_color(&config.key.separator_color, [244, 247, 250, 255]);
        self.templates = templates;
        self.config = config.sanitize();
        self.labels = labels;
        self.cache.clear();
        self.cache_order.clear();
        Ok(())
    }

    pub fn render_shortcut(
        &mut self,
        shortcut: &str,
        dpi_scale: f32,
    ) -> Result<Arc<tiny_skia::Pixmap>, OverlayRenderError> {
        let tokens = shortcut.split('+').map(str::to_owned).collect::<Vec<_>>();
        self.render_row(&tokens, true, dpi_scale)
    }

    pub fn render_row(
        &mut self,
        tokens: &[String],
        with_separator: bool,
        dpi_scale: f32,
    ) -> Result<Arc<tiny_skia::Pixmap>, OverlayRenderError> {
        let scale_key = (dpi_scale * 100.0).round() as u32;
        let cache_key = format!(
            "{scale_key}:{}:{}",
            if with_separator {
                "shortcut"
            } else {
                "sequence"
            },
            tokens.join("\u{1f}")
        );
        if let Some(pixmap) = self.cache.get(&cache_key).cloned() {
            self.touch_cache(&cache_key);
            return Ok(pixmap);
        }

        let key_height = scaled(self.config.key.height, dpi_scale).max(1);
        let padding = scaled(self.config.key.padding_x, dpi_scale);
        let min_width = scaled(self.config.key.min_width, dpi_scale);
        let item_gap = scaled(self.config.key.separator_gap, dpi_scale);
        let font_size = self.config.key.font_size * dpi_scale;
        let mut parts = Vec::with_capacity(tokens.len());

        for token in tokens {
            let display = bounded_label(self.labels.display(token));
            let text_width = measure_text_width(&self.font, &display, font_size);
            let width = min_width.max(text_width.saturating_add(padding.saturating_mul(2)));
            let template = self.templates.for_token(token);
            let mut key = render_svg(template, width, key_height)?;
            draw_centered_text(&mut key, &self.font, &display, font_size, self.text_color);
            parts.push(key);
        }

        let separator = if with_separator && parts.len() > 1 {
            Some(render_text_pixmap(
                &self.font,
                "+",
                font_size * 0.92,
                self.separator_color,
            )?)
        } else {
            None
        };
        let between_width = separator
            .as_ref()
            .map(|pixmap| pixmap.width().saturating_add(item_gap.saturating_mul(2)))
            .unwrap_or(item_gap);
        let width = parts
            .iter()
            .map(|part| part.width())
            .sum::<u32>()
            .saturating_add(between_width.saturating_mul(parts.len().saturating_sub(1) as u32));
        let height = key_height.max(separator.as_ref().map_or(0, tiny_skia::Pixmap::height));
        // Do not clip here. The overlay manager wraps complete keycaps to the
        // monitor Work Area before creating the native layered window.
        let canvas_width = width.max(1);
        let mut canvas = tiny_skia::Pixmap::new(canvas_width, height)
            .ok_or(OverlayRenderError::PixmapAllocation)?;
        let mut x = 0i32;

        for (index, part) in parts.iter().enumerate() {
            if x >= canvas_width as i32 {
                break;
            }
            let y = ((height.saturating_sub(part.height())) / 2) as i32;
            canvas.draw_pixmap(
                x,
                y,
                part.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                tiny_skia::Transform::identity(),
                None,
            );
            x += part.width() as i32;
            if index + 1 < parts.len() {
                if let Some(separator) = separator.as_ref() {
                    x += item_gap as i32;
                    let sep_y = ((height.saturating_sub(separator.height())) / 2) as i32;
                    canvas.draw_pixmap(
                        x,
                        sep_y,
                        separator.as_ref(),
                        &tiny_skia::PixmapPaint::default(),
                        tiny_skia::Transform::identity(),
                        None,
                    );
                    x += separator.width() as i32 + item_gap as i32;
                } else {
                    x += item_gap as i32;
                }
            }
        }

        let rendered = Arc::new(canvas);
        self.cache.insert(cache_key.clone(), Arc::clone(&rendered));
        self.cache_order.push_back(cache_key);
        while self.cache_order.len() > self.config.cache_items {
            if let Some(oldest) = self.cache_order.pop_front() {
                self.cache.remove(&oldest);
            }
        }
        Ok(rendered)
    }

    pub fn render_token(
        &mut self,
        token: &str,
        dpi_scale: f32,
    ) -> Result<Arc<tiny_skia::Pixmap>, OverlayRenderError> {
        self.render_row(&[token.to_owned()], false, dpi_scale)
    }

    /// Renders the repeat counter (for example `×12`) from the same schema
    /// used by the settings preview. This is intentionally not cached because
    /// the count changes rapidly while a repeat run is active.
    pub fn render_repeat_badge(
        &self,
        count: u32,
        dpi_scale: f32,
    ) -> Result<Arc<tiny_skia::Pixmap>, OverlayRenderError> {
        let theme = &self.config.repeat_counter;
        let text = format!("×{count}");
        let font_size = theme.font_size * dpi_scale;
        let (text_width, text_height) = measure_text(&self.font, &text, font_size);
        let pad_x = scaled(8, dpi_scale);
        let pad_y = scaled(5, dpi_scale);
        let shadow = scaled(3, dpi_scale);
        let body_width = text_width
            .saturating_add(pad_x.saturating_mul(2))
            .max(scaled(30, dpi_scale));
        let body_height = text_height
            .saturating_add(pad_y.saturating_mul(2))
            .max(scaled(25, dpi_scale));
        let mut badge = tiny_skia::Pixmap::new(body_width, body_height.saturating_add(shadow))
            .ok_or(OverlayRenderError::PixmapAllocation)?;
        let accent = parse_color(&theme.accent_color, [185, 39, 91, 255]);
        let background = parse_color(&theme.background_color, [255, 79, 139, 255]);
        let text_color = parse_color(&theme.text_color, [255, 255, 255, 255]);
        let radius = match theme.shape {
            RepeatBadgeShape::Pill => body_height as f32 / 2.0,
            RepeatBadgeShape::Rounded => 7.0 * dpi_scale,
            RepeatBadgeShape::Square => 2.0 * dpi_scale,
        };
        fill_rounded_rect(
            &mut badge,
            0.0,
            shadow as f32,
            body_width as f32,
            body_height as f32,
            radius,
            accent,
        );
        fill_rounded_rect(
            &mut badge,
            0.0,
            0.0,
            body_width as f32,
            body_height as f32,
            radius,
            background,
        );
        draw_text_at(
            &mut badge,
            &self.font,
            &text,
            font_size,
            text_color,
            0,
            0,
            body_width,
            body_height,
        );
        let rotated = rotate_pixmap(&badge, theme.rotation_deg)?;
        Ok(Arc::new(rotated))
    }

    fn touch_cache(&mut self, key: &str) {
        if let Some(position) = self.cache_order.iter().position(|entry| entry == key) {
            self.cache_order.remove(position);
        }
        self.cache_order.push_back(key.to_owned());
    }
}

struct SvgTemplates {
    key: usvg::Tree,
    modifier: usvg::Tree,
    mouse: usvg::Tree,
}

impl SvgTemplates {
    fn load(directory: &Path) -> Result<Self, OverlayRenderError> {
        Ok(Self {
            key: load_svg(&directory.join("key.svg"))?,
            modifier: load_svg(&directory.join("modifier.svg"))?,
            mouse: load_svg(&directory.join("mouse.svg"))?,
        })
    }

    fn for_token(&self, token: &str) -> &usvg::Tree {
        if matches!(token, "Ctrl" | "Alt" | "Win" | "Shift") {
            &self.modifier
        } else if matches!(
            token,
            "MouseLeft"
                | "MouseRight"
                | "MouseMiddle"
                | "MouseX1"
                | "MouseX2"
                | "WheelUp"
                | "WheelDown"
                | "WheelLeft"
                | "WheelRight"
        ) {
            &self.mouse
        } else {
            &self.key
        }
    }
}

fn load_svg(path: &Path) -> Result<usvg::Tree, OverlayRenderError> {
    let data = fs::read(path).map_err(OverlayRenderError::SvgRead)?;
    usvg::Tree::from_data(&data, &usvg::Options::default()).map_err(OverlayRenderError::SvgParse)
}

fn render_svg(
    tree: &usvg::Tree,
    width: u32,
    height: u32,
) -> Result<tiny_skia::Pixmap, OverlayRenderError> {
    let mut pixmap = tiny_skia::Pixmap::new(width.max(1), height.max(1))
        .ok_or(OverlayRenderError::PixmapAllocation)?;
    let source = tree.size();
    let transform = tiny_skia::Transform::from_scale(
        width as f32 / source.width(),
        height as f32 / source.height(),
    );
    resvg::render(tree, transform, &mut pixmap.as_mut());
    Ok(pixmap)
}

fn render_text_pixmap(
    font: &Font,
    text: &str,
    font_size: f32,
    color: [u8; 4],
) -> Result<tiny_skia::Pixmap, OverlayRenderError> {
    let (width, height) = measure_text(font, text, font_size);
    let mut pixmap = tiny_skia::Pixmap::new(width.max(1), height.max(1))
        .ok_or(OverlayRenderError::PixmapAllocation)?;
    draw_text_at(
        &mut pixmap,
        font,
        text,
        font_size,
        color,
        0,
        0,
        width,
        height,
    );
    Ok(pixmap)
}

fn draw_centered_text(
    pixmap: &mut tiny_skia::Pixmap,
    font: &Font,
    text: &str,
    font_size: f32,
    color: [u8; 4],
) {
    let (text_width, text_height) = measure_text(font, text, font_size);
    let x = pixmap.width().saturating_sub(text_width) / 2;
    let y = pixmap.height().saturating_sub(text_height) / 2;
    draw_text_at(
        pixmap,
        font,
        text,
        font_size,
        color,
        x as i32,
        y as i32,
        text_width,
        text_height,
    );
}

fn measure_text_width(font: &Font, text: &str, font_size: f32) -> u32 {
    measure_text(font, text, font_size).0
}

fn measure_text(font: &Font, text: &str, font_size: f32) -> (u32, u32) {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings::default());
    layout.append(
        std::slice::from_ref(font),
        &TextStyle::new(text, font_size, 0),
    );
    let mut max_x = 0.0f32;
    let mut max_y = 0.0f32;
    for glyph in layout.glyphs() {
        max_x = max_x.max(glyph.x + glyph.width as f32);
        max_y = max_y.max(glyph.y + glyph.height as f32);
    }
    (
        max_x.ceil().max(1.0) as u32,
        max_y.ceil().max(font_size) as u32,
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_text_at(
    pixmap: &mut tiny_skia::Pixmap,
    font: &Font,
    text: &str,
    font_size: f32,
    color: [u8; 4],
    offset_x: i32,
    offset_y: i32,
    region_width: u32,
    region_height: u32,
) {
    let mut layout = Layout::new(CoordinateSystem::PositiveYDown);
    layout.reset(&LayoutSettings::default());
    layout.append(
        std::slice::from_ref(font),
        &TextStyle::new(text, font_size, 0),
    );
    let (measured_width, measured_height) = measure_text(font, text, font_size);
    let center_x = region_width.saturating_sub(measured_width) as i32 / 2;
    let center_y = region_height.saturating_sub(measured_height) as i32 / 2;
    let pixmap_width = pixmap.width() as i32;
    let pixmap_height = pixmap.height() as i32;
    let data = pixmap.data_mut();

    for glyph in layout.glyphs() {
        let (_, bitmap) = font.rasterize_config(glyph.key);
        let glyph_x = offset_x + center_x + glyph.x.round() as i32;
        let glyph_y = offset_y + center_y + glyph.y.round() as i32;
        for row in 0..glyph.height {
            for column in 0..glyph.width {
                let x = glyph_x + column as i32;
                let y = glyph_y + row as i32;
                if x < 0 || y < 0 || x >= pixmap_width || y >= pixmap_height {
                    continue;
                }
                let coverage = bitmap[row * glyph.width + column] as u32;
                if coverage == 0 {
                    continue;
                }
                let color_alpha = color[3] as u32;
                let alpha = (coverage * color_alpha + 127) / 255;
                let inverse = 255 - alpha;
                let index = (y as usize * pixmap_width as usize + x as usize) * 4;
                let src_r = color[0] as u32 * alpha / 255;
                let src_g = color[1] as u32 * alpha / 255;
                let src_b = color[2] as u32 * alpha / 255;
                let dst_r = data[index] as u32;
                let dst_g = data[index + 1] as u32;
                let dst_b = data[index + 2] as u32;
                let dst_a = data[index + 3] as u32;
                data[index] = (src_r + dst_r * inverse / 255).min(255) as u8;
                data[index + 1] = (src_g + dst_g * inverse / 255).min(255) as u8;
                data[index + 2] = (src_b + dst_b * inverse / 255).min(255) as u8;
                data[index + 3] = (alpha + dst_a * inverse / 255).min(255) as u8;
            }
        }
    }
}

fn fill_rounded_rect(
    pixmap: &mut tiny_skia::Pixmap,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    color: [u8; 4],
) {
    let Some(path) = rounded_rect_path_at(x, y, width, height, radius) else {
        return;
    };
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(tiny_skia::Color::from_rgba8(
        color[0], color[1], color[2], color[3],
    ));
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );
}

fn rounded_rect_path_at(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
) -> Option<tiny_skia::Path> {
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    let k = 0.552_284_8;
    let left = x;
    let top = y;
    let right = x + width;
    let bottom = y + height;
    let mut builder = tiny_skia::PathBuilder::new();
    builder.move_to(left + radius, top);
    builder.line_to(right - radius, top);
    builder.cubic_to(
        right - radius + radius * k,
        top,
        right,
        top + radius - radius * k,
        right,
        top + radius,
    );
    builder.line_to(right, bottom - radius);
    builder.cubic_to(
        right,
        bottom - radius + radius * k,
        right - radius + radius * k,
        bottom,
        right - radius,
        bottom,
    );
    builder.line_to(left + radius, bottom);
    builder.cubic_to(
        left + radius - radius * k,
        bottom,
        left,
        bottom - radius + radius * k,
        left,
        bottom - radius,
    );
    builder.line_to(left, top + radius);
    builder.cubic_to(
        left,
        top + radius - radius * k,
        left + radius - radius * k,
        top,
        left + radius,
        top,
    );
    builder.close();
    builder.finish()
}

fn rotate_pixmap(
    source: &tiny_skia::Pixmap,
    degrees: f32,
) -> Result<tiny_skia::Pixmap, OverlayRenderError> {
    if degrees.abs() < 0.01 {
        let mut copy = tiny_skia::Pixmap::new(source.width(), source.height())
            .ok_or(OverlayRenderError::PixmapAllocation)?;
        copy.data_mut().copy_from_slice(source.data());
        return Ok(copy);
    }
    let radians = degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let sw = source.width() as f32;
    let sh = source.height() as f32;
    let dw = (sw * cos.abs() + sh * sin.abs()).ceil().max(1.0) as u32;
    let dh = (sw * sin.abs() + sh * cos.abs()).ceil().max(1.0) as u32;
    let mut target = tiny_skia::Pixmap::new(dw, dh).ok_or(OverlayRenderError::PixmapAllocation)?;
    let src = source.data();
    let dst = target.data_mut();
    let scx = (sw - 1.0) / 2.0;
    let scy = (sh - 1.0) / 2.0;
    let dcx = (dw as f32 - 1.0) / 2.0;
    let dcy = (dh as f32 - 1.0) / 2.0;
    for y in 0..dh {
        for x in 0..dw {
            let dx = x as f32 - dcx;
            let dy = y as f32 - dcy;
            let sx = cos * dx + sin * dy + scx;
            let sy = -sin * dx + cos * dy + scy;
            let sx = sx.round() as i32;
            let sy = sy.round() as i32;
            if sx < 0 || sy < 0 || sx >= source.width() as i32 || sy >= source.height() as i32 {
                continue;
            }
            let source_index = (sy as usize * source.width() as usize + sx as usize) * 4;
            let target_index = (y as usize * dw as usize + x as usize) * 4;
            dst[target_index..target_index + 4]
                .copy_from_slice(&src[source_index..source_index + 4]);
        }
    }
    Ok(target)
}

fn load_windows_font() -> Result<Font, OverlayRenderError> {
    let windows = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let fonts = windows.join("Fonts");
    for name in ["YuGothM.ttc", "meiryo.ttc", "segoeui.ttf", "arial.ttf"] {
        let path = fonts.join(name);
        let Ok(data) = fs::read(path) else { continue };
        let settings = FontSettings {
            collection_index: 0,
            ..FontSettings::default()
        };
        if let Ok(font) = Font::from_bytes(data, settings) {
            return Ok(font);
        }
    }
    Err(OverlayRenderError::FontUnavailable)
}

fn parse_color(value: &str, fallback: [u8; 4]) -> [u8; 4] {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 && hex.len() != 8 {
        return fallback;
    }
    let parsed = u32::from_str_radix(hex, 16).ok();
    match (hex.len(), parsed) {
        (6, Some(value)) => [
            ((value >> 16) & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
            255,
        ],
        (8, Some(value)) => [
            ((value >> 24) & 0xff) as u8,
            ((value >> 16) & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
        ],
        _ => fallback,
    }
}

fn scaled(value: u32, scale: f32) -> u32 {
    (value as f32 * scale).round().max(1.0) as u32
}

fn bounded_label(value: &str) -> String {
    const MAX_CHARS: usize = 24;
    if value.chars().count() <= MAX_CHARS {
        value.to_owned()
    } else {
        value
            .chars()
            .take(MAX_CHARS - 1)
            .chain(std::iter::once('…'))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_accept_rgb_and_rgba() {
        assert_eq!(parse_color("#112233", [0; 4]), [0x11, 0x22, 0x33, 0xff]);
        assert_eq!(parse_color("AABBCC80", [0; 4]), [0xaa, 0xbb, 0xcc, 0x80]);
    }

    #[test]
    fn background_color_uses_configured_opacity() {
        let color = parse_hex_with_opacity("#202020", 0.8).expect("color");
        assert_eq!((color.r, color.g, color.b), (32, 32, 32));
        assert!((color.a as i32 - 204).abs() <= 1);
    }

    #[test]
    fn long_user_label_is_bounded() {
        let label = bounded_label("abcdefghijklmnopqrstuvwxyz123456789");
        assert_eq!(label.chars().count(), 24);
        assert!(label.ends_with('…'));
    }
}
