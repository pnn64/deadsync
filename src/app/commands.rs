use super::App;
use deadsync_profile::compat as profile;
use deadsync_screens::{DensityGraphSlot, DensityGraphSource};
use deadsync_shell::{
    BannerSlot, Command, CommandTimingLog, banner_slot, build_density_graph_mesh, command_label,
    command_timing_log, fallback_banner_key, spawn_online_grade_fetch,
};
use log::{debug, warn};
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::event_loop::ActiveEventLoop;

impl App {
    pub(super) fn run_commands(
        &mut self,
        commands: Vec<Command>,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        for command in commands {
            self.execute_command(command, event_loop)?;
        }
        Ok(())
    }

    fn execute_command(
        &mut self,
        command: Command,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let kind = command.kind();
        let label = command_label(kind);
        let started = Instant::now();
        match command {
            Command::ExitNow => {
                event_loop.exit();
            }
            Command::Shutdown => {
                if let Err(e) = deadlib_platform::power::shutdown_host() {
                    warn!("host shutdown failed; exiting application only: {e}");
                }
                event_loop.exit();
            }
            Command::SetBanner(path_opt) => self.apply_banner(path_opt),
            Command::SetCdTitle(path_opt) => self.apply_cdtitle(path_opt),
            Command::SetPackBanner(path_opt) => self.apply_pack_banner(path_opt),
            Command::SetWheelItemBackgrounds(paths) => self.apply_wheel_item_backgrounds(paths),
            Command::SetDensityGraph { slot, chart_opt } => {
                self.apply_density_graph(slot, chart_opt)
            }
            Command::FetchOnlineGrade(hash) => spawn_online_grade_fetch(hash),
            Command::PlayMusic {
                path,
                looped,
                volume,
            } => self.play_music_command(path, looped, volume),
            Command::StopMusic => self.stop_music_command(),
            Command::SetDynamicBackground(path_opt) => self.apply_dynamic_background(path_opt),
            Command::UpdateScrollSpeed { side, setting } => {
                profile::update_scroll_speed_for_side(side, setting);
            }
            Command::UpdateSessionMusicRate(rate) => {
                deadsync_profile::compat::set_session_music_rate(rate);
            }
            Command::UpdatePreferredDifficulty(idx) => {
                self.state.session.preferred_difficulty_index = idx;
            }
            Command::UpdateLastPlayed {
                side,
                play_style,
                music_path,
                chart_hash,
                difficulty_index,
            } => {
                profile::update_last_played_for_side(
                    side,
                    play_style,
                    music_path.as_deref(),
                    chart_hash.as_deref(),
                    difficulty_index,
                );
            }
        }
        let elapsed = started.elapsed();
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
        match command_timing_log(kind, elapsed_ms) {
            CommandTimingLog::Slow => {
                warn!(
                    "Slow command: {} took {:.2}ms on screen {:?}",
                    label, elapsed_ms, self.state.screens.current_screen
                );
            }
            CommandTimingLog::FrameCost => {
                debug!(
                    "Frame-cost command: {} took {:.2}ms on screen {:?}",
                    label, elapsed_ms, self.state.screens.current_screen
                );
            }
            CommandTimingLog::CommandTiming => {
                debug!(
                    "Command timing: {} took {:.2}ms on screen {:?}",
                    label, elapsed_ms, self.state.screens.current_screen
                );
            }
            CommandTimingLog::None => {}
        }
        Ok(())
    }

    pub(super) fn apply_banner(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            let slot = banner_slot(self.state.screens.current_screen);
            if let Some(path) = path_opt {
                let key =
                    self.dynamic_media
                        .set_banner(&mut self.asset_manager, backend, Some(path));
                match slot {
                    BannerSlot::SelectCourse => {
                        self.state.screens.select_course_state.current_banner_key = key;
                    }
                    BannerSlot::SelectMusic => {
                        self.state.screens.select_music_state.current_banner_key = key;
                    }
                }
            } else {
                self.dynamic_media
                    .destroy_banner(&mut self.asset_manager, backend);
                let color_index = match slot {
                    BannerSlot::SelectCourse => {
                        self.state.screens.select_course_state.active_color_index
                    }
                    BannerSlot::SelectMusic => {
                        self.state.screens.select_music_state.active_color_index
                    }
                };
                let key = fallback_banner_key(color_index);
                match slot {
                    BannerSlot::SelectCourse => {
                        self.state.screens.select_course_state.current_banner_key = key;
                    }
                    BannerSlot::SelectMusic => {
                        self.state.screens.select_music_state.current_banner_key = key;
                    }
                }
            }
        }
    }

    pub(super) fn apply_cdtitle(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            self.state.screens.select_music_state.current_cdtitle_key = self
                .dynamic_media
                .set_cdtitle(&mut self.asset_manager, backend, path_opt);
        }
    }

    fn apply_pack_banner(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            self.dynamic_media
                .set_pack_banner(&mut self.asset_manager, backend, path_opt);
        }
    }

    fn apply_wheel_item_backgrounds(&mut self, paths: Vec<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            self.dynamic_media
                .set_wheel_item_backgrounds(&mut self.asset_manager, backend, paths);
        }
    }

    pub(super) fn apply_density_graph(
        &mut self,
        slot: DensityGraphSlot,
        chart_opt: Option<DensityGraphSource>,
    ) {
        let mesh = build_density_graph_mesh(chart_opt, deadlib_present::space::is_wide());

        match slot {
            DensityGraphSlot::SelectMusicP1 => {
                self.state.screens.select_music_state.current_graph_mesh = mesh;
                self.state.screens.select_music_state.current_graph_key = "__white".to_string();
            }
            DensityGraphSlot::SelectMusicP2 => {
                self.state.screens.select_music_state.current_graph_mesh_p2 = mesh;
                self.state.screens.select_music_state.current_graph_key_p2 = "__white".to_string();
            }
        }
    }

    fn play_music_command(&self, path: PathBuf, looped: bool, volume: f32) {
        deadsync_audio_stream::play_music(
            path,
            deadsync_audio_stream::Cut::default(),
            looped,
            volume,
        );
    }

    fn stop_music_command(&self) {
        deadsync_audio_stream::stop_music();
    }

    pub(super) fn apply_dynamic_background(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            let video_started_at_sec = self
                .state
                .screens
                .gameplay_state
                .as_ref()
                .map(|state| {
                    deadsync_core::song_time::song_time_ns_to_seconds(state.current_music_time_ns())
                })
                .or_else(|| {
                    self.state.screens.practice_state.as_ref().map(|state| {
                        deadsync_core::song_time::song_time_ns_to_seconds(
                            state.gameplay.current_music_time_ns(),
                        )
                    })
                })
                .unwrap_or(0.0);
            let show_video_backgrounds = crate::config::get().show_video_backgrounds;
            let key = self.dynamic_media.set_background(
                &mut self.asset_manager,
                backend,
                path_opt.clone(),
                video_started_at_sec,
                show_video_backgrounds,
            );
            let key = Arc::<str>::from(key);
            let path_key = path_opt.as_deref().map(crate::assets::media_path_key);
            if let Some(gs) = &mut self.state.screens.gameplay_state {
                let was_dirty = gs.background_path_dirty;
                gs.current_background_path = path_opt.clone();
                gs.current_background_key = path_key.clone();
                gs.background_allow_video = show_video_backgrounds;
                gs.background_path_dirty = was_dirty;
                gs.background_texture_key = key.clone();
            }
            if let Some(ps) = &mut self.state.screens.practice_state {
                let was_dirty = ps.gameplay.background_path_dirty;
                ps.gameplay.current_background_path = path_opt;
                ps.gameplay.current_background_key = path_key;
                ps.gameplay.background_allow_video = show_video_backgrounds;
                ps.gameplay.background_path_dirty = was_dirty;
                ps.gameplay.background_texture_key = key;
            }
        }
    }
}
