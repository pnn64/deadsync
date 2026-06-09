//! Stable per-pad `PadId` assignment.
//!
//! Generic HID dance pads (FSR / Teensy class) are assigned a runtime
//! `PadId(u32)` index in the order the OS enumerates them. That order is not
//! stable across reboots/replugs, so two pads can swap which one is "Pad 0" vs
//! "Pad 1" — and since per-pad arrow bindings are pinned to that index, the user
//! has to re-map every launch.
//!
//! To fix this we persist an **append-only, per-backend** list of device UUIDs.
//! A UUID's position in its backend's list is the `PadId` index that physical
//! pad always receives. Known UUIDs keep their slot; unknown UUIDs are appended
//! (never inserted/sorted), so existing pads are never renumbered. This mirrors
//! how SMX pads pin a serial → player slot, but leaves SMX untouched.

use super::*;
use deadsync_input_native::{PAD_ORDER_BACKENDS, PadOrderBackend};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{LazyLock, Mutex};

/// Maximum UUIDs persisted per backend (bounds the saved array's growth).
const PAD_ORDER_CAP: usize = 64;

/// Input backends that persist a stable pad order. SMX is intentionally excluded
/// — it has its own serial-based assignment.
/// Append-only, per-backend order of pad device UUIDs. The index of a UUID in
/// its backend's vec is the stable `PadId` that pad receives.
static PAD_DEVICE_ORDER: LazyLock<Mutex<BTreeMap<PadOrderBackend, Vec<[u8; 16]>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// Stable `PadId` index for `uuid` on the given backend.
///
/// Returns the UUID's existing slot, or appends it (persisting the updated
/// order) and returns the new slot. Append-only: known devices are never
/// renumbered, so per-pad mappings stay bound to the same physical pad.
pub fn pad_index_for_uuid(backend: PadOrderBackend, uuid: [u8; 16]) -> u32 {
    let (index, changed) = {
        let mut order = PAD_DEVICE_ORDER.lock().unwrap();
        let list = order.entry(backend).or_default();
        if let Some(i) = list.iter().position(|u| *u == uuid) {
            (i, false)
        } else if list.len() >= PAD_ORDER_CAP {
            // Saved order is full; hand out an ephemeral index without persisting.
            (list.len(), false)
        } else {
            list.push(uuid);
            (list.len() - 1, true)
        }
        // Guard dropped here, before save_without_keymaps() re-locks to serialize.
    };
    if changed {
        save_without_keymaps();
    }
    index as u32
}

/// Replace the in-memory order from the loaded config file.
pub(super) fn load_order_from_ini(conf: &SimpleIni) {
    let mut order = PAD_DEVICE_ORDER.lock().unwrap();
    order.clear();
    for backend in PAD_ORDER_BACKENDS {
        let Some(raw) = conf.get("Options", ini_key(backend)) else {
            continue;
        };
        let parsed = sanitize(raw.split(',').filter_map(uuid_from_hex).collect());
        if !parsed.is_empty() {
            order.insert(backend, parsed);
        }
    }
}

/// Clear the persisted order (used when the config file can't be read).
pub(super) fn reset() {
    PAD_DEVICE_ORDER.lock().unwrap().clear();
}

/// Comma-separated hex serialization of `backend`'s order (empty when none).
pub(super) fn serialized(backend: PadOrderBackend) -> String {
    PAD_DEVICE_ORDER
        .lock()
        .unwrap()
        .get(&backend)
        .map(|list| list.iter().map(uuid_to_hex).collect::<Vec<_>>().join(","))
        .unwrap_or_default()
}

pub(super) const fn all_backends() -> [PadOrderBackend; 6] {
    PAD_ORDER_BACKENDS
}

pub(super) const fn ini_key(backend: PadOrderBackend) -> &'static str {
    match backend {
        PadOrderBackend::RawInput => "PadOrderRawInput",
        PadOrderBackend::Wgi => "PadOrderWGI",
        PadOrderBackend::IoHid => "PadOrderIoHid",
        PadOrderBackend::Hidraw => "PadOrderHidraw",
        PadOrderBackend::LinuxEvdev => "PadOrderLinuxEvdev",
        PadOrderBackend::FreeBsdEvdev => "PadOrderFreeBsdEvdev",
    }
}

/// Drop duplicates (keeping first occurrence) and cap the list length.
fn sanitize(list: Vec<[u8; 16]>) -> Vec<[u8; 16]> {
    let mut out: Vec<[u8; 16]> = Vec::with_capacity(list.len().min(PAD_ORDER_CAP));
    for u in list {
        if out.len() >= PAD_ORDER_CAP {
            break;
        }
        if !out.contains(&u) {
            out.push(u);
        }
    }
    out
}

fn uuid_to_hex(uuid: &[u8; 16]) -> String {
    let mut s = String::with_capacity(32);
    for b in uuid {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn uuid_from_hex(s: &str) -> Option<[u8; 16]> {
    let s = s.trim();
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let uuid = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let hex = uuid_to_hex(&uuid);
        assert_eq!(hex, "00112233445566778899aabbccddeeff");
        assert_eq!(uuid_from_hex(&hex), Some(uuid));
    }

    #[test]
    fn rejects_malformed_hex() {
        assert_eq!(uuid_from_hex(""), None);
        assert_eq!(uuid_from_hex("00112233"), None); // too short
        assert_eq!(uuid_from_hex(&"0".repeat(33)), None); // too long
        assert_eq!(uuid_from_hex(&"g".repeat(32)), None); // non-hex
    }

    #[test]
    fn sanitize_dedups_and_caps() {
        let a = [1u8; 16];
        let b = [2u8; 16];
        assert_eq!(sanitize(vec![a, b, a, b]), vec![a, b]);

        let many: Vec<[u8; 16]> = (0..(PAD_ORDER_CAP as u16 + 10))
            .map(|i| {
                let mut u = [0u8; 16];
                u[0..2].copy_from_slice(&i.to_le_bytes());
                u
            })
            .collect();
        assert_eq!(sanitize(many).len(), PAD_ORDER_CAP);
    }
}
