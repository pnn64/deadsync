use super::*;
use crate::frame_pacing_trace::{GameplayPacingReport, GameplayStorageSample};
use crate::live_case::{ExpectedRuntime, LiveCase, sha256_file, verify_hash};
use deadsync_theme_simply_love::views::PlayerOptionsPlayerView;
use serde::Serialize;

pub(super) struct LiveCaseRuntime {
    spec: LiveCase,
    launched: bool,
    warmup_frames_seen: u32,
    warmup_storage: GameplayStorageSample,
    resolved: Option<ResolvedCase>,
    display: Option<RuntimeDisplay>,
}

#[derive(Serialize)]
struct ResolvedCase {
    simfile: String,
    simfile_sha256: String,
    music: String,
    music_sha256: String,
    chart_hashes: [String; 2],
    chart_difficulties: [String; 2],
}

#[derive(Serialize)]
struct RuntimeDisplay {
    name: String,
    window_width: u32,
    window_height: u32,
    monitor_width: u32,
    monitor_height: u32,
    monitor_x: i32,
    monitor_y: i32,
    scale_factor: f64,
    refresh_millihertz: u32,
}

#[derive(Serialize)]
struct ArtifactBuild<'a> {
    version: &'a str,
    hash: &'a str,
    stamp: &'a str,
    executable: String,
    executable_sha256: String,
    os: &'a str,
    arch: &'a str,
}

#[derive(Serialize)]
struct AudioDevice {
    index: usize,
    name: String,
    is_default: bool,
    sample_rates_hz: Vec<u32>,
}

#[derive(Serialize)]
struct LiveCaseArtifact<'a> {
    schema: u32,
    case_name: &'a str,
    manifest: String,
    manifest_sha256: &'a str,
    data_dir: String,
    config: String,
    config_sha256: String,
    expected_runtime: &'a ExpectedRuntime,
    display: &'a RuntimeDisplay,
    audio_devices: Vec<AudioDevice>,
    build: ArtifactBuild<'a>,
    play_style: String,
    player_side: String,
    joined: [bool; 2],
    music_rate: f32,
    autoplay: bool,
    warmup_frames: u32,
    measured_frames: u32,
    resolved: &'a ResolvedCase,
    pacing: GameplayPacingReport,
}

impl LiveCaseRuntime {
    pub(super) fn new(spec: LiveCase) -> Self {
        Self {
            spec,
            launched: false,
            warmup_frames_seen: 0,
            warmup_storage: GameplayStorageSample::default(),
            resolved: None,
            display: None,
        }
    }

    #[inline(always)]
    pub(super) const fn launched(&self) -> bool {
        self.launched
    }

    pub(super) fn note_warmup_frame(
        &mut self,
        storage: GameplayStorageSample,
    ) -> Option<GameplayStorageSample> {
        if !self.launched || self.resolved.is_none() {
            return None;
        }
        self.warmup_storage.include(storage);
        self.warmup_frames_seen = self.warmup_frames_seen.saturating_add(1);
        (self.warmup_frames_seen == self.spec.warmup_frames).then_some(self.warmup_storage)
    }
}

impl App {
    pub(super) fn launch_live_case(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let Some(mut runtime) = self.live_case.take() else {
            return Ok(());
        };
        let result = self.launch_live_case_inner(&mut runtime, event_loop);
        self.live_case = Some(runtime);
        result
    }

    fn launch_live_case_inner(
        &mut self,
        runtime: &mut LiveCaseRuntime,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), String> {
        let spec = &runtime.spec;
        let display = runtime_display(
            self.window
                .as_deref()
                .ok_or("performance case requires an initialized window")?,
        )?;
        let expected = &spec.expected_runtime;
        if display.name != expected.display_name
            || display.window_width != expected.display_width
            || display.window_height != expected.display_height
            || display.refresh_millihertz != expected.display_refresh_millihertz
        {
            return Err(format!(
                "performance case display mismatch: expected '{}' {}x{} @ {} mHz, got '{}' {}x{} @ {} mHz",
                expected.display_name,
                expected.display_width,
                expected.display_height,
                expected.display_refresh_millihertz,
                display.name,
                display.window_width,
                display.window_height,
                display.refresh_millihertz
            ));
        }
        let cfg = config::get();
        let song = Arc::new(song_loading::parse_song_for_test(
            &spec.simfile,
            cfg.global_offset_seconds,
        )?);
        let music_path = song
            .music_path
            .as_deref()
            .ok_or_else(|| "performance case simfile has no playable music path".to_owned())?;
        let music_sha256 = sha256_file(music_path)?;
        verify_hash("music", &spec.music_sha256, &music_sha256)?;

        let chart_type = spec.play_style.chart_type();
        let resolve_steps = |player: usize| {
            let hash = &spec.chart_hashes[player];
            let steps = song
                .steps_index_for_chart_hash(chart_type, hash)
                .ok_or_else(|| {
                    format!(
                        "player {} chart hash {} is absent for chart type {}",
                        player + 1,
                        hash,
                        chart_type
                    )
                })?;
            let chart = song
                .chart_for_steps_index(chart_type, steps)
                .expect("hash-derived steps index must resolve to its source chart");
            if !chart
                .difficulty
                .eq_ignore_ascii_case(&spec.chart_difficulties[player])
            {
                return Err(format!(
                    "player {} chart difficulty mismatch for {}: expected {}, got {}",
                    player + 1,
                    hash,
                    spec.chart_difficulties[player],
                    chart.difficulty
                ));
            }
            Ok(steps)
        };
        let chart_steps_index = [resolve_steps(0)?, resolve_steps(1)?];
        let chart_plan = gameplay_chart_entry_plan(
            &song,
            chart_steps_index,
            chart_steps_index,
            spec.play_style,
            spec.player_side,
        );
        let actual_chart_hashes = chart_plan
            .charts
            .each_ref()
            .map(|chart| chart.short_hash.clone());
        for (player, actual_hash) in actual_chart_hashes.iter().enumerate() {
            if !actual_hash.eq_ignore_ascii_case(&spec.chart_hashes[player]) {
                return Err(format!(
                    "player {} chart hash mismatch for {}: expected {}, got {}",
                    player + 1,
                    spec.chart_difficulties[player],
                    spec.chart_hashes[player],
                    actual_hash
                ));
            }
        }

        std::fs::create_dir_all(&spec.artifact_dir).map_err(|error| {
            format!(
                "cannot create case artifact directory '{}': {error}",
                spec.artifact_dir.display()
            )
        })?;
        profile::set_session_play_style(spec.play_style);
        profile::set_session_player_side(spec.player_side);
        profile::set_session_joined(spec.joined[0], spec.joined[1]);
        profile::set_session_play_mode(profile_data::PlayMode::Regular);
        profile::set_session_music_rate(spec.music_rate);
        self.begin_play_session();

        let mut init_view = crate::player_options::init_view();
        init_view.players = std::array::from_fn(|_| PlayerOptionsPlayerView::default());
        let player_options = player_options::init_for_gameplay(
            Arc::clone(&song),
            chart_steps_index,
            chart_steps_index,
            self.state.screens.menu_state.active_color_index,
            CurrentScreen::SelectMusic,
            None,
            noteskin_catalog_view(),
            crate::smx_config::smx_gif_catalog_view(),
            crate::heart_rate::devices_view(),
            init_view,
        );
        self.state.screens.player_options_state = Some(player_options);
        self.state.screens.current_screen = CurrentScreen::PlayerOptions;

        let prev = CurrentScreen::PlayerOptions;
        let target = CurrentScreen::Gameplay;
        self.commit_screen_change(target);
        let mut commands = self.handle_audio_and_profile_on_fade(prev, target);
        self.prepare_screen_state(prev, target);
        commands.extend(self.handle_screen_entry_on_fade(prev, target));
        self.state.shell.transition = TransitionState::Idle;
        self.run_commands(commands, event_loop);
        let gameplay = self
            .state
            .screens
            .gameplay_state
            .as_mut()
            .ok_or_else(|| "performance case failed to construct Gameplay state".to_owned())?;
        gameplay.gameplay.set_live_autoplay_enabled(spec.autoplay);

        runtime.resolved = Some(ResolvedCase {
            simfile: spec.simfile.display().to_string(),
            simfile_sha256: spec.simfile_sha256.clone(),
            music: music_path.display().to_string(),
            music_sha256,
            chart_hashes: actual_chart_hashes,
            chart_difficulties: spec.chart_difficulties.clone(),
        });
        runtime.display = Some(display);
        runtime.launched = true;
        info!(
            "Performance case '{}' entered Gameplay; warmup_frames={} measured_frames={} autoplay={}",
            spec.name, spec.warmup_frames, spec.measured_frames, spec.autoplay
        );
        Ok(())
    }

    pub(super) fn advance_live_case_capture(
        &mut self,
        frame_finished: Instant,
        storage: GameplayStorageSample,
    ) {
        let Some(runtime) = self.live_case.as_mut() else {
            return;
        };
        if self.state.screens.current_screen != CurrentScreen::Gameplay || !runtime.launched() {
            return;
        }
        if !self.state.shell.gameplay_pacing_trace.capture_active()
            && let Some(warmed_storage) = runtime.note_warmup_frame(storage)
        {
            self.state.shell.gameplay_pacing_trace.start_capture(
                frame_finished,
                runtime.spec.measured_frames,
                warmed_storage,
            );
            info!(
                "Performance case '{}' warmup complete; collecting {} frames",
                runtime.spec.name, runtime.spec.measured_frames
            );
        }
    }

    pub(super) fn finish_live_case(
        &self,
        report: GameplayPacingReport,
        event_loop: &ActiveEventLoop,
    ) {
        let Some(runtime) = self.live_case.as_ref() else {
            return;
        };
        let Some(resolved) = runtime.resolved.as_ref() else {
            error!("Performance case completed without resolved fixture state");
            event_loop.exit();
            return;
        };
        let Some(display) = runtime.display.as_ref() else {
            error!("Performance case completed without captured display identity");
            event_loop.exit();
            return;
        };
        let config_path = dirs::app_dirs().config_path();
        let config_sha256 = match sha256_file(&config_path) {
            Ok(hash) => hash,
            Err(error) => {
                error!("Performance case could not hash config: {error}");
                event_loop.exit();
                return;
            }
        };
        if let Err(error) = verify_hash("config", &runtime.spec.config_sha256, &config_sha256) {
            error!("Performance case config changed during execution: {error}");
            event_loop.exit();
            return;
        }
        let executable = match std::env::current_exe() {
            Ok(path) => path,
            Err(error) => {
                error!("Performance case cannot resolve its executable: {error}");
                event_loop.exit();
                return;
            }
        };
        let executable_sha256 = match sha256_file(&executable) {
            Ok(hash) => hash,
            Err(error) => {
                error!("Performance case cannot hash its executable: {error}");
                event_loop.exit();
                return;
            }
        };
        let expected_audio = &runtime.spec.expected_runtime;
        let audio_matches = report
            .audio_backend
            .eq_ignore_ascii_case(expected_audio.audio_backend.trim())
            && report
                .audio_requested_output_mode
                .eq_ignore_ascii_case(expected_audio.audio_output_mode.trim())
            && report.audio_fallback_from_native == expected_audio.audio_fallback_from_native
            && expected_audio
                .audio_sample_rate_hz
                .is_none_or(|rate| report.audio_sample_rate_hz == rate);
        if !audio_matches {
            error!(
                "Performance case audio mismatch: expected backend={} requested_mode={} fallback={} rate={:?}; got backend={} requested_mode={} fallback={} rate={}",
                expected_audio.audio_backend,
                expected_audio.audio_output_mode,
                expected_audio.audio_fallback_from_native,
                expected_audio.audio_sample_rate_hz,
                report.audio_backend,
                report.audio_requested_output_mode,
                report.audio_fallback_from_native,
                report.audio_sample_rate_hz
            );
            event_loop.exit();
            return;
        }
        let artifact = LiveCaseArtifact {
            schema: crate::live_case::LIVE_CASE_SCHEMA,
            case_name: &runtime.spec.name,
            manifest: runtime.spec.manifest_path.display().to_string(),
            manifest_sha256: &runtime.spec.manifest_sha256,
            data_dir: runtime.spec.data_dir.display().to_string(),
            config: config_path.display().to_string(),
            config_sha256,
            expected_runtime: &runtime.spec.expected_runtime,
            display,
            audio_devices: self
                .audio
                .startup_output_devices()
                .iter()
                .enumerate()
                .map(|(index, device)| AudioDevice {
                    index,
                    name: device.name.clone(),
                    is_default: device.is_default,
                    sample_rates_hz: device.sample_rates_hz.clone(),
                })
                .collect(),
            build: ArtifactBuild {
                version: runtime.spec.build.version,
                hash: runtime.spec.build.hash,
                stamp: runtime.spec.build.stamp,
                executable: executable.display().to_string(),
                executable_sha256,
                os: std::env::consts::OS,
                arch: std::env::consts::ARCH,
            },
            play_style: format!("{:?}", runtime.spec.play_style),
            player_side: format!("{:?}", runtime.spec.player_side),
            joined: runtime.spec.joined,
            music_rate: runtime.spec.music_rate,
            autoplay: runtime.spec.autoplay,
            warmup_frames: runtime.spec.warmup_frames,
            measured_frames: runtime.spec.measured_frames,
            resolved,
            pacing: report,
        };
        let path = runtime.spec.artifact_dir.join("result.json");
        let result = serde_json::to_vec_pretty(&artifact)
            .map_err(|error| format!("cannot encode case artifact: {error}"))
            .and_then(|bytes| {
                std::fs::write(&path, bytes)
                    .map_err(|error| format!("cannot write '{}': {error}", path.display()))
            });
        match result {
            Ok(()) => info!(
                "Performance case '{}' complete: {} frames -> '{}'",
                runtime.spec.name,
                artifact.pacing.frames,
                path.display()
            ),
            Err(error) => error!("Performance case failed to write artifact: {error}"),
        }
        event_loop.exit();
    }
}

fn runtime_display(window: &winit::window::Window) -> Result<RuntimeDisplay, String> {
    let window_size = window.inner_size();
    let monitor = window
        .current_monitor()
        .ok_or("performance case window is not attached to a monitor")?;
    let monitor_size = monitor.size();
    let monitor_position = monitor.position();
    let refresh_millihertz = match window.fullscreen() {
        Some(winit::window::Fullscreen::Exclusive(mode)) => mode.refresh_rate_millihertz(),
        _ => monitor
            .refresh_rate_millihertz()
            .ok_or("performance case monitor did not report a refresh rate")?,
    };
    Ok(RuntimeDisplay {
        name: monitor
            .name()
            .ok_or("performance case monitor did not report a name")?,
        window_width: window_size.width,
        window_height: window_size.height,
        monitor_width: monitor_size.width,
        monitor_height: monitor_size.height,
        monitor_x: monitor_position.x,
        monitor_y: monitor_position.y,
        scale_factor: window.scale_factor(),
        refresh_millihertz,
    })
}
