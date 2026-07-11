use super::App;
use crate::command::{
    BannerSlot, Command, DeferredCommandEffect, DeferredCommandResourceContext,
    DeferredCommandRootEffect, apply_deferred_command_process_plan, deferred_command_apply_plan,
    execute_command_resources, log_command_timing_for_screen,
};
use deadsync_config::prelude as config;
use deadsync_screens::DensityGraphSlot;
use std::error::Error;
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
        let plan = deferred_command_apply_plan(effect);
        if apply_deferred_command_process_plan(plan.process) {
            if let Some(event_loop) = event_loop {
                event_loop.exit();
            }
        }
        match plan.root_effect {
            DeferredCommandRootEffect::None => {}
            DeferredCommandRootEffect::Banner { slot, key } => match slot {
                BannerSlot::SelectCourse => {
                    self.state.screens.select_course_state.current_banner_key = key;
                }
                BannerSlot::SelectMusic => {
                    self.state.screens.select_music_state.current_banner_key = key;
                }
            },
            DeferredCommandRootEffect::CdTitle(key) => {
                self.state.screens.select_music_state.current_cdtitle_key = key;
            }
            DeferredCommandRootEffect::DensityGraph {
                slot,
                mesh,
                graph_key,
            } => match slot {
                DensityGraphSlot::SelectMusicP1 => {
                    self.state.screens.select_music_state.current_graph_mesh = mesh;
                    self.state.screens.select_music_state.current_graph_key = graph_key.to_string();
                }
                DensityGraphSlot::SelectMusicP2 => {
                    self.state.screens.select_music_state.current_graph_mesh_p2 = mesh;
                    self.state.screens.select_music_state.current_graph_key_p2 =
                        graph_key.to_string();
                }
            },
            DeferredCommandRootEffect::DynamicBackground {
                media,
                update_gameplay,
                update_practice,
                preserve_dirty,
            } => {
                if update_gameplay && let Some(gs) = &mut self.state.screens.gameplay_state {
                    let was_dirty = gs.background_path_dirty;
                    gs.current_background_path = media.path.clone();
                    gs.current_background_key = media.path_key.clone();
                    gs.background_allow_video = media.allow_video;
                    if preserve_dirty {
                        gs.background_path_dirty = was_dirty;
                    }
                    gs.background_texture_key = media.texture_key.clone();
                }
                if update_practice && let Some(ps) = &mut self.state.screens.practice_state {
                    let was_dirty = ps.gameplay.background_path_dirty;
                    ps.gameplay.current_background_path = media.path;
                    ps.gameplay.current_background_key = media.path_key;
                    ps.gameplay.background_allow_video = media.allow_video;
                    if preserve_dirty {
                        ps.gameplay.background_path_dirty = was_dirty;
                    }
                    ps.gameplay.background_texture_key = media.texture_key;
                }
            }
        }
    }
}
