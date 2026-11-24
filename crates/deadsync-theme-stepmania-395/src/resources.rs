use deadlib_assets::{EmbeddedPng, FontAssetSpec, TextureAssetSpec};
use deadsync_theme::{ThemeAssetManifest, ThemeResources, ThemeSounds, UiCue, UiSound};

use crate::StepManiaTheme;

pub(crate) const TITLE_FONT: &str = "stepmania-395/title";
pub(crate) const NORMAL_FONT: &str = "stepmania-395/normal";
pub(crate) const WHEEL_FONT: &str = "stepmania-395/wheel";
pub(crate) const ITEM_FONT: &str = "stepmania-395/options";

macro_rules! png {
    ($name:literal) => {
        EmbeddedPng {
            key: concat!("stepmania-395/", $name),
            bytes: include_bytes!(concat!("../assets/", $name, ".png")),
        }
    };
}

impl ThemeResources for StepManiaTheme {
    const SOUNDS: ThemeSounds = ThemeSounds {
        effects: &[],
        cues: &[
            (
                UiCue::Change,
                UiSound::File("assets/media/stepmania-395/change.ogg"),
            ),
            (
                UiCue::Confirm,
                UiSound::File("assets/media/stepmania-395/start.ogg"),
            ),
            (
                UiCue::Wheel,
                UiSound::File("assets/media/stepmania-395/wheel.ogg"),
            ),
            (
                UiCue::NextRow,
                UiSound::File("assets/media/stepmania-395/next-row.ogg"),
            ),
            (
                UiCue::PreviousRow,
                UiSound::File("assets/media/stepmania-395/previous-row.ogg"),
            ),
            (
                UiCue::Value,
                UiSound::File("assets/media/stepmania-395/value.ogg"),
            ),
            (
                UiCue::Easier,
                UiSound::File("assets/media/stepmania-395/difficulty.ogg"),
            ),
            (
                UiCue::Harder,
                UiSound::File("assets/media/stepmania-395/difficulty.ogg"),
            ),
            (
                UiCue::OpenPlayerOptions,
                UiSound::File("assets/media/stepmania-395/options.ogg"),
            ),
        ],
    };

    fn asset_manifest() -> ThemeAssetManifest<impl Iterator<Item = TextureAssetSpec>> {
        ThemeAssetManifest {
            fonts: &[
                FontAssetSpec {
                    name: ITEM_FONT,
                    ini_path: "assets/fonts/stepmania-395/_Arial 14 shadow.ini",
                    fallback_font_name: Some(NORMAL_FONT),
                },
                FontAssetSpec {
                    name: WHEEL_FONT,
                    ini_path: "assets/fonts/stepmania-395/_shared2.ini",
                    fallback_font_name: Some(TITLE_FONT),
                },
                FontAssetSpec {
                    name: TITLE_FONT,
                    ini_path: "assets/fonts/stepmania-395/_shared1.ini",
                    fallback_font_name: Some("miso"),
                },
                FontAssetSpec {
                    name: NORMAL_FONT,
                    ini_path: "assets/fonts/stepmania-395/Common Normal.ini",
                    fallback_font_name: Some(TITLE_FONT),
                },
            ],
            textures: std::iter::empty(),
            embedded_pngs: &[
                png!("options-page"),
                png!("options-cursor"),
                png!("options-underline"),
                png!("options-header"),
                png!("options-icon"),
                png!("footer"),
                png!("music-background"),
                png!("music-song"),
                png!("music-section"),
                png!("music-highlight"),
                png!("music-banner-frame"),
                png!("music-banner-mask"),
                png!("music-fallback"),
                png!("music-difficulty"),
                png!("music-difficulty-p1"),
                png!("music-difficulty-p2"),
                png!("music-meter-p1"),
                png!("music-meter-p2"),
                png!("music-feet"),
                png!("music-options"),
                png!("music-header"),
                png!("music-radar"),
                png!("music-radar-labels"),
                png!("music-difficulties"),
                png!("music-stage-1"),
                png!("music-stage-2"),
                png!("music-stage-3"),
                png!("music-stage-4"),
                png!("music-stage-5"),
                png!("music-stage-6"),
                png!("music-stage-event"),
                png!("music-stage-final"),
                png!("stage-1"),
                png!("stage-2"),
                png!("stage-3"),
                png!("stage-4"),
                png!("stage-5"),
                png!("stage-6"),
                png!("stage-final"),
                png!("stage-event"),
                png!("stage-nonstop"),
                png!("stage-oni"),
                png!("stage-endless"),
                png!("stage-underline"),
                EmbeddedPng {
                    key: "stepmania-395/title-background",
                    bytes: include_bytes!("../assets/title-background.png"),
                },
                EmbeddedPng {
                    key: "stepmania-395/title-dance",
                    bytes: include_bytes!("../assets/title-dance.png"),
                },
                EmbeddedPng {
                    key: "stepmania-395/title-pump",
                    bytes: include_bytes!("../assets/title-pump.png"),
                },
                EmbeddedPng {
                    key: "stepmania-395/company",
                    bytes: include_bytes!("../assets/company.png"),
                },
                EmbeddedPng {
                    key: "stepmania-395/company-shadow",
                    bytes: include_bytes!("../assets/company-shadow.png"),
                },
            ],
            texture_needs_repeat_sampler: |_| false,
        }
    }
}
