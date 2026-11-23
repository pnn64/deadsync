use std::sync::Arc;

use deadlib_present::actors::Actor;
use deadsync_gameplay::GameplayAction;
use deadsync_online::gameplay_lobby::LobbyRuntimeView;
use deadsync_profile_gameplay::stage_scores::GameplayScoreRefresh;
use deadsync_simfile::runtime_cache::random_chart_with_music;
use deadsync_song_lua::gameplay_session::SongLuaGameplayEffect;
use deadsync_theme::attract::{
    AttractDemoChart, AttractDemoRequest, AttractMedia, AttractPresentation,
};
use deadsync_theme::views::GameplayLivePresentation;

use super::{
    App, CurrentScreen, GameplayInitView, GameplayLobbySession, GameplaySession,
    GameplaySfx, GameplayStageInput, GameplayStageIntro, GameplayStageView, GameplayState,
    LeadInTiming, config, gameplay_config_from_config, gameplay_viewport, init,
    prepare_gameplay_stage, prewarm_gameplay_assets, prewarm_gameplay_sfx, profile_data,
    song_loading,
};
use crate::gameplay_runtime;

pub(super) struct AttractRuntime {
    pub(super) timeline: Box<dyn AttractPresentation<Actor>>,
    pub(super) video_key: Option<Arc<str>>,
    pub(super) demo: Option<Box<AttractDemo>>,
    current_media: Option<AttractMedia>,
}

impl AttractRuntime {
    pub(super) fn new(timeline: Box<dyn AttractPresentation<Actor>>) -> Self {
        Self {
            timeline,
            video_key: None,
            demo: None,
            current_media: None,
        }
    }

    pub(super) fn update_media(&mut self) -> Option<AttractMedia> {
        let media = self.timeline.media();
        if self.current_media == Some(media) {
            return None;
        }
        self.current_media = Some(media);
        Some(media)
    }

    pub(super) fn reset(&mut self) {
        self.timeline.reset();
        self.video_key = None;
        self.demo = None;
        self.current_media = None;
    }
}

pub(crate) struct AttractDemo {
    pub(super) stage: GameplayState,
    pub(super) request: AttractDemoRequest,
    sounds: GameplaySfx,
}

impl App {
    pub(super) fn step_attract_demonstration(&mut self, delta_time: f32) {
        let screens = &mut self.state.screens;
        if screens.current_screen != CurrentScreen::Init || init::is_loading(&screens.init_state) {
            return;
        }
        let Some(intro) = screens.attract.as_mut() else {
            return;
        };
        if intro.timeline.demonstration().is_none() {
            return;
        }
        if let Some(demo) = intro.demo.as_mut() {
            let mut complete = false;
            let stage = &mut demo.stage;
            stage.stage.update(
                delta_time,
                gameplay_runtime::snapshot(&mut self.music_clock),
                deadlib_platform::host_time::now_nanos,
                crate::navigation::song_lua_presentation_screen,
                |effect| {
                    complete |= matches!(
                        effect,
                        SongLuaGameplayEffect::Action(
                            GameplayAction::Navigate(_) | GameplayAction::NavigateNoFade(_)
                        )
                    );
                },
                |game, delta| stage.presentation.update_gameplay_visuals(game, delta),
            );
            gameplay_runtime::drain_audio(
                &mut stage.stage.gameplay,
                &mut self.audio,
                &demo.sounds,
                &mut self.music_clock,
            );
            if complete {
                intro.timeline.finish_demonstration();
            } else {
                intro.timeline.advance(delta_time);
            }
            if intro.timeline.demonstration() == Some(demo.request) {
                return;
            }
            log::debug!("Attract {:?} demonstration ended", demo.request);
            self.audio.stop_music();
            self.audio.stop_mine_sfx();
            intro.demo = None;
        }
        while let Some(kind) = self
            .state
            .screens
            .attract
            .as_ref()
            .and_then(|intro| intro.timeline.demonstration())
        {
            let result = self.prepare_attract_demo(kind);
            let intro = self
                .state
                .screens
                .attract
                .as_mut()
                .expect("active attract presentation");
            match result {
                Ok(Some(demo)) => {
                    let game = &demo.stage.gameplay;
                    intro
                        .timeline
                        .prepare_demonstration(game.timing_for_player(0).map(|timing| {
                            AttractDemoChart {
                                notes: game.notes(),
                                note_times: game.note_time_cache_ns(),
                                timing,
                            }
                        }));
                    intro.demo = Some(Box::new(demo));
                    return;
                }
                Ok(None) => log::debug!("Attract {kind:?} demonstration has no eligible chart"),
                Err(error) => log::warn!("Attract {kind:?} demonstration skipped: {error}"),
            }
            intro.timeline.finish_demonstration();
        }
        self.sync_attract_media();
    }

    fn prepare_attract_demo(
        &mut self,
        request: AttractDemoRequest,
    ) -> Result<Option<AttractDemo>, String> {
        let config = config::runtime::get();
        if config.game_flag.as_str() != request.game {
            return Ok(None);
        }
        let Some((song, chart_index)) =
            random_chart_with_music(request.chart_type, request.max_difficulty)
        else {
            return Ok(None);
        };
        let started = std::time::Instant::now();
        let metadata = song.charts[chart_index].clone();
        let chart = song_loading::load_gameplay_charts(
            &song,
            &[chart_index],
            config.global_offset_seconds,
        )?
        .pop()
        .ok_or("prepared chart is missing")?;
        let metadata = Arc::new(metadata);
        let chart = Arc::new(chart);
        let mut gameplay_config = gameplay_config_from_config(&config);
        if request.force_uncentered_notefield {
            gameplay_config.center_1player_notefield = false;
        }
        let (mut prepared, scene) = prepare_gameplay_stage(GameplayStageInput {
            song,
            charts: [Arc::clone(&metadata), metadata],
            gameplay_charts: [Arc::clone(&chart), chart],
            viewport: gameplay_viewport(self.state.shell.metrics),
            session: GameplaySession::default(),
            config: gameplay_config,
            active_color_index: self.state.screens.menu_state.color_index(),
            music_rate: 1.0,
            scroll_speed: [deadsync_rules::scroll::ScrollSpeedSetting::XMod(1.0); 2],
            player_profiles: Default::default(),
            replay_edges: None,
            replay_offsets: None,
            lead_in_timing: Some(LeadInTiming {
                min_seconds_to_step: 0.0,
                min_seconds_to_music: 0.0,
            }),
            course_display_carry: None,
            course_display_totals: None,
            course_display_timing: None,
            course_modifiers: None,
            course_life_config: Default::default(),
            include_post_fail_passes: false,
            combo_carry: [0; 2],
            song_lua_data: Default::default(),
            mini_indicator: Default::default(),
        });
        let presentation = self.gameplay_presentation;
        (presentation.prepare)(&mut prepared);
        let view = GameplayStageView {
            intro: GameplayStageIntro::Demonstration,
            replay: None,
            course: None,
        };
        let mut stage = prepared.into_host(
            scene,
            GameplayLobbySession::new(
                LobbyRuntimeView::default(),
                [true, false],
                profile_data::PlayerSide::P1,
            ),
            GameplayScoreRefresh::default(),
            None,
            |stage, assets| {
                (presentation.build)(stage, assets, &view, GameplayInitView::default(), Default::default())
            },
        );
        if let Some(hud) = self.state.screens.stage_presentation.gameplay_hud() {
            stage.presentation.set_hud(hud);
        }
        let sounds = prewarm_gameplay_sfx(
            &mut self.audio,
            stage.song_lua_visuals(),
            &stage.stage.song_media.sound_paths,
        );
        if let Some(backend) = self.backend.as_mut() {
            prewarm_gameplay_assets(
                &mut self.asset_manager,
                backend,
                [
                    &stage.noteskin_assets.noteskin,
                    &stage.noteskin_assets.mine_noteskin,
                    &stage.noteskin_assets.receptor_noteskin,
                    &stage.noteskin_assets.tap_explosion_noteskin,
                ],
                stage.song(),
                &[],
                stage.song_lua_visuals(),
            );
            let path = stage.song().background_path.to_owned();
            stage.presentation.background_texture_key =
                Arc::from(self.dynamic_media.set_background(
                    &mut self.asset_manager,
                    backend,
                    path,
                    0.0,
                    false,
                ));
        }
        stage.gameplay.set_live_autoplay_enabled(true);
        stage.gameplay.start_stage_music();
        gameplay_runtime::drain_audio(
            &mut stage.stage.gameplay,
            &mut self.audio,
            &sounds,
            &mut self.music_clock,
        );
        log::debug!(
            "Attract {request:?} demonstration started for '{}' ({:.1} ms preparation)",
            stage.song().title,
            started.elapsed().as_secs_f64() * 1000.0,
        );
        Ok(Some(AttractDemo {
            stage,
            request,
            sounds,
        }))
    }
}
