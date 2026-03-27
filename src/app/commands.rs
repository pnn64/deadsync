use super::{App, CurrentScreen};
use crate::game::{profile, scores, scroll::ScrollSpeedSetting};
use crate::screens::components::shared::density_graph::{DensityGraphSlot, DensityGraphSource};
use log::{debug, warn};
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::event_loop::ActiveEventLoop;

/// Imperative effects to be executed by the shell.
pub(super) enum Command {
    ExitNow,
    SetBanner(Option<PathBuf>),
    SetCdTitle(Option<PathBuf>),
    SetPackBanner(Option<PathBuf>),
    SetDensityGraph {
        slot: DensityGraphSlot,
        chart_opt: Option<DensityGraphSource>,
    },
    FetchOnlineGrade(String),
    PlayMusic {
        path: PathBuf,
        looped: bool,
        volume: f32,
    },
    StopMusic,
    SetDynamicBackground(Option<PathBuf>),
    UpdateScrollSpeed {
        side: profile::PlayerSide,
        setting: ScrollSpeedSetting,
    },
    UpdateSessionMusicRate(f32),
    UpdatePreferredDifficulty(usize),
    UpdateLastPlayed {
        side: profile::PlayerSide,
        play_style: profile::PlayStyle,
        music_path: Option<PathBuf>,
        chart_hash: Option<String>,
        difficulty_index: usize,
    },
}

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

    #[inline(always)]
    const fn should_log_command_timing(command: &Command) -> bool {
        matches!(
            command,
            Command::SetBanner(_)
                | Command::SetCdTitle(_)
                | Command::SetPackBanner(_)
                | Command::SetDensityGraph { .. }
                | Command::SetDynamicBackground(_)
                | Command::PlayMusic { .. }
        )
    }

    #[inline(always)]
    const fn command_label(command: &Command) -> &'static str {
        match command {
            Command::ExitNow => "ExitNow",
            Command::SetBanner(_) => "SetBanner",
            Command::SetCdTitle(_) => "SetCdTitle",
            Command::SetPackBanner(_) => "SetPackBanner",
            Command::SetDensityGraph { .. } => "SetDensityGraph",
            Command::FetchOnlineGrade(_) => "FetchOnlineGrade",
            Command::PlayMusic { .. } => "PlayMusic",
            Command::StopMusic => "StopMusic",
            Command::SetDynamicBackground(_) => "SetDynamicBackground",
            Command::UpdateScrollSpeed { .. } => "UpdateScrollSpeed",
            Command::UpdateSessionMusicRate(_) => "UpdateSessionMusicRate",
            Command::UpdatePreferredDifficulty(_) => "UpdatePreferredDifficulty",
            Command::UpdateLastPlayed { .. } => "UpdateLastPlayed",
        }
    }

    fn execute_command(
        &mut self,
        command: Command,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let label = Self::command_label(&command);
        let always_log_timing = Self::should_log_command_timing(&command);
        let started = Instant::now();
        match command {
            Command::ExitNow => {
                event_loop.exit();
            }
            Command::SetBanner(path_opt) => self.apply_banner(path_opt),
            Command::SetCdTitle(path_opt) => self.apply_cdtitle(path_opt),
            Command::SetPackBanner(path_opt) => self.apply_pack_banner(path_opt),
            Command::SetDensityGraph { slot, chart_opt } => {
                self.apply_density_graph(slot, chart_opt)
            }
            Command::FetchOnlineGrade(hash) => self.spawn_grade_fetch(hash),
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
                crate::game::profile::set_session_music_rate(rate);
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
        if elapsed_ms >= 100.0 {
            warn!(
                "Slow command: {} took {:.2}ms on screen {:?}",
                label, elapsed_ms, self.state.screens.current_screen
            );
        } else if elapsed_ms >= 16.7 {
            debug!(
                "Frame-cost command: {} took {:.2}ms on screen {:?}",
                label, elapsed_ms, self.state.screens.current_screen
            );
        } else if always_log_timing {
            debug!(
                "Command timing: {} took {:.2}ms on screen {:?}",
                label, elapsed_ms, self.state.screens.current_screen
            );
        }
        Ok(())
    }

    pub(super) fn apply_banner(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            if let Some(path) = path_opt {
                let key =
                    self.dynamic_media
                        .set_banner(&mut self.asset_manager, backend, Some(path));
                match self.state.screens.current_screen {
                    CurrentScreen::SelectCourse => {
                        self.state.screens.select_course_state.current_banner_key = key;
                    }
                    _ => {
                        self.state.screens.select_music_state.current_banner_key = key;
                    }
                }
            } else {
                self.dynamic_media
                    .destroy_banner(&mut self.asset_manager, backend);
                let color_index = match self.state.screens.current_screen {
                    CurrentScreen::SelectCourse => {
                        self.state.screens.select_course_state.active_color_index
                    }
                    _ => self.state.screens.select_music_state.active_color_index,
                };
                let banner_num = color_index.rem_euclid(12) + 1;
                let key = format!("banner{banner_num}.png");
                match self.state.screens.current_screen {
                    CurrentScreen::SelectCourse => {
                        self.state.screens.select_course_state.current_banner_key = key;
                    }
                    _ => {
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

    pub(super) fn apply_density_graph(
        &mut self,
        slot: DensityGraphSlot,
        chart_opt: Option<DensityGraphSource>,
    ) {
        let (graph_w, graph_h) = if crate::engine::space::is_wide() {
            (286.0_f32, 64.0_f32)
        } else {
            (276.0_f32, 64.0_f32)
        };
        let mesh = chart_opt.and_then(|chart| {
            let verts =
                crate::screens::components::shared::density_graph::build_density_histogram_mesh(
                    &chart.measure_nps_vec,
                    chart.max_nps,
                    &chart.measure_seconds_vec,
                    chart.first_second,
                    chart.last_second,
                    graph_w,
                    graph_h,
                    0.0,
                    graph_w,
                    None,
                    1.0,
                );
            if verts.is_empty() {
                None
            } else {
                Some(Arc::from(verts.into_boxed_slice()))
            }
        });

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

    fn spawn_grade_fetch(&self, hash: String) {
        debug!("Fetching online grade for chart hash: {hash}");
        let mut spawned = 0;
        for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
            if !profile::is_session_side_joined(side) {
                continue;
            }
            let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
                continue;
            };
            let profile = profile::get_for_side(side);
            if profile.groovestats_api_key.is_empty() || profile.groovestats_username.is_empty() {
                continue;
            }

            spawned += 1;
            let hash = hash.clone();
            std::thread::spawn(move || {
                if let Err(e) = scores::fetch_and_store_grade(profile_id, profile, hash) {
                    warn!("Failed to fetch online grade: {e}");
                }
            });
        }
        if spawned == 0 {
            warn!(
                "Skipping GrooveStats grade fetch: no joined local profile with GrooveStats configured"
            );
        }
    }

    fn play_music_command(&self, path: PathBuf, looped: bool, volume: f32) {
        crate::engine::audio::play_music(
            path,
            crate::engine::audio::Cut::default(),
            looped,
            volume,
        );
    }

    fn stop_music_command(&self) {
        crate::engine::audio::stop_music();
    }

    pub(super) fn apply_dynamic_background(&mut self, path_opt: Option<PathBuf>) {
        if let Some(backend) = self.backend.as_mut() {
            let key = self.dynamic_media.set_background(
                &mut self.asset_manager,
                backend,
                path_opt.clone(),
            );
            if let Some(gs) = &mut self.state.screens.gameplay_state {
                gs.current_background_path = path_opt;
                gs.background_texture_key = key;
            }
        }
    }
}
