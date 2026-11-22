//! Theme palette selection and shared color types.

pub use deadsync_core::color::{
    DifficultyColorScheme, JudgmentColorRole, JudgmentPalette, JudgmentPalettePreset,
};

/// The built-in palette used to resolve and edit judgment colors for this theme.
pub trait ThemeJudgmentPalette: crate::Theme {
    const JUDGMENT_PRESET: JudgmentPalettePreset;
}
