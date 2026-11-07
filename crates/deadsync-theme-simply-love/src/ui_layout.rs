use deadlib_present::space::{screen_height, screen_width};

/// Shared layout families based on the active screen's aspect ratio.
///
/// Squareish covers ratios up to 4:3, Wide covers ratios up to 16:9, and
/// UltraWide covers anything wider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UILayoutType {
    Squareish,
    Wide,
    UltraWide,
}

impl UILayoutType {
    /// Classifies an aspect ratio, treating invalid values as Squareish.
    #[must_use]
    pub fn from_aspect_ratio(aspect_ratio: f32) -> Self {
        if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 || aspect_ratio <= 4.0 / 3.0 {
            Self::Squareish
        } else if aspect_ratio <= 16.0 / 9.0 {
            Self::Wide
        } else {
            Self::UltraWide
        }
    }

    /// Returns the layout family for the active screen dimensions.
    #[must_use]
    pub fn current() -> Self {
        let height = screen_height();
        let aspect_ratio = if height > 0.0 {
            screen_width() / height
        } else {
            0.0
        };
        Self::from_aspect_ratio(aspect_ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::UILayoutType;

    #[test]
    fn classifies_aspect_ratio_boundaries() {
        assert_eq!(
            UILayoutType::from_aspect_ratio(1.0),
            UILayoutType::Squareish
        );
        assert_eq!(
            UILayoutType::from_aspect_ratio(5.0 / 4.0),
            UILayoutType::Squareish
        );
        assert_eq!(
            UILayoutType::from_aspect_ratio(4.0 / 3.0),
            UILayoutType::Squareish
        );
        assert_eq!(
            UILayoutType::from_aspect_ratio(3.0 / 2.0),
            UILayoutType::Wide
        );
        assert_eq!(
            UILayoutType::from_aspect_ratio(16.0 / 9.0),
            UILayoutType::Wide
        );
        assert_eq!(
            UILayoutType::from_aspect_ratio(21.0 / 9.0),
            UILayoutType::UltraWide
        );
    }

    #[test]
    fn invalid_aspect_ratios_use_squareish_layout() {
        assert_eq!(
            UILayoutType::from_aspect_ratio(0.0),
            UILayoutType::Squareish
        );
        assert_eq!(
            UILayoutType::from_aspect_ratio(f32::NAN),
            UILayoutType::Squareish
        );
    }
}
