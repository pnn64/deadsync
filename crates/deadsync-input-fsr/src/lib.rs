use std::path::Path;

use deadsync_input::fsr::{BackendKind, PadDeviceId, PadView};

#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
mod fsrio;
#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
mod mock;
#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
mod smx;

#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
pub struct Monitor {
    fsrio: fsrio::Monitor,
    smx: smx::Monitor,
    /// Fake SMX pads for UI development (`DEADSYNC_MOCK_PADS`). When set, the
    /// mock owns the SMX backend slot: native SMX never starts (no pads, no
    /// USB writes) and SMX-routed calls land here instead.
    mock: Option<mock::Monitor>,
}

#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        Self {
            fsrio: fsrio::Monitor::new(),
            smx: smx::Monitor::new(),
            mock: mock::Monitor::from_env(),
        }
    }

    pub fn write_debug_dump(&mut self, path: &Path) -> Result<(), String> {
        let mut out = String::new();
        out.push_str(&self.fsrio.debug_dump());
        out.push_str("\n\n");
        match &mut self.mock {
            Some(m) => out.push_str(&m.debug_dump()),
            None => out.push_str(&self.smx.debug_dump()),
        }
        write_dump_file(path, out)
    }

    /// Enumerate every connected FSR pad across all backends.
    pub fn poll_pads(&mut self) -> Vec<PadView> {
        let mut pads = self.fsrio.poll_pads();
        match &mut self.mock {
            Some(m) => pads.extend(m.poll_pads()),
            None => pads.extend(self.smx.poll_pads()),
        }
        pads
    }

    /// Set a single threshold on a specific pad (the backend derives its own
    /// release side). `sensor` of `None` applies to every sensor in the button
    /// (Simple mode); `Some(i)` targets one sensor. Load-cell pads edit their
    /// press/release pair through `set_threshold_pair` instead.
    pub fn set_threshold(
        &mut self,
        device: PadDeviceId,
        button: usize,
        sensor: Option<usize>,
        value: u16,
    ) -> bool {
        match device.backend {
            BackendKind::Fsrio => self.fsrio.set_threshold(device, button, sensor, value),
            BackendKind::Smx => match &mut self.mock {
                Some(m) => m.set_threshold(device, button, sensor, value),
                None => self.smx.set_threshold(device, button, sensor, value),
            },
        }
    }

    /// Set a load-cell button's press (high) and release (low) thresholds in
    /// one config write, so the pad can never observe an inverted intermediate
    /// pair. Only SMX load-cell pads support this.
    pub fn set_threshold_pair(
        &mut self,
        device: PadDeviceId,
        button: usize,
        press: u16,
        release: u16,
    ) -> bool {
        match device.backend {
            BackendKind::Fsrio => false,
            BackendKind::Smx => match &mut self.mock {
                Some(m) => m.set_threshold_pair(device, button, press, release),
                None => self.smx.set_threshold_pair(device, button, press, release),
            },
        }
    }

    /// Enable or disable a single sensor within a button (Advanced mode).
    pub fn set_sensor_enabled(
        &mut self,
        device: PadDeviceId,
        button: usize,
        sensor: usize,
        enabled: bool,
    ) -> bool {
        match device.backend {
            BackendKind::Fsrio => self
                .fsrio
                .set_sensor_enabled(device, button, sensor, enabled),
            BackendKind::Smx => match &mut self.mock {
                Some(m) => m.set_sensor_enabled(device, button, sensor, enabled),
                None => self.smx.set_sensor_enabled(device, button, sensor, enabled),
            },
        }
    }

    /// Turn auto-recalibration on/off for a whole pad (Extra Advanced).
    pub fn set_auto_recalibration(&mut self, device: PadDeviceId, enabled: bool) -> bool {
        match device.backend {
            BackendKind::Fsrio => self.fsrio.set_auto_recalibration(device, enabled),
            BackendKind::Smx => match &mut self.mock {
                Some(m) => m.set_auto_recalibration(device, enabled),
                None => self.smx.set_auto_recalibration(device, enabled),
            },
        }
    }

    /// Set the per-panel debounce (microseconds) for a whole pad (Extra Advanced).
    pub fn set_debounce_micros(&mut self, device: PadDeviceId, micros: u16) -> bool {
        match device.backend {
            BackendKind::Fsrio => self.fsrio.set_debounce_micros(device, micros),
            BackendKind::Smx => match &mut self.mock {
                Some(m) => m.set_debounce_micros(device, micros),
                None => self.smx.set_debounce_micros(device, micros),
            },
        }
    }

    /// Enter/leave live read mode (e.g. SMX sensor test mode). Call with `true`
    /// while the config screen is open and `false` when leaving it.
    pub fn set_active(&mut self, active: bool) {
        self.fsrio.set_active(active);
        match &mut self.mock {
            Some(m) => m.set_active(active),
            None => self.smx.set_active(active),
        }
    }
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
)))]
mod unsupported {
    use super::{PadDeviceId, PadView};
    use std::fmt::Write as _;
    use std::path::Path;
    use std::time::SystemTime;

    #[derive(Default)]
    pub struct Monitor;

    impl Monitor {
        pub const fn new() -> Self {
            Self
        }

        pub fn poll_pads(&mut self) -> Vec<PadView> {
            Vec::new()
        }

        pub fn set_threshold(
            &mut self,
            _device: PadDeviceId,
            _button: usize,
            _sensor: Option<usize>,
            _value: u16,
        ) -> bool {
            false
        }

        pub fn set_threshold_pair(
            &mut self,
            _device: PadDeviceId,
            _button: usize,
            _press: u16,
            _release: u16,
        ) -> bool {
            false
        }

        pub fn set_sensor_enabled(
            &mut self,
            _device: PadDeviceId,
            _button: usize,
            _sensor: usize,
            _enabled: bool,
        ) -> bool {
            false
        }

        pub fn set_auto_recalibration(&mut self, _device: PadDeviceId, _enabled: bool) -> bool {
            false
        }

        pub fn set_debounce_micros(&mut self, _device: PadDeviceId, _micros: u16) -> bool {
            false
        }

        pub fn set_active(&mut self, _active: bool) {}

        pub fn write_debug_dump(&mut self, path: &Path) -> Result<(), String> {
            let mut out = String::new();
            let _ = writeln!(out, "DeadSync FSR debug dump");
            let _ = writeln!(out, "generated: {:?}", SystemTime::now());
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "FSR HID diagnostics are not available on this platform."
            );
            super::write_dump_file(path, out)
        }
    }
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
)))]
pub use unsupported::Monitor;

fn write_dump_file(path: &Path, content: String) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create '{}': {e}", parent.display()))?;
    }
    std::fs::write(path, content).map_err(|e| format!("failed to write '{}': {e}", path.display()))
}
