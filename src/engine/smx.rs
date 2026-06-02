//! Shared StepManiaX SDK manager.
//!
//! Provides a process-wide `SmxManager` instance that both the input backend
//! and the FSR monitor can use. Events are routed to registered listeners.

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
