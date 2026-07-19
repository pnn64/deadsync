use super::App;
use crate::command::{
    BannerSlot, Command, DeferredCommandEffect, DeferredCommandResourceContext,
    execute_command_resources, log_command_timing_for_screen,
};
use deadsync_config::prelude as config;
use deadsync_theme_simply_love::views::SimplyLoveDensityGraphSlot as DensityGraphSlot;
use log::warn;
use std::error::Error;
use winit::event_loop::ActiveEventLoop;

const WHITE_GRAPH_KEY: &str = "__white";

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
        let context = self.deferred_command_context();
        let result = execute_command_resources(
            command,
            &mut self.state.session,
            &mut self.dynamic_media,
            &mut self.asset_manager,
            self.backend.as_mut(),
            context,
        );
        self.apply_deferred_effect(result.effect, Some(event_loop));
        log_command_timing_for_screen(result.timing, self.state.screens.current_screen);
        Ok(())
    }

    fn deferred_command_context(&self) -> DeferredCommandResourceContext {
        DeferredCommandResourceContext {
            current_screen: self.state.screens.current_screen,
            select_music_color_index: self.state.screens.select_music_state.active_color_index,
            select_course_color_index: self.state.screens.select_course_state.active_color_index,
            video_started_at_sec: self.background_video_started_at_sec(),
            show_video_backgrounds: config::get().show_video_backgrounds,
            wide_screen: deadlib_present::space::is_wide(),
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

    fn apply_deferred_effect(
        &mut self,
        effect: DeferredCommandEffect,
        event_loop: Option<&ActiveEventLoop>,
    ) {
        match effect {
            DeferredCommandEffect::None => {}
            DeferredCommandEffect::ExitNow => {
                if let Some(event_loop) = event_loop {
                    event_loop.exit();
                }
            }
            DeferredCommandEffect::Shutdown => {
                if let Err(e) = deadlib_platform::power::shutdown_host() {
                    warn!("host shutdown failed; exiting application only: {e}");
                }
                if let Some(event_loop) = event_loop {
                    event_loop.exit();
                }
            }
            DeferredCommandEffect::Banner { slot, key } => match slot {
                BannerSlot::SelectCourse => {
                    self.state.screens.select_course_state.current_banner_key = key;
                }
                BannerSlot::SelectMusic => {
                    self.state.screens.select_music_state.current_banner_key = key;
                }
            },
            DeferredCommandEffect::CdTitle(key) => {
                self.state.screens.select_music_state.current_cdtitle_key = key;
            }
            DeferredCommandEffect::DensityGraph { slot, mesh } => match slot {
                DensityGraphSlot::SelectMusicP1 => {
                    self.state.screens.select_music_state.current_graph_mesh = mesh;
                    self.state.screens.select_music_state.current_graph_key =
                        WHITE_GRAPH_KEY.to_string();
                }
                DensityGraphSlot::SelectMusicP2 => {
                    self.state.screens.select_music_state.current_graph_mesh_p2 = mesh;
                    self.state.screens.select_music_state.current_graph_key_p2 =
                        WHITE_GRAPH_KEY.to_string();
                }
            },
            DeferredCommandEffect::DynamicBackground(media) => {
                if let Some(gs) = &mut self.state.screens.gameplay_state {
                    let was_dirty = gs.background_path_dirty;
                    gs.current_background_path = media.path.clone();
                    gs.current_background_key = media.path_key.clone();
                    gs.background_allow_video = media.allow_video;
                    gs.background_path_dirty = was_dirty;
                    gs.background_texture_key = media.texture_key.clone();
                }
                if let Some(ps) = &mut self.state.screens.practice_state {
                    let was_dirty = ps.gameplay.background_path_dirty;
                    ps.gameplay.current_background_path = media.path;
                    ps.gameplay.current_background_key = media.path_key;
                    ps.gameplay.background_allow_video = media.allow_video;
                    ps.gameplay.background_path_dirty = was_dirty;
                    ps.gameplay.background_texture_key = media.texture_key;
                }
            }
        }
    }
}
