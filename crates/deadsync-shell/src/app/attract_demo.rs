use std::sync::Arc;

use deadsync_gameplay::GameplayAction;
use deadsync_online::gameplay_lobby::LobbyRuntimeView;
use deadsync_profile_gameplay::stage_scores::GameplayScoreRefresh;
use deadsync_simfile::runtime_cache::random_chart_with_music;
use deadsync_song_lua::gameplay_session::SongLuaGameplayEffect;
use deadsync_theme::views::GameplayLivePresentation;
use deadsync_theme_openitg::{GAMEPLAY_HUD, OpenItgDemonstration};

use super::{
    ActiveTheme, App, CurrentScreen, GameplayInitView, GameplayLobbySession, GameplaySession,
    GameplaySfx, GameplayStageInput, GameplayStageIntro, GameplayStageView, GameplayState,
    LeadInTiming, config, gameplay_config_from_config, gameplay_viewport, init,
    prepare_gameplay_stage, prewarm_gameplay_assets, prewarm_gameplay_sfx, profile_data,
    song_loading,
};
use crate::gameplay_runtime;

pub(crate) struct AttractDemo {
    pub(super) stage: GameplayState,
    kind: OpenItgDemonstration,
    sounds: GameplaySfx,
    training_cues: [Vec<f32>; 4],
}

impl AttractDemo {
    pub(super) fn is_training(&self) -> bool {
        self.kind == OpenItgDemonstration::Training
    }

    pub(super) fn training_lights(&self) -> [f32; 4] {
        let time = self.stage.current_music_time_display();
        self.training_cues.each_ref().map(|cues| {
            let end = cues.partition_point(|&cue| cue <= time);
            end.checked_sub(1)
                .map_or(0.0, |index| (1.0 - (time - cues[index]) / 0.2).max(0.0))
        })
    }
}

impl App {
    pub(super) fn step_openitg_demonstration(&mut self, delta_time: f32) {
        let screens = &mut self.state.screens;
        if screens.current_screen != CurrentScreen::Init || init::is_loading(&screens.init_state) {
            return;
        }
        let Some(intro) = screens.openitg_intro.as_mut() else {
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
                crate::navigation::song_lua_presentation_screen::<ActiveTheme>,
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
            if intro.timeline.demonstration() == Some(demo.kind) {
                return;
            }
            log::debug!("OpenITG {:?} demonstration ended", demo.kind);
            self.audio.stop_music();
            self.audio.stop_mine_sfx();
            intro.demo = None;
        }
        while let Some(kind) = self
            .state
            .screens
            .openitg_intro
            .as_ref()
            .and_then(|intro| intro.timeline.demonstration())
        {
            let result = self.prepare_attract_demo(kind);
            let intro = self
                .state
                .screens
                .openitg_intro
                .as_mut()
                .expect("active OpenITG intro");
            match result {
                Ok(Some(demo)) => {
                    intro.demo = Some(Box::new(demo));
                    return;
                }
                Ok(None) => log::debug!("OpenITG {kind:?} demonstration has no eligible chart"),
                Err(error) => log::warn!("OpenITG {kind:?} demonstration skipped: {error}"),
            }
            intro.timeline.finish_demonstration();
        }
        self.sync_openitg_intro_media();
    }

    fn prepare_attract_demo(
        &mut self,
        kind: OpenItgDemonstration,
    ) -> Result<Option<AttractDemo>, String> {
        let config = config::runtime::get();
        if config.game_flag != config::theme::GameFlag::Dance {
            return Ok(None);
        }
        let training = kind == OpenItgDemonstration::Training;
        let max_difficulty = if training { 0 } else { 2 };
        let Some((song, chart_index)) = random_chart_with_music("dance-single", max_difficulty)
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
        if training {
            gameplay_config.center_1player_notefield = false;
        }
        let (prepared, scene) = prepare_gameplay_stage(GameplayStageInput {
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
        let present = ActiveTheme::gameplay_presenter(GameplayInitView::default());
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
            |stage, assets| present(stage, assets, &view, Default::default()),
        );
        stage.presentation.set_hud(GAMEPLAY_HUD);
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
        let training_cues =
            if let Some(timing) = stage.gameplay.timing_for_player(0).filter(|_| training) {
                let mut training_rows: Vec<_> = stage
                    .gameplay
                    .notes()
                    .iter()
                    .filter(|note| {
                        note.can_be_judged && note.note_type != deadsync_core::note::NoteType::Mine
                    })
                    .map(|note| note.beat)
                    .collect();
                training_rows.sort_unstable_by(f32::total_cmp);
                training_rows.dedup();
                std::array::from_fn(|index| {
                    let mut cues: Vec<_> = timing
                        .get_times_for_beats_exact(
                            training_rows.iter().map(|&beat| beat - index as f32),
                        )
                        .collect();
                    cues.sort_unstable_by(f32::total_cmp);
                    cues
                })
            } else {
                Default::default()
            };
        stage.gameplay.set_live_autoplay_enabled(true);
        stage.gameplay.start_stage_music();
        gameplay_runtime::drain_audio(
            &mut stage.stage.gameplay,
            &mut self.audio,
            &sounds,
            &mut self.music_clock,
        );
        log::debug!(
            "OpenITG {kind:?} demonstration started for '{}' ({:.1} ms preparation)",
            stage.song().title,
            started.elapsed().as_secs_f64() * 1000.0,
        );
        Ok(Some(AttractDemo {
            stage,
            kind,
            sounds,
            training_cues,
        }))
    }
}
