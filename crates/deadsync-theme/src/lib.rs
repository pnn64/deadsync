pub mod color;
mod effect;
pub mod evaluation;
pub mod main_menu;
pub mod player_options;
pub mod profile_picker;
mod resources;
mod runtime;
mod screen;
mod transition;
pub mod views;

pub use effect::{ThemeEffect, ThemeFlowEvent, ThemeInputResult};
pub use resources::{ThemeResources, ThemeSounds, UiCue};
pub use runtime::{
    AudioCut, AudioOutputModeChoice, AudioRequest, AudioVolumeTarget, DisplayModeChoice,
    FullscreenChoice, GraphicsRequest, PlatformRequest, PresentPolicyChoice, RendererChoice,
    RevealPathKind, thread_choice_index, thread_count_from_choice,
};
pub use screen::{Theme, ThemeScreenFlow, ThemeScreenId, ThemeScreens, ThemeSessionFlow};
pub use transition::{
    ThemeTransitionAudio, ThemeTransitions, TransitionAudio, TransitionCompletion, TransitionFrame,
    TransitionKind, TransitionMusic, TransitionState,
};

// Keep the theme-facing API while game/engine consumers use the owning crates.
pub use deadlib_assets::AssetManifest as ThemeAssetManifest;
pub use deadsync_core::hud_style::{
    ColumnCueStyle, ColumnFlashLayoutStyle, ColumnFlashStyle, ComboFeedbackStyle, CounterHudStyle,
    ErrorBarLayers, ErrorBarPalette, ErrorBarStyle, JudgmentFeedbackStyle, MiniIndicatorStyle,
    NotefieldHudStyle,
};

/// Semantic font roles a concrete theme maps to its bundled font resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontRole {
    /// Default body text.
    Normal,
    /// Emphasized labels.
    Bold,
    /// Screen and section titles.
    Header,
    /// Bottom action prompts.
    Footer,
    /// Numeric statistics.
    Numbers,
    /// Evaluation panel numerics.
    ScreenEval,
    /// Large headline numerals.
    Headline,
}
