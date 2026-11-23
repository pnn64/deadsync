//! Presentation interfaces supplied by the executable at startup.

pub use crate::title_menu::InstalledMenu;
use deadlib_present::actors::Actor;
use deadlib_present::actors::TextContent;
use deadsync_assets::gameplay_stage::{
    ActiveGameplayStage, GameplayStageAssets, PreparedGameplayStage,
};
use deadsync_assets::progress::LibraryLoadView;
use deadsync_config::app_config::Config;
use deadsync_profile_gameplay::stage_scores::GameplayScoreSnapshots;
use deadsync_theme::ThemeSounds;
use deadsync_theme::attract::AttractPresentation;
use deadsync_theme::player_options::PlayerOptionsLayout;
use deadsync_theme::selection::SelectionPresentation;
use deadsync_theme::stage::StagePresentation;
use deadsync_theme::startup::StartupPresentation;
use deadsync_theme::system_options::SystemOptionsPresentation;
use deadsync_theme::views::GameplayStageView;
use deadsync_theme_simply_love::{
    screens::gameplay::GameplayPresentation, views::GameplayInitView,
};

#[derive(Clone, Copy, Default)]
pub struct SupplementalResources {
    pub fonts: &'static [deadlib_assets::FontAssetSpec],
    pub embedded_pngs: &'static [deadlib_assets::EmbeddedPng],
    pub sounds: ThemeSounds,
}

pub type PlayerOptionsLayoutFactory = fn() -> Box<dyn PlayerOptionsLayout<Actor, TextContent>>;
pub type StartupScreen = dyn StartupPresentation<Actor, LibraryLoadView, Config>;

pub struct ThemeMount {
    pub title: fn(i32) -> InstalledMenu,
    pub startup: fn(i32) -> Box<StartupScreen>,
    pub resources: SupplementalResources,
    pub stage: Box<dyn StagePresentation<Actor>>,
    pub gameplay: GameplayPresentationMount,
    pub selection: Option<Box<dyn SelectionPresentation<Actor>>>,
    pub player_options: Option<PlayerOptionsLayoutFactory>,
    pub system_options: Option<Box<dyn SystemOptionsPresentation<Actor>>>,
    pub attract: Option<Box<dyn AttractPresentation<Actor>>>,
    pub chat: fn() -> Box<ChatPresentation>,
}

pub type ChatPresentation = dyn deadsync_theme::views::TwitchChatPresentation<
        Actor,
        deadsync_online::twitch::Entry,
        deadsync_config::streaming::TwitchOptions,
    >;

/// Stage construction calls these hooks on the loader thread, before live Lua is attached.
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
}
