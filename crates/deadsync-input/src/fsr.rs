use smallvec::SmallVec;
use std::sync::Arc;

/// Common controllers fit inline; custom serial firmware can expose more.
pub const INLINE_PAD_BUTTONS: usize = 16;
pub const INLINE_BUTTON_SENSORS: usize = 12;

/// Which FSR backend owns a given pad, so edits can be routed back to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    Fsrio,
    Smx,
    Teejusb,
}

/// Backend-defined transform from raw sensor units to a displayed bar height.
///
/// Sensor fills and threshold lines must use the same curve. Most pads are
/// linear; Analog Dance Pad firmware uses a quartic/linear blend to compensate
/// for its FSR response.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValueCurve {
    Linear {
        max_raw: u16,
    },
    QuarticBlend {
        max_raw: u16,
        quartic_weight: f32,
        linear_weight: f32,
    },
}

impl ValueCurve {
    #[must_use]
    pub const fn linear(max_raw: u16) -> Self {
        Self::Linear { max_raw }
    }

    #[must_use]
    pub const fn quartic_blend(max_raw: u16, quartic_weight: f32, linear_weight: f32) -> Self {
        Self::QuarticBlend {
            max_raw,
            quartic_weight,
            linear_weight,
        }
    }

    #[must_use]
    pub const fn normalize(self, value: u16) -> f32 {
        let max_raw = match self {
            Self::Linear { max_raw } | Self::QuarticBlend { max_raw, .. } => max_raw,
        };
        if max_raw == 0 {
            return 0.0;
        }
        let raw = if value > max_raw { max_raw } else { value } as f32;
        let max = max_raw as f32;
        match self {
            Self::Linear { .. } => raw / max,
            Self::QuarticBlend {
                quartic_weight,
                linear_weight,
                ..
            } => {
                let raw_squared = raw * raw;
                let max_squared = max * max;
                let quartic_max = (max_squared * max_squared) / max;
                let quartic = (raw_squared * raw_squared) / quartic_max;
                (quartic * quartic_weight + raw * linear_weight) / max
            }
        }
    }
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

/// Common pads stay inline; larger custom groups spill without truncation.
pub type SensorViews = SmallVec<[SensorView; INLINE_BUTTON_SENSORS]>;

/// Label shown for a logical controller button group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonLabel {
    /// A backend-defined semantic panel name, such as SMX L/D/U/R.
    Named(&'static str),
    /// A one-based HID button number from Analog Dance Pad firmware.
    Hid(u8),
    /// Serial firmware exposes sensor indices, not HID button mappings.
    Sensor(u16),
    /// A user-configured group of serial sensors (not a claimed HID mapping).
    Group(u16),
}

/// One logical controller button and the sensors that drive it.
///
/// `sensors` may be empty for a button with no mapped sensors. `aggregate_*`
/// summarize the button for Simple mode (peak value / representative
/// threshold); `min/max_raw_threshold` bound the editable range.
#[derive(Clone, Debug)]
pub struct ButtonView {
    pub label: ButtonLabel,
    pub sensors: SensorViews,
    pub min_raw_threshold: u16,
    pub max_raw_threshold: u16,
    pub aggregate_value: u16,
    pub aggregate_threshold: u16,
    pub active: bool,
    /// Curve shared by live bars, threshold lines, and pending threshold edits.
    /// Its raw maximum may exceed `max_raw_threshold`; backends pick it so the
    /// editable range covers most of the bar.
    pub value_curve: ValueCurve,
    /// The release (low) threshold, when the pad exposes it as user-editable
    /// (SMX load-cell pads). `None` means the backend derives it from the
    /// press threshold and the Simple view shows a single editable value.
    pub release_threshold: Option<u16>,
}

/// Inline for SMX/FSRio, with room to grow for custom serial controllers.
pub type ButtonViews = SmallVec<[ButtonView; INLINE_PAD_BUTTONS]>;

/// A single connected FSR pad, exposed to the config screen.
#[derive(Clone, Debug)]
pub struct PadView {
    pub device_id: PadDeviceId,
    /// Immutable hardware label shared across live snapshots. Backends update
    /// this only when device identity changes, so cloning a frame never copies
    /// or reallocates the label text.
    pub device_name: Arc<str>,
    /// A connected device may need an explicit protocol choice before writes.
    pub thresholds_editable: bool,
    /// Connection/configuration feedback, displayed by the shared editor.
    pub status: Option<Arc<str>>,
    /// Player side for SMX play-style filtering, taken from the device slot
    /// (slot 1 = P2), not the hardware jumper. FSRio and serial controllers have
    /// no side assignment; their placeholder value is ignored when filtering.
    pub is_p2_side: bool,
    pub buttons: ButtonViews,
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
