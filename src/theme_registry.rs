//! The executable connects installed themes to the application runtime.

use super::mounted_theme::{
    ChatPresentation, GameplayPresentationMount, InstalledMenu, ScreenTransitionMount,
    StartupScreen, SupplementalResources, ThemeMount,
};
use deadsync_config::theme::ThemeFlag;
use deadsync_theme::{ThemeResources, ThemeTransitionAudio, ThemeTransitions};
use deadsync_theme_openitg::{
    OpenItgIntro, OpenItgPlayerOptions, OpenItgSelectionPresentation, OpenItgStagePresentation,
    OpenItgSystemOptions, OpenItgTheme,
};
use deadsync_theme_simply_love::SimplyLoveTheme;
use deadsync_theme_simply_love::screens::{
    gameplay, transition_audio::TransitionAudioContext, transitions,
};
use deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView;
use deadsync_theme_stepmania_395::{
    StepManiaIntro, StepManiaMusicWheel, StepManiaPlayerOptions, StepManiaStageCard,
    StepManiaStagePresentation, StepManiaStartup, StepManiaSystemOptions, StepManiaTheme,
};

const GAMEPLAY_COMPOSER: GameplayPresentationMount = GameplayPresentationMount {
    prepare: SimplyLoveTheme::prepare_gameplay_presentation,
    build: SimplyLoveTheme::build_gameplay_presentation,
    entry: |state, assets, restart, config| {
        gameplay::in_transition(
            state,
            assets,
            restart,
            SimplyLoveVisualPolicyView::from_config(config),
        )
    },
    smx_overlay_alpha: transitions::gameplay_smx_overlay_alpha,
};

const FALLBACK_TRANSITIONS: ScreenTransitionMount = ScreenTransitionMount {
    kind: SimplyLoveTheme::transition_kind,
    actor_entry_seconds: SimplyLoveTheme::actor_entry_duration,
    actor_exit_seconds: SimplyLoveTheme::actor_exit_duration,
    skip_entry_fade: SimplyLoveTheme::skip_global_entry_fade,
    entry: |screen| SimplyLoveTheme::in_transition(screen, None),
    exit: SimplyLoveTheme::out_transition,
    screen_alpha: |screen, state| transitions::screen_fade_view(screen, state).screen_alpha,
    music: |from, to, kind, config| {
        SimplyLoveTheme::transition_audio(
            from,
            to,
            kind,
            TransitionAudioContext {
                menu_music_enabled: config.menu_music,
                style: config.visual_style,
                variant: config.srpg_variant,
            },
        )
    },
    stop_screen_sfx_on_exit: SimplyLoveTheme::stop_screen_sfx_on_exit,
};

pub(crate) fn mount(theme: ThemeFlag) -> ThemeMount {
    match theme {
        ThemeFlag::StepMania395 => ThemeMount {
            title: Box::new(|index| InstalledMenu::mount::<StepManiaTheme>(index, |_| ())),
            startup: |_| Box::new(StepManiaStartup::default()),
            transitions: ScreenTransitionMount {
                music: |from, to, kind, config| {
                    StepManiaTheme::transition_audio(from, to, kind, config.menu_music)
                },
                ..FALLBACK_TRANSITIONS
            },
            resources: resources::<StepManiaTheme>(),
            stage: Box::new(StepManiaStagePresentation::default()),
            stage_card: Some(Box::new(StepManiaStageCard)),
            gameplay: GAMEPLAY_COMPOSER,
            selection: Some(Box::new(StepManiaMusicWheel::default())),
            music_wheel: None,
            player_options: Some(Box::new(|| Box::new(StepManiaPlayerOptions::default()))),
            system_options: Some(Box::new(StepManiaSystemOptions)),
            attract: Some(Box::new(StepManiaIntro::default())),
            chat: chat_overlay,
        },
        ThemeFlag::SimplyLove => ThemeMount {
            title: Box::new(|index| {
                InstalledMenu::mount::<SimplyLoveTheme>(index, |policy| policy)
            }),
            startup: startup_screen,
            transitions: FALLBACK_TRANSITIONS,
            resources: SupplementalResources::default(),
            stage: Box::new(SimplyLoveTheme),
            stage_card: None,
            gameplay: GAMEPLAY_COMPOSER,
            selection: None,
            music_wheel: None,
            player_options: None,
            system_options: None,
            attract: None,
            chat: chat_overlay,
        },
        ThemeFlag::OpenItg => ThemeMount {
            title: Box::new(|index| InstalledMenu::mount::<OpenItgTheme>(index, |_| ())),
            startup: startup_screen,
            transitions: FALLBACK_TRANSITIONS,
            resources: resources::<OpenItgTheme>(),
            stage: Box::new(OpenItgStagePresentation::default()),
            stage_card: None,
            gameplay: GAMEPLAY_COMPOSER,
            selection: Some(Box::new(OpenItgSelectionPresentation::default())),
            music_wheel: None,
            player_options: Some(Box::new(|| Box::new(OpenItgPlayerOptions::default()))),
            system_options: Some(Box::new(OpenItgSystemOptions::default())),
            attract: Some(Box::new(OpenItgIntro::default())),
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
