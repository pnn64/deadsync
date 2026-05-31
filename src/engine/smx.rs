//! Shared StepManiaX SDK manager.
//!
//! Provides a process-wide `SmxManager` instance that both the input backend
//! and the FSR monitor can use. Events are routed to registered listeners.

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use rustmaniax_sdk::{SmxConfig, SmxEvent, SmxInfo, SmxManager, SensorTestData, SensorTestMode};

use crate::engine::input::{GpSystemEvent, PadBackend, PadCode, PadEvent, PadId, uuid_from_bytes};

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
        }),
        Err(e) => {
            log::warn!("Failed to initialize SMX SDK: {e}");
            return false;
        }
    };

    let _ = SHARED.set(shared);
    true
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
            // Reset the delta baseline so a reconnected pad starts from "all released".
            if pad < shared.prev_input.len() {
                shared.prev_input[pad].store(0, Ordering::Relaxed);
            }

            // Assign serial if missing.
            if !info.has_serial_number {
                shared.manager.set_serial_numbers();
            }

            let name = format!(
                "StepManiaX P{} (fw {})",
                if info.is_player2 { 2 } else { 1 },
                info.firmware_version
            );
            let sys_event = GpSystemEvent::Connected {
                name,
                id: pad_id_from_serial(&info.serial, pad),
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
            if pad < shared.prev_input.len() {
                shared.prev_input[pad].store(0, Ordering::Relaxed);
            }
            let info = shared.manager.get_info(pad);
            let sys_event = GpSystemEvent::Disconnected {
                name: format!("StepManiaX pad {pad}"),
                id: pad_id_from_serial(&info.serial, pad),
                backend: PadBackend::Smx,
                initial: false,
            };
            for listener in shared.sys_listeners.lock().unwrap().iter() {
                listener(sys_event.clone());
            }
        }
        SmxEvent::InputState { pad, state } => {
            if pad >= shared.prev_input.len() {
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

            let timestamp = Instant::now();
            let host_nanos = crate::engine::host_time::now_nanos();
            let info = shared.manager.get_info(pad);
            let id = pad_id_from_serial(&info.serial, pad);
            let uuid = uuid_from_bytes(info.serial.as_bytes());

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

/// Derive a stable PadId from the device serial number.
fn pad_id_from_serial(serial: &str, pad: usize) -> PadId {
    if serial.is_empty() {
        // Fallback if no serial (shouldn't happen after set_serial_numbers).
        return PadId(0x534D5800 + pad as u32);
    }
    // Use a simple hash of the serial to get a stable u32 ID.
    let mut hash: u32 = 0x534D_5800; // "SMX\0"
    for byte in serial.as_bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(*byte as u32);
    }
    PadId(hash)
}
