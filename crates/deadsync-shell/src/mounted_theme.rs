//! Presentation interfaces supplied by the executable at startup.

pub use crate::title_menu::InstalledMenu;
use deadlib_assets::AssetManager;
use deadlib_present::actors::Actor;
use deadlib_present::actors::TextContent;
use deadsync_assets::gameplay_stage::{
    ActiveGameplayStage, GameplayHost, GameplayStageAssets, PreparedGameplayStage,
};
use deadsync_assets::progress::LibraryLoadView;
use deadsync_config::app_config::Config;
use deadsync_online::gameplay_lobby::GameplayLobbySession;
use deadsync_profile_gameplay::stage_scores::GameplayScoreSnapshots;
use deadsync_theme::attract::AttractPresentation;
use deadsync_theme::player_options::PlayerOptionsLayout;
use deadsync_theme::selection::{MusicWheelPresentation, SelectionPresentation};
use deadsync_theme::stage::{StageCardPresentation, StagePresentation};
use deadsync_theme::startup::StartupPresentation;
use deadsync_theme::system_options::SystemOptionsPresentation;
use deadsync_theme::views::GameplayStageView;
use deadsync_theme::{Screen, ThemeSounds, TransitionKind, TransitionMusic, TransitionState};
use deadsync_theme_simply_love::{
    screens::gameplay::GameplayPresentation, views::GameplayInitView,
};

#[derive(Default)]
pub struct SupplementalResources {
    pub fonts: &'static [deadlib_assets::FontAssetSpec],
    pub embedded_pngs: &'static [deadlib_assets::EmbeddedPng],
    pub sounds: ThemeSounds,
}

pub type PlayerOptionsLayoutFactory =
    Box<dyn Fn() -> Box<dyn PlayerOptionsLayout<Actor, TextContent>>>;
pub type StartupScreen = dyn StartupPresentation<Actor, LibraryLoadView, Config>;

pub struct ThemeMount {
    pub session_screen_support: deadsync_core::session_flow::SessionNavigationPolicy,
    pub title: Box<dyn FnOnce(i32) -> InstalledMenu>,
    pub startup: fn(i32) -> Box<StartupScreen>,
    pub transitions: ScreenTransitionMount,
    pub resources: SupplementalResources,
    pub stage: Box<dyn StagePresentation<Actor>>,
    pub stage_card: Option<Box<dyn StageCardPresentation<Actor>>>,
    pub session_end: Option<Box<dyn deadsync_theme::post_game::SessionEndPresentation<Actor>>>,
    pub gameplay: GameplayPresentationMount,
    pub selection: Option<Box<dyn SelectionPresentation<Actor>>>,
    pub music_wheel: Option<Box<dyn MusicWheelPresentation<Actor>>>,
    pub player_options: Option<PlayerOptionsLayoutFactory>,
    pub system_options: Option<Box<dyn SystemOptionsPresentation<Actor>>>,
    pub attract: Option<Box<dyn AttractPresentation<Actor>>>,
    pub chat: fn() -> Box<ChatPresentation>,
}

/// Fallback presentation for screens without their own mounted transitions.
#[derive(Clone, Copy)]
pub struct ScreenTransitionMount {
    pub kind: fn(Screen, Screen) -> TransitionKind,
    pub actor_entry_seconds: fn(Screen) -> Option<f32>,
    pub actor_exit_seconds: fn(Screen) -> f32,
    pub skip_entry_fade: fn(Screen, Screen) -> bool,
    pub entry: fn(Screen) -> (Vec<Actor>, f32),
    pub exit: fn(Screen, Screen) -> (Vec<Actor>, f32),
    pub screen_alpha: fn(Screen, &TransitionState<Screen>) -> f32,
    pub music: fn(Screen, Screen, TransitionKind, &Config) -> TransitionMusic,
    pub stop_screen_sfx_on_exit: fn(Screen, Screen) -> bool,
    pub entry_sound: fn(Screen) -> Option<&'static str>,
    pub exit_sound: fn(Screen, Screen) -> Option<&'static str>,
}

pub type ChatPresentation = dyn deadsync_theme::views::TwitchChatPresentation<
        Actor,
        deadsync_online::twitch::Entry,
        deadsync_config::streaming::TwitchOptions,
    >;

/// Stage construction calls `prepare` and `build` on the loader thread, before live Lua is attached.
/// The current gameplay host still carries the fallback composer's state type.
#[derive(Clone, Copy)]
pub struct GameplayPresentationMount {
    pub prepare: fn(&mut PreparedGameplayStage),
    pub build: fn(
        &ActiveGameplayStage,
        GameplayStageAssets,
        &GameplayStageView,
        GameplayInitView,
        GameplayScoreSnapshots,
    ) -> GameplayPresentation,
    pub entry: fn(
        Option<&GameplayHost<GameplayLobbySession, GameplayPresentation>>,
        &AssetManager,
        bool,
        &Config,
    ) -> (Vec<Actor>, f32),
    pub smx_overlay_alpha: fn(bool, &TransitionState<Screen>) -> f32,
}
