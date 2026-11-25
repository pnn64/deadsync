//! The gameplay screen lifecycle supplied by the mounted theme.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::gameplay_stage::{ActiveGameplayStage, GameplayStagePresenter};
use deadlib_assets::AssetView;
use deadlib_present::actors::Actor;
use deadlib_present::compose::{ComposeScratch, TextLayoutCache};
use deadsync_config::app_config::Config;
use deadsync_config::theme::MachineFont;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_notefield::{TimingSegmentLabel, ViewOverride};
use deadsync_profile::GameplayHudSnapshot;
use deadsync_score::SelectMusicScoreboxFilter;
use deadsync_song_lua::gameplay_presentation::GameplayFrameView;
use deadsync_song_lua::playback::{
    NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY, PLAYER_ACTOR_SCRATCH_CAPACITY,
};

use deadsync_core::color::JudgmentPalette;
use deadsync_core::hud_style::{GameplayChatLayout, GameplayHudMotion};

pub mod background;
pub mod display_mods;
pub mod field;
pub mod field_style;
pub mod life;
pub mod standard;
pub mod text_slots;

/// Full-screen cover shown while the shell is preparing the gameplay stage.
pub fn loading_cover() -> Vec<Actor> {
    use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
    let mut cover = deadlib_present::dsl::SpriteBuilder::solid();
    cover.xy(screen_center_x(), screen_center_y());
    cover.size(screen_width(), screen_height());
    cover.diffuse([0.0, 0.0, 0.0, 1.0]);
    cover.z(1200);
    vec![cover.build(0)]
}

/// User-provided stage-start sounds shared by the installed screens.
pub fn start_sfx(restart_count: u32) -> Option<PathBuf> {
    if restart_count == 0 {
        crate::audio_folder::random_sfx("assets/sounds/song_start")
    } else {
        crate::audio_folder::indexed_sfx(
            "assets/sounds/song_start/restart",
            restart_count,
            "restart.ogg",
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayRenderView {
    pub notefield: ViewOverride,
    pub hide_gameplay_hud: bool,
    pub hud_motion: GameplayHudMotion,
    pub smx_overlay_alpha: f32,
}

impl Default for GameplayRenderView {
    fn default() -> Self {
        Self {
            notefield: ViewOverride::default(),
            hide_gameplay_hud: false,
            hud_motion: GameplayHudMotion::Settled,
            smx_overlay_alpha: 1.0,
        }
    }
}

impl From<GameplayRenderView> for GameplayFrameView {
    fn from(view: GameplayRenderView) -> Self {
        Self {
            notefield: view.notefield,
            hide_gameplay_hud: view.hide_gameplay_hud,
        }
    }
}

/// Shared facts resolved by the host before constructing a theme's presentation.
#[derive(Clone)]
pub struct GameplayPresentationInit {
    pub hud: GameplayHudSnapshot,
    pub judgment_palettes: [JudgmentPalette; MAX_PLAYERS],
    pub scorebox_pane_filter: SelectMusicScoreboxFilter,
}

/// Media binding updated by the shell when the current song background changes.
pub struct GameplayBackgroundMedia {
    pub background_path_dirty: bool,
    pub current_background_path: Option<PathBuf>,
    pub current_background_key: Option<Arc<str>>,
    pub background_allow_video: bool,
    pub background_texture_key: Arc<str>,
}

impl Default for GameplayBackgroundMedia {
    fn default() -> Self {
        Self {
            background_path_dirty: true,
            current_background_path: None,
            current_background_key: None,
            background_allow_video: false,
            background_texture_key: Arc::from("__black"),
        }
    }
}

/// Text caches shared by the shell and the screen's entry warmup.
pub struct GameplayTextPrewarm<'a> {
    pub cache: &'a mut TextLayoutCache,
    pub scratch: &'a mut ComposeScratch,
    pub assets: &'a dyn AssetView,
    pub edit_measure_text: bool,
}

pub fn prewarm_compose_storage(scratch: &mut ComposeScratch, players: usize) {
    let draw_floor = PLAYER_ACTOR_SCRATCH_CAPACITY.saturating_mul(players);
    let sorted_sprite_floor = usize::from(players > 1).saturating_mul(draw_floor);
    let textured_mesh_floor =
        NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY.saturating_mul(players.saturating_add(1));
    scratch.retain_working_set_headroom(
        draw_floor,
        sorted_sprite_floor,
        textured_mesh_floor,
        NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
    );
}

/// The shell supplies stage facts and media while each presenter keeps its own visual state.
pub trait GameplayScreen<L, H>:
    GameplayStagePresenter<L, View = GameplayRenderView, Policy = ()> + PracticeGameplayHud + Send
{
    fn background(&self) -> &GameplayBackgroundMedia;
    fn background_mut(&mut self) -> &mut GameplayBackgroundMedia;
    fn hide_song_intro(&self) -> bool;
    fn set_hide_song_intro(&mut self, hide: bool);
    fn pack_banner_path(&self) -> Option<&Path> {
        None
    }
    fn replay_status_text(&self) -> Option<&str> {
        None
    }
    fn visible_banner_paths<'a>(
        &'a self,
        _stage: &'a ActiveGameplayStage,
    ) -> [Option<&'a Path>; 2] {
        [None; 2]
    }
    fn chat_layout(&self, _stage: &ActiveGameplayStage, _lobby: &L) -> Option<GameplayChatLayout> {
        None
    }
    fn begin_background_transition(
        &mut self,
        _previous: Arc<str>,
        _transition: &str,
        _start_time: f32,
    ) {
    }
    fn install_hud(&mut self, _hud: H) {}
    fn has_hud(&self) -> bool {
        true
    }
    fn refresh_frame(&mut self, _stage: &mut ActiveGameplayStage, _config: &Config) {}
    fn prewarm_assets(&mut self) {}
    fn prewarm_text(
        &self,
        _stage: &ActiveGameplayStage,
        _lobby: &L,
        _prewarm: GameplayTextPrewarm<'_>,
    ) {
    }
    fn entry(
        &self,
        stage: &ActiveGameplayStage,
        lobby: &L,
        assets: &dyn AssetView,
        restarting: bool,
        config: &Config,
    ) -> (Vec<Actor>, f32);
}

pub type PracticeTimingLabels = [Box<[TimingSegmentLabel]>; MAX_PLAYERS];

/// Gameplay HUD hooks used by the Practice editor.
pub trait PracticeGameplayHud {
    fn machine_font(&self) -> MachineFont;
    fn set_practice_timing_labels(&mut self, _labels: PracticeTimingLabels) {}
    fn music_rate_changed(&mut self, _stage: &ActiveGameplayStage) {}
}
