//! Startup focus policy for unmanaged platform windows.

use winit::window::Window;

/// Give a newly shown window focus when no desktop window manager can do it.
///
/// Normal desktops retain control of focus. The fallback only acts on bare
/// X11, where no client owns `SubstructureRedirectMask` on the window's root.
pub fn focus_unmanaged_startup_window(window: &Window) -> bool {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        focus_unmanaged_x11(window)
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        let _ = window;
        false
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn focus_unmanaged_x11(window: &Window) -> bool {
    use log::{debug, warn};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use xcb::{XidNew, x};

    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let window_id = match handle.as_raw() {
        RawWindowHandle::Xlib(handle) => match u32::try_from(handle.window) {
            Ok(window_id) => window_id,
            Err(_) => return false,
        },
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        _ => return false,
    };
    let Ok((conn, _)) = xcb::Connection::connect(None) else {
        return false;
    };

    // The ID comes from winit's live raw X11 handle, not from this auxiliary
    // connection's allocator. XIDs are server-global and valid across clients.
    let xwindow = x::Window::new(window_id);
    let tree = match conn.wait_for_reply(conn.send_request(&x::QueryTree { window: xwindow })) {
        Ok(tree) => tree,
        Err(err) => {
            warn!("Failed to inspect the X11 window hierarchy for startup focus: {err}");
            return false;
        }
    };
    let root_attrs = match conn.wait_for_reply(conn.send_request(&x::GetWindowAttributes {
        window: tree.root(),
    })) {
        Ok(attrs) => attrs,
        Err(err) => {
            warn!("Failed to inspect the X11 root window for startup focus: {err}");
            return false;
        }
    };
    if root_attrs
        .all_event_masks()
        .contains(x::EventMask::SUBSTRUCTURE_REDIRECT)
    {
        debug!("X11 window manager detected; leaving startup focus to the desktop");
        return false;
    }

    match conn.send_and_check_request(&x::SetInputFocus {
        revert_to: x::InputFocus::PointerRoot,
        focus: xwindow,
        time: x::CURRENT_TIME,
    }) {
        Ok(()) => {
            debug!("Focused startup window directly because X11 has no window manager");
            true
        }
        Err(err) => {
            warn!("Failed to focus the startup window on unmanaged X11: {err}");
            false
        }
    }
}
