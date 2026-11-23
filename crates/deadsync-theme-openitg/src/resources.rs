use crate::{GAMEPLAY_HUD, OpenItgTheme};
use deadlib_assets::{EmbeddedPng, FontAssetSpec, TextureAssetSpec};
use deadsync_theme::{ThemeAssetManifest, ThemeResources, ThemeSounds, UiCue, UiSound};

pub(crate) const TITLE_BACKDROP: &str = "openitg/title-backdrop.png";
pub(crate) const TITLE_CHROME: &str = "openitg/title-chrome.png";
pub(crate) const TITLE_GLINT: &str = "openitg/title-glint.png";
pub(crate) const TITLE_STREAK: &str = "openitg/title-streak.png";
pub(crate) const TITLE_FONT: &str = "openitg/title";
pub(crate) const MENU_HEADING_FONT: &str = "openitg/eurostile-glow";
pub(crate) const MENU_NORMAL_FONT: &str = "openitg/eurostile";

macro_rules! png {
    ($name:literal) => {
        EmbeddedPng {
            key: concat!("openitg/", $name, ".png"),
            bytes: include_bytes!(concat!("../assets/", $name, ".png")),
        }
    };
}
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
            fonts: &[
                FontAssetSpec {
                    name: TITLE_FONT,
                    ini_path: "assets/fonts/openitg/_eurostile outline.ini",
                    fallback_font_name: Some("miso"),
                },
                FontAssetSpec {
                    name: MENU_HEADING_FONT,
                    ini_path: "assets/fonts/openitg/_eurostile blue glow.ini",
                    fallback_font_name: Some(TITLE_FONT),
                },
                FontAssetSpec {
                    name: MENU_NORMAL_FONT,
                    ini_path: "assets/fonts/openitg/_eurostile normal.ini",
                    fallback_font_name: Some(TITLE_FONT),
                },
            ],
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
                png!("selection-backdrop"),
                png!("selection-arrows"),
                png!("selection-dots"),
                png!("selection-cursor"),
                png!("selection-style-character"),
                png!("selection-mode-character"),
                png!("selection-explanation"),
                png!("selection-single"),
                png!("selection-versus"),
                png!("selection-double"),
                png!("selection-dance"),
                png!("selection-left-foot"),
                png!("selection-right-foot"),
                png!("selection-marathon"),
                png!("selection-flag"),
                png!("selection-platform"),
                png!("wheel-expanded"),
                png!("wheel-fallback-banner"),
                png!("wheel-frame-edge"),
                png!("wheel-frame-left"),
                png!("wheel-frame-middle"),
                png!("wheel-highlight"),
                png!("wheel-meter"),
                png!("wheel-mode"),
                png!("wheel-p1"),
                png!("wheel-p2"),
                png!("wheel-pane"),
                png!("wheel-section"),
                png!("wheel-song"),
                png!("options-cursor"),
                png!("options-frame"),
                png!("options-highlight"),
                png!("options-page"),
                png!("options-seat"),
                png!("options-seat-p1"),
                png!("options-seat-p2"),
                png!("options-underline"),
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
