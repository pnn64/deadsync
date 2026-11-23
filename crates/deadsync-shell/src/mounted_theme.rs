//! Startup composition of concrete presentation implementations.

use crate::mounted_player_options::PlayerOptionsLayoutFactory;
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

pub(crate) fn player_options_layout(theme: ThemeFlag) -> Option<PlayerOptionsLayoutFactory> {
    match theme {
        ThemeFlag::SimplyLove => None,
        ThemeFlag::OpenItg => {
            Some(|| Box::new(deadsync_theme_openitg::OpenItgPlayerOptions::default()))
        }
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

pub(crate) type ChatPresentation = dyn deadsync_theme::views::TwitchChatPresentation<
    Actor,
    deadsync_online::twitch::Entry,
    deadsync_config::streaming::TwitchOptions,
>;

pub(crate) fn mount_twitch_chat() -> Box<ChatPresentation> {
    Box::new(deadsync_theme_simply_love::screens::components::shared::twitch_chat::Overlay::default())
}

pub(crate) fn gameplay_presenter(
    prepared: &mut deadsync_assets::gameplay_stage::PreparedGameplayStage,
    init: deadsync_theme_simply_love::views::GameplayInitView,
) -> impl FnOnce(
    &deadsync_assets::gameplay_stage::ActiveGameplayStage,
    deadsync_assets::gameplay_stage::GameplayStageAssets,
    &deadsync_theme::views::GameplayStageView,
    deadsync_theme_simply_love::views::GameplayScoreInitView,
) -> deadsync_theme_simply_love::screens::gameplay::GameplayPresentation + use<> {
    deadsync_theme_simply_love::SimplyLoveTheme::gameplay_presenter(prepared, init)
}
