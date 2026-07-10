//! Initial application-window construction and platform icon policy.

use std::sync::Arc;

use deadlib_platform::dirs;
use deadlib_platform::display::{self, FullscreenType};
use deadlib_render::BackendType;
use deadlib_renderer::{render_size_for_window, request_window_size, with_requested_window_size};
use deadsync_config::app_config::DisplayMode;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Icon, Window};

const WINDOW_ICON_PATHS: [&str; 2] = [
    "assets/graphics/icon/icon-256.png",
    "assets/graphics/icon/icon.png",
];

#[derive(Clone, Copy)]
pub struct AppWindowConfig {
    pub backend_type: BackendType,
    pub high_dpi: bool,
    pub width: u32,
    pub height: u32,
    pub monitor: usize,
    pub display_mode: DisplayMode,
    pub fallback_fullscreen_type: FullscreenType,
    pub hide_cursor: bool,
    pub pending_position: Option<PhysicalPosition<i32>>,
}

pub struct AppWindowSetup {
    pub window: Arc<Window>,
    pub monitor_count: usize,
    pub monitor: usize,
    pub fullscreen_type: FullscreenType,
}

#[derive(Clone, Copy)]
pub struct DisplayModeChange {
    pub backend_type: BackendType,
    pub high_dpi: bool,
    pub width: u32,
    pub height: u32,
    pub monitor: usize,
    pub monitor_override: Option<usize>,
    pub previous_mode: DisplayMode,
    pub mode: DisplayMode,
    pub fallback_fullscreen_type: FullscreenType,
    pub pending_position: Option<PhysicalPosition<i32>>,
}

pub struct DisplayModeResult {
    pub width: u32,
    pub height: u32,
    pub monitor: usize,
    pub monitor_count: usize,
    pub pending_position: Option<PhysicalPosition<i32>>,
    pub fullscreen_type: FullscreenType,
    pub immediate_size: Option<PhysicalSize<u32>>,
}

#[derive(Clone, Copy)]
pub struct ResolutionChange {
    pub backend_type: BackendType,
    pub high_dpi: bool,
    pub width: u32,
    pub height: u32,
    pub monitor: usize,
    pub display_mode: DisplayMode,
}

pub struct ResolutionResult {
    pub monitor: usize,
    pub immediate_size: Option<PhysicalSize<u32>>,
}

pub fn create_app_window(
    event_loop: &ActiveEventLoop,
    config: AppWindowConfig,
) -> Result<AppWindowSetup, winit::error::OsError> {
    let mut attributes = Window::default_attributes()
        .with_title("DeadSync")
        .with_resizable(true)
        .with_transparent(false)
        // Keep the window hidden until startup assets are ready so the first
        // visible frame starts Init animations at t=0.
        .with_visible(false);
    set_macos_app_icon();
    if let Some(icon) = load_window_icon() {
        attributes = attributes.with_window_icon(Some(icon));
    }
    #[cfg(target_os = "macos")]
    if config.backend_type == BackendType::OpenGL {
        attributes = attributes.with_disallow_hidpi(!config.high_dpi);
    }

    let (monitor_handle, monitor_count, monitor) =
        display::resolve_monitor(event_loop, config.monitor);
    let fullscreen_type =
        effective_fullscreen_type(config.display_mode, config.fallback_fullscreen_type);
    attributes = with_requested_window_size(
        attributes,
        config.backend_type,
        config.high_dpi,
        config.width,
        config.height,
    );

    match config.display_mode {
        DisplayMode::Fullscreen(fullscreen_type) => {
            attributes = attributes.with_fullscreen(display::fullscreen_mode(
                fullscreen_type,
                config.width,
                config.height,
                monitor_handle,
                event_loop,
            ));
        }
        DisplayMode::Windowed => {
            if let Some(position) = config.pending_position {
                attributes = attributes.with_position(position);
            } else if let Some(position) =
                display::default_window_position(config.width, config.height, monitor_handle)
            {
                attributes = attributes.with_position(position);
            }
        }
    }

    let window = Arc::new(event_loop.create_window(attributes)?);
    // Re-assert the opaque hint so compositors do not apply alpha-based blending.
    window.set_transparent(false);
    window.set_cursor_visible(!config.hide_cursor);
    Ok(AppWindowSetup {
        window,
        monitor_count,
        monitor,
        fullscreen_type,
    })
}

/// Apply a runtime window-mode transition and return state for config/UI synchronization.
pub fn apply_window_display_mode(
    window: Option<&Window>,
    event_loop: &ActiveEventLoop,
    change: DisplayModeChange,
) -> DisplayModeResult {
    let (monitor_handle, monitor_count, monitor) = display::resolve_monitor(
        event_loop,
        change.monitor_override.unwrap_or(change.monitor),
    );
    let mut width = change.width;
    let mut height = change.height;
    let mut pending_position = change.pending_position;

    if let Some(window) = window
        && matches!(change.previous_mode, DisplayMode::Windowed)
    {
        let size = render_size_for_window(window, change.backend_type, change.high_dpi);
        width = size.width;
        height = size.height;
        if let Ok(position) = window.outer_position() {
            pending_position = Some(position);
        }
    }

    let immediate_size = window.and_then(|window| match change.mode {
        DisplayMode::Windowed => {
            window.set_fullscreen(None);
            let immediate_size =
                request_window_size(window, change.backend_type, change.high_dpi, width, height);
            if let Some(position) = pending_position.take() {
                window.set_outer_position(position);
            } else if let Some(position) =
                display::default_window_position(width, height, monitor_handle)
            {
                window.set_outer_position(position);
            }
            immediate_size
        }
        DisplayMode::Fullscreen(fullscreen_type) => {
            let fullscreen = display::fullscreen_mode(
                fullscreen_type,
                width,
                height,
                monitor_handle,
                event_loop,
            );
            let immediate_size =
                request_window_size(window, change.backend_type, change.high_dpi, width, height);
            window.set_fullscreen(fullscreen);
            immediate_size
        }
    });

    DisplayModeResult {
        width,
        height,
        monitor,
        monitor_count,
        pending_position,
        fullscreen_type: transition_fullscreen_type(
            change.mode,
            change.previous_mode,
            change.fallback_fullscreen_type,
        ),
        immediate_size,
    }
}

/// Apply a runtime resolution change to the current window and monitor.
pub fn apply_window_resolution(
    window: Option<&Window>,
    event_loop: &ActiveEventLoop,
    change: ResolutionChange,
) -> ResolutionResult {
    let (monitor_handle, _, monitor) = display::resolve_monitor(event_loop, change.monitor);
    let immediate_size = window.and_then(|window| match change.display_mode {
        DisplayMode::Windowed => request_window_size(
            window,
            change.backend_type,
            change.high_dpi,
            change.width,
            change.height,
        ),
        DisplayMode::Fullscreen(fullscreen_type) => {
            let fullscreen = display::fullscreen_mode(
                fullscreen_type,
                change.width,
                change.height,
                monitor_handle,
                event_loop,
            );
            let immediate_size = request_window_size(
                window,
                change.backend_type,
                change.high_dpi,
                change.width,
                change.height,
            );
            window.set_fullscreen(fullscreen);
            immediate_size
        }
    });
    ResolutionResult {
        monitor,
        immediate_size,
    }
}

pub const fn effective_fullscreen_type(
    display_mode: DisplayMode,
    fallback: FullscreenType,
) -> FullscreenType {
    match display_mode {
        DisplayMode::Fullscreen(fullscreen_type) => fullscreen_type,
        DisplayMode::Windowed => fallback,
    }
}

pub const fn transition_fullscreen_type(
    mode: DisplayMode,
    previous_mode: DisplayMode,
    fallback: FullscreenType,
) -> FullscreenType {
    match mode {
        DisplayMode::Fullscreen(fullscreen_type) => fullscreen_type,
        DisplayMode::Windowed => match previous_mode {
            DisplayMode::Fullscreen(fullscreen_type) => fullscreen_type,
            DisplayMode::Windowed => fallback,
        },
    }
}

fn load_window_icon() -> Option<Icon> {
    let dirs = dirs::app_dirs();
    for path in WINDOW_ICON_PATHS {
        let resolved = dirs.resolve_asset_path(path);
        let Ok(image) = image::open(&resolved) else {
            continue;
        };
        let rgba = image.into_rgba8();
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
    let dirs = dirs::app_dirs();
    for path in MACOS_APP_ICON_PATHS {
        let resolved = dirs.resolve_asset_path(path);
        let ns_path = NSString::from_str(&resolved.to_string_lossy());
        if let Some(icon_image) = NSImage::initWithContentsOfFile(NSImage::alloc(), &ns_path) {
            // SAFETY: both objects are valid AppKit objects on the required main thread.
            unsafe {
                app.setApplicationIconImage(Some(&icon_image));
            }
            return;
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_macos_app_icon() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_mode_uses_explicit_or_fallback_type() {
        let fallback = FullscreenType::Borderless;
        assert_eq!(
            effective_fullscreen_type(DisplayMode::Windowed, fallback),
            fallback
        );
        assert_eq!(
            effective_fullscreen_type(DisplayMode::Fullscreen(FullscreenType::Exclusive), fallback,),
            FullscreenType::Exclusive
        );
        assert_eq!(
            transition_fullscreen_type(
                DisplayMode::Windowed,
                DisplayMode::Fullscreen(FullscreenType::Exclusive),
                fallback,
            ),
            FullscreenType::Exclusive
        );
    }
}
