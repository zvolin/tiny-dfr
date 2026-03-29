//! Error display UI

use egui::{Context, CentralPanel, Frame, Color32, RichText};

/// Render an error message on the Touch Bar
pub fn render_error(ctx: &Context, message: &str) {
    CentralPanel::default()
        .frame(Frame::none().fill(Color32::from_rgb(80, 0, 0)))
        .show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("⚠").size(32.0).color(Color32::YELLOW));
                    ui.add_space(16.0);
                    let msg = if message.len() > 50 {
                        format!("{}...", &message[..47])
                    } else {
                        message.to_string()
                    };
                    ui.label(RichText::new(msg).size(18.0).color(Color32::WHITE));
                });
            });
        });
}
