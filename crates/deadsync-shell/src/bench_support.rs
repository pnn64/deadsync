use crate::{
    lighting, pad_config, profile_import, qr_login, score_import, smx_config, sync_analysis,
};
use deadsync_config::prelude as config;
use deadsync_rules::timing::{StopSegment, TimingData, TimingSegments};
use deadsync_theme_simply_love::screens::SimplyLoveScreen;
use deadsync_theme_simply_love::screens::components::shared::heart_rate::HeartRateViewSyncBenchmark;
use deadsync_theme_simply_love::screens::gameplay::{HeartRatePlayerView, HeartRateView};
use deadsync_theme_simply_love::{
    SimplyLoveQrLoginEvent, SimplyLoveQrLoginService, SimplyLoveSyncEvent, SimplyLoveSyncOwner,
};
use std::hint::black_box;

/// Old and current steady-state paths for failed gameplay banner preparation.
pub struct GameplayMediaFailureBenchmark {
    banner: crate::dynamic_media::BannerVideoFailureBenchmark,
}

/// Old reconciliation and current settled paths for stable gameplay banners.
pub struct GameplayBannerSyncBenchmark {
    banner: crate::dynamic_media::BannerVideoSyncBenchmark,
}

impl Default for GameplayBannerSyncBenchmark {
    fn default() -> Self {
        Self {
            banner: crate::dynamic_media::BannerVideoSyncBenchmark::default(),
        }
    }
}

impl GameplayBannerSyncBenchmark {
    pub fn legacy_frame(&mut self) -> u64 {
        self.banner.legacy_frame() as u64
    }

    pub fn settled_frame(&mut self) -> u64 {
        self.banner.settled_frame() as u64
    }
}

impl Default for GameplayMediaFailureBenchmark {
    fn default() -> Self {
        Self {
            banner: crate::dynamic_media::BannerVideoFailureBenchmark::default(),
        }
    }
}

impl GameplayMediaFailureBenchmark {
    pub fn legacy_banner_retry_frame(&mut self) -> usize {
        self.banner.legacy_retry_frame()
    }

    pub fn saturated_banner_failure_frame(&self) -> usize {
        self.banner.saturated_frame()
    }
}

const POLLS_PER_FRAME: usize = 256;
const BACKGROUND_ACTIVE_BEAT: f32 = 700.0;

const EVALUATION_SUBMISSION_QUERIES: [Option<(deadsync_profile::PlayerSide, &'static str)>; 2] = [
    Some((
        deadsync_profile::PlayerSide::P1,
        "benchmark-evaluation-submit-p1",
    )),
    Some((
        deadsync_profile::PlayerSide::P2,
        "benchmark-evaluation-submit-p2",
    )),
];

/// Old per-field and current batched Evaluation submission-state reads.
pub struct EvaluationSubmissionBenchmark {
    queries: [Option<(deadsync_profile::PlayerSide, &'static str)>; 2],
}

/// Old split and current coherent lobby refresh reads.
pub struct LobbyRefreshBenchmark;

impl Default for LobbyRefreshBenchmark {
    fn default() -> Self {
        deadsync_online::lobbies::runtime_disconnect();
        Self
    }
}

impl LobbyRefreshBenchmark {
    pub fn legacy_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0usize, |checksum, sample| {
            black_box(deadsync_online::lobbies::runtime_view());
            let (snapshot, reconnect_status) = deadsync_online::lobbies::runtime_view();
            checksum.rotate_left(5)
                ^ lobby_view_checksum(&snapshot, reconnect_status.as_deref())
                ^ sample
        })
    }

    pub fn refreshed_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0usize, |checksum, sample| {
            let (snapshot, reconnect_status) =
                deadsync_online::lobbies::runtime_refresh_view_default();
            checksum.rotate_left(5)
                ^ lobby_view_checksum(&snapshot, reconnect_status.as_deref())
                ^ sample
        })
    }
}

fn lobby_view_checksum(
    snapshot: &deadsync_online::lobbies::Snapshot,
    status: Option<&str>,
) -> usize {
    snapshot.available_lobbies.len()
        ^ (snapshot.joined_lobby.is_some() as usize).rotate_left(7)
        ^ status.map_or(0, str::len).rotate_left(13)
}

impl Default for EvaluationSubmissionBenchmark {
    fn default() -> Self {
        for (idx, query) in EVALUATION_SUBMISSION_QUERIES.iter().flatten().enumerate() {
            let (side, hash) = *query;
            deadsync_online::groovestats::reset_submit_ui_status(side, hash);
            deadsync_online::groovestats::reset_submit_event_ui(side, hash);
            deadsync_online::groovestats::reset_submit_retry(side, hash);
            deadsync_online::groovestats::set_submit_ui_status(
                side,
                hash,
                10_000 + idx as u64,
                deadsync_score::GrooveStatsSubmitUiStatus::TimedOut,
            );
            deadsync_online::arrowcloud::reset_submit_ui_status(side, hash);
            deadsync_online::arrowcloud::reset_submit_retry(side, hash);
            deadsync_online::arrowcloud::set_submit_ui_status(
                side,
                hash,
                20_000 + idx as u64,
                deadsync_score::ArrowCloudSubmitUiStatus::TimedOut,
            );
        }
        Self {
            queries: EVALUATION_SUBMISSION_QUERIES,
        }
    }
}

impl EvaluationSubmissionBenchmark {
    pub fn legacy_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let value = black_box(&self.queries).iter().flatten().fold(
                0usize,
                |checksum, &(side, hash)| {
                    checksum.rotate_left(7)
                        ^ submission_checksum(
                            deadsync_online::groovestats::submit_ui_status_for_side(hash, side)
                                .is_some(),
                            deadsync_online::arrowcloud::submit_ui_status_for_side(hash, side)
                                .is_some(),
                            deadsync_online::groovestats::submit_event_progress_for_side(
                                hash, side,
                            )
                            .len(),
                            deadsync_online::groovestats::submit_record_banner_for_side(hash, side)
                                .is_some(),
                            deadsync_online::groovestats::next_retry_remaining_secs(hash, side),
                            deadsync_online::arrowcloud::next_retry_remaining_secs(hash, side),
                            deadsync_online::groovestats::next_retry_is_auto(hash, side),
                            deadsync_online::arrowcloud::next_retry_is_auto(hash, side),
                        )
                },
            );
            checksum.rotate_left(5) ^ value ^ sample
        })
    }

    pub fn snapshot_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let snapshots = deadsync_online::score_compat::evaluation_submission_snapshots(
                black_box(&self.queries),
            );
            let value = snapshots.iter().fold(0usize, |checksum, snapshot| {
                checksum.rotate_left(7)
                    ^ submission_checksum(
                        snapshot.groovestats_status.is_some(),
                        snapshot.arrowcloud_status.is_some(),
                        snapshot.event_progress.len(),
                        snapshot.record_banner.is_some(),
                        snapshot.groovestats_next_retry_secs,
                        snapshot.arrowcloud_next_retry_secs,
                        snapshot.groovestats_next_retry_is_auto,
                        snapshot.arrowcloud_next_retry_is_auto,
                    )
            });
            checksum.rotate_left(5) ^ value ^ sample
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn submission_checksum(
    groovestats_status: bool,
    arrowcloud_status: bool,
    event_progress_len: usize,
    record_banner: bool,
    groovestats_next_retry_secs: Option<u32>,
    arrowcloud_next_retry_secs: Option<u32>,
    groovestats_next_retry_is_auto: bool,
    arrowcloud_next_retry_is_auto: bool,
) -> usize {
    groovestats_status as usize
        ^ ((arrowcloud_status as usize) << 1)
        ^ (event_progress_len << 2)
        ^ ((record_banner as usize) << 10)
        ^ ((groovestats_next_retry_secs.unwrap_or_default() as usize) << 11)
        ^ ((arrowcloud_next_retry_secs.unwrap_or_default() as usize) << 19)
        ^ ((groovestats_next_retry_is_auto as usize) << 27)
        ^ ((arrowcloud_next_retry_is_auto as usize) << 28)
}

/// Old and current idle worker-maintenance paths used by the release benchmark.
pub struct GameplayIdleWorkersBenchmark {
    qr_login: qr_login::Service,
    profile_import: profile_import::Service,
    score_import: score_import::Service,
    sync_analysis: sync_analysis::Service,
    heart_rate: HeartRateViewSyncBenchmark,
    options_lights: deadsync_smx::OptionsLightPreview,
    player_options_lights: deadsync_smx::PlayerOptionsLightPreview,
    fsr_enabled: [bool; 2],
    frame_config: config::Config,
    frame_config_generation: u64,
    background_timing: TimingData,
    background_start_seconds: Vec<f32>,
    raw_capture_request: Option<bool>,
}

impl Default for GameplayIdleWorkersBenchmark {
    fn default() -> Self {
        let generation = deadsync_heart_rate::player_readings_generation();
        let heart_rate = HeartRateViewSyncBenchmark::new(generation, current_heart_rate_view());
        let (frame_config_generation, frame_config) = config::snapshot();
        let timing_segments = TimingSegments {
            bpms: (0..96)
                .map(|index| (index as f32 * 8.0, 90.0 + (index % 11) as f32 * 13.0))
                .collect(),
            stops: (1..16)
                .map(|index| StopSegment {
                    beat: index as f32 * 44.0,
                    duration: 0.025 * (index % 5) as f32,
                })
                .collect(),
            ..TimingSegments::default()
        };
        let background_timing = TimingData::from_segments(0.0, 0.0, &timing_segments, &[]);
        let background_start_seconds =
            vec![background_timing.get_time_for_beat(BACKGROUND_ACTIVE_BEAT)];
        #[cfg(windows)]
        deadsync_input_native::benchmark_seed_raw_keyboard_capture(true);
        Self {
            qr_login: qr_login::Service::default(),
            profile_import: profile_import::Service::default(),
            score_import: score_import::Service::default(),
            sync_analysis: sync_analysis::Service::default(),
            heart_rate,
            options_lights: deadsync_smx::OptionsLightPreview::default(),
            player_options_lights: deadsync_smx::PlayerOptionsLightPreview::default(),
            fsr_enabled: [true; 2],
            frame_config,
            frame_config_generation,
            background_timing,
            background_start_seconds,
            raw_capture_request: Some(true),
        }
    }
}

impl GameplayIdleWorkersBenchmark {
    pub fn legacy_qr_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| route_qr(black_box(&mut self.qr_login).poll_idle_legacy()))
            .sum()
    }

    pub fn gated_qr_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| black_box(&mut self.qr_login).poll().map_or(0, route_qr))
            .sum()
    }

    pub fn legacy_score_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| black_box(&mut self.score_import).poll_idle_legacy().len())
            .sum()
    }

    pub fn legacy_profile_import_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| black_box(&mut self.profile_import).poll_idle_legacy().len())
            .sum()
    }

    pub fn gated_profile_import_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| {
                black_box(&mut self.profile_import)
                    .poll()
                    .map_or(0, |events| events.len())
            })
            .sum()
    }

    pub fn gated_score_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| {
                black_box(&mut self.score_import)
                    .poll()
                    .map_or(0, |events| events.len())
            })
            .sum()
    }

    pub fn legacy_sync_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| route_sync(black_box(&mut self.sync_analysis).poll_idle_legacy()))
            .sum()
    }

    pub fn gated_sync_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME)
            .map(|_| {
                black_box(&mut self.sync_analysis)
                    .poll()
                    .map_or(0, route_sync)
            })
            .sum()
    }

    pub fn legacy_heart_rate_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let view = current_heart_rate_view();
            checksum.rotate_left(5) ^ self.heart_rate.sync_legacy(view) ^ sample
        })
    }

    pub fn gated_heart_rate_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let generation = deadsync_heart_rate::player_readings_generation();
            let value = self
                .heart_rate
                .sync_generation(generation, current_heart_rate_view);
            checksum.rotate_left(5) ^ value ^ sample
        })
    }

    pub fn legacy_fsr_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let screen = black_box(SimplyLoveScreen::Gameplay);
            let enabled = black_box(&mut self.fsr_enabled);
            enabled[0] = true;
            enabled[1] = true;
            let plan = pad_config::pad_config_fsr_plan(
                screen,
                black_box(true),
                black_box(false),
                black_box(false),
                black_box(true),
            );
            checksum.rotate_left(5)
                ^ usize::from(plan.is_some())
                ^ usize::from(enabled[0])
                ^ (usize::from(enabled[1]) << 1)
                ^ sample
        })
    }

    pub fn gated_fsr_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let screen = black_box(SimplyLoveScreen::Gameplay);
            let active = black_box(false);
            let needed = pad_config::pad_config_fsr_frame_needed(screen, active);
            let plan = if needed {
                let enabled = black_box(&mut self.fsr_enabled);
                enabled[0] = true;
                enabled[1] = true;
                pad_config::pad_config_fsr_plan(
                    screen,
                    black_box(true),
                    black_box(false),
                    active,
                    black_box(true),
                )
            } else {
                None
            };
            let enabled = black_box(self.fsr_enabled);
            checksum.rotate_left(5)
                ^ usize::from(plan.is_some())
                ^ usize::from(enabled[0])
                ^ (usize::from(enabled[1]) << 1)
                ^ sample
        })
    }

    pub fn legacy_options_lights_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let screen = black_box(SimplyLoveScreen::Gameplay);
            let active = smx_config::smx_options_light_preview_active(
                screen,
                black_box(true),
                black_box(false),
            );
            let colors = if active {
                [Some([1, 2, 3]); 2]
            } else {
                [None; 2]
            };
            let restored = black_box(&mut self.options_lights).update(
                active,
                1.0 / 240.0,
                colors,
                100,
                (false, false),
                false,
            );
            checksum.rotate_left(5) ^ usize::from(restored) ^ sample
        })
    }

    pub fn gated_options_lights_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let screen = black_box(SimplyLoveScreen::Gameplay);
            let preview = black_box(&mut self.options_lights);
            let needed = smx_config::smx_options_light_frame_needed(screen, preview.is_active());
            let restored =
                needed && preview.update(false, 1.0 / 240.0, [None; 2], 100, (false, false), false);
            checksum.rotate_left(5) ^ usize::from(restored) ^ sample
        })
    }

    pub fn legacy_player_options_lights_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let screen = black_box(SimplyLoveScreen::Gameplay);
            let preview =
                smx_config::smx_player_options_light_preview_allowed(screen, black_box(true))
                    .then(|| [Some(50), None]);
            black_box(&mut self.player_options_lights).update(preview, 1.0 / 240.0, false);
            checksum.rotate_left(5) ^ sample
        })
    }

    pub fn gated_player_options_lights_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let screen = black_box(SimplyLoveScreen::Gameplay);
            let preview = black_box(&mut self.player_options_lights);
            if smx_config::smx_player_options_light_frame_needed(screen, preview.is_active()) {
                preview.update(None, 1.0 / 240.0, false);
            }
            checksum.rotate_left(5) ^ sample
        })
    }

    pub fn legacy_disabled_lighting_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0usize, |checksum, sample| {
            black_box(deadsync_profile::compat::get_session_snapshot());
            checksum.rotate_left(5) ^ sample
        })
    }

    pub fn gated_disabled_lighting_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0usize, |checksum, sample| {
            let active = lighting::lighting_frame_active(
                black_box(SimplyLoveScreen::Gameplay),
                black_box(deadsync_lights::DriverKind::Off),
                black_box(false),
                black_box(false),
            );
            if active {
                black_box(deadsync_profile::compat::get_session_snapshot());
            }
            checksum.rotate_left(5) ^ sample
        })
    }

    pub fn legacy_idle_submit_retry_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0usize, |checksum, sample| {
            black_box(deadsync_online::runtime::active_groovestats_service());
            black_box(config::get().enable_groovestats);
            black_box(deadsync_online::groovestats::due_auto_submit_retries());
            black_box(config::get().enable_arrowcloud);
            black_box(deadsync_online::arrowcloud::due_auto_submit_retries());
            checksum.rotate_left(5) ^ sample
        })
    }

    pub fn gated_idle_submit_retry_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0usize, |checksum, sample| {
            black_box(deadsync_online::score_compat::tick_evaluation_auto_retries(
                true, false, true,
            ));
            checksum.rotate_left(5) ^ sample
        })
    }

    pub fn legacy_config_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let config = config::get();
            checksum.rotate_left(5) ^ config_checksum(config) ^ sample
        })
    }

    pub fn gated_config_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            if let Some((generation, config)) =
                config::snapshot_if_changed(self.frame_config_generation)
            {
                self.frame_config = config;
                self.frame_config_generation = generation;
            }
            checksum.rotate_left(5) ^ config_checksum(black_box(self.frame_config)) ^ sample
        })
    }

    pub fn legacy_auto_screenshot_config_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            checksum.rotate_left(5) ^ config::get().auto_screenshot_eval as usize ^ sample
        })
    }

    pub fn frame_auto_screenshot_config_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            checksum.rotate_left(5)
                ^ black_box(self.frame_config.auto_screenshot_eval) as usize
                ^ sample
        })
    }

    pub fn legacy_evaluation_config_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let runtime = config::get();
            let context = config::get();
            checksum.rotate_left(5) ^ evaluation_config_checksum(&runtime, &context) ^ sample
        })
    }

    pub fn frame_evaluation_config_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let config = black_box(&self.frame_config);
            checksum.rotate_left(5) ^ evaluation_config_checksum(config, config) ^ sample
        })
    }

    #[cfg(windows)]
    pub fn direct_raw_capture_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            deadsync_input_native::set_raw_keyboard_capture_enabled(black_box(true));
            checksum.rotate_left(5) ^ usize::from(self.raw_capture_request == Some(true)) ^ sample
        })
    }

    #[cfg(windows)]
    pub fn gated_raw_capture_frame(&mut self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let requested = black_box(true);
            if self.raw_capture_request != Some(requested)
                || !deadsync_input_native::raw_keyboard_capture_synced(requested)
            {
                deadsync_input_native::set_raw_keyboard_capture_enabled(requested);
                self.raw_capture_request = Some(requested);
            }
            checksum.rotate_left(5) ^ usize::from(self.raw_capture_request == Some(true)) ^ sample
        })
    }

    pub fn legacy_background_timing_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let start = black_box(&self.background_timing)
                .get_time_for_beat(black_box(BACKGROUND_ACTIVE_BEAT));
            checksum.rotate_left(5) ^ start.to_bits() as usize ^ sample
        })
    }

    pub fn cached_background_timing_frame(&self) -> usize {
        (0..POLLS_PER_FRAME).fold(0, |checksum, sample| {
            let next_change_ix = black_box(1usize);
            let start = next_change_ix
                .checked_sub(1)
                .and_then(|index| black_box(&self.background_start_seconds).get(index))
                .copied()
                .expect("benchmark active background timestamp");
            checksum.rotate_left(5) ^ start.to_bits() as usize ^ sample
        })
    }
}

#[inline(always)]
fn config_checksum(config: config::Config) -> usize {
    config.max_fps as usize
        ^ config.master_volume as usize
        ^ config.simply_love_color as usize
        ^ ((config.show_video_backgrounds as usize) << 8)
        ^ ((config.smx_input as usize) << 9)
        ^ config.bg_brightness.to_bits() as usize
}

fn evaluation_config_checksum(runtime: &config::Config, context: &config::Config) -> usize {
    runtime.enable_groovestats as usize
        ^ ((runtime.enable_arrowcloud as usize) << 1)
        ^ ((runtime.auto_populate_gs_scores as usize) << 2)
        ^ ((context.autosubmit_course_scores_individually as usize) << 3)
        ^ ((context.submit_arrowcloud_fails as usize) << 4)
        ^ ((context.smooth_histogram as usize) << 5)
        ^ ((context.shade_scatterplot_judgments as usize) << 6)
        ^ context.simply_love_color as usize
}

fn current_heart_rate_view() -> HeartRateView {
    HeartRateView {
        players: deadsync_heart_rate::player_readings().map(|reading| HeartRatePlayerView {
            configured: reading.configured,
            connected: reading.connected,
            bpm: reading.bpm,
        }),
    }
}

fn route_qr(events: Vec<SimplyLoveQrLoginEvent>) -> usize {
    let mut arrowcloud = Vec::new();
    let mut groovestats = Vec::new();
    for event in events {
        match event.service() {
            SimplyLoveQrLoginService::ArrowCloud => arrowcloud.push(event),
            SimplyLoveQrLoginService::GrooveStats => groovestats.push(event),
        }
    }
    arrowcloud.len() + groovestats.len()
}

fn route_sync(events: Vec<(SimplyLoveSyncOwner, SimplyLoveSyncEvent)>) -> usize {
    let mut song = Vec::new();
    let mut select_pack = Vec::new();
    let mut options_pack = Vec::new();
    for (owner, event) in events {
        match owner {
            SimplyLoveSyncOwner::SelectMusicSong => song.push(event),
            SimplyLoveSyncOwner::SelectMusicPack => select_pack.push(event),
            SimplyLoveSyncOwner::OptionsPack => options_pack.push(event),
        }
    }
    song.len() + select_pack.len() + options_pack.len()
}
