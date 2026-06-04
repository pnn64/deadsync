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
///
/// Sensors are listed in display order (left-to-right in the UI), which is not
/// necessarily the firmware index order — `firmware_index` is what threshold /
/// enable edits target.
#[derive(Clone, Copy, Debug)]
pub struct SensorView {
    /// Index used when addressing this sensor for edits (`set_threshold` /
    /// `set_sensor_enabled`). May differ from the display position.
    pub firmware_index: usize,
    /// Short edge label (e.g. SMX "L"/"D"/"U"/"R"); `None` shows a 1-based number.
    pub label: Option<&'static str>,
    pub raw_value: u16,
    pub value_norm: f32,
    pub raw_threshold: u16,
    pub threshold_norm: f32,
    pub active: bool,
    /// Whether the firmware currently uses this sensor (Advanced mode toggle).
    /// Backends without per-sensor enable always report `true`.
    pub enabled: bool,
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
    /// Full-scale value for normalizing the live bars (FSR 250, load cell 500).
    /// May exceed `max_raw_threshold` (load-cell readings outrun their threshold range).
    pub value_scale: u16,
}

/// A single connected FSR pad, exposed to the config screen.
#[derive(Clone, Debug)]
pub struct PadView {
    pub device_id: PadDeviceId,
    pub device_name: String,
    /// Player side the pad maps to (P2 vs P1), used to filter by play style.
    pub is_player2: bool,
    pub buttons: [ButtonView; PAD_BUTTON_COUNT],
    /// Whether the Advanced view is available for this pad. Load-cell pads are
    /// Simple-only (per-sensor config isn't possible on them).
    pub supports_advanced: bool,
    /// Whether the Simple view should draw each sensor as its own thin bar
    /// (load cells: show all 4 corner readings) vs a single aggregate bar (FSR).
    pub simple_per_sensor_bars: bool,
    /// Whether this backend supports enabling/disabling individual sensors.
    pub supports_sensor_toggle: bool,
    /// Current auto-recalibration state, if the backend exposes it (SMX).
    /// `None` means the control is unsupported and is hidden in the UI.
    pub auto_recalibration: Option<bool>,
    /// Current per-panel debounce in microseconds, if the backend exposes it.
    /// `None` means the control is unsupported and is hidden in the UI.
    pub debounce_micros: Option<u16>,
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
            BackendKind::Smx => self.smx.set_sensor_enabled(device, button, sensor, enabled),
        }
    }

    /// Turn auto-recalibration on/off for a whole pad (Extra Advanced).
    pub fn set_auto_recalibration(&mut self, device: PadDeviceId, enabled: bool) -> bool {
        match device.backend {
            BackendKind::Fsrio => self.fsrio.set_auto_recalibration(device, enabled),
            BackendKind::Smx => self.smx.set_auto_recalibration(device, enabled),
        }
    }

    /// Set the per-panel debounce (microseconds) for a whole pad (Extra Advanced).
    pub fn set_debounce_micros(&mut self, device: PadDeviceId, micros: u16) -> bool {
        match device.backend {
            BackendKind::Fsrio => self.fsrio.set_debounce_micros(device, micros),
            BackendKind::Smx => self.smx.set_debounce_micros(device, micros),
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
