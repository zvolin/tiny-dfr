//! Button representation and styling

use egui::Color32;
use input_linux::Key;

/// Font style flags
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontStyle {
    pub bold: bool,
    pub italic: bool,
}

/// A button in a function layer
#[derive(Debug, Clone)]
pub struct Button {
    /// Display text (may contain Nerd Font icons)
    pub text: String,
    /// Key to emit when pressed
    pub action: Key,
    /// Whether button is currently pressed
    pub active: bool,
    /// Font size in points
    pub font_size: f32,
    /// Font family name (empty for default)
    pub font_family: String,
    /// Font style (bold, italic)
    pub style: FontStyle,
    /// Text color
    pub foreground: Color32,
    /// Background color when inactive
    pub background: Color32,
    /// Background color when pressed
    pub background_active: Color32,
    /// Corner radius for rounded rectangle
    pub corner_radius: f32,
}

impl Default for Button {
    fn default() -> Self {
        Self {
            text: "?".to_string(),
            action: Key::Unknown,
            active: false,
            font_size: 24.0,
            font_family: String::new(),
            style: FontStyle::default(),
            foreground: Color32::WHITE,
            background: Color32::from_gray(51),
            background_active: Color32::from_gray(102),
            corner_radius: 8.0,
        }
    }
}
