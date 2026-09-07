//! DeadSync INI keys and UUID serialization for stable controller ordering.

use crate::ini::SimpleIni;
use crate::runtime::save_without_keymaps;
use crate::writer::push_line;
use deadlib_input_native::{PadOrderBackend, pad_index_for_uuid, pad_order, set_pad_order};
use std::fmt::Display;

const PAD_ORDER_FIELDS: [(PadOrderBackend, &str); 6] = [
    (PadOrderBackend::RawInput, "PadOrderRawInput"),
    (PadOrderBackend::Wgi, "PadOrderWGI"),
    (PadOrderBackend::IoHid, "PadOrderIoHid"),
    (PadOrderBackend::Hidraw, "PadOrderHidraw"),
    (PadOrderBackend::LinuxEvdev, "PadOrderLinuxEvdev"),
    (PadOrderBackend::FreeBsdEvdev, "PadOrderFreeBsdEvdev"),
];

pub const DEFAULT_PAD_ORDER_INI_LINES: [(&str, &str); 6] = {
    let mut lines = [("", ""); 6];
    let mut index = 0;
    while index < lines.len() {
        lines[index] = (PAD_ORDER_FIELDS[index].1, "");
        index += 1;
    }
    lines
};

/// Assigns a stable native slot, saving configuration only for a newly recorded UUID.
#[must_use]
pub fn pad_index_for_uuid_saved(backend: PadOrderBackend, uuid: [u8; 16]) -> u32 {
    let assignment = pad_index_for_uuid(backend, uuid);
    if assignment.changed {
        save_without_keymaps();
    }
    assignment.index
}

/// Loads all controller orders, ignoring malformed UUIDs and clearing missing keys.
pub fn load_order_from_ini(conf: &SimpleIni) {
    for (backend, key) in PAD_ORDER_FIELDS {
        let raw = conf.get("Options", key).unwrap_or_default();
        set_pad_order(backend, raw.split(',').filter_map(uuid_from_hex));
    }
}

/// Serializes controller UUIDs in the established DeadSync options-key order.
#[must_use]
pub fn pad_order_ini_lines() -> Vec<(&'static str, String)> {
    PAD_ORDER_FIELDS
        .into_iter()
        .map(|(backend, key)| (key, serialize_uuid_list(&pad_order(backend))))
        .collect()
}

pub fn push_pad_order_option_lines<I, V>(content: &mut String, lines: I)
where
    I: IntoIterator<Item = (&'static str, V)>,
    V: Display,
{
    for (key, value) in lines {
        push_line(content, key, value);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_input_native::{PAD_ORDER_CAP, PadOrderAssignment};

    #[test]
    fn canonical_uuid_text_roundtrips_and_rejects_malformed_input() {
        let uuid = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let hex = serialize_uuid_list(&[uuid]);
        assert_eq!(hex, "00112233445566778899aabbccddeeff");
        assert_eq!(uuid_from_hex(&hex.to_uppercase()), Some(uuid));
        for raw in ["", "00112233", &"0".repeat(33), &"g".repeat(32)] {
            assert_eq!(uuid_from_hex(raw), None);
        }
        assert_eq!(serialize_uuid_list(&[]), "");
        assert_eq!(
            serialize_uuid_list(&[[0x01; 16], [0xfe; 16]]),
            "01010101010101010101010101010101,fefefefefefefefefefefefefefefefe"
        );
    }

    #[test]
    fn ini_roundtrip_preserves_slots_and_sanitizes_orders() {
        let mut conf = SimpleIni::new();
        conf.load_str("[Options]\nPadOrderRawInput=bad, 00112233445566778899AABBCCDDEEFF, 11111111111111111111111111111111, 00112233445566778899aabbccddeeff\nPadOrderHidraw=33333333333333333333333333333333\nPadOrderWGI=bad\nPadOrderSmx=11111111111111111111111111111111\n");
        load_order_from_ini(&conf);
        assert_eq!(
            pad_index_for_uuid(PadOrderBackend::RawInput, [0x11; 16]),
            PadOrderAssignment {
                index: 1,
                changed: false
            }
        );
        assert!(pad_order(PadOrderBackend::Wgi).is_empty());
        let lines = pad_order_ini_lines();
        assert_eq!(
            lines[0],
            (
                "PadOrderRawInput",
                "00112233445566778899aabbccddeeff,11111111111111111111111111111111".to_string()
            )
        );
        assert_eq!(
            lines[3],
            (
                "PadOrderHidraw",
                "33333333333333333333333333333333".to_string()
            )
        );
        let mut saved = String::from("[Options]\n");
        push_pad_order_option_lines(&mut saved, lines.clone());
        conf.load_str(&saved);
        load_order_from_ini(&conf);
        assert_eq!(pad_order_ini_lines(), lines);

        // Capacity truncation retains the first occurrences and the overflow slot.
        let uuids: Vec<_> = (0..PAD_ORDER_CAP + 10).map(|n| [n as u8; 16]).collect();
        conf.load_str(&format!(
            "[Options]\nPadOrderWGI={}\n",
            serialize_uuid_list(&uuids)
        ));
        load_order_from_ini(&conf);
        assert_eq!(
            pad_order(PadOrderBackend::Wgi).as_slice(),
            &uuids[..PAD_ORDER_CAP]
        );
        assert_eq!(
            pad_index_for_uuid(PadOrderBackend::Wgi, [255; 16]),
            PadOrderAssignment {
                index: PAD_ORDER_CAP as u32,
                changed: false
            }
        );
        assert!(pad_order(PadOrderBackend::RawInput).is_empty());

        conf.load_str("[Other]\nPadOrderRawInput=11111111111111111111111111111111\n[Options]\nPadOrderrawinput=11111111111111111111111111111111\n");
        load_order_from_ini(&conf);
        assert!(
            pad_order_ini_lines()
                .iter()
                .all(|(_, value)| value.is_empty())
        );
        let mut defaults = String::new();
        push_pad_order_option_lines(&mut defaults, DEFAULT_PAD_ORDER_INI_LINES);
        assert_eq!(
            defaults,
            "PadOrderRawInput=\nPadOrderWGI=\nPadOrderIoHid=\nPadOrderHidraw=\nPadOrderLinuxEvdev=\nPadOrderFreeBsdEvdev=\n"
        );
    }
}
