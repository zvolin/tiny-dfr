//! Touch Bar state and constants

/// Touch Bar display width in pixels
pub const DFR_WIDTH: i32 = 2008;

/// Touch Bar display height in pixels
pub const DFR_HEIGHT: i32 = 60;

/// DRM framebuffer stride
pub const DFR_STRIDE: i32 = 64;

/// Event loop timeout in milliseconds
pub const TIMEOUT_MS: i32 = 10 * 1000;

/// Current state of the Touch Bar
#[derive(Debug, Clone, Default)]
pub enum TouchBarState {
    /// Normal operation with function layers
    #[default]
    Normal,
    /// Error state displaying a message
    Error(String),
}
