//! Core domain types: buttons, layers, state

mod button;
mod dimension;
mod layer;
mod state;

pub use button::{Button, FontStyle};
pub use dimension::Dimension;
pub use layer::FunctionLayer;
pub use state::{TouchBarState, DFR_WIDTH, DFR_HEIGHT, DFR_STRIDE, BUTTON_SPACING_PX, TIMEOUT_MS};
