use super::App;
use crate::config::{self, DisplayMode};
use crate::screens::{options, select_music};
use deadlib_platform::display;
use deadlib_present::space;
use deadlib_render::BackendType;
use deadlib_renderer::{create_backend, render_size_for_physical, render_size_for_window};
use deadsync_screens::DensityGraphSlot;
use deadsync_shell::{
    AppWindowConfig, DisplayModeChange, ResolutionChange, apply_window_display_mode,
    apply_window_resolution, create_app_window,
};
use log::{error, info};
use std::error::Error;
use std::time::Instant;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event_loop::ActiveEventLoop,
    monitor::MonitorHandle,
};

impl App {
    pub(super) fn init_graphics(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        // Collect monitors and update options immediately so the initial menu state is correct.
        self.update_options_monitor_specs(event_loop);

        let window_width = self.state.shell.display_width;
        let window_height = self.state.shell.display_height;
        let runtime_config = config::get();
        let setup = create_app_window(
            event_loop,
            AppWindowConfig {
                backend_type: self.backend_type,
                high_dpi: runtime_config.high_dpi,
                width: window_width,
                height: window_height,
                monitor: self.state.shell.display_monitor,
                display_mode: self.state.shell.display_mode,
                fallback_fullscreen_type: runtime_config.fullscreen_type,
                hide_cursor: runtime_config.hide_mouse_cursor,
                pending_position: self.state.shell.pending_window_position.take(),
            },
        )?;
        self.state.shell.display_monitor = setup.monitor;
        options::sync_display_mode(
            &mut self.state.screens.options_state,
            self.state.shell.display_mode,
            setup.fullscreen_type,
            self.state.shell.display_monitor,
            setup.monitor_count,
        );
        let window = setup.window;
        let high_dpi = runtime_config.high_dpi;
        let sz = render_size_for_window(&window, self.backend_type, high_dpi);
        self.state.shell.metrics = space::metrics_for_window(sz.width, sz.height);
        space::set_current_metrics(self.state.shell.metrics);
        let mut backend = create_backend(
            self.backend_type,
            window.clone(),
            self.state.shell.vsync_enabled,
            self.state.shell.present_mode_policy,
            self.gfx_debug_enabled,
            high_dpi,
        )?;

        if self.backend_type == BackendType::Software {
            let threads = match self.software_renderer_threads {
                0 => None,
                n => Some(n as usize),
            };
            backend.configure_software_threads(threads);
        }

        self.asset_manager.load_initial_assets(&mut backend)?;
        self.dynamic_media
            .preload_profile_avatars(&mut self.asset_manager, &mut backend);
        // Text layout cache entries borrow glyph texture keys from font storage.
        // Renderer reinit reloads fonts, so cached layouts must be dropped before compose.
        self.ui_text_layout_cache.clear();
        self.gameplay_text_layout_cache.clear();

        let now = Instant::now();
        self.state.shell.start_time = now;
        self.state.shell.last_title_update = now;
        self.state.shell.reset_frame_clock(now);
        self.state.shell.frame_count = 0;
        self.state.shell.current_frame_vpf = 0;

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
        if target == self.backend_type && !force_recreate {
            return Ok(());
        }

        let previous_backend = self.backend_type;
        let mut old_window_pos: Option<PhysicalPosition<i32>> = None;
        if let Some((w, h)) = desired_size {
            self.state.shell.display_width = w;
            self.state.shell.display_height = h;
        }
        if let Some(window) = &self.window {
            if desired_size.is_none() {
                let sz = render_size_for_window(window, self.backend_type, config::get().high_dpi);
                self.state.shell.display_width = sz.width;
                self.state.shell.display_height = sz.height;
            }
            if matches!(self.state.shell.display_mode, DisplayMode::Fullscreen(_)) {
                window.set_fullscreen(None);
            }
            if matches!(self.state.shell.display_mode, DisplayMode::Windowed)
                && let Ok(pos) = window.outer_position()
            {
                old_window_pos = Some(pos);
            }
            window.set_visible(false);
        }

        if let Some(mut backend) = self.backend.take() {
            self.dynamic_media
                .destroy_assets(&mut self.asset_manager, &mut backend);
            let mut textures = self.asset_manager.take_textures();
            backend.dispose_textures(&mut textures);
            backend.cleanup();
        }
        self.backend = None;
        self.window = None;
        // The window is gone; mark unfocused so the global raw-input backends
        // stop forwarding keystrokes during the tear-down/init gap. The new
        // window's focus will be seeded by `init_graphics` below.
        self.apply_window_focus_change(false, Instant::now(), None);
        self.state.shell.pending_window_position = old_window_pos;

        self.backend_type = target;
        self.state.shell.frame_count = 0;
        let now = Instant::now();
        self.state.shell.last_title_update = now;
        self.state.shell.reset_frame_clock(now);

        match self.init_graphics(event_loop) {
            Ok(()) => {
                config::update_video_renderer(target);
                options::sync_video_renderer(&mut self.state.screens.options_state, target);
                deadlib_present::runtime::clear_all();
                self.reset_dynamic_assets_after_renderer_switch();
                if let Some(window) = self.window.clone() {
                    self.request_redraw(&window, "switch_renderer");
                }
                info!("Switched renderer to {target:?}");
                Ok(())
            }
            Err(error) => {
                error!("Failed to switch renderer to {target:?}: {error}");
                self.backend_type = previous_backend;
                if let Err(restoration_err) = self.init_graphics(event_loop) {
                    error!(
                        "Failed to restore previous renderer {previous_backend:?}: {restoration_err}"
                    );
                }
                options::sync_video_renderer(
                    &mut self.state.screens.options_state,
                    previous_backend,
                );
                let (_, monitor_count, monitor_idx) =
                    display::resolve_monitor(event_loop, self.state.shell.display_monitor);
                self.state.shell.display_monitor = monitor_idx;
                let fullscreen_type = match self.state.shell.display_mode {
                    DisplayMode::Fullscreen(ft) => ft,
                    DisplayMode::Windowed => config::get().fullscreen_type,
                };
                options::sync_display_mode(
                    &mut self.state.screens.options_state,
                    self.state.shell.display_mode,
                    fullscreen_type,
                    monitor_idx,
                    monitor_count,
                );
                self.state.shell.pending_window_position = None;
                config::update_video_renderer(previous_backend);
                Err(error)
            }
        }
    }

    pub(super) fn sync_window_size(&mut self, size: PhysicalSize<u32>) {
        let size = self.window.as_ref().map_or(size, |window| {
            render_size_for_physical(window, self.backend_type, config::get().high_dpi, size)
        });
        if size.width > 0 && size.height > 0 {
            self.state.shell.metrics = space::metrics_for_window(size.width, size.height);
            space::set_current_metrics(self.state.shell.metrics);
        }
        if let Some(backend) = &mut self.backend {
            backend.resize(size.width, size.height);
        }
    }

    pub(super) fn apply_display_mode(
        &mut self,
        mode: DisplayMode,
        monitor_override: Option<usize>,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let previous_mode = self.state.shell.display_mode;
        let runtime_config = config::get();
        let result = apply_window_display_mode(
            self.window.as_deref(),
            event_loop,
            DisplayModeChange {
                backend_type: self.backend_type,
                high_dpi: runtime_config.high_dpi,
                width: self.state.shell.display_width,
                height: self.state.shell.display_height,
                monitor: self.state.shell.display_monitor,
                monitor_override,
                previous_mode,
                mode,
                fallback_fullscreen_type: runtime_config.fullscreen_type,
                pending_position: self.state.shell.pending_window_position,
            },
        );
        self.state.shell.display_width = result.width;
        self.state.shell.display_height = result.height;
        self.state.shell.display_monitor = result.monitor;
        self.state.shell.pending_window_position = result.pending_position;
        if let Some(size) = result.immediate_size {
            self.sync_window_size(size);
        }
        self.state.shell.display_mode = mode;
        config::update_display_mode(mode);
        config::update_display_monitor(self.state.shell.display_monitor);
        options::sync_display_mode(
            &mut self.state.screens.options_state,
            mode,
            result.fullscreen_type,
            self.state.shell.display_monitor,
            result.monitor_count,
        );
        Ok(())
    }

    pub(super) fn apply_resolution(
        &mut self,
        width: u32,
        height: u32,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        self.state.shell.display_width = width;
        self.state.shell.display_height = height;
        let result = apply_window_resolution(
            self.window.as_deref(),
            event_loop,
            ResolutionChange {
                backend_type: self.backend_type,
                high_dpi: config::get().high_dpi,
                width,
                height,
                monitor: self.state.shell.display_monitor,
                display_mode: self.state.shell.display_mode,
            },
        );
        self.state.shell.display_monitor = result.monitor;
        if let Some(size) = result.immediate_size {
            self.sync_window_size(size);
        }
        Ok(())
    }

    fn reset_dynamic_assets_after_renderer_switch(&mut self) {
        self.apply_banner(None);
        self.apply_cdtitle(None);
        self.apply_density_graph(DensityGraphSlot::SelectMusicP1, None);
        self.apply_density_graph(DensityGraphSlot::SelectMusicP2, None);
        self.apply_dynamic_background(None);

        select_music::trigger_immediate_refresh(&mut self.state.screens.select_music_state);
        self.state.screens.select_music_state.current_graph_key = "__white".to_string();
        self.state.screens.select_music_state.current_graph_key_p2 = "__white".to_string();
    }

    pub(super) fn update_options_monitor_specs(&mut self, event_loop: &ActiveEventLoop) {
        let monitors: Vec<MonitorHandle> = event_loop.available_monitors().collect();
        let specs = display::monitor_specs(&monitors);
        options::update_monitor_specs(&mut self.state.screens.options_state, specs);
    }
}
