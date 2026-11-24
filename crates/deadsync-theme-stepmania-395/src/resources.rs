use deadlib_assets::{EmbeddedPng, FontAssetSpec, TextureAssetSpec};
use deadsync_theme::{ThemeAssetManifest, ThemeResources, ThemeSounds, UiCue, UiSound};

use crate::StepManiaTheme;

pub(crate) const TITLE_FONT: &str = "stepmania-395/title";
pub(crate) const NORMAL_FONT: &str = "stepmania-395/normal";

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
        ],
    };

    fn asset_manifest() -> ThemeAssetManifest<impl Iterator<Item = TextureAssetSpec>> {
        ThemeAssetManifest {
            fonts: &[
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
