use crate::{Theme, ThemeAssetManifest};
use deadlib_assets::TextureAssetSpec;

/// Presentation resources installed when the shell starts or recreates a renderer.
pub trait ThemeResources: Theme {
    const SOUNDS: ThemeSounds;

    fn asset_manifest() -> ThemeAssetManifest<impl Iterator<Item = TextureAssetSpec>>;
}

/// Feedback for common interactions initiated by the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiCue {
    Change,
    Confirm,
    Screenshot,
}

impl UiCue {
    pub const ALL: [Self; 3] = [Self::Change, Self::Confirm, Self::Screenshot];

    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ThemeSounds {
    /// Fixed paths referenced by the theme's own screens.
    pub effects: &'static [&'static str],
    pub change: Option<&'static str>,
    pub confirm: Option<&'static str>,
    pub screenshot: Option<&'static str>,
}

impl ThemeSounds {
    pub const fn cue(self, cue: UiCue) -> Option<&'static str> {
        match cue {
            UiCue::Change => self.change,
            UiCue::Confirm => self.confirm,
            UiCue::Screenshot => self.screenshot,
        }
    }
}
