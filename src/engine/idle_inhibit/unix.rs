use log::{debug, info, warn};
use std::env::var_os;
use std::time::{Duration, Instant};
use xcb::{Extension, Xid, dpms, x, xtest};

const X11_IDLE_POKE_INTERVAL: Duration = Duration::from_secs(60);
const X11_MOTION_NOTIFY: u8 = 6;
const XTEST_RELATIVE_MOTION: u8 = 1;
const XTEST_CURRENT_TIME: u32 = 0;
const XTEST_CORE_DEVICE: u8 = 0;

pub struct IdleInhibitor {
    x11: X11IdleInhibitor,
}

impl IdleInhibitor {
    pub fn acquire() -> Option<Self> {
        if var_os("DISPLAY").is_none() {
            debug!("No X11 DISPLAY set; idle display inhibition unavailable");
            return None;
        }
        X11IdleInhibitor::acquire().map(|x11| Self { x11 })
    }

    pub fn ping(&mut self, now: Instant) {
        self.x11.ping(now);
    }
}

struct X11IdleInhibitor {
    conn: xcb::Connection,
    restore_dpms_enabled: Option<bool>,
    xtest_enabled: bool,
    last_idle_poke: Option<Instant>,
}

impl X11IdleInhibitor {
    fn acquire() -> Option<Self> {
        let (conn, _) = match xcb::Connection::connect_with_extensions(
            None,
            &[],
            &[Extension::Dpms, Extension::Test],
        ) {
            Ok(conn) => conn,
            Err(err) => {
                warn!("Failed to open X11 display for idle inhibition: {err}");
                return None;
            }
        };

        let has_dpms = conn.active_extensions().any(|ext| ext == Extension::Dpms);
        let xtest_enabled = conn.active_extensions().any(|ext| ext == Extension::Test);
        if !has_dpms && !xtest_enabled {
            debug!("X11 idle inhibition unavailable: DPMS and XTEST extensions are missing");
            return None;
        }

        let restore_dpms_enabled = if has_dpms {
            disable_dpms(&conn)
        } else {
            debug!("X11 DPMS extension missing; display power management was not disabled");
            None
        };

        if !xtest_enabled {
            debug!("X11 XTEST extension missing; screensaver idle timer will not be reset");
        }

        info!(
            "X11 idle inhibition active: dpms={} xtest={}",
            restore_dpms_enabled.is_some(),
            xtest_enabled
        );
        Some(Self {
            conn,
            restore_dpms_enabled,
            xtest_enabled,
            last_idle_poke: Some(Instant::now()),
        })
    }

    fn ping(&mut self, now: Instant) {
        if !self.xtest_enabled {
            return;
        }
        if self
            .last_idle_poke
            .is_some_and(|last| now.duration_since(last) < X11_IDLE_POKE_INTERVAL)
        {
            return;
        }
        self.last_idle_poke = Some(now);
        self.conn.send_request(&xtest::FakeInput {
            r#type: X11_MOTION_NOTIFY,
            detail: XTEST_RELATIVE_MOTION,
            time: XTEST_CURRENT_TIME,
            root: x::Window::none(),
            root_x: 0,
            root_y: 0,
            deviceid: XTEST_CORE_DEVICE,
        });
        if let Err(err) = self.conn.flush() {
            warn!("Failed to reset X11 screensaver idle timer: {err}");
            self.xtest_enabled = false;
        }
    }
}

impl Drop for X11IdleInhibitor {
    fn drop(&mut self) {
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
