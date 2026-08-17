use super::App;
use crate::command::{
    Command, build_density_graph_mesh, command_timing_result, fallback_banner_key,
    log_command_timing_for_screen, spawn_online_grade_fetch,
};
use deadsync_assets::media_path_key;
use deadsync_config::prelude as config;
use deadsync_profile::compat as profile;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
use deadsync_theme_simply_love::views::SimplyLoveDensityGraphSlot as DensityGraphSlot;
use log::warn;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use winit::event_loop::ActiveEventLoop;

impl App {
    pub(super) fn run_commands(&mut self, commands: Vec<Command>, event_loop: &ActiveEventLoop) {
        for command in commands {
            self.execute_command(command, event_loop);
        }
    }

    pub(super) fn execute_command(&mut self, command: Command, event_loop: &ActiveEventLoop) {
        let kind = command.kind();
        let started = Instant::now();
        match command {
            Command::ExitNow => event_loop.exit(),
            Command::Shutdown => {
                if let Err(e) = deadlib_platform::power::shutdown_host() {
                    warn!("host shutdown failed; exiting application only: {e}");
                }
                event_loop.exit();
            }
            Command::SetBanner(path) => self.set_banner(path),
            Command::SetCdTitle(path) => self.set_cdtitle(path),
            Command::SetPackBanner(path) => {
                if let Some(backend) = self.backend.as_mut() {
                    self.dynamic_media
                        .set_pack_banner(&mut self.asset_manager, backend, path);
                }
            }
            Command::SetWheelItemBackgrounds(paths) => {
                if let Some(backend) = self.backend.as_mut() {
                    self.dynamic_media.set_wheel_item_backgrounds(
                        &mut self.asset_manager,
                        backend,
                        paths,
                    );
                }
            }
            Command::SetDensityGraph { slot, chart_opt } => {
                self.set_density_graph(slot, chart_opt);
            }
            Command::FetchOnlineGrade(hash) => spawn_online_grade_fetch(hash),
            Command::PlayMusic {
                path,
                looped,
                volume,
            } => self
                .audio
                .play_music(path, deadsync_audio_stream::Cut::default(), looped, volume),
            Command::StopMusic => self.audio.stop_music(),
            Command::SetDynamicBackground(path) => self.set_dynamic_background(path),
            Command::UpdateScrollSpeed { side, setting } => {
                profile::update_scroll_speed_for_side(side, setting);
            }
            Command::UpdateSessionMusicRate(rate) => profile::set_session_music_rate(rate),
            Command::UpdatePreferredDifficulty(index) => {
                self.state.session.preferred_difficulty_index = index;
            }
            Command::UpdateLastPlayed {
                side,
                play_style,
                music_path,
                chart_hash,
                difficulty_index,
            } => profile::update_last_played_for_side(
                side,
                play_style,
                music_path.as_deref(),
                chart_hash.as_deref(),
                difficulty_index,
            ),
        }
        let timing = command_timing_result(kind, started.elapsed().as_secs_f64() * 1000.0);
        log_command_timing_for_screen(timing, self.state.screens.current_screen);
    }

    fn set_banner(&mut self, path: Option<PathBuf>) {
        let course = self.state.screens.current_screen == Screen::SelectCourse;
        let color_index = if course {
            self.state.screens.select_course_state.active_color_index
        } else {
            self.state.screens.select_music_state.active_color_index
        };
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let key = if let Some(path) = path {
            self.dynamic_media
                .set_banner(&mut self.asset_manager, backend, Some(path))
        } else {
            self.dynamic_media
                .destroy_banner(&mut self.asset_manager, backend);
            Arc::<str>::from(fallback_banner_key(color_index))
        };
        if course {
            self.state.screens.select_course_state.current_banner_key = key;
        } else {
            self.state.screens.select_music_state.current_banner_key = key;
        }
    }

    fn set_cdtitle(&mut self, path: Option<PathBuf>) {
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        self.state.screens.select_music_state.current_cdtitle_key =
            self.dynamic_media
                .set_cdtitle(&mut self.asset_manager, backend, path);
    }

    fn set_density_graph(
        &mut self,
        slot: DensityGraphSlot,
        chart: Option<deadsync_theme::views::DensityGraphView>,
    ) {
        let mesh = build_density_graph_mesh(chart, deadlib_present::space::is_wide());
        match slot {
            DensityGraphSlot::SelectMusicP1 => {
                self.state.screens.select_music_state.current_graph_mesh = mesh;
            }
            DensityGraphSlot::SelectMusicP2 => {
                self.state.screens.select_music_state.current_graph_mesh_p2 = mesh;
            }
        }
    }

    fn set_dynamic_background(&mut self, path: Option<PathBuf>) {
        let started_at = self.background_video_started_at_sec();
        let allow_video = config::get().show_video_backgrounds;
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let texture_key = Arc::<str>::from(self.dynamic_media.set_background(
            &mut self.asset_manager,
            backend,
            path.clone(),
            started_at,
            allow_video,
        ));
        let path_key = path.as_deref().map(media_path_key);

        if let Some(state) = &mut self.state.screens.gameplay_state {
            let was_dirty = state.background_path_dirty;
            state.current_background_path = path.clone();
            state.current_background_key = path_key.clone();
            state.background_allow_video = allow_video;
            state.background_path_dirty = was_dirty;
            state.background_texture_key = texture_key.clone();
        }
        if let Some(state) = &mut self.state.screens.practice_state {
            let was_dirty = state.gameplay.background_path_dirty;
            state.gameplay.current_background_path = path;
            state.gameplay.current_background_key = path_key;
            state.gameplay.background_allow_video = allow_video;
            state.gameplay.background_path_dirty = was_dirty;
            state.gameplay.background_texture_key = texture_key;
        }
    }

    fn background_video_started_at_sec(&self) -> f32 {
        self.state
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
            .unwrap_or(0.0)
    }
}
