// Frozen function bodies from c54347f4c.
use deadlib_input_native::{PadOrderBackend, pad_order, set_pad_order};
use deadsync_config::ini::SimpleIni;
use std::fmt::{Display, Write as _};

const PAD_ORDER_FIELDS: [(PadOrderBackend, &str); 6] = [
    (PadOrderBackend::RawInput, "PadOrderRawInput"),
    (PadOrderBackend::Wgi, "PadOrderWGI"),
    (PadOrderBackend::IoHid, "PadOrderIoHid"),
    (PadOrderBackend::Hidraw, "PadOrderHidraw"),
    (PadOrderBackend::LinuxEvdev, "PadOrderLinuxEvdev"),
    (PadOrderBackend::FreeBsdEvdev, "PadOrderFreeBsdEvdev"),
];

pub fn push_line(content: &mut String, key: &str, value: impl Display) {
    writeln!(content, "{key}={value}").expect("writing into String cannot fail");
}

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

pub fn serialize_uuid_list(list: &[[u8; 16]]) -> String {
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

pub fn append_uuid_hex(out: &mut String, uuid: &[u8; 16]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = [0; 32];
    for (pair, &byte) in text.as_chunks_mut::<2>().0.iter_mut().zip(uuid) {
        pair[0] = HEX[usize::from(byte >> 4)];
        pair[1] = HEX[usize::from(byte & 0x0f)];
    }
    out.push_str(std::str::from_utf8(&text).expect("hex digits are ASCII"));
}

pub fn uuid_from_hex(s: &str) -> Option<[u8; 16]> {
    let bytes = s.trim().as_bytes();
    if bytes.len() != 32 {
        return None;
    }
    let digit = |byte: u8| {
        let number = byte.wrapping_sub(b'0');
        if number < 10 {
            return Some(number);
        }
        let letter = (byte | 0x20).wrapping_sub(b'a');
        if letter < 6 { Some(letter + 10) } else { None }
    };
    let mut out = [0u8; 16];
    for (byte, pair) in out.iter_mut().zip(bytes.as_chunks::<2>().0) {
        *byte = (digit(pair[0])? << 4) | digit(pair[1])?;
    }
    Some(out)
}

pub const DEFAULT_KEYMAP_INI_LINES: [(&str, &str); 32] = [
    ("P1_Back", "KeyCode::Escape"),
    ("P1_Down", "KeyCode::ArrowDown,KeyCode::KeyQ"),
    ("P1_Left", "KeyCode::ArrowLeft,KeyCode::KeyZ"),
    ("P1_MenuDown", ""),
    ("P1_MenuLeft", ""),
    ("P1_MenuRight", ""),
    ("P1_MenuUp", ""),
    ("P1_Operator", "KeyCode::ScrollLock"),
    ("P1_Restart", ""),
    ("P1_Right", "KeyCode::ArrowRight,KeyCode::KeyC"),
    ("P1_Select", "KeyCode::Slash"),
    ("P1_Start", "KeyCode::Enter"),
    ("P1_Up", "KeyCode::ArrowUp,KeyCode::KeyE"),
    ("P2_Back", "KeyCode::Numpad0"),
    ("P2_Down", "KeyCode::Numpad2"),
    ("P2_Left", "KeyCode::Numpad4"),
    ("P2_MenuDown", ""),
    ("P2_MenuLeft", ""),
    ("P2_MenuRight", ""),
    ("P2_MenuUp", ""),
    ("P2_Operator", ""),
    ("P2_Restart", ""),
    ("P2_Right", "KeyCode::Numpad6"),
    ("P2_Select", "KeyCode::NumpadDecimal"),
    ("P2_Start", "KeyCode::NumpadEnter"),
    ("P2_Up", "KeyCode::Numpad8"),
    ("System_FastForward", "KeyCode::Tab"),
    ("System_SlowDown", "KeyCode::Backquote"),
    ("P1_Center", "KeyCode::Space,KeyCode::KeyS"),
    ("P2_Center", "KeyCode::Numpad5"),
    ("P1_Coin", "KeyCode::F1"),
    ("P2_Coin", ""),
];

pub fn write_default_keymap_ini_section(content: &mut String) {
    content.push_str("[Keymaps]\n");
    for (key, value) in DEFAULT_KEYMAP_INI_LINES {
        writeln!(content, "{key}={value}").expect("writing into String cannot fail");
    }
    content.push('\n');
}
