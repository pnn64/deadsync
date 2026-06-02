//! Shared StepManiaX SDK manager.
//!
//! Provides a process-wide `SmxManager` instance that both the input backend
//! and the FSR monitor can use. Events are routed to registered listeners.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use rustmaniax_sdk::{SmxConfig, SmxEvent, SmxInfo, SmxManager, SensorTestData, SensorTestMode};

use crate::engine::input::{GpSystemEvent, PadBackend, uuid_from_bytes};
use deadsync_input::{PadCode, PadEvent, PadId};

/// Number of panels per SMX pad.
pub const PANEL_COUNT: usize = 9;

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
    }
}

/// Get a reference to the shared manager (None if not initialized).
pub fn manager() -> Option<&'static SmxManager> {
    SHARED.get().map(|s| &s.manager)
}

/// Register a listener for pad input events.
pub fn add_input_listener(listener: Box<dyn Fn(PadEvent) + Send>) {
    if let Some(s) = SHARED.get() {
        s.input_listeners.lock().unwrap().push(listener);
    }
}

/// Register a listener for system events (connect/disconnect).
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
/// Serialized byte length of `PadConfigData`:
/// 9 panels x (4 fsr_low + 4 fsr_high + lc_low + lc_high) + enabled[5] + tare(2) + debounce(2).
const PAD_CONFIG_BYTES: usize = PAD_CONFIG_PANELS * 10 + 5 + 2 + 2;

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
    /// Serialize to a compact lowercase-hex blob for `padconfig.ini`.
    pub fn to_hex(&self) -> String {
        let mut bytes = Vec::with_capacity(PAD_CONFIG_BYTES);
        for p in &self.panels {
            bytes.extend_from_slice(&p.fsr_low);
            bytes.extend_from_slice(&p.fsr_high);
            bytes.push(p.load_cell_low);
            bytes.push(p.load_cell_high);
        }
        bytes.extend_from_slice(&self.enabled_sensors);
        bytes.extend_from_slice(&self.auto_calibration_max_tare.to_le_bytes());
        bytes.extend_from_slice(&self.panel_debounce_us.to_le_bytes());
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            let _ = write!(s, "{b:02x}");
        }
        s
    }

    /// Parse a hex blob written by `to_hex`. Returns `None` if malformed.
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.len() != PAD_CONFIG_BYTES * 2 {
            return None;
        }
        let mut bytes = [0u8; PAD_CONFIG_BYTES];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        let mut off = 0;
        let mut panels = [PanelThresholds {
            fsr_low: [0; PAD_CONFIG_SENSORS],
            fsr_high: [0; PAD_CONFIG_SENSORS],
            load_cell_low: 0,
            load_cell_high: 0,
        }; PAD_CONFIG_PANELS];
        for p in panels.iter_mut() {
            p.fsr_low.copy_from_slice(&bytes[off..off + 4]);
            off += 4;
            p.fsr_high.copy_from_slice(&bytes[off..off + 4]);
            off += 4;
            p.load_cell_low = bytes[off];
            off += 1;
            p.load_cell_high = bytes[off];
            off += 1;
        }
        let mut enabled_sensors = [0u8; 5];
        enabled_sensors.copy_from_slice(&bytes[off..off + 5]);
        off += 5;
        let auto_calibration_max_tare = u16::from_le_bytes([bytes[off], bytes[off + 1]]);
        off += 2;
        let panel_debounce_us = u16::from_le_bytes([bytes[off], bytes[off + 1]]);
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
    let config = get_config(pad)?;
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
                "SMX: pad {pad} connected (P{}, fw {}, serial {}, has_serial={})",
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
                if info.is_player2 { 2 } else { 1 },
                info.firmware_version
            );
            let sys_event = GpSystemEvent::Connected {
                name,
                id: pad_device_id(pad),
                vendor_id: Some(0x2341),
                product_id: Some(0x8037),
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
            log::debug!("SMX: pad {pad} input {prev:#06x} -> {state:#06x} (changed {changed:#06x})");

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

/// Friendly label for an SMX trigger, e.g. `SMX[40ea] R`.
///
/// `device` is the pad slot (the `PadId`/device index carried by a binding or
/// raw event) and `code` is the panel index. Returns `None` unless that slot
/// currently has a connected SMX pad and the code is in range, so callers can
/// fall back to a generic label. The serial prefix (first 4 hex chars)
/// disambiguates two pads even when both are assigned to the same player.
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
    let prefix: String = s.serial[device].lock().unwrap().chars().take(4).collect();
    if prefix.is_empty() {
        Some(format!("SMX {panel}"))
    } else {
        Some(format!("SMX[{prefix}] {panel}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{PadConfigData, PanelThresholds, preset_thresholds};
    use crate::config::SmxPadPreset;

    #[test]
    fn pad_config_data_hex_round_trips() {
        let mut data = PadConfigData {
            panels: [PanelThresholds {
                fsr_low: [0; 4],
                fsr_high: [0; 4],
                load_cell_low: 0,
                load_cell_high: 0,
            }; 9],
            enabled_sensors: [0x12, 0x34, 0x56, 0x78, 0x9a],
            auto_calibration_max_tare: 0xFFFF,
            panel_debounce_us: 4000,
        };
        for (i, p) in data.panels.iter_mut().enumerate() {
            let b = i as u8;
            p.fsr_low = [b, b + 1, b + 2, b + 3];
            p.fsr_high = [b + 4, b + 5, b + 6, b + 7];
            p.load_cell_low = b + 8;
            p.load_cell_high = b + 9;
        }
        let hex = data.to_hex();
        assert_eq!(hex.len(), super::PAD_CONFIG_BYTES * 2);
        assert_eq!(PadConfigData::from_hex(&hex), Some(data));
        assert_eq!(PadConfigData::from_hex("nothex"), None);
        assert_eq!(PadConfigData::from_hex(""), None);
    }

    #[test]
    fn preset_thresholds_match_official_values() {
        let low = preset_thresholds(SmxPadPreset::Low);
        assert_eq!(
            (low.load_cell_low, low.load_cell_high, low.fsr_low, low.fsr_high),
            (70, 80, 217, 218)
        );

        let med = preset_thresholds(SmxPadPreset::Medium);
        assert_eq!(
            (med.load_cell_low, med.load_cell_high, med.fsr_low, med.fsr_high),
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
}
