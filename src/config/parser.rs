//! Configuration parsing utilities

use std::fs::read_to_string;
use anyhow::{Result, anyhow};
use egui::Color32;

use crate::core::{Button, Dimension, FontStyle, FunctionLayer};
use super::types::{Config, ButtonConfig, ButtonStyleConfig, ConfigProxy};

/// Parse hex color string to Color32
///
/// Accepts formats: "#RRGGBB" or "RRGGBB"
pub fn parse_hex_color(hex: &str) -> Option<Color32> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}

/// Parse dimension from TOML value
///
/// Accepts:
/// - Integer: pixels (e.g., 42)
/// - Float: pixels (e.g., 42.5)
/// - String ending in %: percentage (e.g., "50%")
pub fn parse_dimension(value: &toml::Value) -> Option<Dimension> {
    match value {
        toml::Value::Integer(n) => Some(Dimension::Pixels(*n as f32)),
        toml::Value::Float(n) => Some(Dimension::Pixels(*n as f32)),
        toml::Value::String(s) if s.ends_with('%') => {
            let num = s.trim_end_matches('%').trim().parse::<f32>().ok()?;
            Some(Dimension::Percent(num / 100.0))
        }
        _ => None,
    }
}

/// Parse style array into FontStyle
pub fn parse_font_style(styles: &Option<Vec<String>>) -> FontStyle {
    let mut result = FontStyle::default();
    if let Some(styles) = styles {
        for s in styles {
            match s.to_lowercase().as_str() {
                "bold" => result.bold = true,
                "italic" => result.italic = true,
                _ => {}
            }
        }
    }
    result
}

/// Apply style config to button
fn apply_style_to_button(
    button: &mut Button,
    style: &ButtonStyleConfig,
    default_font: &Option<String>,
) {
    if let Some(ref v) = style.width {
        if let Some(dim) = parse_dimension(v) {
            button.width = dim;
        }
    }
    if let Some(ref v) = style.height {
        if let Some(dim) = parse_dimension(v) {
            button.height = dim;
        }
    }
    if let Some(m) = style.margin {
        button.margin = m;
    }
    if let Some(r) = style.border_radius {
        button.corner_radius = r;
    }
    if let Some(w) = style.border_width {
        button.border_width = w;
    }
    if let Some(ref c) = style.border_color {
        if let Some(color) = parse_hex_color(c) {
            button.border_color = color;
        }
    }
    if let Some(ref c) = style.background {
        if let Some(color) = parse_hex_color(c) {
            button.background = color;
        }
    }
    if let Some(ref c) = style.background_active {
        if let Some(color) = parse_hex_color(c) {
            button.background_active = color;
        }
    }
    if let Some(ref c) = style.foreground {
        if let Some(color) = parse_hex_color(c) {
            button.foreground = color;
        }
    }
    if let Some(ref f) = style.font.clone().or_else(|| default_font.clone()) {
        button.font_family = f.clone();
    }
    if let Some(size) = style.font_size {
        button.font_size = size;
    }
    if let Some(ref s) = style.font_style {
        button.style = parse_font_style(&Some(s.clone()));
    }
}

/// Create Button from ButtonConfig with style resolution
pub fn button_from_config(
    cfg: ButtonConfig,
    default_font: &Option<String>,
    styles: &std::collections::HashMap<String, ButtonStyleConfig>,
) -> Button {
    // Start with defaults
    let mut button = Button::default();

    // Apply named style if specified
    if let Some(style_name) = &cfg.style {
        if let Some(style) = styles.get(style_name) {
            apply_style_to_button(&mut button, style, default_font);
        } else {
            eprintln!("Warning: unknown style '{}'", style_name);
        }
    }

    // Apply inline overrides
    button.text = cfg.text.unwrap_or_else(|| button.text.clone());
    button.action = cfg.action;

    if let Some(size) = cfg.font_size {
        button.font_size = size;
    }
    if let Some(font) = cfg.font.or_else(|| default_font.clone()) {
        button.font_family = font;
    }
    if let Some(ref styles) = cfg.font_style {
        button.style = parse_font_style(&Some(styles.clone()));
    }
    if let Some(ref c) = cfg.foreground {
        if let Some(color) = parse_hex_color(c) {
            button.foreground = color;
        }
    }
    if let Some(ref c) = cfg.background {
        if let Some(color) = parse_hex_color(c) {
            button.background = color;
        }
    }
    if let Some(ref c) = cfg.background_active {
        if let Some(color) = parse_hex_color(c) {
            button.background_active = color;
        }
    }
    if let Some(r) = cfg.corner_radius {
        button.corner_radius = r;
    }
    if let Some(ref v) = cfg.width {
        if let Some(dim) = parse_dimension(v) {
            button.width = dim;
        }
    }
    if let Some(ref v) = cfg.height {
        if let Some(dim) = parse_dimension(v) {
            button.height = dim;
        }
    }
    if let Some(m) = cfg.margin {
        button.margin = m;
    }
    if let Some(w) = cfg.border_width {
        button.border_width = w;
    }
    if let Some(ref c) = cfg.border_color {
        if let Some(color) = parse_hex_color(c) {
            button.border_color = color;
        }
    }

    button
}

/// Load configuration from TOML file
pub fn load_config(path: &str) -> Result<(Config, [FunctionLayer; 2])> {
    let content = read_to_string(path)?;
    let proxy: ConfigProxy = toml::from_str(&content)?;

    let default_font = proxy.default_font.clone();
    let styles = proxy.styles.unwrap_or_default();

    let media_buttons: Vec<Button> = proxy
        .media_layer_keys
        .ok_or_else(|| anyhow!("Missing media_layer_keys"))?
        .into_iter()
        .map(|c| button_from_config(c, &default_font, &styles))
        .collect();

    let primary_buttons: Vec<Button> = proxy
        .primary_layer_keys
        .ok_or_else(|| anyhow!("Missing primary_layer_keys"))?
        .into_iter()
        .map(|c| button_from_config(c, &default_font, &styles))
        .collect();

    if media_buttons.is_empty() || primary_buttons.is_empty() {
        return Err(anyhow!("Layers must have at least one button"));
    }

    let media_layer = FunctionLayer::new(media_buttons);
    let primary_layer = FunctionLayer::new(primary_buttons);

    let layers = if proxy.media_layer_default.unwrap_or(false) {
        [media_layer, primary_layer]
    } else {
        [primary_layer, media_layer]
    };

    let cfg = Config {
        styles: styles.clone(),
        show_button_outlines: proxy.show_button_outlines.unwrap_or(true),
        enable_pixel_shift: proxy.enable_pixel_shift.unwrap_or(true),
        adaptive_brightness: proxy.adaptive_brightness.unwrap_or(true),
        active_brightness: proxy.active_brightness.unwrap_or(100),
    };

    Ok((cfg, layers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_color_with_hash() {
        assert_eq!(
            parse_hex_color("#FF0000"),
            Some(Color32::from_rgb(255, 0, 0))
        );
    }

    #[test]
    fn parse_hex_color_without_hash() {
        assert_eq!(
            parse_hex_color("00FF00"),
            Some(Color32::from_rgb(0, 255, 0))
        );
    }

    #[test]
    fn parse_hex_color_lowercase() {
        assert_eq!(
            parse_hex_color("#aabbcc"),
            Some(Color32::from_rgb(170, 187, 204))
        );
    }

    #[test]
    fn parse_hex_color_invalid_length() {
        assert_eq!(parse_hex_color("#FFF"), None);
        assert_eq!(parse_hex_color(""), None);
    }

    #[test]
    fn parse_hex_color_invalid_chars() {
        assert_eq!(parse_hex_color("#GGGGGG"), None);
    }

    #[test]
    fn parse_font_style_none() {
        assert_eq!(parse_font_style(&None), FontStyle::default());
    }

    #[test]
    fn parse_font_style_bold() {
        let style = parse_font_style(&Some(vec!["bold".to_string()]));
        assert!(style.bold);
        assert!(!style.italic);
    }

    #[test]
    fn parse_font_style_both() {
        let style = parse_font_style(&Some(vec!["bold".to_string(), "italic".to_string()]));
        assert!(style.bold);
        assert!(style.italic);
    }

    #[test]
    fn parse_font_style_case_insensitive() {
        let style = parse_font_style(&Some(vec!["BOLD".to_string(), "Italic".to_string()]));
        assert!(style.bold);
        assert!(style.italic);
    }

    #[test]
    fn parse_font_style_ignores_unknown() {
        let style = parse_font_style(&Some(vec!["bold".to_string(), "underline".to_string()]));
        assert!(style.bold);
        assert!(!style.italic);
    }

    #[test]
    fn parse_dimension_integer() {
        let val = toml::Value::Integer(100);
        assert_eq!(parse_dimension(&val), Some(Dimension::Pixels(100.0)));
    }

    #[test]
    fn parse_dimension_float() {
        let val = toml::Value::Float(42.5);
        assert_eq!(parse_dimension(&val), Some(Dimension::Pixels(42.5)));
    }

    #[test]
    fn parse_dimension_percent() {
        let val = toml::Value::String("50%".to_string());
        assert_eq!(parse_dimension(&val), Some(Dimension::Percent(0.5)));
    }

    #[test]
    fn parse_dimension_percent_with_space() {
        let val = toml::Value::String("50 %".to_string());
        assert_eq!(parse_dimension(&val), Some(Dimension::Percent(0.5)));
    }

    #[test]
    fn parse_dimension_invalid() {
        let val = toml::Value::String("invalid".to_string());
        assert_eq!(parse_dimension(&val), None);
    }
}
