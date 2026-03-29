//! Function layer containing buttons

use super::button::Button;
use super::Dimension;
use super::state::{DFR_WIDTH, DFR_HEIGHT, BUTTON_SPACING_PX};
use crate::system::PIXEL_SHIFT_WIDTH_PX;

/// A layer of function buttons
#[derive(Debug, Default, Clone)]
pub struct FunctionLayer {
    pub buttons: Vec<Button>,
}

impl FunctionLayer {
    /// Create a new layer with the given buttons
    pub fn new(buttons: Vec<Button>) -> Self {
        Self { buttons }
    }

    /// Check if a touch at (x, y) hits button at index
    pub fn button_hit(&self, btn_idx: usize, x: f64, y: f64) -> bool {
        let num_buttons = self.buttons.len();
        if num_buttons == 0 || btn_idx >= num_buttons {
            return false;
        }

        let button_width = (DFR_WIDTH - (BUTTON_SPACING_PX * (num_buttons - 1) as i32)) as f64
            / num_buttons as f64;
        let left_edge = btn_idx as f64 * (button_width + BUTTON_SPACING_PX as f64);

        if x < left_edge || x > (left_edge + button_width) {
            return false;
        }

        // Touch must be in middle 80% of height (10% margin top and bottom)
        y > 0.1 * DFR_HEIGHT as f64 && y < 0.9 * DFR_HEIGHT as f64
    }

    /// Get button index at touch position
    pub fn button_at(&self, x: f64, _y: f64, pixel_shift_enabled: bool) -> Option<usize> {
        let available_width = DFR_WIDTH as f32;
        let _available_height = DFR_HEIGHT as f32;

        let pixel_shift_width = if pixel_shift_enabled {
            PIXEL_SHIFT_WIDTH_PX as f32
        } else {
            0.0
        };

        let usable_width = available_width - pixel_shift_width;
        let start_x = pixel_shift_width / 2.0;

        // Calculate layout (same logic as renderer)
        let mut fixed_width_total = 0.0f32;
        let mut auto_count = 0usize;
        let mut total_margin = 0.0f32;

        for button in &self.buttons {
            total_margin += button.margin * 2.0;
            match button.width {
                Dimension::Pixels(px) => fixed_width_total += px,
                Dimension::Percent(pct) if (pct - 1.0).abs() < f32::EPSILON => auto_count += 1,
                Dimension::Percent(pct) => fixed_width_total += usable_width * pct,
            }
        }

        let remaining = usable_width - fixed_width_total - total_margin;
        let auto_width = if auto_count > 0 && remaining > 0.0 {
            remaining / auto_count as f32
        } else {
            0.0
        };

        // Find button at x position
        let mut x_offset = start_x;
        let touch_x = x as f32;

        for (i, button) in self.buttons.iter().enumerate() {
            let margin = button.margin;
            x_offset += margin;

            let width = match button.width {
                Dimension::Pixels(px) => px,
                Dimension::Percent(pct) if (pct - 1.0).abs() < f32::EPSILON => auto_width,
                Dimension::Percent(pct) => usable_width * pct,
            };

            if touch_x >= x_offset && touch_x < x_offset + width {
                return Some(i);
            }

            x_offset += width + margin;
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_layer(num_buttons: usize) -> FunctionLayer {
        let buttons = (0..num_buttons)
            .map(|i| Button {
                text: format!("F{}", i + 1),
                ..Default::default()
            })
            .collect();
        FunctionLayer::new(buttons)
    }

    #[test]
    fn button_hit_center_of_first_button() {
        let layer = make_test_layer(4);
        // Button 0 spans roughly 0-489px, center at ~245
        assert!(layer.button_hit(0, 245.0, 30.0));
    }

    #[test]
    fn button_hit_wrong_button_index() {
        let layer = make_test_layer(4);
        // x=245 is in button 0, not button 1
        assert!(!layer.button_hit(1, 245.0, 30.0));
    }

    #[test]
    fn button_hit_too_high() {
        let layer = make_test_layer(4);
        // y=3 is in top 10% margin
        assert!(!layer.button_hit(0, 245.0, 3.0));
    }

    #[test]
    fn button_hit_too_low() {
        let layer = make_test_layer(4);
        // y=57 is in bottom 10% margin
        assert!(!layer.button_hit(0, 245.0, 57.0));
    }

    #[test]
    fn button_hit_empty_layer() {
        let layer = FunctionLayer::default();
        assert!(!layer.button_hit(0, 100.0, 30.0));
    }

    #[test]
    fn button_hit_out_of_bounds_index() {
        let layer = make_test_layer(4);
        assert!(!layer.button_hit(10, 100.0, 30.0));
    }
}
