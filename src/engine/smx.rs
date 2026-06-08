//! Shared StepManiaX SDK manager.
//!
//! Provides a process-wide `SmxManager` instance that both the input backend
//! and the FSR monitor can use. Events are routed to registered listeners.

use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use rustmaniax_sdk::{
    BYTES_PER_PAD_25, ConfigFlags, NUM_PANELS, SMX_USB_PRODUCT_ID, SMX_USB_VENDOR_ID,
    SensorTestData, SensorTestMode, SmxConfig, SmxEvent, SmxInfo, SmxManager,
};

use crate::engine::input::{GpSystemEvent, PadBackend, uuid_from_bytes};
use deadsync_input::{PadCode, PadEvent, PadId};

/// Number of panels per SMX pad (from the SDK's hardware-shape constants).
pub const PANEL_COUNT: usize = NUM_PANELS;

/// Shared state accessible by both the input backend and FSR monitor.
struct SmxShared {
    manager: SmxManager,
    /// Listeners for input events (pad presses).
    input_listeners: Mutex<Vec<Box<dyn Fn(PadEvent) + Send>>>,
    /// Listeners for system events (connect/disconnect).
    sys_listeners: Mutex<Vec<Box<dyn Fn(GpSystemEvent) + Send>>>,
    /// Last dispatched input bitmask per pad, used to emit only changed panels.
    prev_input: [AtomicU16; 2],
    /// Stable per-pad device UUID (derived from the serial), cached at connect.
    ///
    /// The SMX event callback fires while the SDK holds its internal state lock,
    /// so the callback must never call back into `manager` (e.g. `get_info`) —
    /// doing so re-locks the same mutex and deadlocks the USB polling thread.
    /// We cache the serial-derived UUID here at connect time and read it from
    /// the input/disconnect handlers instead. This is our own mutex, not the
    /// SDK's, so locking it inside the callback is safe.
    uuid: [Mutex<[u8; 16]>; 2],
    /// Per-pad serial string, cached at connect, used for friendly trigger labels.
    serial: [Mutex<String>; 2],
    /// Set while a deferred `set_serial_numbers()` is in flight, so a burst of
    /// serial-less connect events only spawns one assignment at a time.
    serial_assign_inflight: AtomicBool,
}

static SHARED: OnceLock<Arc<SmxShared>> = OnceLock::new();

/// Initialize the shared SMX manager. Call once at startup.
/// Returns false if initialization failed (e.g., hidapi unavailable).
pub fn init() -> bool {
    if SHARED.get().is_some() {
        return true;
    }

    let shared = match SmxManager::start(|event| {
        if let Some(s) = SHARED.get() {
            dispatch_event(s, event);
        }
    }) {
        Ok(mgr) => Arc::new(SmxShared {
            manager: mgr,
            input_listeners: Mutex::new(Vec::new()),
            sys_listeners: Mutex::new(Vec::new()),
            prev_input: [AtomicU16::new(0), AtomicU16::new(0)],
            uuid: [Mutex::new([0u8; 16]), Mutex::new([0u8; 16])],
            serial: [Mutex::new(String::new()), Mutex::new(String::new())],
            serial_assign_inflight: AtomicBool::new(false),
        }),
        Err(e) => {
            log::warn!("SMX: failed to initialize SDK: {e}");
            return false;
        }
    };

    let _ = SHARED.set(shared);
    set_usb_polling_us(crate::config::get().smx_usb_polling_us);
    // Push any saved pad→player assignment so the SDK orders slots by serial as
    // pads connect (overriding the jumper). No-op when nothing is saved.
    let (p1, p2) = crate::config::smx_pad_assignment();
    if p1.is_some() || p2.is_some() {
        set_player_assignment(p1, p2);
    }
    log::info!("SMX: SDK initialized, polling for pads");
    true
}

/// Default SMX main-thread poll interval (ms). We only expose the USB rate to
/// the user, keeping the SDK's main-thread cadence at its default.
const DEFAULT_MAIN_THREAD_MS: i32 = 50;

/// Apply the USB polling interval (microseconds) to the running SMX manager.
pub fn set_usb_polling_us(micros: u16) {
    if let Some(s) = SHARED.get() {
        s.manager
            .set_polling_rate(DEFAULT_MAIN_THREAD_MS, i32::from(micros));
        log::debug!("SMX: USB polling interval set to {micros}us");
    }
}

/// Get a reference to the shared manager (None if not initialized).
pub fn manager() -> Option<&'static SmxManager> {
    SHARED.get().map(|s| &s.manager)
}

/// Register a listener for pad input events. Append-only and intended to be
/// called once at startup; there is no removal.
pub fn add_input_listener(listener: Box<dyn Fn(PadEvent) + Send>) {
    if let Some(s) = SHARED.get() {
        s.input_listeners.lock().unwrap().push(listener);
    }
}

/// Register a listener for system events (connect/disconnect). Append-only and
/// intended to be called once at startup; there is no removal.
pub fn add_sys_listener(listener: Box<dyn Fn(GpSystemEvent) + Send>) {
    if let Some(s) = SHARED.get() {
        s.sys_listeners.lock().unwrap().push(listener);
    }
}

/// Get device info for a pad slot (0 or 1).
pub fn get_info(pad: usize) -> SmxInfo {
    SHARED
        .get()
        .map(|s| s.manager.get_info(pad))
        .unwrap_or_default()
}

/// Get config for a pad.
pub fn get_config(pad: usize) -> Option<SmxConfig> {
    SHARED.get().and_then(|s| s.manager.get_config(pad))
}

/// Set config for a pad.
pub fn set_config(pad: usize, config: SmxConfig) {
    if let Some(s) = SHARED.get() {
        s.manager.set_config(pad, config);
    }
}

const PAD_CONFIG_PANELS: usize = 9;
const PAD_CONFIG_SENSORS: usize = 4;

/// Backend identifier stored with saved pad configs, so an SMX-tuned config is
/// only ever applied to a StepManiaX pad (FSRio and future FSR backends use their
/// own, different config schema).
pub const BACKEND_ID: &str = "smx";

/// Sensor technology of an SMX pad. FSR and load-cell pads interpret the
/// thresholds differently, so a config tuned for one must not be applied to the
/// other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmxPadType {
    Fsr,
    LoadCell,
}

impl SmxPadType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fsr => "fsr",
            Self::LoadCell => "loadcell",
        }
    }
}

/// Whether a pad's config describes an FSR pad (vs a load-cell pad), matching the
/// official tool: master version >= 4 with the FSR flag set.
pub fn is_fsr(config: &SmxConfig) -> bool {
    config.master_version >= 4
        && ConfigFlags::from_bits_truncate(config.flags).contains(ConfigFlags::FSR)
}

/// Whether a USB vendor/product pair is a StepManiaX stage, by the SDK's
/// `SMX_USB_VENDOR_ID` / `SMX_USB_PRODUCT_ID`.
pub fn is_smx_usb_device(vendor: Option<u16>, product: Option<u16>) -> bool {
    vendor == Some(SMX_USB_VENDOR_ID) && product == Some(SMX_USB_PRODUCT_ID)
}

/// Whether the OS gamepad backends should skip a device because native
/// StepManiaX input already owns it.
///
/// The stage also enumerates as a generic HID game controller on every OS (that
/// is how it works as a plug-and-play pad without the SDK). While `smx_input` is
/// on, the SDK opens the pad directly and emits its own labelled events, so a
/// generic backend would otherwise deliver the same physical step a second time
/// with a different label, e.g. "SMX P2 D" (native) versus "Pad 2 Btn 0x90008"
/// (generic HID). Gated on `smx_input` (the same flag that starts the SDK), so
/// with native SMX off the pad still works as a plain gamepad.
pub fn native_smx_owns_device(vendor: Option<u16>, product: Option<u16>) -> bool {
    native_smx_owns_device_with_flag(vendor, product, crate::config::get().smx_input)
}

/// Pure form of [`native_smx_owns_device`] with the `smx_input` flag supplied,
/// so the skip contract (only skip an SMX pad, and only while native input is
/// on) is testable without the process-global config.
fn native_smx_owns_device_with_flag(
    vendor: Option<u16>,
    product: Option<u16>,
    smx_input: bool,
) -> bool {
    smx_input && is_smx_usb_device(vendor, product)
}

/// The sensor type of a connected pad (`None` if its config isn't available yet).
pub fn pad_sensor_type(pad: usize) -> Option<SmxPadType> {
    get_config(pad).map(|c| {
        if is_fsr(&c) {
            SmxPadType::Fsr
        } else {
            SmxPadType::LoadCell
        }
    })
}

/// `enabled_sensors` nibble layout (official tool `Widgets.cs`): panel `p` uses
/// byte `p / 2`, the high nibble (`0xF0`) for even panels and the low nibble
/// (`0x0F`) for odd panels; sensor `s` is bit `base + s`. Shared by the config
/// encode/decode here and the live per-sensor edits in the input backend, so the
/// firmware bit layout has a single source of truth.
pub(crate) fn enabled_bit(panel: usize, sensor: usize) -> (usize, u8) {
    let byte = panel / 2;
    let base = if panel % 2 == 0 { 4 } else { 0 };
    (byte, 1u8 << (base + sensor))
}

/// One panel's threshold state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelThresholds {
    pub fsr_low: [u8; PAD_CONFIG_SENSORS],
    pub fsr_high: [u8; PAD_CONFIG_SENSORS],
    pub load_cell_low: u8,
    pub load_cell_high: u8,
}

/// The DeadSync-managed threshold state of a pad, used for user pad-config
/// profiles. Captured from / applied onto an `SmxConfig` (the remaining config
/// fields, e.g. lighting/version, are preserved on apply).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadConfigData {
    pub panels: [PanelThresholds; PAD_CONFIG_PANELS],
    pub enabled_sensors: [u8; 5],
    pub auto_calibration_max_tare: u16,
    pub panel_debounce_us: u16,
}

impl PadConfigData {
    /// Encode to a human-readable, hand-editable key/value list for
    /// `padconfig.ini`: per-panel FSR low/high arrays, load-cell low/high, and
    /// per-panel sensor enables, plus the auto-calibration tare and the panel
    /// debounce (in milliseconds).
    pub fn to_settings(&self) -> Vec<(String, String)> {
        let join = |xs: &[u8]| xs.iter().map(u8::to_string).collect::<Vec<_>>().join(" ");
        let mut out = Vec::with_capacity(PAD_CONFIG_PANELS * 5 + 2);
        for (p, panel) in self.panels.iter().enumerate() {
            out.push((format!("Panel{p}.FsrLow"), join(&panel.fsr_low)));
            out.push((format!("Panel{p}.FsrHigh"), join(&panel.fsr_high)));
            out.push((
                format!("Panel{p}.LoadCellLow"),
                panel.load_cell_low.to_string(),
            ));
            out.push((
                format!("Panel{p}.LoadCellHigh"),
                panel.load_cell_high.to_string(),
            ));
            let enabled: Vec<u8> = (0..PAD_CONFIG_SENSORS)
                .map(|s| {
                    let (byte, mask) = enabled_bit(p, s);
                    u8::from(self.enabled_sensors[byte] & mask != 0)
                })
                .collect();
            out.push((format!("Panel{p}.Enabled"), join(&enabled)));
        }
        out.push((
            "AutoCalibrationMaxTare".to_string(),
            self.auto_calibration_max_tare.to_string(),
        ));
        out.push((
            "DebounceMs".to_string(),
            format!("{}", f32::from(self.panel_debounce_us) / 1000.0),
        ));
        out
    }

    /// Decode from the key/value list written by `to_settings`. Returns `None`
    /// if any expected key is missing or malformed.
    pub fn from_settings(settings: &[(String, String)]) -> Option<Self> {
        let get = |key: &str| {
            settings
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        let arr = |key: &str| -> Option<[u8; PAD_CONFIG_SENSORS]> {
            let nums: Vec<u8> = get(key)?
                .split_whitespace()
                .map(|t| t.parse::<u8>().ok())
                .collect::<Option<Vec<u8>>>()?;
            if nums.len() != PAD_CONFIG_SENSORS {
                return None;
            }
            let mut a = [0u8; PAD_CONFIG_SENSORS];
            a.copy_from_slice(&nums);
            Some(a)
        };
        let byte = |key: &str| get(key)?.trim().parse::<u8>().ok();

        let mut panels = [PanelThresholds {
            fsr_low: [0; PAD_CONFIG_SENSORS],
            fsr_high: [0; PAD_CONFIG_SENSORS],
            load_cell_low: 0,
            load_cell_high: 0,
        }; PAD_CONFIG_PANELS];
        let mut enabled_sensors = [0u8; 5];
        for (p, panel) in panels.iter_mut().enumerate() {
            panel.fsr_low = arr(&format!("Panel{p}.FsrLow"))?;
            panel.fsr_high = arr(&format!("Panel{p}.FsrHigh"))?;
            panel.load_cell_low = byte(&format!("Panel{p}.LoadCellLow"))?;
            panel.load_cell_high = byte(&format!("Panel{p}.LoadCellHigh"))?;
            for (s, &e) in arr(&format!("Panel{p}.Enabled"))?.iter().enumerate() {
                if e != 0 {
                    let (bidx, mask) = enabled_bit(p, s);
                    enabled_sensors[bidx] |= mask;
                }
            }
        }
        let auto_calibration_max_tare =
            get("AutoCalibrationMaxTare")?.trim().parse::<u16>().ok()?;
        let debounce_ms = get("DebounceMs")?.trim().parse::<f32>().ok()?;
        let panel_debounce_us = (debounce_ms * 1000.0)
            .round()
            .clamp(0.0, f32::from(u16::MAX)) as u16;
        Some(Self {
            panels,
            enabled_sensors,
            auto_calibration_max_tare,
            panel_debounce_us,
        })
    }
}

/// Capture a connected pad's managed threshold state (None if no config yet).
pub fn capture_config(pad: usize) -> Option<PadConfigData> {
    let Some(config) = get_config(pad) else {
        log::trace!("SMX: capture_config pad {pad} skipped (config unavailable)");
        return None;
    };
    let panels = std::array::from_fn(|i| {
        let s = &config.panel_settings[i];
        PanelThresholds {
            fsr_low: s.fsr_low_threshold,
            fsr_high: s.fsr_high_threshold,
            load_cell_low: s.load_cell_low_threshold,
            load_cell_high: s.load_cell_high_threshold,
        }
    });
    let auto_calibration_max_tare = config.auto_calibration_max_tare;
    let panel_debounce_us = config.panel_debounce_us;
    Some(PadConfigData {
        panels,
        enabled_sensors: config.enabled_sensors,
        auto_calibration_max_tare,
        panel_debounce_us,
    })
}

/// Overlay a captured config onto a pad's current `SmxConfig` and write it.
/// Returns false if the pad's config isn't available yet.
pub fn apply_config_data(pad: usize, data: &PadConfigData) -> bool {
    let Some(mut config) = get_config(pad) else {
        log::trace!("SMX: apply_config_data pad {pad} skipped (config unavailable)");
        return false;
    };
    for (i, p) in data.panels.iter().enumerate() {
        let s = &mut config.panel_settings[i];
        s.fsr_low_threshold = p.fsr_low;
        s.fsr_high_threshold = p.fsr_high;
        s.load_cell_low_threshold = p.load_cell_low;
        s.load_cell_high_threshold = p.load_cell_high;
    }
    config.enabled_sensors = data.enabled_sensors;
    config.auto_calibration_max_tare = data.auto_calibration_max_tare;
    config.panel_debounce_us = data.panel_debounce_us;
    set_config(pad, config);
    log::trace!("SMX: apply_config_data pad {pad} written");
    true
}

/// Threshold values for a built-in pad preset, matching the official SMX config
/// tool (`ConfigPresets.cs`). Presets set both FSR and load-cell thresholds so
/// one preset works regardless of pad type; the center panel uses its own pair.
struct PresetThresholds {
    load_cell_low: u8,
    load_cell_high: u8,
    load_cell_low_center: u8,
    load_cell_high_center: u8,
    fsr_low: u8,
    fsr_high: u8,
    fsr_low_center: u8,
    fsr_high_center: u8,
}

fn preset_thresholds(preset: crate::config::SmxPadPreset) -> PresetThresholds {
    use crate::config::SmxPadPreset;
    match preset {
        SmxPadPreset::Low => PresetThresholds {
            load_cell_low: 70,
            load_cell_high: 80,
            load_cell_low_center: 100,
            load_cell_high_center: 120,
            fsr_low: 217,
            fsr_high: 218,
            fsr_low_center: 217,
            fsr_high_center: 218,
        },
        SmxPadPreset::Medium => PresetThresholds {
            load_cell_low: 33,
            load_cell_high: 42,
            load_cell_low_center: 35,
            load_cell_high_center: 60,
            fsr_low: 174,
            fsr_high: 175,
            fsr_low_center: 199,
            fsr_high_center: 200,
        },
        SmxPadPreset::High => PresetThresholds {
            load_cell_low: 20,
            load_cell_high: 25,
            load_cell_low_center: 20,
            load_cell_high_center: 30,
            fsr_low: 152,
            fsr_high: 153,
            fsr_low_center: 152,
            fsr_high_center: 153,
        },
    }
}

/// Flash a built-in preset to a pad: every panel's FSR and load-cell thresholds
/// (center panel 4 overridden), mirroring the official SMX tool. Returns false
/// if the pad's config isn't available yet.
pub fn apply_preset(pad: usize, preset: crate::config::SmxPadPreset) -> bool {
    let Some(mut config) = get_config(pad) else {
        log::trace!("SMX: apply_preset pad {pad} skipped (config unavailable)");
        return false;
    };
    let t = preset_thresholds(preset);
    for panel in 0..9 {
        let (lc_low, lc_high, fsr_low, fsr_high) = if panel == 4 {
            (
                t.load_cell_low_center,
                t.load_cell_high_center,
                t.fsr_low_center,
                t.fsr_high_center,
            )
        } else {
            (t.load_cell_low, t.load_cell_high, t.fsr_low, t.fsr_high)
        };
        let s = &mut config.panel_settings[panel];
        s.load_cell_low_threshold = lc_low;
        s.load_cell_high_threshold = lc_high;
        for i in 0..4 {
            s.fsr_low_threshold[i] = fsr_low;
            s.fsr_high_threshold[i] = fsr_high;
        }
    }
    // A built-in preset is a full baseline: also restore auto-recalibration on
    // (max tare 0xFFFF) and the default 4ms panel debounce.
    config.auto_calibration_max_tare = 0xFFFF;
    config.panel_debounce_us = 4000;
    set_config(pad, config);
    log::debug!("SMX: apply_preset pad {pad} -> {} preset", preset.as_str());
    true
}

/// Set sensor test mode for a pad.
pub fn set_test_mode(pad: usize, mode: SensorTestMode) {
    if let Some(s) = SHARED.get() {
        s.manager.set_test_mode(pad, mode);
    }
}

/// Get sensor test data for a pad.
pub fn get_test_data(pad: usize) -> Option<SensorTestData> {
    SHARED.get().and_then(|s| s.manager.get_test_data(pad))
}

/// Assign serial numbers to any connected pads that don't have one.
pub fn set_serial_numbers() {
    if let Some(s) = SHARED.get() {
        s.manager.set_serial_numbers();
    }
}

/// Pin pad serials to player slots (`p1` → slot 0, `p2` → slot 1), overriding the
/// hardware P1/P2 jumper. `None` for a side follows the jumper. Pushed to the SDK,
/// which re-orders the slots live.
pub fn set_player_assignment(p1: Option<String>, p2: Option<String>) {
    if let Some(s) = SHARED.get() {
        s.manager.set_player_assignment(p1, p2);
    }
}

/// The serial connected at each slot (index 0 = P1, 1 = P2), or `None` if that
/// slot has no connected pad (or its serial isn't known yet). This reflects the
/// SDK's *current* ordering, i.e. what is actually assigned right now.
pub fn connected_serials() -> [Option<String>; 2] {
    std::array::from_fn(|slot| {
        let info = get_info(slot);
        (info.connected && !info.serial.is_empty()).then_some(info.serial)
    })
}

/// First 4 chars of a serial, for a compact pad label (e.g. `40ea`). An empty
/// serial (not read yet) yields `????` so the label keeps its width.
pub fn serial_prefix(serial: &str) -> String {
    if serial.is_empty() {
        "????".to_owned()
    } else {
        serial.chars().take(4).collect()
    }
}

/// Pure: do two pads' jumpers conflict? Both connected and reporting the same
/// P1/P2 jumper, so the SDK can't order them by jumper alone and the user must
/// assign them manually.
fn jumpers_conflict(a: &SmxInfo, b: &SmxInfo) -> bool {
    a.connected && b.connected && a.is_player2 == b.is_player2
}

/// Pure: is a same-jumper conflict still unresolved? True when the jumpers
/// conflict and the saved assignment does not pin both player sides.
fn conflict_unresolved(jumpers_conflict: bool, p1_assigned: bool, p2_assigned: bool) -> bool {
    jumpers_conflict && (!p1_assigned || !p2_assigned)
}

/// The jumper-derived P1/P2 serial pair to auto-save for a clean, unambiguous pad
/// pair: both connected, both with real serials, and *distinct* jumpers. The SDK
/// orders slot 0 = P1-jumper and slot 1 = P2-jumper, so slot 0's serial is P1 and
/// slot 1's is P2. Returns `None` when the pair is incomplete or ambiguous (same
/// jumper), leaving it for manual assignment.
pub fn jumper_derived_pair(a: &SmxInfo, b: &SmxInfo) -> Option<(String, String)> {
    let distinct = a.connected
        && b.connected
        && a.has_serial_number
        && b.has_serial_number
        && a.is_player2 != b.is_player2;
    distinct.then(|| (a.serial.clone(), b.serial.clone()))
}

/// True when both pads are connected and report the *same* P1/P2 jumper, so the
/// SDK can't order them by jumper alone and the user should assign them manually.
pub fn same_jumper_conflict() -> bool {
    jumpers_conflict(&get_info(0), &get_info(1))
}

/// Whether to surface the "both pads share a jumper, assign them" warning: an
/// unresolved same-jumper conflict (no saved assignment covers both pads). Single
/// source of truth for the main-Menu badge, the options-page warning, and the
/// auto-prompt, so they always agree.
pub fn conflict_warning_active() -> bool {
    let (p1, p2) = crate::config::smx_pad_assignment();
    conflict_unresolved(same_jumper_conflict(), p1.is_some(), p2.is_some())
}

/// Light each pad a solid colour by slot (`colors[0]` = P1 slot, `colors[1]` =
/// P2 slot; `None` turns that pad off). One-shot, so re-send to hold the colour.
pub fn set_player_lights(colors: [Option<[u8; 3]>; 2]) {
    let Some(s) = SHARED.get() else { return };
    // A full 25-LED-per-pad frame (9 panels × 25 LEDs × 3); firmware on 16-LED
    // pads ignores the inner-ring bytes, so one buffer size covers both.
    let mut buf = vec![0u8; 2 * BYTES_PER_PAD_25];
    for (pad, color) in colors.iter().enumerate() {
        let Some(rgb) = color else { continue };
        let base = pad * BYTES_PER_PAD_25;
        for led in buf[base..base + BYTES_PER_PAD_25].chunks_exact_mut(3) {
            led.copy_from_slice(rgb);
        }
    }
    s.manager.set_lights(&buf);
}

/// Re-enable the pads' built-in automatic lighting (call when leaving a screen
/// that drove the lights directly, so the pads stop showing our static colour).
pub fn reenable_auto_lights() {
    if let Some(s) = SHARED.get() {
        s.manager.reenable_auto_lights();
    }
}

/// Player-indicator colours: P1 = blue, P2 = red. Used by the pad-assignment
/// screen so the user can see which physical pad is which without reading serials.
pub const PLAYER1_LIGHT: [u8; 3] = [0, 80, 255];
pub const PLAYER2_LIGHT: [u8; 3] = [255, 0, 0];
/// Shown when a connected pad's player side is ambiguous (both pads share a
/// jumper and no assignment resolves them).
pub const PLAYER_UNCONFIGURED_LIGHT: [u8; 3] = [110, 110, 110];

/// On-screen amber used to flag an unresolved pad-assignment conflict (the main
/// Menu badge and the assignment screen). RGB only; callers apply their own alpha.
pub const CONFLICT_WARNING_RGB: [f32; 3] = [1.0, 0.78, 0.2];

/// Pure: indicator colour for a slot. P1 (slot 0) blue, P2 (slot 1) red, white
/// when the assignment is ambiguous, `None` for an empty slot.
fn indicator_color(connected: bool, ambiguous: bool, slot: usize) -> Option<[u8; 3]> {
    if !connected {
        None
    } else if ambiguous {
        Some(PLAYER_UNCONFIGURED_LIGHT)
    } else if slot == 1 {
        Some(PLAYER2_LIGHT)
    } else {
        Some(PLAYER1_LIGHT)
    }
}

/// Per-slot indicator colours for the StepManiaX options page: P1 (slot 0) blue,
/// P2 (slot 1) red, or white when the assignment is ambiguous; `None` for an
/// empty slot. Recomputed each frame so a live swap is reflected immediately.
pub fn player_indicator_colors() -> [Option<[u8; 3]>; 2] {
    let ambiguous = conflict_warning_active();
    std::array::from_fn(|slot| indicator_color(get_info(slot).connected, ambiguous, slot))
}

// ─── Internal Event Dispatch ─────────────────────────────────────────────────

fn dispatch_event(shared: &SmxShared, event: SmxEvent) {
    match event {
        SmxEvent::Connected { pad, ref info } => {
            if pad >= shared.uuid.len() {
                return;
            }
            // Reset the delta baseline so a reconnected pad starts from "all released".
            shared.prev_input[pad].store(0, Ordering::Relaxed);

            // Cache the stable device UUID + serial for the input/disconnect
            // handlers and friendly trigger labels.
            *shared.uuid[pad].lock().unwrap() = uuid_from_bytes(info.serial.as_bytes());
            *shared.serial[pad].lock().unwrap() = info.serial.clone();

            log::info!(
                "SMX: pad {pad} connected (P{} slot, jumper P{}, fw {}, serial {}, has_serial={})",
                if pad == 1 { 2 } else { 1 },
                if info.is_player2 { 2 } else { 1 },
                info.firmware_version,
                info.serial,
                info.has_serial_number,
            );

            // Assign a serial if the pad lacks one. This must NOT run in the
            // callback (it locks the SDK state we are already holding), so defer
            // it to a short-lived thread that acquires the lock once the USB
            // loop releases it. The in-flight guard collapses duplicate requests.
            if !info.has_serial_number
                && !shared.serial_assign_inflight.swap(true, Ordering::AcqRel)
            {
                log::info!("SMX: pad {pad} has no serial; scheduling assignment");
                std::thread::spawn(|| {
                    if let Some(s) = SHARED.get() {
                        s.manager.set_serial_numbers();
                        s.serial_assign_inflight.store(false, Ordering::Release);
                        log::info!("SMX: serial assignment complete");
                    }
                });
            }

            let name = format!(
                "StepManiaX P{} (fw {})",
                if pad == 1 { 2 } else { 1 },
                info.firmware_version
            );
            let sys_event = GpSystemEvent::Connected {
                name,
                id: pad_device_id(pad),
                vendor_id: Some(SMX_USB_VENDOR_ID),
                product_id: Some(SMX_USB_PRODUCT_ID),
                backend: PadBackend::Smx,
                initial: false,
            };
            for listener in shared.sys_listeners.lock().unwrap().iter() {
                listener(sys_event.clone());
            }
        }
        SmxEvent::Disconnected { pad } => {
            if pad >= shared.uuid.len() {
                return;
            }
            shared.prev_input[pad].store(0, Ordering::Relaxed);
            log::info!("SMX: pad {pad} disconnected");
            let sys_event = GpSystemEvent::Disconnected {
                name: format!("StepManiaX pad {pad}"),
                id: pad_device_id(pad),
                backend: PadBackend::Smx,
                initial: false,
            };
            for listener in shared.sys_listeners.lock().unwrap().iter() {
                listener(sys_event.clone());
            }
        }
        SmxEvent::InputState { pad, state } => {
            if pad >= shared.uuid.len() {
                return;
            }
            // The SDK only fires InputState when the pad's bitmask changes, but it
            // reports the whole mask. Emit events only for panels that actually
            // flipped since the last dispatch.
            let prev = shared.prev_input[pad].swap(state, Ordering::Relaxed);
            let changed = prev ^ state;
            if changed == 0 {
                return;
            }
            log::debug!(
                "SMX: pad {pad} input {prev:#06x} -> {state:#06x} (changed {changed:#06x})"
            );

            let timestamp = Instant::now();
            let host_nanos = crate::engine::host_time::now_nanos();
            let id = pad_device_id(pad);
            let uuid = *shared.uuid[pad].lock().unwrap();

            let listeners = shared.input_listeners.lock().unwrap();
            for panel in 0..PANEL_COUNT {
                if changed & (1 << panel) == 0 {
                    continue;
                }
                let pressed = (state & (1 << panel)) != 0;
                let event = PadEvent::RawButton {
                    id,
                    timestamp,
                    host_nanos,
                    code: PadCode(panel as u32),
                    uuid,
                    value: if pressed { 1.0 } else { 0.0 },
                    pressed,
                };
                for listener in listeners.iter() {
                    listener(event);
                }
            }
        }
        _ => {}
    }
}

/// Runtime device index for an SMX pad slot.
///
/// `PadId` is used by the input pipeline as a small per-device index into
/// fixed-size slot arrays (`usize::from(id) * pad_stride`), so it must stay
/// small — the pad slot (0 or 1) is the natural choice. Stable cross-run
/// identity is carried separately by the device UUID, not the `PadId`.
//
// NOTE: this can collide with indices assigned by the native gamepad backends
// if other pads are connected at the same time; a shared id allocator across
// backends would be needed to fully disambiguate.
fn pad_device_id(pad: usize) -> PadId {
    PadId(pad as u32)
}

/// SMX panel index → 3x3-grid label, matching the SDK's panel naming.
const PANEL_NAMES: [&str; PANEL_COUNT] = ["UL", "U", "UR", "L", "C", "R", "DL", "D", "DR"];

/// Friendly label for an SMX trigger, e.g. `SMX P1 R`.
///
/// `device` is the pad slot (the `PadId`/device index carried by a binding or
/// raw event) and `code` is the panel index. The slot is authoritative for the
/// player side (slot 0 = P1, slot 1 = P2, per the pad→player assignment), so the
/// label names the player rather than the opaque serial. Returns `None` unless
/// that slot currently has a connected SMX pad and the code is in range, so
/// callers can fall back to a generic label.
///
/// NOTE: identification is by slot index, which can collide with a native
/// gamepad sharing that index (see `pad_device_id`); the label is best-effort.
pub fn trigger_label(device: usize, code: u32) -> Option<String> {
    let s = SHARED.get()?;
    let panel = PANEL_NAMES.get(code as usize)?;
    if device >= s.uuid.len() {
        return None;
    }
    // Only label slots that currently hold a connected SMX pad: the uuid is
    // zeroed until a pad connects and caches its identity.
    if *s.uuid[device].lock().unwrap() == [0u8; 16] {
        return None;
    }
    let player = if device == 1 { 2 } else { 1 };
    Some(format!("SMX P{player} {panel}"))
}

#[cfg(test)]
mod tests {
    use super::{
        PLAYER_UNCONFIGURED_LIGHT, PLAYER1_LIGHT, PLAYER2_LIGHT, PadConfigData, PanelThresholds,
        conflict_unresolved, indicator_color, jumper_derived_pair, jumpers_conflict,
        preset_thresholds,
    };
    use crate::config::SmxPadPreset;
    use rustmaniax_sdk::SmxInfo;

    #[test]
    fn is_smx_usb_device_requires_both_vid_and_pid() {
        use super::is_smx_usb_device;
        use rustmaniax_sdk::{SMX_USB_PRODUCT_ID, SMX_USB_VENDOR_ID};

        let (vid, pid) = (SMX_USB_VENDOR_ID, SMX_USB_PRODUCT_ID);
        assert!(is_smx_usb_device(Some(vid), Some(pid)));
        // Arduino's vendor id (0x2341) is shared by many devices, so matching the
        // vendor alone must never be treated as a StepManiaX pad.
        assert!(!is_smx_usb_device(Some(vid), Some(pid ^ 0x1)));
        assert!(!is_smx_usb_device(Some(vid ^ 0x1), Some(pid)));
        assert!(!is_smx_usb_device(Some(vid), None));
        assert!(!is_smx_usb_device(None, None));
    }

    #[test]
    fn native_smx_owns_device_only_when_smx_input_on() {
        use super::native_smx_owns_device_with_flag as owns;
        use rustmaniax_sdk::{SMX_USB_PRODUCT_ID, SMX_USB_VENDOR_ID};

        let (vid, pid) = (Some(SMX_USB_VENDOR_ID), Some(SMX_USB_PRODUCT_ID));
        // The pad is skipped only while native StepManiaX input is on; with it
        // off the pad must stay available to the generic gamepad backends.
        assert!(owns(vid, pid, true));
        assert!(!owns(vid, pid, false));
        // A non-SMX controller is never skipped, even with native input on.
        assert!(!owns(Some(0x046D), Some(0xC216), true));
    }

    #[test]
    fn pad_config_data_settings_round_trips() {
        let mut data = PadConfigData {
            panels: [PanelThresholds {
                fsr_low: [0; 4],
                fsr_high: [0; 4],
                load_cell_low: 0,
                load_cell_high: 0,
            }; 9],
            // Mixed enable bits to exercise the nibble packing both ways. Byte 4's
            // low nibble must stay 0 — there is no panel 9, so those bits are
            // unused and would (correctly) not survive a per-panel round trip.
            enabled_sensors: [0x12, 0x34, 0x56, 0x78, 0x90],
            auto_calibration_max_tare: 0xFFFF,
            panel_debounce_us: 4500,
        };
        for (i, p) in data.panels.iter_mut().enumerate() {
            let b = i as u8;
            p.fsr_low = [b, b + 1, b + 2, b + 3];
            p.fsr_high = [b + 4, b + 5, b + 6, b + 7];
            p.load_cell_low = b + 8;
            p.load_cell_high = b + 9;
        }
        let settings = data.to_settings();
        // Human-readable: e.g. "Panel0.FsrLow" -> "0 1 2 3", "DebounceMs" -> "4.5".
        assert!(
            settings
                .iter()
                .any(|(k, v)| k == "Panel0.FsrLow" && v == "0 1 2 3")
        );
        assert!(
            settings
                .iter()
                .any(|(k, v)| k == "DebounceMs" && v == "4.5")
        );
        assert_eq!(PadConfigData::from_settings(&settings), Some(data));

        // Missing a required key -> None.
        let mut missing = data.to_settings();
        missing.retain(|(k, _)| k != "Panel3.FsrHigh");
        assert_eq!(PadConfigData::from_settings(&missing), None);
        assert_eq!(PadConfigData::from_settings(&[]), None);
    }

    #[test]
    fn preset_thresholds_match_official_values() {
        let low = preset_thresholds(SmxPadPreset::Low);
        assert_eq!(
            (
                low.load_cell_low,
                low.load_cell_high,
                low.fsr_low,
                low.fsr_high
            ),
            (70, 80, 217, 218)
        );

        let med = preset_thresholds(SmxPadPreset::Medium);
        assert_eq!(
            (
                med.load_cell_low,
                med.load_cell_high,
                med.fsr_low,
                med.fsr_high
            ),
            (33, 42, 174, 175)
        );
        // Center panel uses its own pair.
        assert_eq!(
            (
                med.load_cell_low_center,
                med.load_cell_high_center,
                med.fsr_low_center,
                med.fsr_high_center
            ),
            (35, 60, 199, 200)
        );

        let high = preset_thresholds(SmxPadPreset::High);
        assert_eq!(
            (
                high.load_cell_low,
                high.load_cell_high,
                high.fsr_low,
                high.fsr_high
            ),
            (20, 25, 152, 153)
        );
    }

    /// Build an `SmxInfo` with just the fields the assignment logic reads.
    fn info(connected: bool, is_player2: bool, has_serial: bool, serial: &str) -> SmxInfo {
        SmxInfo {
            connected,
            is_player2,
            has_serial_number: has_serial,
            serial: serial.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn jumpers_conflict_only_when_both_connected_same_jumper() {
        let p1 = info(true, false, true, "A");
        // Distinct jumpers: no conflict.
        assert!(!jumpers_conflict(&p1, &info(true, true, true, "B")));
        // Same jumper, both connected: conflict.
        assert!(jumpers_conflict(&p1, &info(true, false, true, "B")));
        // Same jumper but one disconnected: no conflict (the lone pad orders fine).
        assert!(!jumpers_conflict(&p1, &info(false, false, false, "")));
    }

    #[test]
    fn conflict_unresolved_needs_both_sides_assigned() {
        // No jumper conflict: never unresolved, whatever the assignment.
        assert!(!conflict_unresolved(false, false, false));
        // Conflict, nothing assigned: unresolved.
        assert!(conflict_unresolved(true, false, false));
        // Conflict, only one side assigned: still unresolved.
        assert!(conflict_unresolved(true, true, false));
        assert!(conflict_unresolved(true, false, true));
        // Conflict, both sides assigned: resolved.
        assert!(!conflict_unresolved(true, true, true));
    }

    #[test]
    fn jumper_derived_pair_orders_by_slot_when_distinct() {
        let slot0 = info(true, false, true, "P1SERIAL");
        let slot1 = info(true, true, true, "P2SERIAL");
        // Distinct jumpers: slot 0 -> P1, slot 1 -> P2.
        assert_eq!(
            jumper_derived_pair(&slot0, &slot1),
            Some(("P1SERIAL".to_owned(), "P2SERIAL".to_owned()))
        );
        // Same jumper: ambiguous, leave for manual assignment.
        assert_eq!(
            jumper_derived_pair(&slot0, &info(true, false, true, "P2SERIAL")),
            None
        );
        // Missing serial: not safe to pin.
        assert_eq!(
            jumper_derived_pair(&slot0, &info(true, true, false, "")),
            None
        );
        // Only one pad connected: no pair.
        assert_eq!(
            jumper_derived_pair(&slot0, &info(false, false, false, "")),
            None
        );
    }

    #[test]
    fn indicator_color_maps_slot_to_player_colour() {
        // Empty slot: no colour.
        assert_eq!(indicator_color(false, false, 0), None);
        // Connected, unambiguous: slot 0 blue (P1), slot 1 red (P2).
        assert_eq!(indicator_color(true, false, 0), Some(PLAYER1_LIGHT));
        assert_eq!(indicator_color(true, false, 1), Some(PLAYER2_LIGHT));
        // Ambiguous: white regardless of slot.
        assert_eq!(
            indicator_color(true, true, 0),
            Some(PLAYER_UNCONFIGURED_LIGHT)
        );
        assert_eq!(
            indicator_color(true, true, 1),
            Some(PLAYER_UNCONFIGURED_LIGHT)
        );
    }
}
