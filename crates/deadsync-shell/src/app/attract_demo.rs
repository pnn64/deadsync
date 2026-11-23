use std::sync::Arc;

use deadsync_core::note::NoteType;
use deadsync_core::song_time::{SongTimeNs, song_time_ns_to_seconds};
use deadsync_gameplay::GameplayAction;
use deadsync_online::gameplay_lobby::LobbyRuntimeView;
use deadsync_profile_gameplay::stage_scores::GameplayScoreRefresh;
use deadsync_rules::{note::Note, timing::TimingData};
use deadsync_simfile::runtime_cache::random_chart_with_music;
use deadsync_song_lua::gameplay_session::SongLuaGameplayEffect;
use deadsync_theme::views::GameplayLivePresentation;
use deadsync_theme_openitg::{GAMEPLAY_HUD, OpenItgDemonstration, OpenItgTrainingFeedback};

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
    training_cues: Option<TrainingCues>,
}

impl AttractDemo {
    pub(super) fn is_training(&self) -> bool {
        self.kind == OpenItgDemonstration::Training
    }

    pub(super) fn training_feedback(&self) -> OpenItgTrainingFeedback {
        self.training_cues
            .as_ref()
            .map_or_else(Default::default, |cues| {
                cues.sample(self.stage.current_music_time_display())
            })
    }
}

struct TrainingRow {
    beat: f32,
    at: f32,
    panels: u8,
}

struct TrainingLightCue {
    start: f32,
    end: f32,
}

struct TrainingCues {
    rows: Vec<TrainingRow>,
    panels: [Vec<f32>; 4],
    preview: [Vec<TrainingLightCue>; 3],
}

impl TrainingCues {
    fn new(notes: &[Note], note_times: &[SongTimeNs], timing: &TimingData) -> Self {
        let mut rows: Vec<_> = notes
            .iter()
            .zip(note_times)
            .filter(|(note, _)| {
                note.can_be_judged
                    && note.column < 4
                    && matches!(
                        note.note_type,
                        NoteType::Tap | NoteType::Hold | NoteType::Roll
                    )
            })
            .map(|(note, &time)| TrainingRow {
                beat: note.beat,
                at: song_time_ns_to_seconds(time),
                panels: 1 << note.column,
            })
            .collect();
        rows.sort_unstable_by(|a, b| a.beat.total_cmp(&b.beat));
        rows.dedup_by(|next, row| {
            if next.beat != row.beat {
                return false;
            }
            row.panels |= next.panels;
            true
        });

        // OpenITG forecasts on the whole-beat messages. Map both ends through
        // the actual timeline so dots arrive in time across stops and BPM changes.
        let preview = std::array::from_fn(|index| {
            let beats_ahead = (index + 1) as f32;
            let ahead_beats = || {
                rows.iter()
                    .filter(|row| row.beat.fract() == 0.0)
                    .map(|row| row.beat - beats_ahead)
            };
            let mut cues: Vec<_> = timing
                .get_times_for_beats_exact(ahead_beats())
                .zip(timing.get_times_for_beats_exact(ahead_beats().map(|beat| beat + 1.0)))
                .map(|(start, end)| TrainingLightCue { start, end })
                .collect();
            cues.sort_unstable_by(|a, b| a.start.total_cmp(&b.start));
            cues
        });
        rows.sort_unstable_by(|a, b| a.at.total_cmp(&b.at).then(a.beat.total_cmp(&b.beat)));
        let panels = std::array::from_fn(|index| {
            rows.iter()
                .filter(|row| row.panels & (1 << index) != 0)
                .map(|row| row.at)
                .collect()
        });
        Self {
            rows,
            panels,
            preview,
        }
    }

    fn sample(&self, now: f32) -> OpenItgTrainingFeedback {
        let step = self.rows[..self.rows.partition_point(|row| row.at <= now)]
            .last()
            .map(|row| (row.panels, now - row.at));
        let mut feedback = OpenItgTrainingFeedback {
            step,
            panel_ages: self.panels.each_ref().map(|times| {
                times[..times.partition_point(|&time| time <= now)]
                    .last()
                    .map(|time| now - time)
            }),
            ..Default::default()
        };
        feedback.light_ages[0] = step.map(|(_, age)| age);
        for (index, cues) in self.preview.iter().enumerate() {
            if let Some(cue) = cues[..cues.partition_point(|cue| cue.start <= now)].last() {
                let age = now - cue.start;
                feedback.light_ages[index + 1] = Some(age);
                if now < cue.end {
                    feedback.dot_progress[index] = Some(age / (cue.end - cue.start));
                }
            }
        }
        feedback
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
        let training_cues = stage
            .gameplay
            .timing_for_player(0)
            .filter(|_| training)
            .map(|timing| {
                TrainingCues::new(
                    stage.gameplay.notes(),
                    stage.gameplay.note_time_cache_ns(),
                    timing,
                )
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_rules::timing::TimingSegments;

    #[test]
    fn training_cues_follow_cached_steps_and_preview_beats_across_a_bpm_change() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0), (3.0, 120.0)],
                ..Default::default()
            },
            &[],
        );
        let note = |beat, column, note_type, is_fake| Note {
            participant: None,
            beat,
            quantization_idx: 0,
            column,
            note_type,
            row_index: (beat * 48.0) as usize,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake,
            can_be_judged: !is_fake,
        };
        let cues = TrainingCues::new(
            &[
                note(3.75, 1, NoteType::Tap, false),
                note(4.0, 0, NoteType::Tap, false),
                note(4.0, 2, NoteType::Hold, false),
                note(4.0, 3, NoteType::Mine, false),
                note(4.0, 3, NoteType::Lift, false),
                note(4.0, 3, NoteType::Tap, true),
            ],
            &[
                3_375_000_000,
                3_510_000_000,
                3_510_000_000,
                3_500_000_000,
                3_500_000_000,
                3_500_000_000,
            ],
            &timing,
        );

        let preview = cues.sample(3.25);
        assert_eq!(preview.dot_progress, [Some(0.5), None, None]);
        assert!(preview.step.is_none());
        assert_eq!(cues.sample(3.505).step.unwrap().0, 0b0010);

        let jump = cues.sample(3.56);
        let (panels, age) = jump.step.unwrap();
        assert_eq!(panels, 0b0101);
        assert!((age - 0.05).abs() < 1e-6);
        assert_eq!(jump.light_ages[0], Some(age));
        assert_eq!(jump.panel_ages[0], Some(age));
        assert!((jump.panel_ages[1].unwrap() - 0.185).abs() < 1e-6);
        assert_eq!(jump.panel_ages[2], Some(age));
        assert!(jump.panel_ages[3].is_none());
    }
}
