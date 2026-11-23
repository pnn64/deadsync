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
    Wheel,
    NextRow,
    PreviousRow,
    Value,
    Easier,
    Harder,
    Expand,
    StageFailed,
}

impl UiCue {
    pub const ALL: [Self; 11] = [
        Self::Change,
        Self::Confirm,
        Self::Screenshot,
        Self::Wheel,
        Self::NextRow,
        Self::PreviousRow,
        Self::Value,
        Self::Easier,
        Self::Harder,
        Self::Expand,
        Self::StageFailed,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}

/// A resolved asset path or a WAV bundled with a theme.
#[derive(Clone, Copy, Debug)]
pub enum UiSound {
    File(&'static str),
    EmbeddedWav {
        name: &'static str,
        bytes: &'static [u8],
    },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ThemeSounds {
    /// Fixed paths referenced by the theme's own screens.
    pub effects: &'static [&'static str],
    pub cues: &'static [(UiCue, UiSound)],
}

impl ThemeSounds {
    pub fn cue(self, cue: UiCue) -> Option<UiSound> {
        self.cues
            .iter()
            .find_map(|&(event, sound)| (event == cue).then_some(sound))
    }

    /// Resolve paths still emitted by a shared screen through the installed cues.
    /// The first cue is the legacy binding when several cues share one sample.
    pub fn cue_for_path(self, path: &str) -> Option<UiCue> {
        self.cues.iter().find_map(|&(cue, sound)| match sound {
            UiSound::File(file) if file == path => Some(cue),
            _ => None,
        })
    }
}
