use std::path::Path;

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
mod smx;

/// Number of playable buttons deadsync configures per FSR pad (L/D/U/R).
pub const PAD_BUTTON_COUNT: usize = 4;
/// Button labels in fixed order, shared by every FSR backend.
pub const PAD_BUTTON_LABELS: [&str; PAD_BUTTON_COUNT] = ["L", "D", "U", "R"];

/// Which FSR backend owns a given pad, so edits can be routed back to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    Fsrio,
    Smx,
}

/// Stable identifier for a connected FSR pad: backend + per-backend index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadDeviceId {
    pub backend: BackendKind,
    pub index: usize,
}

/// One physical sensor within a button group.
#[derive(Clone, Copy, Debug)]
pub struct SensorView {
    pub raw_value: u16,
    pub value_norm: f32,
    pub raw_threshold: u16,
    pub threshold_norm: f32,
    pub active: bool,
}

/// One playable button (L/D/U/R) and the sensors that drive it.
///
/// `sensors` may be empty for a button with no mapped sensors. `aggregate_*`
/// summarize the button for Simple mode (peak value / representative
/// threshold); `min/max_raw_threshold` bound the editable range.
#[derive(Clone, Debug)]
pub struct ButtonView {
    pub label: &'static str,
    pub sensors: Vec<SensorView>,
    pub min_raw_threshold: u16,
    pub max_raw_threshold: u16,
    pub aggregate_value: u16,
    pub aggregate_threshold: u16,
    pub active: bool,
}

/// A single connected FSR pad, exposed to the config screen.
#[derive(Clone, Debug)]
pub struct PadView {
    pub device_id: PadDeviceId,
    pub device_name: String,
    pub buttons: [ButtonView; PAD_BUTTON_COUNT],
}

#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
pub struct Monitor {
    fsrio: fsrio::Monitor,
    smx: smx::Monitor,
}

#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
impl Monitor {
    pub fn new() -> Self {
        Self {
            fsrio: fsrio::Monitor::new(),
            smx: smx::Monitor::new(),
        }
    }

    pub fn write_debug_dump(&mut self, path: &Path) -> Result<(), String> {
        let mut out = String::new();
        out.push_str(&self.fsrio.debug_dump());
        out.push_str("\n\n");
        out.push_str(&self.smx.debug_dump());
        write_dump_file(path, out)
    }

    /// Enumerate every connected FSR pad across all backends.
    pub fn poll_pads(&mut self) -> Vec<PadView> {
        let mut pads = self.fsrio.poll_pads();
        pads.extend(self.smx.poll_pads());
        pads
    }

    /// Set a threshold on a specific pad. `sensor` of `None` applies to every
    /// sensor in the button (Simple mode); `Some(i)` targets one sensor.
    pub fn set_threshold(
        &mut self,
        device: PadDeviceId,
        button: usize,
        sensor: Option<usize>,
        value: u16,
    ) -> bool {
        match device.backend {
            BackendKind::Fsrio => self.fsrio.set_threshold(device, button, sensor, value),
            BackendKind::Smx => self.smx.set_threshold(device, button, sensor, value),
        }
    }

    /// Enter/leave live read mode (e.g. SMX sensor test mode). Call with `true`
    /// while the config screen is open and `false` when leaving it.
    pub fn set_active(&mut self, active: bool) {
        self.fsrio.set_active(active);
        self.smx.set_active(active);
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
