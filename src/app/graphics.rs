use super::App;
use crate::config::{self, DisplayMode};
use crate::engine::display;
use crate::engine::gfx::{BackendType, create_backend};
use crate::engine::space;
use crate::screens::{DensityGraphSlot, options, select_music};
use log::{error, info};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event_loop::ActiveEventLoop,
    monitor::MonitorHandle,
    window::{Icon, Window},
};

fn load_window_icon() -> Option<Icon> {
    const WINDOW_ICON_PATHS: [&str; 2] = [
        "assets/graphics/icon/icon-256.png",
        "assets/graphics/icon/icon.png",
    ];
    for path in WINDOW_ICON_PATHS {
        let Ok(img) = image::open(Path::new(path)) else {
            continue;
        };
        let rgba = img.into_rgba8();
        let (width, height) = rgba.dimensions();
        if let Ok(icon) = Icon::from_rgba(rgba.into_raw(), width, height) {
            return Some(icon);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn set_macos_app_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSString;

    const MACOS_APP_ICON_PATHS: [&str; 2] = [
        "assets/graphics/icon/icon.icns",
        "assets/graphics/icon/icon-512.png",
    ];

    let mtm = MainThreadMarker::new().expect("AppKit icon setup requires the main thread");
    let app = NSApplication::sharedApplication(mtm);
    for path in MACOS_APP_ICON_PATHS {
        let ns_path = NSString::from_str(path);
        let icon_image = NSImage::initWithContentsOfFile(NSImage::alloc(), &ns_path);
        if let Some(icon_image) = icon_image {
            // SAFETY: `app` and `icon_image` are valid AppKit objects on the main
            // thread, which is the required calling context for this setter.
            unsafe {
                app.setApplicationIconImage(Some(&icon_image));
            }
            return;
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_macos_app_icon() {}

impl App {
    pub(super) fn init_graphics(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        // Collect monitors and update options immediately so the initial menu state is correct.
        self.update_options_monitor_specs(event_loop);

        let mut window_attributes = Window::default_attributes()
            .with_title("DeadSync")
            .with_resizable(true)
            .with_transparent(false)
            // Keep the window hidden until startup assets are ready so the first
            // visible frame starts Init animations at t=0.
            .with_visible(false);
        set_macos_app_icon();
        if let Some(icon) = load_window_icon() {
            window_attributes = window_attributes.with_window_icon(Some(icon));
        }

        let window_width = self.state.shell.display_width;
        let window_height = self.state.shell.display_height;
        let (monitor_handle, monitor_count, monitor_idx) =
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
            self.state.shell.display_monitor,
            monitor_count,
        );

        match self.state.shell.display_mode {
            DisplayMode::Fullscreen(fullscreen_type) => {
                let fullscreen = display::fullscreen_mode(
                    fullscreen_type,
                    window_width,
                    window_height,
                    monitor_handle,
                    event_loop,
                );
                window_attributes = window_attributes.with_fullscreen(fullscreen);
            }
            DisplayMode::Windowed => {
                window_attributes = window_attributes
                    .with_inner_size(PhysicalSize::new(window_width, window_height));
                if let Some(pos) = self.state.shell.pending_window_position.take() {
                    window_attributes = window_attributes.with_position(pos);
                } else if let Some(pos) =
                    display::default_window_position(window_width, window_height, monitor_handle)
                {
                    window_attributes = window_attributes.with_position(pos);
                }
            }
        }

        let window = Arc::new(event_loop.create_window(window_attributes)?);
        // Re-assert the opaque hint so compositors do not apply alpha-based blending.
        window.set_transparent(false);
        let sz = window.inner_size();
        self.state.shell.metrics = space::metrics_for_window(sz.width, sz.height);
        space::set_current_metrics(self.state.shell.metrics);
        let mut backend = create_backend(
            self.backend_type,
            window.clone(),
            self.state.shell.vsync_enabled,
            self.state.shell.present_mode_policy,
            self.gfx_debug_enabled,
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
    ) -> Result<(), Box<dyn Error>> {
        if target == self.backend_type {
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
                let sz = window.inner_size();
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
                crate::engine::present::runtime::clear_all();
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
        let (monitor_handle, monitor_count, resolved_monitor) = display::resolve_monitor(
            event_loop,
            monitor_override.unwrap_or(self.state.shell.display_monitor),
        );
        self.state.shell.display_monitor = resolved_monitor;
        let previous_mode = self.state.shell.display_mode;

        if let Some(window) = &self.window {
            if matches!(previous_mode, DisplayMode::Windowed) {
                let sz = window.inner_size();
                self.state.shell.display_width = sz.width;
                self.state.shell.display_height = sz.height;
                if let Ok(pos) = window.outer_position() {
                    self.state.shell.pending_window_position = Some(pos);
                }
            }

            match mode {
                DisplayMode::Windowed => {
                    window.set_fullscreen(None);
                    let size = PhysicalSize::new(
                        self.state.shell.display_width,
                        self.state.shell.display_height,
                    );
                    let _ = window.request_inner_size(size);
                    if let Some(pos) = self.state.shell.pending_window_position.take() {
                        window.set_outer_position(pos);
                    } else if let Some(pos) = display::default_window_position(
                        self.state.shell.display_width,
                        self.state.shell.display_height,
                        monitor_handle,
                    ) {
                        window.set_outer_position(pos);
                    }
                }
                DisplayMode::Fullscreen(fullscreen_type) => {
                    let fullscreen = display::fullscreen_mode(
                        fullscreen_type,
                        self.state.shell.display_width,
                        self.state.shell.display_height,
                        monitor_handle,
                        event_loop,
                    );
                    window.set_fullscreen(fullscreen);
                }
            }

            let sz = window.inner_size();
            self.sync_window_size(sz);
        }

        self.state.shell.display_mode = mode;

        let fullscreen_type = match mode {
            DisplayMode::Fullscreen(ft) => ft,
            DisplayMode::Windowed => match previous_mode {
                DisplayMode::Fullscreen(ft) => ft,
                DisplayMode::Windowed => config::get().fullscreen_type,
            },
        };
        config::update_display_mode(mode);
        config::update_display_monitor(self.state.shell.display_monitor);
        options::sync_display_mode(
            &mut self.state.screens.options_state,
            mode,
            fullscreen_type,
            self.state.shell.display_monitor,
            monitor_count,
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
        let (monitor_handle, _, resolved_monitor) =
            display::resolve_monitor(event_loop, self.state.shell.display_monitor);
        self.state.shell.display_monitor = resolved_monitor;

        if let Some(window) = &self.window {
            match self.state.shell.display_mode {
                DisplayMode::Windowed => {
                    let size = PhysicalSize::new(width, height);
                    let _ = window.request_inner_size(size);
                }
                DisplayMode::Fullscreen(fullscreen_type) => {
                    let fullscreen = display::fullscreen_mode(
                        fullscreen_type,
                        width,
                        height,
                        monitor_handle,
                        event_loop,
                    );
                    window.set_fullscreen(fullscreen);
                }
            }

            let sz = window.inner_size();
            self.sync_window_size(sz);
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
