//! egui rendering and DRM output

use anyhow::Result;
use egui::{
    Context, RawInput, Rect, Pos2, Vec2, CentralPanel, Frame,
    Color32, FontId, Align2, FontFamily,
};
use drm::control::ClipRect;

use crate::core::{
    Button, Dimension, FunctionLayer, DFR_WIDTH, DFR_HEIGHT, DFR_STRIDE,
};
use crate::config::Config;
use crate::system::PIXEL_SHIFT_WIDTH_PX;
use super::drm::DrmBackend;
use super::painter::TinySkiaPainter;

/// Run warm-up frames to build egui font atlas
///
/// Must process texture deltas or the font atlas gets discarded.
pub fn warm_up_fonts(
    ctx: &Context,
    painter: &mut TinySkiaPainter,
    layers: &[FunctionLayer; 2],
    cfg: &Config,
) {
    for _ in 0..3 {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(DFR_WIDTH as f32, DFR_HEIGHT as f32),
            )),
            ..Default::default()
        };

        let output = ctx.run(input, |ctx| {
            render_function_layer(ctx, &layers[0], cfg, (0.0, 0.0));
            render_function_layer(ctx, &layers[1], cfg, (0.0, 0.0));
        });

        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        painter.paint(&primitives, &output.textures_delta);
    }
}

/// Calculate button layout positions
/// Returns: Vec<(x_offset, width, height, margin)>
fn calculate_button_layout(
    buttons: &[Button],
    available_width: f32,
    available_height: f32,
) -> Vec<(f32, f32, f32, f32)> {
    // First pass: calculate fixed widths and count auto buttons
    let mut fixed_width_total = 0.0f32;
    let mut auto_count = 0usize;
    let mut total_margin = 0.0f32;

    for button in buttons {
        total_margin += button.margin * 2.0;
        match button.width {
            Dimension::Pixels(px) => fixed_width_total += px,
            Dimension::Percent(pct) if (pct - 1.0).abs() < f32::EPSILON => auto_count += 1,
            Dimension::Percent(pct) => fixed_width_total += available_width * pct,
        }
    }

    // Calculate auto width
    let remaining = available_width - fixed_width_total - total_margin;
    let auto_width = if auto_count > 0 && remaining > 0.0 {
        remaining / auto_count as f32
    } else {
        0.0
    };

    // Second pass: build layout
    let mut result = Vec::with_capacity(buttons.len());
    let mut x_offset = 0.0f32;

    for button in buttons {
        let margin = button.margin;
        x_offset += margin; // Left margin

        let width = match button.width {
            Dimension::Pixels(px) => px,
            Dimension::Percent(pct) if (pct - 1.0).abs() < f32::EPSILON => auto_width,
            Dimension::Percent(pct) => available_width * pct,
        };

        let height = match button.height {
            Dimension::Pixels(px) => px,
            Dimension::Percent(pct) => available_height * pct,
        };

        result.push((x_offset, width, height, margin));
        x_offset += width + margin; // Width + right margin
    }

    result
}

/// Render a function layer using egui
pub fn render_function_layer(
    ctx: &Context,
    layer: &FunctionLayer,
    cfg: &Config,
    pixel_shift: (f64, f64),
) {
    CentralPanel::default()
        .frame(Frame::none().fill(Color32::BLACK))
        .show(ctx, |ui| {
            let available_width = ui.available_width();
            let available_height = ui.available_height();
            let buttons = &layer.buttons;

            if buttons.is_empty() {
                return;
            }

            let pixel_shift_width = if cfg.enable_pixel_shift {
                PIXEL_SHIFT_WIDTH_PX as f32
            } else {
                0.0
            };

            // Calculate button positions with per-button dimensions
            let usable_width = available_width - pixel_shift_width;
            let button_rects = calculate_button_layout(buttons, usable_width, available_height);

            // Check for overflow
            let total_width: f32 = button_rects.iter().map(|(_, w, _, m)| w + m * 2.0).sum();
            if total_width > usable_width {
                eprintln!("Warning: buttons overflow available width ({} > {})", total_width, usable_width);
                return; // Render black bar
            }

            let start_x = (pixel_shift_width / 2.0) + pixel_shift.0 as f32;

            for (i, button) in buttons.iter().enumerate() {
                let (x_offset, width, height, _margin) = button_rects[i];
                let button_y = (available_height - height) / 2.0 + pixel_shift.1 as f32;
                let button_x = start_x + x_offset;

                render_button(ui, button, button_x, button_y, width, height, cfg);
            }
        });
}

fn render_button(
    ui: &mut egui::Ui,
    button: &Button,
    button_x: f32,
    button_y: f32,
    button_width: f32,
    button_height: f32,
    cfg: &Config,
) {
    use egui::Stroke;

    let rect = Rect::from_min_size(
        Pos2::new(button_x, button_y),
        Vec2::new(button_width, button_height),
    );

    let bg_color = if button.active {
        button.background_active
    } else if cfg.show_button_outlines {
        button.background
    } else {
        Color32::BLACK
    };

    // Draw background with optional border
    let stroke = if button.border_width > 0.0 {
        Stroke::new(button.border_width, button.border_color)
    } else {
        Stroke::NONE
    };

    ui.painter().rect(rect, button.corner_radius, bg_color, stroke);

    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        &button.text,
        FontId::new(button.font_size, FontFamily::Proportional),
        button.foreground,
    );
}

/// Copy painter pixels to DRM buffer with 90° rotation
pub fn copy_to_drm(painter: &TinySkiaPainter, drm: &mut DrmBackend) -> Result<()> {
    let pixels = painter.pixels();
    let mut map = drm.map()?;
    let drm_data = map.as_mut();

    let src_width = DFR_WIDTH as usize;
    let src_height = DFR_HEIGHT as usize;
    let dst_stride = DFR_STRIDE as usize;

    for src_y in 0..src_height {
        for src_x in 0..src_width {
            // Rotate 90° CCW
            let dst_x = (DFR_HEIGHT - 1) as usize - src_y;
            let dst_y = src_x;

            let src_idx = (src_y * src_width + src_x) * 4;
            let dst_idx = (dst_y * dst_stride + dst_x) * 4;

            if src_idx + 3 < pixels.len() && dst_idx + 3 < drm_data.len() {
                drm_data[dst_idx] = pixels[src_idx + 2];     // B
                drm_data[dst_idx + 1] = pixels[src_idx + 1]; // G
                drm_data[dst_idx + 2] = pixels[src_idx];     // R
                drm_data[dst_idx + 3] = pixels[src_idx + 3]; // A
            }
        }
    }

    drop(map);
    drm.dirty(&[ClipRect::new(0, 0, DFR_HEIGHT as u16, DFR_WIDTH as u16)])?;

    Ok(())
}
