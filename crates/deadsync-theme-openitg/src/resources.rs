use crate::{GAMEPLAY_HUD, OpenItgTheme};
use deadlib_assets::{EmbeddedPng, FontAssetSpec, TextureAssetSpec};
use deadsync_theme::{ThemeAssetManifest, ThemeResources, ThemeSounds, UiCue, UiSound};

pub(crate) const TITLE_BACKDROP: &str = "openitg/title-backdrop.png";
pub(crate) const TITLE_CHROME: &str = "openitg/title-chrome.png";
pub(crate) const TITLE_GLINT: &str = "openitg/title-glint.png";
pub(crate) const TITLE_STREAK: &str = "openitg/title-streak.png";
pub(crate) const TITLE_FONT: &str = "openitg/title";
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
            fonts: &[FontAssetSpec {
                name: TITLE_FONT,
                ini_path: "assets/fonts/openitg/_eurostile outline.ini",
                fallback_font_name: Some("miso"),
            }],
            textures: std::iter::empty(),
            embedded_pngs: &[
                EmbeddedPng {
                    key: TITLE_BACKDROP,
                    bytes: include_bytes!("../assets/title-backdrop.png"),
                },
                EmbeddedPng {
                    key: TITLE_CHROME,
                    bytes: include_bytes!("../assets/title-chrome.png"),
                },
                EmbeddedPng {
                    key: TITLE_GLINT,
                    bytes: include_bytes!("../assets/title-glint.png"),
                },
                EmbeddedPng {
                    key: TITLE_STREAK,
                    bytes: include_bytes!("../assets/title-streak.png"),
                },
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
