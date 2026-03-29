//! Font loading via fontconfig

use std::collections::HashSet;
use egui::{Context, FontDefinitions, FontData, FontFamily};

use crate::core::{FontStyle, FunctionLayer};

/// Build fontconfig query string with style modifiers
pub fn build_fc_query(name: &str, style: FontStyle) -> String {
    let mut query = name.to_string();
    if style.bold {
        query.push_str(":bold");
    }
    if style.italic {
        query.push_str(":italic");
    }
    query
}

/// Load a font from the system using fontconfig
pub fn load_system_font(name: &str, style: FontStyle) -> Option<Vec<u8>> {
    let query = build_fc_query(name, style);

    let output = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", &query])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8_lossy(&output.stdout);
    let path = path.trim();

    if path.is_empty() {
        return None;
    }

    std::fs::read(path).ok()
}

/// Unique font identifier (name + style)
#[derive(Hash, Eq, PartialEq, Clone)]
struct FontKey {
    name: String,
    style: FontStyle,
}

impl FontKey {
    fn to_family_name(&self) -> String {
        let mut name = self.name.clone();
        if self.style.bold {
            name.push_str("-Bold");
        }
        if self.style.italic {
            name.push_str("-Italic");
        }
        name
    }
}

/// Load custom fonts from layers into egui context
pub fn load_custom_fonts(ctx: &Context, layers: &[FunctionLayer; 2]) {
    // Collect unique (font, style) combinations
    let mut font_keys: HashSet<FontKey> = HashSet::new();

    for layer in layers {
        for button in &layer.buttons {
            if !button.font_family.is_empty() {
                font_keys.insert(FontKey {
                    name: button.font_family.clone(),
                    style: button.style,
                });
            }
        }
    }

    if font_keys.is_empty() {
        return;
    }

    let mut fonts = FontDefinitions::default();
    let mut loaded_names: Vec<String> = Vec::new();

    for key in &font_keys {
        let font_data = if key.name.contains('/') {
            // Path to font file
            std::fs::read(&key.name).ok()
        } else {
            // Font name - use fontconfig
            load_system_font(&key.name, key.style)
        };

        if let Some(data) = font_data {
            let family_name = key.to_family_name();
            let mut fd = FontData::from_owned(data);
            fd.index = 0;
            fonts.font_data.insert(family_name.clone(), fd);
            loaded_names.push(family_name);
        }
    }

    if !loaded_names.is_empty() {
        // Add loaded fonts at front of Proportional family
        let mut proportional = loaded_names;
        proportional.push("Ubuntu-Light".to_owned());
        fonts.families.insert(FontFamily::Proportional, proportional);
    }

    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_fc_query_plain() {
        let style = FontStyle::default();
        assert_eq!(build_fc_query("Nerd Font", style), "Nerd Font");
    }

    #[test]
    fn build_fc_query_bold() {
        let style = FontStyle { bold: true, italic: false };
        assert_eq!(build_fc_query("Nerd Font", style), "Nerd Font:bold");
    }

    #[test]
    fn build_fc_query_italic() {
        let style = FontStyle { bold: false, italic: true };
        assert_eq!(build_fc_query("Nerd Font", style), "Nerd Font:italic");
    }

    #[test]
    fn build_fc_query_bold_italic() {
        let style = FontStyle { bold: true, italic: true };
        assert_eq!(build_fc_query("Nerd Font", style), "Nerd Font:bold:italic");
    }

    #[test]
    fn font_key_to_family_name() {
        let key = FontKey {
            name: "Test".to_string(),
            style: FontStyle { bold: true, italic: false },
        };
        assert_eq!(key.to_family_name(), "Test-Bold");
    }
}
