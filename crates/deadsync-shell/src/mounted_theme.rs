//! Presentation interfaces supplied by the executable at startup.

pub use crate::title_menu::InstalledMenu;
use deadlib_present::actors::Actor;
use deadlib_present::actors::TextContent;
use deadsync_assets::gameplay_presentation::{
    GameplayPresentationInit, GameplayRenderView, GameplayScreen, PracticeGameplayHud,
    PracticeTimingLabels, standard::StandardGameplay,
};
use deadsync_assets::gameplay_stage::{
    ActiveGameplayStage, GameplayStageAssets, PreparedGameplayStage,
};
use deadsync_assets::gameplay_stage::{GameplayCoreState, GameplayStagePresenter};
use deadsync_assets::progress::LibraryLoadView;
use deadsync_config::app_config::Config;
use deadsync_config::theme::MachineFont;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_online::gameplay_lobby::GameplayLobbySession;
use deadsync_profile_gameplay::stage_scores::GameplayScoreSnapshots;
use deadsync_score::CachedPlayerLeaderboardData;
use deadsync_song_lua::gameplay_presentation::{FramePresentationContext, SongFrameContent};
use deadsync_song_lua::playback::{FrameScratch, GameplayActorSegments};
use deadsync_theme::attract::AttractPresentation;
use deadsync_theme::views::{
    GameplayHud, GameplayLivePresentation, HeartRateView, SmxSensorPadView,
};
use std::ops::{Deref, DerefMut};

use deadsync_theme::player_options::PlayerOptionsLayout;
use deadsync_theme::selection::{MusicWheelPresentation, SelectionPresentation};
use deadsync_theme::stage::{StageCardPresentation, StagePresentation};
use deadsync_theme::startup::StartupPresentation;
use deadsync_theme::system_options::SystemOptionsPresentation;
use deadsync_theme::views::GameplayStageView;
use deadsync_theme::{Screen, ThemeSounds, TransitionKind, TransitionMusic, TransitionState};

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
    pub practice_gameplay: GameplayPresentationMount,
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
#[derive(Clone, Copy)]
pub struct GameplayPresentationMount {
    pub prepare: fn(&mut PreparedGameplayStage),
    pub build: fn(
        &ActiveGameplayStage,
        GameplayStageAssets,
        &GameplayStageView,
        &Config,
        Option<GameplayPresentationInit>,
        GameplayScoreSnapshots,
    ) -> MountedGameplay,
    pub smx_overlay_alpha: fn(bool, &TransitionState<Screen>) -> f32,
}

pub trait GameplayPresenter:
    GameplayScreen<GameplayLobbySession, GameplayHud<Actor>>
    + GameplayLivePresentation<GameplayCoreState>
{
}

impl<T> GameplayPresenter for T where
    T: GameplayScreen<GameplayLobbySession, GameplayHud<Actor>>
        + GameplayLivePresentation<GameplayCoreState>
{
}

/// One mounted presenter per stage. Dispatch happens at the screen boundary.
pub struct MountedGameplay(GameplayMount);

enum GameplayMount {
    Custom(Box<dyn GameplayPresenter>),
    Standard(Box<StandardGameplay<GameplayLobbySession>>),
}

impl MountedGameplay {
    pub fn new(presentation: impl GameplayPresenter + 'static) -> Self {
        Self(GameplayMount::Custom(Box::new(presentation)))
    }

    pub fn standard(presentation: StandardGameplay<GameplayLobbySession>) -> Self {
        Self(GameplayMount::Standard(Box::new(presentation)))
    }
}

impl Deref for MountedGameplay {
    type Target = dyn GameplayScreen<GameplayLobbySession, GameplayHud<Actor>>;

    fn deref(&self) -> &Self::Target {
        match &self.0 {
            GameplayMount::Custom(presenter) => presenter.as_ref(),
            GameplayMount::Standard(presenter) => presenter.as_ref(),
        }
    }
}

impl DerefMut for MountedGameplay {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match &mut self.0 {
            GameplayMount::Custom(presenter) => presenter.as_mut(),
            GameplayMount::Standard(presenter) => presenter.as_mut(),
        }
    }
}

impl GameplayStagePresenter<GameplayLobbySession> for MountedGameplay {
    type View = GameplayRenderView;
    type Policy = ();

    fn compose_stage_frame(
        &mut self,
        stage: &ActiveGameplayStage,
        lobby: &GameplayLobbySession,
        song: &mut FrameScratch,
        actors: &mut Vec<Actor>,
        context: FramePresentationContext<'_, Self::View, Self::Policy>,
        content: SongFrameContent,
    ) -> GameplayActorSegments {
        self.deref_mut()
            .compose_stage_frame(stage, lobby, song, actors, context, content)
    }

    fn append_song_lua_exit(&self, actors: &mut Vec<Actor>, fade_progress: f32) {
        self.deref().append_song_lua_exit(actors, fade_progress);
    }
}

impl GameplayLivePresentation<GameplayCoreState> for MountedGameplay {
    fn update_gameplay_visuals(&mut self, game: &GameplayCoreState, delta_time: f32) {
        match &mut self.0 {
            GameplayMount::Custom(presenter) => presenter.update_gameplay_visuals(game, delta_time),
            GameplayMount::Standard(presenter) => presenter.update_visuals(game, delta_time),
        }
    }

    fn sync_leaderboards(&mut self, updates: [Option<CachedPlayerLeaderboardData>; MAX_PLAYERS]) {
        if let GameplayMount::Custom(presenter) = &mut self.0 {
            presenter.sync_leaderboards(updates);
        }
    }

    fn sync_itl_cmod_warning(&mut self, warnings: [bool; MAX_PLAYERS]) {
        match &mut self.0 {
            GameplayMount::Custom(presenter) => presenter.sync_itl_cmod_warning(warnings),
            GameplayMount::Standard(presenter) => presenter.set_cmod_warning(warnings),
        }
    }

    fn toggle_lobby_hud(&mut self) {
        match &mut self.0 {
            GameplayMount::Custom(presenter) => presenter.toggle_lobby_hud(),
            GameplayMount::Standard(presenter) => presenter.toggle_lobby(),
        }
    }

    fn sync_smx_sensors(&mut self, views: [Option<SmxSensorPadView>; MAX_PLAYERS]) {
        if let GameplayMount::Custom(presenter) = &mut self.0 {
            presenter.sync_smx_sensors(views);
        }
    }

    fn sync_heart_rate(&mut self, generation: (u64, u64), view: HeartRateView) {
        if let GameplayMount::Custom(presenter) = &mut self.0 {
            presenter.sync_heart_rate(generation, view);
        }
    }
}

impl PracticeGameplayHud for MountedGameplay {
    fn machine_font(&self) -> MachineFont {
        self.deref().machine_font()
    }

    fn set_practice_timing_labels(&mut self, labels: PracticeTimingLabels) {
        self.deref_mut().set_practice_timing_labels(labels);
    }

    fn music_rate_changed(&mut self, stage: &ActiveGameplayStage) {
        self.deref_mut().music_rate_changed(stage);
    }
}
