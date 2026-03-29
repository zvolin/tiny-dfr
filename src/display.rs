//! Display subsystem: DRM backend, painting, rendering

mod drm;
mod painter;
mod renderer;
mod ui;

pub use drm::DrmBackend;
pub use painter::TinySkiaPainter;
pub use renderer::{warm_up_fonts, render_function_layer, copy_to_drm};
pub use ui::render_error;
