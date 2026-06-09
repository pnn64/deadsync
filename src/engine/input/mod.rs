mod backends;
pub mod fsr;

use deadsync_input::PadEvent;
use deadsync_input::backend::{GpSystemEvent, RawKeyboardEvent, WindowsPadBackend};

/// Run the platform pad backend on the current thread.
///
/// This is intended to be called from a dedicated thread which forwards
/// `PadEvent` and `GpSystemEvent` into the winit `EventLoopProxy`.
#[cfg_attr(windows, allow(dead_code))]
pub fn run_pad_backend(
    win_backend: WindowsPadBackend,
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
) {
    #[cfg(not(windows))]
    let _ = win_backend;

    #[cfg(windows)]
    match win_backend {
        WindowsPadBackend::Auto | WindowsPadBackend::RawInput => {
            backends::w32_raw_input::run(emit_pad, emit_sys, |_| {}, backends::host())
        }
        #[cfg(target_vendor = "win7")]
        WindowsPadBackend::Wgi => {
            backends::w32_raw_input::run(emit_pad, emit_sys, |_| {}, backends::host())
        }
        #[cfg(not(target_vendor = "win7"))]
        WindowsPadBackend::Wgi => backends::wgi::run(emit_pad, emit_sys, backends::host()),
    }
    #[cfg(target_os = "linux")]
    return backends::evdev::run_pad_only(emit_pad, emit_sys, backends::host());
    #[cfg(target_os = "freebsd")]
    {
        let mut emit_pad = emit_pad;
        let mut emit_sys = emit_sys;
        if let Err(err) = backends::hidraw::run(&mut emit_pad, &mut emit_sys, backends::host()) {
            log::warn!("freebsd hidraw unavailable or unusable ({err}); falling back to evdev");
        }
        return backends::evdev::run_pad_only(emit_pad, emit_sys, backends::host());
    }
    #[cfg(target_os = "macos")]
    return backends::iohid::run(emit_pad, emit_sys, |_| {}, backends::host());

    #[cfg(not(any(
        windows,
        target_os = "linux",
        target_os = "freebsd",
        target_os = "macos"
    )))]
    {
        let _ = emit_pad;
        let _ = emit_sys;
        loop {
            std::thread::park();
        }
    }
}

#[cfg(target_os = "linux")]
pub fn run_linux_backend(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
    emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static,
) {
    backends::evdev::run(emit_pad, emit_sys, emit_key, backends::host());
}

#[cfg(target_os = "freebsd")]
pub fn run_freebsd_backend(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
    emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static,
) {
    backends::evdev::run(emit_pad, emit_sys, emit_key, backends::host());
}

#[cfg(target_os = "macos")]
pub fn run_macos_backend(
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
    emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static,
) {
    backends::iohid::run(emit_pad, emit_sys, emit_key, backends::host());
}

#[cfg(windows)]
pub fn run_windows_backend(
    win_backend: WindowsPadBackend,
    emit_pad: impl FnMut(PadEvent) + Send + 'static,
    emit_sys: impl FnMut(GpSystemEvent) + Send + 'static,
    emit_key: impl FnMut(RawKeyboardEvent) + Send + 'static,
) {
    match win_backend {
        WindowsPadBackend::Auto | WindowsPadBackend::RawInput => {
            backends::w32_raw_input::run(emit_pad, emit_sys, emit_key, backends::host());
        }
        #[cfg(target_vendor = "win7")]
        WindowsPadBackend::Wgi => {
            log::warn!("WGI gamepad backend is unavailable in Windows 7 builds; using Raw Input");
            backends::w32_raw_input::run(emit_pad, emit_sys, emit_key, backends::host());
        }
        #[cfg(not(target_vendor = "win7"))]
        WindowsPadBackend::Wgi => {
            std::thread::spawn(move || backends::wgi::run(emit_pad, emit_sys, backends::host()));
            backends::w32_raw_input::run_keyboard_only(emit_key, backends::host());
        }
    }
}

#[cfg(windows)]
#[inline(always)]
pub fn set_raw_keyboard_window_focused(focused: bool) {
    backends::w32_raw_input::set_window_focused(focused);
}

#[cfg(windows)]
#[inline(always)]
pub fn set_raw_keyboard_capture_enabled(enabled: bool) {
    backends::w32_raw_input::set_capture_enabled(enabled);
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[inline(always)]
pub fn set_raw_keyboard_window_focused(focused: bool) {
    backends::evdev::set_keyboard_window_focused(focused);
}

#[cfg(target_os = "macos")]
#[inline(always)]
pub fn set_raw_keyboard_window_focused(focused: bool) {
    backends::iohid::set_keyboard_window_focused(focused);
}

#[cfg(all(
    not(windows),
    not(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))
))]
#[inline(always)]
pub fn set_raw_keyboard_window_focused(focused: bool) {
    let _ = focused;
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[inline(always)]
pub fn set_raw_keyboard_capture_enabled(enabled: bool) {
    backends::evdev::set_keyboard_capture_enabled(enabled);
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[inline(always)]
pub fn unix_raw_keyboard_backend_active() -> bool {
    backends::evdev::keyboard_backend_active()
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
#[inline(always)]
pub fn unix_raw_keyboard_backend_active() -> bool {
    true
}

#[cfg(target_os = "macos")]
#[inline(always)]
pub fn set_raw_keyboard_capture_enabled(enabled: bool) {
    backends::iohid::set_keyboard_capture_enabled(enabled);
}

#[cfg(all(
    not(windows),
    not(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))
))]
#[inline(always)]
pub fn set_raw_keyboard_capture_enabled(enabled: bool) {
    let _ = enabled;
}
