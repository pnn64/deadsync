use super::App;
use crate::config::{self, DisplayMode};
use crate::screens::{options, select_music};
use deadlib_render::{BackendType, PresentModePolicy};
use deadsync_shell::GraphicsRuntimeUpdate as RuntimeUpdate;
use deadsync_shell::{
    GraphicsChangeContext, GraphicsDisplaySync, GraphicsRuntimeSettings, GraphicsWindowPlan,
    RendererStartupSettings, RendererSwitchRequest, RendererSwitchResourceResetPlan,
    apply_graphics_runtime_settings, apply_recreate_display_change,
    apply_renderer_switch_restore_display, apply_runtime_display_mode, apply_runtime_resolution,
    available_monitor_specs, begin_renderer_switch, graphics_change_context, graphics_change_plan,
    graphics_runtime_updates, recreate_display_sync, refresh_present_config,
    renderer_startup_config, renderer_switch_begin_plan, renderer_switch_failure_plan,
    renderer_switch_plan, renderer_switch_resource_reset_plan, renderer_switch_success_plan,
    restore_display_sync, runtime_display_mode_sync, start_renderer_runtime, startup_display_sync,
    sync_renderer_window_size,
};
use log::{debug, error, info};
use std::error::Error;
use std::time::Instant;
use winit::{dpi::PhysicalSize, event_loop::ActiveEventLoop};

impl App {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_graphics_change(
        &mut self,
        renderer: Option<BackendType>,
        display_mode: Option<DisplayMode>,
        resolution: Option<(u32, u32)>,
        monitor: Option<usize>,
        vsync: Option<bool>,
        present_mode_policy: Option<PresentModePolicy>,
        max_fps: Option<u16>,
        high_dpi: Option<bool>,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        // Ensure options menu reflects current hardware state before processing changes.
        self.update_options_monitor_specs(event_loop);

        let applied = apply_graphics_runtime_settings(
            &mut self.state.shell,
            GraphicsRuntimeSettings {
                renderer,
                display_mode,
                resolution,
                monitor_requested: monitor.is_some(),
                vsync,
                present_mode_policy,
                max_fps,
                high_dpi,
            },
        );
        self.apply_graphics_runtime_updates(graphics_runtime_updates(&applied));

        let plan = graphics_change_plan(
            applied.request,
            self.graphics_change_context(event_loop, monitor),
        );

        match plan.window {
            GraphicsWindowPlan::Recreate {
                renderer,
                resolution,
                force_recreate,
                display,
            } => {
                if let Some(display) = display {
                    // Avoid touching the old window: create the replacement directly in
                    // the requested display mode and on the chosen monitor.
                    let display = apply_recreate_display_change(&mut self.state.shell, display);
                    self.apply_graphics_display_sync(recreate_display_sync(display));
                }
                self.switch_renderer(renderer, resolution, event_loop, force_recreate)?;
            }
            GraphicsWindowPlan::Reconfigure {
                display,
                resolution,
            } => {
                if let Some(display) = display {
                    self.apply_display_mode(display.mode, Some(display.monitor), event_loop)?;
                }
                if let Some((width, height)) = resolution {
                    self.apply_resolution(width, height, event_loop)?;
                }
            }
        }

        refresh_present_config(
            &mut self.backend,
            &self.state.shell,
            plan.refresh_present_config,
        );
        Ok(())
    }

    pub(super) fn init_graphics(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        // Collect monitors and update options immediately so the initial menu state is correct.
        self.update_options_monitor_specs(event_loop);

        let runtime_config = config::get();
        let startup_config = renderer_startup_config(
            &mut self.state.shell,
            RendererStartupSettings {
                backend_type: self.backend_type,
                high_dpi: runtime_config.high_dpi,
                fallback_fullscreen_type: runtime_config.fullscreen_type,
                hide_cursor: runtime_config.hide_mouse_cursor,
                gfx_debug_enabled: self.gfx_debug_enabled,
                software_renderer_threads: self.software_renderer_threads,
            },
        );
        let startup = start_renderer_runtime(
            &mut self.state.shell,
            event_loop,
            startup_config,
            &mut self.asset_manager,
            &mut self.dynamic_media,
            Instant::now(),
        )?;
        self.apply_graphics_display_sync(startup_display_sync(
            &self.state.shell,
            startup.fullscreen_type,
            startup.monitor_count,
        ));
        let window = startup.window;
        let backend = startup.backend;
        // Text layout cache entries borrow glyph texture keys from font storage.
        // Renderer reinit reloads fonts, so cached layouts must be dropped before compose.
        self.ui_text_layout_cache.clear();
        self.gameplay_text_layout_cache.clear();

        window.set_visible(true);
        // Seed window focus from the OS now that the window is visible. If the
        // game launched into the background, `has_focus()` returns false and the
        // raw input backends keep dropping global keystrokes. If it launched
        // focused, this propagates true through `apply_window_focus_change` and
        // wakes the rest of the input pipeline.
        let focused_now = window.has_focus();
        self.apply_window_focus_change(focused_now, Instant::now(), Some(&window));
        self.request_redraw(&window, "init_graphics");

        self.window = Some(window);
        self.backend = Some(backend);
        info!("Starting event loop...");
        Ok(())
    }

    pub(super) fn switch_renderer(
        &mut self,
        target: BackendType,
        desired_size: Option<(u32, u32)>,
        event_loop: &ActiveEventLoop,
        force_recreate: bool,
    ) -> Result<(), Box<dyn Error>> {
        let Some(plan) = renderer_switch_plan(
            &self.state.shell,
            RendererSwitchRequest {
                current: self.backend_type,
                target,
                force_recreate,
            },
            config::get().high_dpi,
            desired_size,
        ) else {
            return Ok(());
        };

        let previous_backend = plan.previous;
        begin_renderer_switch(
            &mut self.state.shell,
            self.window.as_deref(),
            &mut self.backend,
            &mut self.asset_manager,
            &mut self.dynamic_media,
            plan.window_config,
            Instant::now(),
        );
        let begin = renderer_switch_begin_plan(&plan);
        if begin.clear_window {
            self.window = None;
        }
        if begin.clear_focus {
            // The window is gone; mark unfocused so the global raw-input backends
            // stop forwarding keystrokes during the tear-down/init gap. The new
            // window's focus will be seeded by `init_graphics` below.
            self.apply_window_focus_change(false, Instant::now(), None);
        }
        self.backend_type = begin.target;

        match self.init_graphics(event_loop) {
            Ok(()) => {
                let success = renderer_switch_success_plan(plan.target);
                if success.persist_renderer {
                    config::update_video_renderer(success.target);
                }
                if success.sync_options_renderer {
                    options::sync_video_renderer(
                        &mut self.state.screens.options_state,
                        success.target,
                    );
                }
                if success.clear_present_runtime {
                    deadlib_present::runtime::clear_all();
                }
                if success.reset_dynamic_assets {
                    self.reset_dynamic_assets_after_renderer_switch(
                        renderer_switch_resource_reset_plan(),
                        event_loop,
                    )?;
                }
                if success.request_redraw
                    && let Some(window) = self.window.clone()
                {
                    self.request_redraw(&window, "switch_renderer");
                }
                info!("Switched renderer to {:?}", success.target);
                Ok(())
            }
            Err(error) => {
                error!("Failed to switch renderer to {:?}: {error}", plan.target);
                let failure = renderer_switch_failure_plan(previous_backend);
                self.backend_type = failure.previous;
                if let Err(restoration_err) = self.init_graphics(event_loop) {
                    error!(
                        "Failed to restore previous renderer {:?}: {restoration_err}",
                        failure.previous
                    );
                }
                if failure.sync_options_renderer {
                    options::sync_video_renderer(
                        &mut self.state.screens.options_state,
                        failure.previous,
                    );
                }
                if failure.restore_display {
                    let display = apply_renderer_switch_restore_display(
                        &mut self.state.shell,
                        event_loop,
                        config::get().fullscreen_type,
                    );
                    self.apply_graphics_display_sync(restore_display_sync(display));
                }
                if failure.persist_renderer {
                    config::update_video_renderer(failure.previous);
                }
                Err(error)
            }
        }
    }

    pub(super) fn sync_window_size(&mut self, size: PhysicalSize<u32>) {
        sync_renderer_window_size(
            &mut self.state.shell,
            self.window.as_deref(),
            &mut self.backend,
            self.backend_type,
            config::get().high_dpi,
            size,
        );
    }

    pub(super) fn apply_display_mode(
        &mut self,
        mode: DisplayMode,
        monitor_override: Option<usize>,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let runtime_config = config::get();
        let result = apply_runtime_display_mode(
            &mut self.state.shell,
            self.window.as_deref(),
            &mut self.backend,
            event_loop,
            self.backend_type,
            runtime_config.high_dpi,
            mode,
            monitor_override,
            runtime_config.fullscreen_type,
        );
        self.apply_graphics_display_sync(runtime_display_mode_sync(
            mode,
            self.state.shell.display_monitor,
            &result,
        ));
        Ok(())
    }

    pub(super) fn apply_resolution(
        &mut self,
        width: u32,
        height: u32,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        apply_runtime_resolution(
            &mut self.state.shell,
            self.window.as_deref(),
            &mut self.backend,
            event_loop,
            self.backend_type,
            config::get().high_dpi,
            width,
            height,
        );
        Ok(())
    }

    fn reset_dynamic_assets_after_renderer_switch(
        &mut self,
        reset: RendererSwitchResourceResetPlan,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        self.run_commands(reset.commands, event_loop)?;

        if reset.refresh_select_music {
            select_music::trigger_immediate_refresh(&mut self.state.screens.select_music_state);
        }
        self.state.screens.select_music_state.current_graph_key = reset.graph_key.to_string();
        self.state.screens.select_music_state.current_graph_key_p2 = reset.graph_key.to_string();
        Ok(())
    }

    pub(super) fn update_options_monitor_specs(&mut self, event_loop: &ActiveEventLoop) {
        options::update_monitor_specs(
            &mut self.state.screens.options_state,
            available_monitor_specs(event_loop),
        );
    }

    fn graphics_change_context(
        &self,
        event_loop: &ActiveEventLoop,
        monitor_override: Option<usize>,
    ) -> GraphicsChangeContext {
        graphics_change_context(
            &self.state.shell,
            self.backend_type,
            config::get().fullscreen_type,
            event_loop,
            monitor_override,
        )
    }

    fn apply_graphics_runtime_updates(&mut self, updates: Vec<RuntimeUpdate>) {
        for update in updates {
            match update {
                RuntimeUpdate::Vsync(vsync) => {
                    debug!("Graphics setting changed: vsync={vsync}");
                    config::update_vsync(vsync);
                    options::sync_vsync(&mut self.state.screens.options_state, vsync);
                }
                RuntimeUpdate::MaxFps(max_fps) => {
                    debug!("Graphics setting changed: max_fps={max_fps}");
                    config::update_max_fps(max_fps);
                    options::sync_max_fps(&mut self.state.screens.options_state, max_fps);
                }
                RuntimeUpdate::PresentModePolicy(policy) => {
                    debug!("Graphics setting changed: present_mode_policy={policy}");
                    config::update_present_mode_policy(policy);
                    options::sync_present_mode_policy(
                        &mut self.state.screens.options_state,
                        policy,
                    );
                }
                RuntimeUpdate::HighDpi(enabled) => {
                    debug!("Graphics setting changed: high_dpi={enabled}");
                    config::update_high_dpi(enabled);
                    options::sync_high_dpi(&mut self.state.screens.options_state, enabled);
                }
                RuntimeUpdate::Resolution(w, h) => {
                    config::update_display_resolution(w, h);
                    options::sync_display_resolution(&mut self.state.screens.options_state, w, h);
                }
            }
        }
    }

    fn apply_graphics_display_sync(&mut self, sync: GraphicsDisplaySync) {
        if sync.persist_mode {
            config::update_display_mode(sync.mode);
        }
        if sync.persist_monitor {
            config::update_display_monitor(sync.monitor);
        }
        options::sync_display_mode(
            &mut self.state.screens.options_state,
            sync.mode,
            sync.fullscreen_type,
            sync.monitor,
            sync.monitor_count,
        );
    }
}
