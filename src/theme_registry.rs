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
        ThemeFlag::SimplyLove => ThemeMount {
            title: |index| InstalledMenu::mount::<SimplyLoveTheme>(index, |policy| policy),
            startup: startup_screen,
            transitions: FALLBACK_TRANSITIONS,
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
            transitions: FALLBACK_TRANSITIONS,
            resources: resources::<OpenItgTheme>(),
            stage: Box::new(OpenItgStagePresentation::default()),
            gameplay: GAMEPLAY_COMPOSER,
            selection: Some(Box::new(OpenItgSelectionPresentation::default())),
            player_options: Some(|| Box::new(OpenItgPlayerOptions::default())),
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
