//! Presentation interfaces supplied by the executable at startup.

pub use crate::title_menu::InstalledMenu;
use deadlib_present::actors::Actor;
use deadlib_present::actors::TextContent;
use deadsync_theme::ThemeSounds;
use deadsync_theme::attract::AttractPresentation;
use deadsync_theme::player_options::PlayerOptionsLayout;
use deadsync_theme::selection::SelectionPresentation;
use deadsync_theme::stage::StagePresentation;
use deadsync_theme::system_options::SystemOptionsPresentation;

#[derive(Clone, Copy, Default)]
pub struct SupplementalResources {
    pub fonts: &'static [deadlib_assets::FontAssetSpec],
    pub embedded_pngs: &'static [deadlib_assets::EmbeddedPng],
    pub sounds: ThemeSounds,
}

pub type PlayerOptionsLayoutFactory = fn() -> Box<dyn PlayerOptionsLayout<Actor, TextContent>>;

pub struct ThemeMount {
    pub title: fn(i32) -> InstalledMenu,
    pub resources: SupplementalResources,
    pub stage: Box<dyn StagePresentation<Actor>>,
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

pub(crate) fn gameplay_presenter(
    prepared: &mut deadsync_assets::gameplay_stage::PreparedGameplayStage,
    init: deadsync_theme_simply_love::views::GameplayInitView,
) -> impl FnOnce(
    &deadsync_assets::gameplay_stage::ActiveGameplayStage,
    deadsync_assets::gameplay_stage::GameplayStageAssets,
    &deadsync_theme::views::GameplayStageView,
    deadsync_theme_simply_love::views::GameplayScoreInitView,
) -> deadsync_theme_simply_love::screens::gameplay::GameplayPresentation
+ use<> {
    deadsync_theme_simply_love::SimplyLoveTheme::gameplay_presenter(prepared, init)
}
