use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{LazyLock, Mutex};

use crate::backend::{PAD_ORDER_BACKENDS, PadOrderBackend};

/// Maximum UUIDs persisted per backend, bounding saved order growth.
pub const PAD_ORDER_CAP: usize = 64;

/// Stable pad index assignment result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadOrderAssignment {
    pub index: u32,
    pub changed: bool,
}

/// Append-only, per-backend order of pad device UUIDs. The index of a UUID in
/// its backend vec is the stable `PadId` that pad receives.
static PAD_DEVICE_ORDER: LazyLock<Mutex<BTreeMap<PadOrderBackend, Vec<[u8; 16]>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// Stable `PadId` index for `uuid` on the given backend.
///
/// Returns the UUID's existing slot, or appends it and returns the new slot.
/// Append-only: known devices are never renumbered, so per-pad mappings stay
/// bound to the same physical pad. `changed` tells the config owner whether it
/// should persist the new order.
pub fn pad_index_for_uuid(backend: PadOrderBackend, uuid: [u8; 16]) -> PadOrderAssignment {
    let mut order = PAD_DEVICE_ORDER.lock().unwrap();
    let list = order.entry(backend).or_default();
    if let Some(i) = list.iter().position(|u| *u == uuid) {
        return PadOrderAssignment {
            index: i as u32,
            changed: false,
        };
    }
    if list.len() >= PAD_ORDER_CAP {
        return PadOrderAssignment {
            index: list.len() as u32,
            changed: false,
        };
    }
    list.push(uuid);
    PadOrderAssignment {
        index: (list.len() - 1) as u32,
        changed: true,
    }
}

/// Replace one backend's in-memory order from a comma-separated hex string.
pub fn load_pad_order_serialized(backend: PadOrderBackend, raw: &str) {
    let parsed = sanitize(raw.split(',').filter_map(uuid_from_hex).collect());
    let mut order = PAD_DEVICE_ORDER.lock().unwrap();
    if parsed.is_empty() {
        order.remove(&backend);
    } else {
        order.insert(backend, parsed);
    }
}

pub const DEFAULT_PAD_ORDER_INI_LINES: [(&str, &str); 6] = [
    ("PadOrderRawInput", ""),
    ("PadOrderWGI", ""),
    ("PadOrderIoHid", ""),
    ("PadOrderHidraw", ""),
    ("PadOrderLinuxEvdev", ""),
    ("PadOrderFreeBsdEvdev", ""),
];

/// Replace the full in-memory order from `[Options]` INI entries.
pub fn load_pad_order_from_ini_entries<'a, I>(entries: Option<I>)
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    reset_pad_order();
    let Some(entries) = entries else {
        return;
    };
    for (key, value) in entries {
        if let Some(backend) = pad_order_backend_from_ini_key(key) {
            load_pad_order_serialized(backend, value);
        }
    }
}

pub fn pad_order_ini_lines() -> Vec<(&'static str, String)> {
    let mut lines = Vec::with_capacity(PAD_ORDER_BACKENDS.len());
    for backend in PAD_ORDER_BACKENDS {
        lines.push((pad_order_ini_key(backend), serialized_pad_order(backend)));
    }
    lines
}

pub const fn pad_order_ini_key(backend: PadOrderBackend) -> &'static str {
    match backend {
        PadOrderBackend::RawInput => "PadOrderRawInput",
        PadOrderBackend::Wgi => "PadOrderWGI",
        PadOrderBackend::IoHid => "PadOrderIoHid",
        PadOrderBackend::Hidraw => "PadOrderHidraw",
        PadOrderBackend::LinuxEvdev => "PadOrderLinuxEvdev",
        PadOrderBackend::FreeBsdEvdev => "PadOrderFreeBsdEvdev",
    }
}

pub fn pad_order_backend_from_ini_key(key: &str) -> Option<PadOrderBackend> {
    match key {
        "PadOrderRawInput" => Some(PadOrderBackend::RawInput),
        "PadOrderWGI" => Some(PadOrderBackend::Wgi),
        "PadOrderIoHid" => Some(PadOrderBackend::IoHid),
        "PadOrderHidraw" => Some(PadOrderBackend::Hidraw),
        "PadOrderLinuxEvdev" => Some(PadOrderBackend::LinuxEvdev),
        "PadOrderFreeBsdEvdev" => Some(PadOrderBackend::FreeBsdEvdev),
        _ => None,
    }
}

/// Clear every backend's in-memory order.
pub fn reset_pad_order() {
    PAD_DEVICE_ORDER.lock().unwrap().clear();
}

/// Comma-separated hex serialization of `backend`'s order, empty when none.
pub fn serialized_pad_order(backend: PadOrderBackend) -> String {
    PAD_DEVICE_ORDER
        .lock()
        .unwrap()
        .get(&backend)
        .map(|list| list.iter().map(uuid_to_hex).collect::<Vec<_>>().join(","))
        .unwrap_or_default()
}

/// Input backends that persist stable pad order.
pub const fn all_pad_order_backends() -> [PadOrderBackend; 6] {
    PAD_ORDER_BACKENDS
}

/// Drop duplicates, keeping first occurrence, and cap the list length.
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
        assert_eq!(uuid_from_hex("00112233"), None);
        assert_eq!(uuid_from_hex(&"0".repeat(33)), None);
        assert_eq!(uuid_from_hex(&"g".repeat(32)), None);
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

    #[test]
    fn assigning_known_uuid_does_not_change_order() {
        reset_pad_order();
        let uuid = [7u8; 16];
        assert_eq!(
            pad_index_for_uuid(PadOrderBackend::RawInput, uuid),
            PadOrderAssignment {
                index: 0,
                changed: true
            }
        );
        assert_eq!(
            pad_index_for_uuid(PadOrderBackend::RawInput, uuid),
            PadOrderAssignment {
                index: 0,
                changed: false
            }
        );
    }

    #[test]
    fn pad_order_ini_keys_round_trip_backends() {
        assert_eq!(DEFAULT_PAD_ORDER_INI_LINES.len(), PAD_ORDER_BACKENDS.len());
        for backend in PAD_ORDER_BACKENDS {
            let key = pad_order_ini_key(backend);
            assert_eq!(pad_order_backend_from_ini_key(key), Some(backend));
            assert!(DEFAULT_PAD_ORDER_INI_LINES.contains(&(key, "")));
        }
        assert_eq!(pad_order_backend_from_ini_key("PadOrderrawinput"), None);
        assert_eq!(pad_order_backend_from_ini_key("PadOrderSmx"), None);
    }

    #[test]
    fn load_pad_order_from_ini_entries_replaces_all_backend_orders() {
        reset_pad_order();
        let raw = "00112233445566778899aabbccddeeff";
        load_pad_order_from_ini_entries(Some([
            ("PadOrderRawInput", raw),
            ("PadOrderWGI", "bad"),
            ("Unknown", raw),
        ]));

        assert_eq!(serialized_pad_order(PadOrderBackend::RawInput), raw);
        assert_eq!(serialized_pad_order(PadOrderBackend::Wgi), "");
        assert_eq!(serialized_pad_order(PadOrderBackend::IoHid), "");

        load_pad_order_from_ini_entries::<[(&str, &str); 0]>(None);
        assert_eq!(serialized_pad_order(PadOrderBackend::RawInput), "");
    }

    #[test]
    fn pad_order_ini_lines_use_backend_order() {
        reset_pad_order();
        let uuid = [3u8; 16];
        assert!(pad_index_for_uuid(PadOrderBackend::Hidraw, uuid).changed);

        let lines = pad_order_ini_lines();
        assert_eq!(lines.len(), PAD_ORDER_BACKENDS.len());
        assert_eq!(lines[0], ("PadOrderRawInput", String::new()));
        assert_eq!(lines[3], ("PadOrderHidraw", uuid_to_hex(&uuid)));
    }
}
