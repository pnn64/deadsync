use std::sync::{LazyLock, Mutex};

use arrayvec::ArrayVec;
use deadsync_input::PAD_ID_COUNT_CAP;

use crate::backend::{PAD_ORDER_BACKENDS, PadOrderBackend};

/// Maximum UUIDs persisted per backend, bounding saved order growth.
pub const PAD_ORDER_CAP: usize = PAD_ID_COUNT_CAP - 1;
const PAD_ORDER_BACKEND_COUNT: usize = PAD_ORDER_BACKENDS.len();

type PadOrderList = ArrayVec<[u8; 16], PAD_ORDER_CAP>;

struct PadDeviceOrder {
    lists: [PadOrderList; PAD_ORDER_BACKEND_COUNT],
}

impl Default for PadDeviceOrder {
    fn default() -> Self {
        Self {
            lists: std::array::from_fn(|_| PadOrderList::new()),
        }
    }
}

impl PadDeviceOrder {
    #[inline(always)]
    fn list(&self, backend: PadOrderBackend) -> &PadOrderList {
        &self.lists[pad_order_backend_index(backend)]
    }

    #[inline(always)]
    fn list_mut(&mut self, backend: PadOrderBackend) -> &mut PadOrderList {
        &mut self.lists[pad_order_backend_index(backend)]
    }

    fn clear(&mut self) {
        for list in &mut self.lists {
            list.clear();
        }
    }
}

#[inline(always)]
const fn pad_order_backend_index(backend: PadOrderBackend) -> usize {
    match backend {
        PadOrderBackend::RawInput => 0,
        PadOrderBackend::Wgi => 1,
        PadOrderBackend::IoHid => 2,
        PadOrderBackend::Hidraw => 3,
        PadOrderBackend::LinuxEvdev => 4,
        PadOrderBackend::FreeBsdEvdev => 5,
    }
}

/// Stable pad index assignment result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadOrderAssignment {
    pub index: u32,
    pub changed: bool,
}

/// Append-only, per-backend order of pad device UUIDs. The index of a UUID in
/// its backend list is the stable `PadId` that pad receives. Storage is bounded
/// and inline so discovering a pad never enters the allocator.
static PAD_DEVICE_ORDER: LazyLock<Mutex<PadDeviceOrder>> =
    LazyLock::new(|| Mutex::new(PadDeviceOrder::default()));

/// Stable `PadId` index for `uuid` on the given backend.
///
/// Returns the UUID's existing slot, or appends it and returns the new slot.
/// Append-only: known devices are never renumbered, so per-pad mappings stay
/// bound to the same physical pad. `changed` tells the config owner whether it
/// should persist the new order.
/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn pad_index_for_uuid(backend: PadOrderBackend, uuid: [u8; 16]) -> PadOrderAssignment {
    let mut order = PAD_DEVICE_ORDER.lock().unwrap();
    let list = order.list_mut(backend);
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
/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn load_pad_order_serialized(backend: PadOrderBackend, raw: &str) {
    let parsed = sanitize(raw.split(',').filter_map(uuid_from_hex));
    let mut order = PAD_DEVICE_ORDER.lock().unwrap();
    *order.list_mut(backend) = parsed;
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

#[must_use]
pub fn pad_order_ini_lines() -> Vec<(&'static str, String)> {
    let mut lines = Vec::with_capacity(PAD_ORDER_BACKENDS.len());
    for backend in PAD_ORDER_BACKENDS {
        lines.push((pad_order_ini_key(backend), serialized_pad_order(backend)));
    }
    lines
}

#[must_use]
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

#[must_use]
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
/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn reset_pad_order() {
    PAD_DEVICE_ORDER.lock().unwrap().clear();
}

/// Comma-separated hex serialization of `backend`'s order, empty when none.
/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn serialized_pad_order(backend: PadOrderBackend) -> String {
    serialize_uuid_list(PAD_DEVICE_ORDER.lock().unwrap().list(backend))
}

/// Input backends that persist stable pad order.
#[must_use]
pub const fn all_pad_order_backends() -> [PadOrderBackend; 6] {
    PAD_ORDER_BACKENDS
}

/// Drop duplicates, keeping first occurrence, and cap the list length.
fn sanitize(list: impl IntoIterator<Item = [u8; 16]>) -> PadOrderList {
    let mut out = PadOrderList::new();
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

#[cfg(test)]
fn uuid_to_hex(uuid: &[u8; 16]) -> String {
    let mut s = String::with_capacity(32);
    append_uuid_hex(&mut s, uuid);
    s
}

fn serialize_uuid_list(list: &[[u8; 16]]) -> String {
    let Some(capacity) = list
        .len()
        .checked_mul(33)
        .and_then(|len| len.checked_sub(1))
    else {
        return String::new();
    };
    let mut out = String::with_capacity(capacity);
    for (index, uuid) in list.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        append_uuid_hex(&mut out, uuid);
    }
    out
}

#[inline]
fn append_uuid_hex(out: &mut String, uuid: &[u8; 16]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for &byte in uuid {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
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

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub mod bench_support {
    use std::collections::BTreeMap;

    use super::*;

    fn fixture_uuid(index: usize) -> [u8; 16] {
        let mut uuid = [0_u8; 16];
        let seed = (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        uuid[..8].copy_from_slice(&seed.to_le_bytes());
        uuid[8..].copy_from_slice(&seed.rotate_left(29).to_be_bytes());
        uuid
    }

    fn checksum_uuids(list: &[[u8; 16]]) -> u64 {
        list.iter().flatten().fold(0_u64, |checksum, &byte| {
            checksum.rotate_left(5) ^ u64::from(byte)
        })
    }

    fn checksum_text(text: &str) -> u64 {
        text.bytes().fold(0_u64, |checksum, byte| {
            checksum.wrapping_mul(131).wrapping_add(u64::from(byte))
        })
    }

    fn assignment_checksum(index: usize, changed: bool, list: &[[u8; 16]]) -> u64 {
        checksum_uuids(list) ^ (index as u64).rotate_left(17) ^ (u64::from(changed) << 63)
    }

    #[must_use]
    pub fn assignment_old(seed: usize) -> u64 {
        let mut order: BTreeMap<PadOrderBackend, Vec<[u8; 16]>> = BTreeMap::new();
        let list = order.entry(PadOrderBackend::RawInput).or_default();
        for offset in 0..PAD_ORDER_CAP {
            let uuid = fixture_uuid(seed.wrapping_add(offset));
            if !list.contains(&uuid) {
                list.push(uuid);
            }
        }
        let known = fixture_uuid(seed.wrapping_add(PAD_ORDER_CAP / 2));
        let index = list.iter().position(|uuid| *uuid == known).unwrap();
        assignment_checksum(index, false, list)
    }

    #[must_use]
    pub fn assignment_new(seed: usize) -> u64 {
        let mut order = PadDeviceOrder::default();
        let list = order.list_mut(PadOrderBackend::RawInput);
        for offset in 0..PAD_ORDER_CAP {
            let uuid = fixture_uuid(seed.wrapping_add(offset));
            if !list.contains(&uuid) {
                list.push(uuid);
            }
        }
        let known = fixture_uuid(seed.wrapping_add(PAD_ORDER_CAP / 2));
        let index = list.iter().position(|uuid| *uuid == known).unwrap();
        assignment_checksum(index, false, list)
    }

    fn sanitize_old(list: Vec<[u8; 16]>) -> Vec<[u8; 16]> {
        let mut out = Vec::with_capacity(list.len().min(PAD_ORDER_CAP));
        for uuid in list {
            if out.len() >= PAD_ORDER_CAP {
                break;
            }
            if !out.contains(&uuid) {
                out.push(uuid);
            }
        }
        out
    }

    #[must_use]
    pub fn parse_old(raw: &str) -> u64 {
        let parsed = sanitize_old(raw.split(',').filter_map(uuid_from_hex).collect());
        checksum_uuids(&parsed)
    }

    #[must_use]
    pub fn parse_new(raw: &str) -> u64 {
        let parsed = sanitize(raw.split(',').filter_map(uuid_from_hex));
        checksum_uuids(&parsed)
    }

    fn uuid_to_hex_old(uuid: &[u8; 16]) -> String {
        use std::fmt::Write as _;

        let mut out = String::with_capacity(32);
        for byte in uuid {
            let _ = write!(out, "{byte:02x}");
        }
        out
    }

    #[must_use]
    pub fn serialize_old(seed: usize) -> u64 {
        let list: [[u8; 16]; PAD_ORDER_CAP] =
            std::array::from_fn(|offset| fixture_uuid(seed.wrapping_add(offset)));
        let text = list
            .iter()
            .map(uuid_to_hex_old)
            .collect::<Vec<_>>()
            .join(",");
        checksum_text(&text)
    }

    #[must_use]
    pub fn serialize_new(seed: usize) -> u64 {
        let list: [[u8; 16]; PAD_ORDER_CAP] =
            std::array::from_fn(|offset| fixture_uuid(seed.wrapping_add(offset)));
        checksum_text(&serialize_uuid_list(&list))
    }

    #[must_use]
    pub fn serialized_fixture() -> String {
        let list: PadOrderList = (0..PAD_ORDER_CAP).map(fixture_uuid).collect();
        let mut text = serialize_uuid_list(&list);
        text.push_str(",invalid,");
        append_uuid_hex(&mut text, &fixture_uuid(0));
        text
    }
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
        assert_eq!(sanitize([a, b, a, b]).as_slice(), &[a, b]);

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
    fn streaming_sanitize_keeps_first_valid_occurrences() {
        let first = [0x11_u8; 16];
        let second = [0xaa_u8; 16];
        let raw = format!(
            "bad, {}, {}, {}, short",
            uuid_to_hex(&first).to_uppercase(),
            uuid_to_hex(&second),
            uuid_to_hex(&first)
        );

        let parsed = sanitize(raw.split(',').filter_map(uuid_from_hex));

        assert_eq!(parsed.as_slice(), &[first, second]);
    }

    #[test]
    fn direct_serialization_is_canonical_and_delimited_once() {
        let first = [0x01_u8; 16];
        let last = [0xfe_u8; 16];

        let serialized = serialize_uuid_list(&[first, last]);

        assert_eq!(
            serialized,
            "01010101010101010101010101010101,fefefefefefefefefefefefefefefefe"
        );
        assert_eq!(serialized.matches(',').count(), 1);
    }

    #[test]
    fn backend_index_table_matches_public_backend_order() {
        for (expected, backend) in PAD_ORDER_BACKENDS.into_iter().enumerate() {
            assert_eq!(pad_order_backend_index(backend), expected);
        }
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
