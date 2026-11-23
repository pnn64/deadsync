use crate::DdrExtremeTheme;
use deadlib_assets::{FontAssetSpec, TextureAssetSpec};
use deadsync_theme::{ThemeAssetManifest, ThemeResources, ThemeSounds, UiCue, UiSound};

pub(crate) const FONT: &str = "ddr-extreme/signal";

const MOVE: UiSound = UiSound::EmbeddedWav {
    name: "ddr-extreme/move.wav",
    bytes: include_bytes!("../assets/move.wav"),
};
const CONFIRM: UiSound = UiSound::EmbeddedWav {
    name: "ddr-extreme/confirm.wav",
    bytes: include_bytes!("../assets/confirm.wav"),
};

impl ThemeResources for DdrExtremeTheme {
    const SOUNDS: ThemeSounds = ThemeSounds {
        effects: &[],
        cues: &[
            (UiCue::Change, MOVE),
            (UiCue::Confirm, CONFIRM),
            (UiCue::Wheel, MOVE),
            (UiCue::NextRow, MOVE),
            (UiCue::PreviousRow, MOVE),
            (UiCue::Value, MOVE),
            (UiCue::OpenPlayerOptions, CONFIRM),
        ],
    };

    fn asset_manifest() -> ThemeAssetManifest<impl Iterator<Item = TextureAssetSpec>> {
        ThemeAssetManifest {
            fonts: &[FontAssetSpec {
                name: FONT,
                ini_path: "assets/fonts/ddr-extreme/signal.ini",
                fallback_font_name: Some("cjk"),
            }],
            textures: std::iter::empty(),
            embedded_pngs: &[],
            texture_needs_repeat_sampler: |_| false,
        }
    }
}
