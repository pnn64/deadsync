use super::*;

#[derive(Clone)]
pub(super) struct SongLuaLoading {
    pub title: Arc<str>,
    pub color_index: i32,
    pub progress: deadsync_song_lua::SongLuaBakeProgress,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::course_transition_tests::{finish_pending_stage, on_app_thread, test_app};
    use crate::gameplay_regression_tests::tests::with_session;

    fn song(root: &std::path::Path) -> Arc<SongData> {
        let path = root.join("loading.ssc");
        std::fs::write(
            &path,
            "#TITLE:Loading test;\n#BPMS:0=120;\n\
            #BGCHANGES:0=background.png=1=0=0=0;\n\
            #FGCHANGES:0=main.lua=1=0=0=0=StretchNoLoop====;\n\
            #NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n\
            #METER:1;\n#NOTES:\n1000\n0000\n0010\n0000\n;\n",
        )
        .unwrap();
        std::fs::write(
            root.join("main.lua"),
            "return Def.Quad { OnCommand = function(self) self:linear(1):x(100) end }",
        )
        .unwrap();
        std::fs::write(root.join("background.png"), []).unwrap();
        Arc::new(deadsync_simfile::app_runtime::parse_song_for_test(&path, 0.0).unwrap())
    }

    fn assert_loading_frame(app: &mut App) {
        let policy = app.frame_policy;
        let (actors, _, segments) = app.get_current_actors(0.0, policy);
        assert!(segments.is_none());
        assert_eq!(
            actors.len(),
            3,
            "loading UI must stay above the held transition"
        );
        assert!(actors.iter().any(|actor| matches!(actor,
            Actor::Text { content, .. } if content.as_str() == "Loading test")));
        assert!(
            actors
                .iter()
                .any(|actor| matches!(actor, Actor::Frame { children, .. }
            if children.iter().any(|actor| matches!(actor, Actor::Text { content, .. }
                if content.as_str().ends_with('%')))))
        );
    }

    #[test]
    fn bake_progress_preload_is_visible_while_waiting_and_clears_on_cancel_or_error() {
        on_app_thread(|| {
            with_session(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P1,
                true,
                false,
                || {
                    let root = tempfile::tempdir().unwrap();
                    let mut app = test_app(root.path());
                    let song = song(root.path());
                    assert!(
                        SongLuaLoading::for_song(&song, config::options::SongLuaMode::Runtime, 5)
                            .is_none()
                    );
                    let mut ordinary = song.as_ref().clone();
                    ordinary.foreground_lua_changes.clear();
                    assert!(
                        SongLuaLoading::for_song(&ordinary, config::options::SongLuaMode::Baked, 5)
                            .is_none()
                    );
                    let (_sender, receiver) = std::sync::mpsc::sync_channel(1);
                    app.state.screens.current_screen = CurrentScreen::SelectMusic;
                    app.gameplay_preload = Some(PendingGameplayPreload {
                        receiver,
                        ready: None,
                        enter_requested: false,
                        loading: SongLuaLoading::for_song(
                            &song,
                            config::options::SongLuaMode::Baked,
                            5,
                        ),
                    });
                    assert!(
                        app.song_lua_loading().is_none(),
                        "keep the options prompt usable"
                    );
                    app.gameplay_preload.as_mut().unwrap().enter_requested = true;
                    assert!(!app.accepts_live_input());
                    assert_loading_frame(&mut app);
                    app.gameplay_preload.as_mut().unwrap().enter_requested = false;
                    assert!(
                        app.song_lua_loading().is_none(),
                        "choosing options hides progress"
                    );
                    app.gameplay_preload.as_mut().unwrap().enter_requested = true;
                    app.gameplay_preload.as_mut().unwrap().ready = Some(Err("test failure".into()));
                    assert!(
                        app.song_lua_loading().is_none(),
                        "a finished worker never leaves a stale bar"
                    );
                },
            )
        });
    }

    #[test]
    fn bake_progress_gameplay_and_practice_workers_show_the_bar_and_finish() {
        on_app_thread(|| {
            with_session(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P1,
                true,
                false,
                || {
                    for target in [CurrentScreen::Gameplay, CurrentScreen::Practice] {
                        let root = tempfile::tempdir().unwrap();
                        let mut app = test_app(root.path());
                        let song = song(root.path());
                        app.state.screens.current_screen = target;
                        app.state.shell.transition = TransitionState::FadingIn {
                            elapsed: 0.0,
                            duration: 2.0,
                        };
                        let wheel = &mut app.state.screens.select_music_state;
                        wheel.entries = vec![select_music::MusicWheelEntry::Song(song.clone())];
                        wheel.selected_index = 0;
                        wheel.selected_steps_index = 0;
                        wheel.preferred_difficulty_index = 3;
                        wheel.active_color_index = 5;
                        app.prepare_screen_state(CurrentScreen::SelectMusic, target);
                        assert!(app.state.pending_stage.is_some());
                        assert!(app.state.screens.player_options_state.is_none());
                        app.handle_screen_entry_on_fade(CurrentScreen::SelectMusic, target);
                        assert!(app.state.pending_stage.is_none());
                        assert!(
                            app.pending_gameplay_init.is_some(),
                            "Lua preparation must be a worker"
                        );
                        let progress = app.song_lua_loading().unwrap().progress.clone();
                        assert_loading_frame(&mut app);
                        assert!(!app.accepts_live_input());
                        finish_pending_stage(&mut app);
                        assert_eq!(progress.fraction(), 1.0);
                        assert!(app.song_lua_loading().is_none());
                        let gameplay = if target == CurrentScreen::Gameplay {
                            app.state.screens.gameplay_state.as_ref().unwrap()
                        } else {
                            &app.state.screens.practice_state.as_ref().unwrap().gameplay
                        };
                        assert!(gameplay.stage.song_frame().uses_song_lua_compositor());
                        assert!(gameplay.song_backgrounds().changes().iter().any(|change| {
                            matches!(&change.target, deadsync_chart::SongBackgroundChangeTarget::File(path)
                                if path.ends_with("background.png"))
                        }));
                        if target == CurrentScreen::Gameplay {
                            assert!(app.state.screens.gameplay_state.is_some());
                        } else {
                            let state = app.state.screens.practice_state.as_mut().unwrap();
                            let now = Instant::now();
                            crate::gameplay_runtime::handle_practice_input(
                                state,
                                &InputEvent::new(
                                    VirtualAction::p1_menu_down,
                                    0,
                                    true,
                                    deadsync_core::input::InputSource::Keyboard,
                                    now,
                                    0,
                                    now,
                                    now,
                                ),
                                &mut app.audio,
                                &app.gameplay_sfx,
                                &mut app.music_clock,
                                &mut Vec::new(),
                            );
                            let before = format!("{:?}", practice::edit_snapshot(state));
                            assert!(app.prepare_restart_stage(
                                song.clone(),
                                [song.charts[0].short_hash.as_str(); MAX_PLAYERS],
                                1.0,
                                [profile_data::Profile::default().scroll_speed; MAX_PLAYERS],
                                5,
                                CurrentScreen::Practice,
                            ));
                            app.handle_screen_entry_on_fade(
                                CurrentScreen::Practice,
                                CurrentScreen::Practice,
                            );
                            assert_loading_frame(&mut app);
                            finish_pending_stage(&mut app);
                            let after = practice::edit_snapshot(
                                app.state.screens.practice_state.as_ref().unwrap(),
                            );
                            assert_eq!(
                                format!("{after:?}"),
                                before,
                                "practice reload retains editor selection"
                            );
                        }
                        assert_eq!(app.get_in_transition_for_screen(target).1, 2.0);
                        assert_eq!(
                            app.get_out_transition_for_route(target, CurrentScreen::Evaluation).1,
                            1.5,
                        );
                        app.state.session.gameplay_restart_count = 1;
                        let entry_duration = if target == CurrentScreen::Gameplay { 0.2 } else { 2.0 };
                        assert_eq!(app.get_in_transition_for_screen(target).1, entry_duration);
                    }
                },
            )
        });
    }

    #[test]
    fn practice_restart_load_failure_restores_options_for_the_requested_stage() {
        on_app_thread(|| {
            with_session(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P1,
                true,
                false,
                || {
                    let root = tempfile::tempdir().unwrap();
                    let mut app = test_app(root.path());
                    let song = song(root.path());
                    let speeds = [
                        ScrollSpeedSetting::CMod(630.0),
                        ScrollSpeedSetting::XMod(2.5),
                    ];
                    app.state.screens.current_screen = CurrentScreen::Practice;
                    assert!(app.prepare_restart_stage(
                        Arc::clone(&song),
                        [song.charts[0].short_hash.as_str(); MAX_PLAYERS],
                        1.25,
                        speeds,
                        5,
                        CurrentScreen::Practice,
                    ));
                    assert!(app.state.screens.player_options_state.is_none());
                    let stage = app.state.pending_stage.as_ref().unwrap();
                    let expected_steps = stage.options.chart_steps_index;
                    let expected_difficulty = stage.chart_difficulty_index;
                    std::fs::remove_file(&song.simfile_path).unwrap();

                    let (target, _) = app.prepare_global_fade_entry(CurrentScreen::Practice);

                    assert_eq!(target, CurrentScreen::PlayerOptions);
                    assert!(app.state.pending_stage.is_none());
                    assert!(app.pending_gameplay_init.is_none());
                    let recovered = app.state.screens.player_options_state.as_ref().unwrap();
                    assert!(Arc::ptr_eq(&recovered.song, &song));
                    assert_eq!(recovered.return_screen, CurrentScreen::Practice);
                    assert_eq!(recovered.active_color_index, 5);
                    assert_eq!(recovered.music_rate, 1.25);
                    assert_eq!(recovered.chart_steps_index, expected_steps);
                    assert_eq!(recovered.chart_difficulty_index, expected_difficulty);
                    assert_eq!(
                        recovered
                            .speed_mod
                            .each_ref()
                            .map(player_options::scroll_speed_for_mod),
                        speeds
                    );
                },
            )
        });
    }
}

impl SongLuaLoading {
    pub fn for_song(
        song: &SongData,
        mode: config::options::SongLuaMode,
        color_index: i32,
    ) -> Option<Self> {
        (mode == config::options::SongLuaMode::Baked
            && (!song.background_lua_changes.is_empty() || !song.foreground_lua_changes.is_empty()))
        .then(|| Self {
            title: song.display_full_title(false).into(),
            color_index,
            progress: Default::default(),
        })
    }
}

impl App {
    pub(super) fn song_lua_loading(&self) -> Option<&SongLuaLoading> {
        self.pending_gameplay_init
            .as_ref()
            .and_then(|pending| pending.loading.as_ref())
            .or_else(|| {
                self.gameplay_preload
                    .as_ref()
                    .filter(|pending| {
                        pending.ready.is_none()
                            && pending.enter_requested
                            && self.state.screens.current_screen == CurrentScreen::SelectMusic
                    })
                    .and_then(|pending| pending.loading.as_ref())
            })
    }
}

impl App {
    pub(super) fn finish_practice_init(
        &mut self,
        mut gs: gameplay::State,
        finish: PracticeInitFinish,
    ) -> Vec<Command> {
        self.gameplay_live_textures.reset();
        let mut commands = Vec::new();
        let payload_ms = finish.payload_ms;
        crate::gameplay_runtime::sync_initial_scores(&mut gs);
        gs.disable_score_for_practice();
        let init_ms = finish.started.elapsed().as_secs_f64() * 1000.0;

        let sfx_prewarm_started = Instant::now();
        self.gameplay_sfx = prewarm_gameplay_sfx(
            &mut self.audio,
            gs.song_lua_visuals(),
            &gs.stage.song_media.sound_paths,
        );
        let sfx_prewarm_ms = sfx_prewarm_started.elapsed().as_secs_f64() * 1000.0;
        let show_video_backgrounds = finish.config.show_video_backgrounds;
        let background_path =
            Self::refresh_gameplay_background_path(&mut gs, show_video_backgrounds);
        let asset_prewarm_started = Instant::now();
        if let Some(backend) = self.backend.as_mut() {
            prewarm_gameplay_assets(
                &mut self.asset_manager,
                backend,
                [
                    &gs.noteskin_assets.noteskin,
                    &gs.noteskin_assets.mine_noteskin,
                    &gs.noteskin_assets.receptor_noteskin,
                    &gs.noteskin_assets.tap_explosion_noteskin,
                ],
                gs.song(),
                gs.song_backgrounds().changes(),
                gs.song_lua_visuals(),
            );
            self.dynamic_media.set_gameplay_background_paths(
                &mut self.asset_manager,
                backend,
                gs.song(),
                gs.song_backgrounds().changes(),
            );
            self.dynamic_media.sync_active_song_lua_videos(
                &mut self.asset_manager,
                backend,
                gs.stage.song_media.video_paths(),
            );
            prewarm_gameplay_banners(
                &mut self.dynamic_media,
                &mut self.asset_manager,
                backend,
                &gs,
                finish.config.gameplay_banner_mode,
            );
        }
        let asset_prewarm_ms = asset_prewarm_started.elapsed().as_secs_f64() * 1000.0;
        let text_prewarm_started = Instant::now();
        prewarm_gameplay_text_layout_cache(
            &self.asset_manager,
            &self.state.shell.metrics,
            &mut self.gameplay_text_layout_cache,
            &mut self.gameplay_compose_scratch,
            &mut self.actor_scratch,
            &mut gs,
            &finish.config,
            true,
        );
        let text_prewarm_ms = text_prewarm_started.elapsed().as_secs_f64() * 1000.0;
        let song = gs.song();
        debug!(
            "Practice transition timing: song='{}' payload_ms={payload_ms:.3} init_ms={init_ms:.3} sfx_prewarm_ms={sfx_prewarm_ms:.3} asset_prewarm_ms={asset_prewarm_ms:.3} text_prewarm_ms={text_prewarm_ms:.3}",
            song.title
        );
        commands.push(Command::SetPackBanner(gs.presentation.pack_banner_path.clone()));
        commands.push(Command::SetDynamicBackground(background_path));
        // Reconcile chart timing and any video fallback once after the
        // deferred command installs the initial media.
        gs.presentation.background_path_dirty = true;
        let mut practice_state = crate::gameplay_runtime::prepare_practice(gs, finish.runtime);
        if let Some(snapshot) = finish.edit_snapshot {
            practice::restore_edit_snapshot(&mut practice_state, snapshot);
        }
        self.state.screens.practice_state = Some(practice_state);
        if let Some(ps) = self.state.screens.practice_state.as_mut() {
            crate::gameplay_runtime::enter_practice(
                ps,
                &mut self.audio,
                &self.gameplay_sfx,
                &mut self.music_clock,
            );
        }
        commands
    }
}
