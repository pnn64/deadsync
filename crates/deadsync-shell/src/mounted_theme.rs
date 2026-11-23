//! Startup composition of concrete presentation implementations.

use deadlib_present::actors::Actor;
use deadsync_config::theme::ThemeFlag;
use deadsync_theme::attract::AttractPresentation;
use deadsync_theme::selection::SelectionPresentation;
use deadsync_theme::stage::StagePresentation;
use deadsync_theme::system_options::SystemOptionsPresentation;
use deadsync_theme::{ThemeResources, ThemeSounds};

#[derive(Default)]
pub(crate) struct SupplementalResources {
    pub fonts: &'static [deadlib_assets::FontAssetSpec],
    pub embedded_pngs: &'static [deadlib_assets::EmbeddedPng],
    pub sounds: ThemeSounds,
}

pub(crate) fn supplemental_resources(theme: ThemeFlag) -> SupplementalResources {
    match theme {
        ThemeFlag::SimplyLove => SupplementalResources::default(),
        ThemeFlag::OpenItg => {
            use deadsync_theme_openitg::OpenItgTheme;
            let manifest = OpenItgTheme::asset_manifest();
            SupplementalResources {
                fonts: manifest.fonts,
                embedded_pngs: manifest.embedded_pngs,
                sounds: OpenItgTheme::SOUNDS,
            }
        }
    }
}

pub(crate) fn mount_stage(theme: ThemeFlag) -> Box<dyn StagePresentation<Actor>> {
    match theme {
        ThemeFlag::SimplyLove => Box::new(deadsync_theme_simply_love::SimplyLoveTheme),
        ThemeFlag::OpenItg => Box::new(deadsync_theme_openitg::OpenItgStagePresentation::default()),
    }
}

pub(crate) fn mount_selection(theme: ThemeFlag) -> Option<Box<dyn SelectionPresentation<Actor>>> {
    match theme {
        ThemeFlag::SimplyLove => None,
        ThemeFlag::OpenItg => Some(Box::new(
            deadsync_theme_openitg::OpenItgSelectionPresentation::default(),
        )),
    }
}

pub(crate) fn mount_system_options(
    theme: ThemeFlag,
) -> Option<Box<dyn SystemOptionsPresentation<Actor>>> {
    match theme {
        ThemeFlag::SimplyLove => None,
        ThemeFlag::OpenItg => Some(Box::new(
            deadsync_theme_openitg::OpenItgSystemOptions::default(),
        )),
    }
}

pub(crate) fn mount_attract(theme: ThemeFlag) -> Option<Box<dyn AttractPresentation<Actor>>> {
    match theme {
        ThemeFlag::SimplyLove => None,
        ThemeFlag::OpenItg => Some(Box::new(deadsync_theme_openitg::OpenItgIntro::default())),
    }
}
