//! Dimension type for pixel or percentage values

/// A dimension that can be specified as pixels or percentage
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    /// Absolute pixel value
    Pixels(f32),
    /// Percentage of available space (0.0 to 1.0)
    /// Special case: 1.0 means "auto-distribute remaining space"
    Percent(f32),
}

impl Default for Dimension {
    fn default() -> Self {
        Dimension::Percent(1.0) // auto-distribute
    }
}

impl Dimension {
    /// Resolve dimension to pixels given available space
    pub fn resolve(&self, available: f32) -> f32 {
        match self {
            Dimension::Pixels(px) => *px,
            Dimension::Percent(pct) => available * pct,
        }
    }

    /// Check if this is an auto-distribute dimension
    pub fn is_auto(&self) -> bool {
        matches!(self, Dimension::Percent(p) if (*p - 1.0).abs() < f32::EPSILON)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_pixels() {
        let dim = Dimension::Pixels(100.0);
        assert_eq!(dim.resolve(500.0), 100.0);
    }

    #[test]
    fn resolve_percent() {
        let dim = Dimension::Percent(0.5);
        assert_eq!(dim.resolve(200.0), 100.0);
    }

    #[test]
    fn is_auto_true() {
        assert!(Dimension::Percent(1.0).is_auto());
    }

    #[test]
    fn is_auto_false() {
        assert!(!Dimension::Percent(0.5).is_auto());
        assert!(!Dimension::Pixels(100.0).is_auto());
    }

    #[test]
    fn default_is_auto() {
        assert!(Dimension::default().is_auto());
    }
}
