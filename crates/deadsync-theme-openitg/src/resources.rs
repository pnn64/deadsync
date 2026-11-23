use crate::{GAMEPLAY_HUD, OpenItgTheme};
use deadlib_assets::{EmbeddedPng, TextureAssetSpec};
use deadsync_theme::{ThemeAssetManifest, ThemeResources, ThemeSounds, UiCue, UiSound};

macro_rules! wav {
    ($stem:literal) => {
        UiSound::EmbeddedWav {
            name: concat!("openitg/", $stem, ".wav"),
            bytes: include_bytes!(concat!("../assets/", $stem, ".wav")),
        }
    };
}

impl ThemeResources for OpenItgTheme {
    const SOUNDS: ThemeSounds = ThemeSounds {
        effects: &[],
        cues: &[
            (UiCue::Change, wav!("menu-move")),
            (UiCue::Confirm, wav!("confirm")),
            (UiCue::Wheel, wav!("wheel")),
            (UiCue::NextRow, wav!("row-next")),
            (UiCue::PreviousRow, wav!("row-previous")),
            (UiCue::Value, wav!("value")),
            (UiCue::Easier, wav!("easier")),
            (UiCue::Harder, wav!("harder")),
            (UiCue::Expand, wav!("expand")),
        ],
    };

    fn asset_manifest() -> ThemeAssetManifest<impl Iterator<Item = TextureAssetSpec>> {
        ThemeAssetManifest {
            fonts: &[],
            textures: std::iter::empty(),
            embedded_pngs: &[
                EmbeddedPng {
                    key: GAMEPLAY_HUD.default_tap_atlas.key,
                    bytes: include_bytes!("../assets/tap-results.png"),
                },
                EmbeddedPng {
                    key: GAMEPLAY_HUD.default_hold_atlas.key,
                    bytes: include_bytes!("../assets/hold-results.png"),
                },
            ],
            texture_needs_repeat_sampler: |_| false,
        }
    }
}
