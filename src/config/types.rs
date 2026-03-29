//! Configuration type definitions

use input_linux::Key;
use serde::Deserialize;

/// Global configuration
#[derive(Debug, Clone)]
pub struct Config {
    pub show_button_outlines: bool,
    pub enable_pixel_shift: bool,
    pub adaptive_brightness: bool,
    pub active_brightness: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_button_outlines: true,
            enable_pixel_shift: true,
            adaptive_brightness: true,
            active_brightness: 100,
        }
    }
}

/// Button configuration from TOML
#[derive(Debug, Deserialize, Clone)]
pub struct ButtonConfig {
    pub text: Option<String>,
    pub action: Key,
    pub font_size: Option<f32>,
    pub font: Option<String>,
    pub style: Option<Vec<String>>,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub background_active: Option<String>,
    pub corner_radius: Option<f32>,
}

/// Internal TOML proxy for deserialization
#[derive(Deserialize)]
pub(crate) struct ConfigProxy {
    pub media_layer_default: Option<bool>,
    pub show_button_outlines: Option<bool>,
    pub enable_pixel_shift: Option<bool>,
    pub default_font: Option<String>,
    pub adaptive_brightness: Option<bool>,
    pub active_brightness: Option<u32>,
    pub primary_layer_keys: Option<Vec<ButtonConfig>>,
    pub media_layer_keys: Option<Vec<ButtonConfig>>,
}
