//! The executable connects installed themes to the application runtime.

use super::mounted_theme::{
    ChatPresentation, GameplayPresentationMount, InstalledMenu, StartupScreen,
    SupplementalResources, ThemeMount,
};
use deadsync_config::theme::ThemeFlag;
use deadsync_theme::ThemeResources;
use deadsync_theme_ddr_extreme::{
    DdrExtremeTheme, ExtremeAttract, ExtremePlayerOptions, ExtremeSelection, ExtremeStage,
    ExtremeSystemOptions,
};
use deadsync_theme_openitg::{
    OpenItgIntro, OpenItgPlayerOptions, OpenItgSelectionPresentation, OpenItgStagePresentation,
    OpenItgSystemOptions, OpenItgTheme,
};
use deadsync_theme_simply_love::SimplyLoveTheme;

const GAMEPLAY_COMPOSER: GameplayPresentationMount = GameplayPresentationMount {
    prepare: SimplyLoveTheme::prepare_gameplay_presentation,
    build: SimplyLoveTheme::build_gameplay_presentation,
};

pub(crate) fn mount(theme: ThemeFlag) -> ThemeMount {
    match theme {
        ThemeFlag::SimplyLove => ThemeMount {
            title: |index| InstalledMenu::mount::<SimplyLoveTheme>(index, |policy| policy),
            startup: startup_screen,
            resources: SupplementalResources::default(),
            stage: Box::new(SimplyLoveTheme),
            gameplay: GAMEPLAY_COMPOSER,
            selection: None,
            player_options: None,
            system_options: None,
            attract: None,
            chat: chat_overlay,
        },
        ThemeFlag::OpenItg => ThemeMount {
            title: |index| InstalledMenu::mount::<OpenItgTheme>(index, |_| ()),
            startup: startup_screen,
            resources: resources::<OpenItgTheme>(),
            stage: Box::new(OpenItgStagePresentation::default()),
            gameplay: GAMEPLAY_COMPOSER,
            selection: Some(Box::new(OpenItgSelectionPresentation::default())),
            player_options: Some(|| Box::new(OpenItgPlayerOptions::default())),
            system_options: Some(Box::new(OpenItgSystemOptions::default())),
            attract: Some(Box::new(OpenItgIntro::default())),
            chat: chat_overlay,
        },
        ThemeFlag::DdrExtreme => ThemeMount {
            title: |index| InstalledMenu::mount::<DdrExtremeTheme>(index, |_| ()),
            startup: startup_screen,
            resources: resources::<DdrExtremeTheme>(),
            stage: Box::new(ExtremeStage::default()),
            gameplay: GAMEPLAY_COMPOSER,
            selection: Some(Box::new(ExtremeSelection)),
            player_options: Some(|| Box::new(ExtremePlayerOptions)),
            system_options: Some(Box::new(ExtremeSystemOptions)),
            attract: Some(Box::new(ExtremeAttract::default())),
            chat: chat_overlay,
        },
    }
}

fn startup_screen(color_index: i32) -> Box<StartupScreen> {
    Box::new(deadsync_theme_simply_love::screens::init::State::new(
        color_index,
    ))
}

fn chat_overlay() -> Box<ChatPresentation> {
    Box::new(
        deadsync_theme_simply_love::screens::components::shared::twitch_chat::Overlay::default(),
    )
}

fn resources<T: ThemeResources>() -> SupplementalResources {
    let manifest = T::asset_manifest();
    SupplementalResources {
        fonts: manifest.fonts,
        embedded_pngs: manifest.embedded_pngs,
        sounds: T::SOUNDS,
    }
}
