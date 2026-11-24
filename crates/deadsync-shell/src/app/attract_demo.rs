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
    App, CurrentScreen, GameplayLobbySession, GameplaySession, GameplaySfx,
    GameplayStageInput, GameplayStageIntro, GameplayStageView, GameplayState, LeadInTiming, config,
    gameplay_config_from_config, gameplay_viewport, prepare_gameplay_stage,
    prewarm_gameplay_assets, prewarm_gameplay_sfx, profile_data, song_loading,
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
    elapsed: f64,
}

impl App {
    pub(super) fn step_attract_demonstration(&mut self, delta_time: f32) {
        let screens = &mut self.state.screens;
        if screens.current_screen != CurrentScreen::Init || screens.startup_load.is_loading() {
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
            demo.elapsed += f64::from(delta_time.max(0.0));
            let audio = if demo.request.play_music {
                gameplay_runtime::snapshot(&mut self.music_clock)
            } else {
                deadsync_gameplay::GameplayAudioSnapshot {
                    stream_clock: deadsync_gameplay::GameplayStreamClockSnapshot {
                        music_nanos: (demo.elapsed * 1_000_000_000.0) as i64,
                        music_seconds_per_second: 1.0,
                        valid_at: std::time::Instant::now(),
                        valid_at_host_nanos: deadlib_platform::host_time::now_nanos(),
                        has_music_mapping: true,
                        ..Default::default()
                    },
                    ..Default::default()
                }
            };
            stage.stage.update(
                delta_time,
                audio,
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
            if demo.request.play_music {
                gameplay_runtime::drain_audio(
                    &mut stage.stage.gameplay,
                    &mut self.audio,
                    &demo.sounds,
                    &mut self.music_clock,
                );
            } else {
                stage.stage.gameplay.drain_audio_commands().for_each(drop);
            }
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
        let joined = [true, request.two_players];
        let session = GameplaySession {
            play_style: if request.two_players {
                deadsync_gameplay::GameplayInputPlayStyle::Versus
            } else {
                deadsync_gameplay::GameplayInputPlayStyle::Single
            },
            joined_sides: joined,
            ..Default::default()
        };
        let (mut prepared, scene) = prepare_gameplay_stage(GameplayStageInput {
            song,
            charts: [Arc::clone(&metadata), metadata],
            gameplay_charts: [Arc::clone(&chart), chart],
            viewport: gameplay_viewport(self.state.shell.metrics),
            session,
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
                joined,
                profile_data::PlayerSide::P1,
            ),
            GameplayScoreRefresh::default(),
            None,
            |stage, assets| {
                (presentation.build)(
                    stage,
                    assets,
                    &view,
                    &config,
                    None,
                    Default::default(),
                )
            },
        );
        if let Some(hud) = self.state.screens.stage_presentation.gameplay_hud() {
            stage.presentation.install_hud(hud);
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
            stage.presentation.background_mut().background_texture_key =
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
        if request.play_music {
            gameplay_runtime::drain_audio(
                &mut stage.stage.gameplay,
                &mut self.audio,
                &sounds,
                &mut self.music_clock,
            );
        } else {
            stage.gameplay.drain_audio_commands().for_each(drop);
            self.audio.stop_music();
        }
        log::debug!(
            "Attract {request:?} demonstration started for '{}' ({:.1} ms preparation)",
            stage.song().title,
            started.elapsed().as_secs_f64() * 1000.0,
        );
        Ok(Some(AttractDemo {
            stage,
            request,
            sounds,
            elapsed: 0.0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::course_transition_tests::{on_app_thread, test_app_with_theme};
    use super::*;
    use crate::gameplay_regression_tests::tests::with_session;
    use deadsync_chart::{SongPack, SyncPref};
    use deadsync_config::theme::ThemeFlag;
    use deadsync_rules::judgment::JudgeGrade;
    use deadsync_simfile::runtime_cache::{get_song_cache, set_song_cache};
    use deadsync_theme::views::AttractRecords;

    #[test]
    fn silent_two_player_demo_advances_and_returns_to_rankings_without_changing_session() {
        on_app_thread(|| {
            with_session(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P1,
                true,
                false,
                || {
                    let root = tempfile::tempdir().unwrap();
                    let path = root.path().join("demo.sm");
                    let source = format!(
                        "#TITLE:Demonstration;\n#MUSIC:unused.ogg;\n#BPMS:0=120;\n\
                #NOTES:dance-single::Easy:2:0,0,0,0,0:\n{};\n",
                        std::iter::repeat_n("1000\n0100\n0010\n0001\n", 20)
                            .collect::<Vec<_>>()
                            .join(",\n")
                    );
                    std::fs::write(&path, &source).unwrap();
                    let parsed = deadsync_simfile::song::parse_song_bytes(
                        source.as_bytes(),
                        "sm",
                        &path,
                        &deadsync_simfile::song::ParseSongOptions::new(
                            Vec::new(),
                            Vec::new(),
                            Vec::new(),
                        ),
                        |_| 40.0,
                    )
                    .unwrap();
                    let mut song = deadsync_simfile::cache::build_song_meta(parsed, 0.0);
                    // No audio file is required by the silent demonstration's clock.
                    song.music_path = Some(root.path().join("unused.ogg"));
                    let previous = get_song_cache().clone();
                    struct Restore(Vec<SongPack>);
                    impl Drop for Restore {
                        fn drop(&mut self) {
                            set_song_cache(std::mem::take(&mut self.0));
                        }
                    }
                    let _restore = Restore(previous);
                    set_song_cache(vec![SongPack {
                        group_name: "Attract".into(),
                        name: "Attract".into(),
                        sort_title: "Attract".into(),
                        translit_title: String::new(),
                        series: String::new(),
                        folder_series: String::new(),
                        year: 0,
                        sync_pref: SyncPref::Default,
                        directory: root.path().into(),
                        banner_path: None,
                        songs: vec![Arc::new(song)],
                    }]);
                    let mut app = test_app_with_theme(root.path(), ThemeFlag::StepMania395);
                    app.state.screens.startup_load = crate::content_reload::StartupLoad::Ready;
                    let intro = app.state.screens.attract.as_mut().unwrap();
                    intro.timeline.start_idle(AttractRecords::default());
                    intro.timeline.advance(43.0);
                    app.step_attract_demonstration(0.0);
                    assert_eq!(
                        app.state
                            .screens
                            .attract
                            .as_ref()
                            .unwrap()
                            .demo
                            .as_ref()
                            .unwrap()
                            .stage
                            .gameplay
                            .num_players(),
                        2
                    );
                    for _ in 0..120 {
                        app.step_attract_demonstration(1.0 / 60.0);
                    }
                    let demo = app
                        .state
                        .screens
                        .attract
                        .as_mut()
                        .unwrap()
                        .demo
                        .as_mut()
                        .unwrap();
                    assert!((demo.stage.gameplay.current_music_time_seconds() - 2.0).abs() < 0.01);
                    for side in 0..2 {
                        let player = demo.stage.gameplay.player(side).unwrap();
                        assert!(player.autoplay_judged);
                        assert!(player.last_judgment.is_some());
                        // Autoplay shows feedback while leaving cabinet scores untouched.
                        assert_eq!(
                            demo.stage
                                .gameplay
                                .display_judgment_count(side, JudgeGrade::Fantastic),
                            0
                        );
                        assert_eq!(
                            demo.stage
                                .gameplay
                                .display_judgment_count(side, JudgeGrade::Miss),
                            0
                        );
                    }
                    assert_eq!(demo.stage.gameplay.drain_audio_commands().count(), 0);
                    for _ in 0..(29 * 60) {
                        app.step_attract_demonstration(1.0 / 60.0);
                    }
                    let intro = app.state.screens.attract.as_ref().unwrap();
                    assert!(intro.demo.is_none());
                    assert_eq!(
                        intro.timeline.media().music,
                        Some("assets/media/stepmania-395/ranking.ogg")
                    );
                    assert_eq!(
                        deadsync_profile::compat::get_session_play_style(),
                        profile_data::PlayStyle::Single
                    );
                    assert!(!deadsync_profile::compat::is_session_side_joined(
                        profile_data::PlayerSide::P2
                    ));
                },
            )
        });
    }
}
