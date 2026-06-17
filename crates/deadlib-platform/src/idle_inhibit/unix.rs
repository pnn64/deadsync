use log::{debug, info, warn};
use std::env::var_os;
use xcb::{Extension, dpms, screensaver};

const SCREENSAVER_SUSPEND: u32 = 1;
const SCREENSAVER_RESUME: u32 = 0;

pub struct IdleInhibitor {
    _x11: X11IdleInhibitor,
}

impl IdleInhibitor {
    pub fn acquire() -> Option<Self> {
        if var_os("DISPLAY").is_none() {
            debug!("No X11 DISPLAY set; idle display inhibition unavailable");
            return None;
        }
        X11IdleInhibitor::acquire().map(|x11| Self { _x11: x11 })
    }
}

struct X11IdleInhibitor {
    conn: xcb::Connection,
    restore_dpms_enabled: Option<bool>,
    screensaver_suspended: bool,
}

impl X11IdleInhibitor {
    fn acquire() -> Option<Self> {
        let (conn, _) = match xcb::Connection::connect_with_extensions(
            None,
            &[],
            &[Extension::Dpms, Extension::ScreenSaver],
        ) {
            Ok(conn) => conn,
            Err(err) => {
                warn!("Failed to open X11 display for idle inhibition: {err}");
                return None;
            }
        };

        let has_dpms = conn.active_extensions().any(|ext| ext == Extension::Dpms);
        let has_screensaver = conn
            .active_extensions()
            .any(|ext| ext == Extension::ScreenSaver);
        if !has_dpms && !has_screensaver {
            debug!("X11 idle inhibition unavailable: DPMS and MIT-SCREEN-SAVER are missing");
            return None;
        }

        let restore_dpms_enabled = if has_dpms {
            disable_dpms(&conn)
        } else {
            debug!("X11 DPMS extension missing; display power management was not disabled");
            None
        };

        let screensaver_suspended = has_screensaver && suspend_screensaver(&conn);
        if restore_dpms_enabled.is_none() && !screensaver_suspended {
            debug!("X11 idle inhibition unavailable: no inhibit request succeeded");
            return None;
        }

        info!(
            "X11 idle inhibition active: dpms={} screensaver={}",
            restore_dpms_enabled.is_some(),
            screensaver_suspended
        );
        Some(Self {
            conn,
            restore_dpms_enabled,
            screensaver_suspended,
        })
    }
}

impl Drop for X11IdleInhibitor {
    fn drop(&mut self) {
        if self.screensaver_suspended
            && let Err(err) = self.conn.send_and_check_request(&screensaver::Suspend {
                suspend: SCREENSAVER_RESUME,
            })
        {
            warn!("Failed to resume X11 screensaver: {err}");
        }

        let Some(enabled) = self.restore_dpms_enabled else {
            return;
        };
        let result = if enabled {
            self.conn.send_and_check_request(&dpms::Enable {})
        } else {
            self.conn.send_and_check_request(&dpms::Disable {})
        };
        if let Err(err) = result {
            warn!("Failed to restore X11 DPMS state: {err}");
        }
    }
}

fn disable_dpms(conn: &xcb::Connection) -> Option<bool> {
    let info = match conn.wait_for_reply(conn.send_request(&dpms::Info {})) {
        Ok(info) => info,
        Err(err) => {
            warn!("Failed to read X11 DPMS state: {err}");
            return None;
        }
    };
    let was_enabled = info.state();
    if let Err(err) = conn.send_and_check_request(&dpms::Disable {}) {
        warn!("Failed to disable X11 DPMS: {err}");
        return None;
    }
    Some(was_enabled)
}

fn suspend_screensaver(conn: &xcb::Connection) -> bool {
    match conn.send_and_check_request(&screensaver::Suspend {
        suspend: SCREENSAVER_SUSPEND,
    }) {
        Ok(()) => true,
        Err(err) => {
            warn!("Failed to suspend X11 screensaver: {err}");
            false
        }
    }
}
