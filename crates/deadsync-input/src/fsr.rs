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
/// necessarily the firmware index order; `firmware_index` is what threshold /
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
    /// Full-scale value for normalizing the live bars and threshold lines.
    /// May exceed `max_raw_threshold`; backends pick it so the threshold range
    /// covers most of the bar (readings past it display as a full bar).
    pub value_scale: u16,
    /// The release (low) threshold, when the pad exposes it as user-editable
    /// (SMX load-cell pads). `None` means the backend derives it from the
    /// press threshold and the Simple view shows a single editable value.
    pub release_threshold: Option<u16>,
}

/// A single connected FSR pad, exposed to the config screen.
#[derive(Clone, Debug)]
pub struct PadView {
    pub device_id: PadDeviceId,
    pub device_name: String,
    /// Player side the pad maps to (P2 vs P1), used to filter by play style. Taken
    /// from the device slot (slot 1 = P2 for SMX), not the hardware jumper.
    pub is_p2_side: bool,
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
