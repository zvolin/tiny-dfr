//! Configuration parsing utilities

use std::fs::read_to_string;
use anyhow::{Result, anyhow};
use egui::Color32;

use crate::core::{Button, FontStyle, FunctionLayer};
use super::types::{Config, ButtonConfig, ConfigProxy};

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

/// Create Button from ButtonConfig
pub fn button_from_config(cfg: ButtonConfig, default_font: &Option<String>) -> Button {
    Button {
        text: cfg.text.unwrap_or_else(|| "?".to_string()),
        action: cfg.action,
        active: false,
        font_size: cfg.font_size.unwrap_or(24.0),
        font_family: cfg.font.or_else(|| default_font.clone()).unwrap_or_default(),
        style: parse_font_style(&cfg.style),
        foreground: cfg.foreground
            .and_then(|c| parse_hex_color(&c))
            .unwrap_or(Color32::WHITE),
        background: cfg.background
            .and_then(|c| parse_hex_color(&c))
            .unwrap_or(Color32::from_gray(51)),
        background_active: cfg.background_active
            .and_then(|c| parse_hex_color(&c))
            .unwrap_or(Color32::from_gray(102)),
        corner_radius: cfg.corner_radius.unwrap_or(8.0),
        ..Default::default()
    }
}

/// Load configuration from TOML file
pub fn load_config(path: &str) -> Result<(Config, [FunctionLayer; 2])> {
    let content = read_to_string(path)?;
    let proxy: ConfigProxy = toml::from_str(&content)?;

    let default_font = proxy.default_font.clone();

    let media_buttons: Vec<Button> = proxy
        .media_layer_keys
        .ok_or_else(|| anyhow!("Missing media_layer_keys"))?
        .into_iter()
        .map(|c| button_from_config(c, &default_font))
        .collect();

    let primary_buttons: Vec<Button> = proxy
        .primary_layer_keys
        .ok_or_else(|| anyhow!("Missing primary_layer_keys"))?
        .into_iter()
        .map(|c| button_from_config(c, &default_font))
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
}
