//! Frozen production functions from 17d5ad286.
use deadlib_input_native::{PadOrderBackend, pad_order, set_pad_order};
use deadsync_config::ini::SimpleIni;
use deadsync_config::theme::{AUTO_SS_FLAG_NAMES, auto_screenshot_bit};

const PAD_ORDER_FIELDS: [(PadOrderBackend, &str); 6] = [
    (PadOrderBackend::RawInput, "PadOrderRawInput"),
    (PadOrderBackend::Wgi, "PadOrderWGI"),
    (PadOrderBackend::IoHid, "PadOrderIoHid"),
    (PadOrderBackend::Hidraw, "PadOrderHidraw"),
    (PadOrderBackend::LinuxEvdev, "PadOrderLinuxEvdev"),
    (PadOrderBackend::FreeBsdEvdev, "PadOrderFreeBsdEvdev"),
];

pub fn load_order_from_ini(conf: &SimpleIni) {
    for (backend, key) in PAD_ORDER_FIELDS {
        let raw = conf.get("Options", key).unwrap_or_default();
        set_pad_order(backend, raw.split(',').filter_map(uuid_from_hex));
    }
}

pub fn pad_order_ini_lines() -> Vec<(&'static str, String)> {
    PAD_ORDER_FIELDS
        .into_iter()
        .map(|(backend, key)| (key, serialize_uuid_list(&pad_order(backend))))
        .collect()
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

pub fn auto_screenshot_mask_to_str(mask: u8) -> String {
    if mask == 0 {
        return "Off".to_string();
    }
    let mut capacity = 0usize;
    let mut selected = 0usize;
    for (idx, name) in AUTO_SS_FLAG_NAMES.iter().enumerate() {
        if (mask & auto_screenshot_bit(idx)) != 0 {
            capacity += name.len();
            selected += 1;
        }
    }
    capacity += selected.saturating_sub(1);

    let mut out = String::with_capacity(capacity);
    for (idx, name) in AUTO_SS_FLAG_NAMES.iter().enumerate() {
        if (mask & auto_screenshot_bit(idx)) == 0 {
            continue;
        }
        if !out.is_empty() {
            out.push('|');
        }
        out.push_str(name);
    }
    out
}
