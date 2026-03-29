//! Error display UI

use egui::{Context, CentralPanel, Frame, Color32, Align2};

/// Render an error message on the Touch Bar
pub fn render_error(ctx: &Context, message: &str) {
    CentralPanel::default()
        .frame(Frame::none().fill(Color32::BLACK))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            let text = format!("⚠ {}", message);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                egui::FontId::new(28.0, egui::FontFamily::Proportional),
                Color32::WHITE,
            );
        });
}
