pub mod app_requests;
pub mod attract;
pub mod color;
mod effect;
pub mod evaluation;
pub mod main_menu;
pub mod player_options;
pub mod profile_picker;
mod resources;
mod runtime;
mod screen;
pub mod select_music;
pub mod selection;
pub mod stage;
pub mod startup;
pub mod system_options;
mod transition;
pub mod views;

pub use effect::{ThemeEffect, ThemeFlowEvent, ThemeInputResult};
pub use resources::{ThemeResources, ThemeSounds, UiCue, UiSound};
pub use runtime::{
    AudioCut, AudioOutputModeChoice, AudioRequest, AudioVolumeTarget, DisplayModeChoice,
    FullscreenChoice, GraphicsRequest, PlatformRequest, PresentPolicyChoice, RendererChoice,
    RevealPathKind, thread_choice_index, thread_count_from_choice,
};
pub use screen::{Screen, Theme, ThemeScreenId};
pub use transition::{
    ThemeTransitionAudio, ThemeTransitions, TransitionCompletion, TransitionFrame,
    TransitionKind, TransitionMusic, TransitionState,
};

// Keep the theme-facing API while game/engine consumers use the owning crates.
pub use deadlib_assets::AssetManifest as ThemeAssetManifest;
pub use deadsync_core::hud_style::{
    ColumnCueStyle, ColumnFlashLayoutStyle, ColumnFlashStyle, ComboFeedbackStyle, CounterHudStyle,
    ErrorBarLayers, ErrorBarPalette, ErrorBarStyle, FieldFeedbackStyle, HoldFeedbackStyle,
    JudgmentFeedbackStyle, MiniIndicatorStyle, NotefieldHudStyle, TapFeedbackTween,
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
