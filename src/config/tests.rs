use super::keybinds::{
    binding_to_token, parse_binding_token, parse_keycode, parse_pad_code, parse_pad_device_binding,
    parse_pad_dir, parse_pad_dir_binding,
};
use super::null_or_die_cfg::{
    clamp_null_or_die_confidence_percent, clamp_null_or_die_magic_offset_ms,
    clamp_null_or_die_positive_ms,
};
use super::*;
use crate::core::input::{GamepadCodeBinding, InputBinding, PadDir};
use winit::keyboard::KeyCode;

fn assert_tenths_eq(actual: f64, expected_tenths: i32) {
    assert_eq!((actual * 10.0).round() as i32, expected_tenths);
}

#[test]
fn clamp_null_or_die_confidence_caps_at_100() {
    assert_eq!(clamp_null_or_die_confidence_percent(0), 0);
    assert_eq!(clamp_null_or_die_confidence_percent(80), 80);
    assert_eq!(clamp_null_or_die_confidence_percent(120), 100);
}

#[test]
fn clamp_null_or_die_positive_ms_uses_tenths() {
    assert_tenths_eq(clamp_null_or_die_positive_ms(0.0), 1);
    assert_tenths_eq(clamp_null_or_die_positive_ms(10.04), 100);
    assert_tenths_eq(clamp_null_or_die_positive_ms(10.05), 101);
    assert_tenths_eq(clamp_null_or_die_positive_ms(1000.0), 1000);
}

#[test]
fn clamp_null_or_die_magic_offset_uses_tenths() {
    assert_tenths_eq(clamp_null_or_die_magic_offset_ms(-200.0), -1000);
    assert_tenths_eq(clamp_null_or_die_magic_offset_ms(0.04), 0);
    assert_tenths_eq(clamp_null_or_die_magic_offset_ms(0.05), 1);
}

#[test]
fn parse_keycode_common_keys() {
    let cases = [
        ("KeyCode::Enter", KeyCode::Enter),
        ("KeyCode::Escape", KeyCode::Escape),
        ("KeyCode::ArrowUp", KeyCode::ArrowUp),
        ("KeyCode::ArrowDown", KeyCode::ArrowDown),
        ("KeyCode::ArrowLeft", KeyCode::ArrowLeft),
        ("KeyCode::ArrowRight", KeyCode::ArrowRight),
        ("KeyCode::Slash", KeyCode::Slash),
        ("KeyCode::KeyA", KeyCode::KeyA),
        ("KeyCode::KeyZ", KeyCode::KeyZ),
        ("KeyCode::Numpad0", KeyCode::Numpad0),
        ("KeyCode::Numpad9", KeyCode::Numpad9),
        ("KeyCode::NumpadEnter", KeyCode::NumpadEnter),
        ("KeyCode::NumpadDecimal", KeyCode::NumpadDecimal),
    ];
    for (token, expected) in cases {
        assert_eq!(
            parse_keycode(token),
            Some(InputBinding::Key(expected)),
            "failed for {token}"
        );
    }
}

#[test]
fn auto_screenshot_mask_roundtrips() {
    let mask = AUTO_SS_PBS | AUTO_SS_CLEARS | AUTO_SS_QUINTS;
    let encoded = auto_screenshot_mask_to_str(mask);
    assert_eq!(encoded, "PBs|Clears|Quints");
    assert_eq!(auto_screenshot_mask_from_str(&encoded), mask);
}

#[test]
fn auto_screenshot_mask_handles_off_and_unknown_tokens() {
    assert_eq!(auto_screenshot_mask_from_str(""), 0);
    assert_eq!(auto_screenshot_mask_from_str("Off"), 0);
    assert_eq!(
        auto_screenshot_mask_from_str("PBs|unknown|Fails"),
        AUTO_SS_PBS | AUTO_SS_FAILS
    );
}

#[test]
fn parse_keycode_previously_missing_keys() {
    let cases = [
        ("KeyCode::Period", KeyCode::Period),
        ("KeyCode::AltLeft", KeyCode::AltLeft),
        ("KeyCode::AltRight", KeyCode::AltRight),
        ("KeyCode::ControlLeft", KeyCode::ControlLeft),
        ("KeyCode::ControlRight", KeyCode::ControlRight),
        ("KeyCode::ShiftLeft", KeyCode::ShiftLeft),
        ("KeyCode::ShiftRight", KeyCode::ShiftRight),
        ("KeyCode::Space", KeyCode::Space),
        ("KeyCode::Tab", KeyCode::Tab),
        ("KeyCode::Backspace", KeyCode::Backspace),
        ("KeyCode::CapsLock", KeyCode::CapsLock),
        ("KeyCode::Delete", KeyCode::Delete),
        ("KeyCode::Home", KeyCode::Home),
        ("KeyCode::End", KeyCode::End),
        ("KeyCode::PageUp", KeyCode::PageUp),
        ("KeyCode::PageDown", KeyCode::PageDown),
        ("KeyCode::Insert", KeyCode::Insert),
        ("KeyCode::F1", KeyCode::F1),
        ("KeyCode::F12", KeyCode::F12),
        ("KeyCode::PrintScreen", KeyCode::PrintScreen),
        ("KeyCode::Comma", KeyCode::Comma),
        ("KeyCode::Minus", KeyCode::Minus),
        ("KeyCode::Equal", KeyCode::Equal),
        ("KeyCode::BracketLeft", KeyCode::BracketLeft),
        ("KeyCode::Backquote", KeyCode::Backquote),
        ("KeyCode::Digit0", KeyCode::Digit0),
        ("KeyCode::Digit9", KeyCode::Digit9),
        ("KeyCode::NumLock", KeyCode::NumLock),
        ("KeyCode::ScrollLock", KeyCode::ScrollLock),
        ("KeyCode::Pause", KeyCode::Pause),
        ("KeyCode::ContextMenu", KeyCode::ContextMenu),
        ("KeyCode::SuperLeft", KeyCode::SuperLeft),
        ("KeyCode::AudioVolumeMute", KeyCode::AudioVolumeMute),
        ("KeyCode::F35", KeyCode::F35),
    ];
    for (token, expected) in cases {
        assert_eq!(
            parse_keycode(token),
            Some(InputBinding::Key(expected)),
            "failed for {token}"
        );
    }
}

#[test]
fn parse_keycode_rejects_invalid() {
    assert_eq!(parse_keycode("KeyCode::NotAKey"), None);
    assert_eq!(parse_keycode("KeyCode::"), None);
    assert_eq!(parse_keycode("NotKeyCode::Enter"), None);
    assert_eq!(parse_keycode("Enter"), None);
    assert_eq!(parse_keycode(""), None);
}

#[test]
fn parse_pad_dir_valid() {
    assert_eq!(parse_pad_dir("Up"), Some(PadDir::Up));
    assert_eq!(parse_pad_dir("Down"), Some(PadDir::Down));
    assert_eq!(parse_pad_dir("Left"), Some(PadDir::Left));
    assert_eq!(parse_pad_dir("Right"), Some(PadDir::Right));
}

#[test]
fn parse_pad_dir_invalid() {
    assert_eq!(parse_pad_dir("up"), None);
    assert_eq!(parse_pad_dir(""), None);
    assert_eq!(parse_pad_dir("UpDown"), None);
}

#[test]
fn parse_pad_dir_binding_short_form() {
    assert_eq!(
        parse_pad_dir_binding("PadDir::Up"),
        Some(InputBinding::PadDir(PadDir::Up))
    );
    assert_eq!(
        parse_pad_dir_binding("PadDir::Right"),
        Some(InputBinding::PadDir(PadDir::Right))
    );
}

#[test]
fn parse_pad_device_binding_any_pad_long_form() {
    assert_eq!(
        parse_pad_device_binding("Pad::Dir::Down"),
        Some(InputBinding::PadDir(PadDir::Down))
    );
}

#[test]
fn parse_pad_device_binding_device_specific() {
    assert_eq!(
        parse_pad_device_binding("Pad0::Dir::Up"),
        Some(InputBinding::PadDirOn {
            device: 0,
            dir: PadDir::Up,
        })
    );
    assert_eq!(
        parse_pad_device_binding("Pad3::Dir::Left"),
        Some(InputBinding::PadDirOn {
            device: 3,
            dir: PadDir::Left,
        })
    );
}

#[test]
fn parse_pad_dir_binding_rejects_invalid() {
    assert_eq!(parse_pad_dir_binding("PadDir::Diagonal"), None);
    assert_eq!(parse_pad_dir_binding("Pad0::Btn::A"), None);
    assert_eq!(parse_pad_dir_binding("Pad0::Dir"), None);
    assert_eq!(parse_pad_dir_binding("NotPad::Dir::Up"), None);
}

#[test]
fn parse_pad_code_hex_only() {
    assert_eq!(
        parse_pad_code("PadCode[0xDEADBEEF]"),
        Some(InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 0xDEADBEEF,
            device: None,
            uuid: None,
        }))
    );
}

#[test]
fn parse_pad_code_decimal() {
    assert_eq!(
        parse_pad_code("PadCode[42]"),
        Some(InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 42,
            device: None,
            uuid: None,
        }))
    );
}

#[test]
fn parse_pad_code_with_device() {
    assert_eq!(
        parse_pad_code("PadCode[0x00000001]@2"),
        Some(InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 1,
            device: Some(2),
            uuid: None,
        }))
    );
}

#[test]
fn parse_pad_code_with_uuid() {
    let uuid_hex = "00112233AABBCCDDEEFF001122334455";
    let token = format!("PadCode[0xFF]#{uuid_hex}");
    let expected_uuid = [
        0x00, 0x11, 0x22, 0x33, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33, 0x44,
        0x55,
    ];
    assert_eq!(
        parse_pad_code(&token),
        Some(InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 0xFF,
            device: None,
            uuid: Some(expected_uuid),
        }))
    );
}

#[test]
fn parse_pad_code_with_device_and_uuid() {
    let token = "PadCode[0xDEADBEEF]@0#00112233AABBCCDDEEFF001122334455";
    let Some(InputBinding::GamepadCode(binding)) = parse_pad_code(token) else {
        panic!("expected gamepad code binding");
    };
    assert_eq!(binding.code_u32, 0xDEADBEEF);
    assert_eq!(binding.device, Some(0));
    assert!(binding.uuid.is_some());
}

#[test]
fn parse_pad_code_rejects_invalid() {
    assert_eq!(parse_pad_code("PadCode[]"), None);
    assert_eq!(parse_pad_code("PadCode[xyz]"), None);
    assert_eq!(parse_pad_code("NotPadCode[0x01]"), None);
    assert_eq!(parse_pad_code(""), None);
}

#[test]
fn parse_binding_token_dispatches_keycode() {
    assert_eq!(
        parse_binding_token("KeyCode::Period"),
        Some(InputBinding::Key(KeyCode::Period))
    );
}

#[test]
fn parse_binding_token_dispatches_pad_dir() {
    assert_eq!(
        parse_binding_token("PadDir::Up"),
        Some(InputBinding::PadDir(PadDir::Up))
    );
}

#[test]
fn parse_binding_token_dispatches_pad_device() {
    assert_eq!(
        parse_binding_token("Pad0::Dir::Left"),
        Some(InputBinding::PadDirOn {
            device: 0,
            dir: PadDir::Left,
        })
    );
}

#[test]
fn parse_binding_token_dispatches_pad_code() {
    assert_eq!(
        parse_binding_token("PadCode[0x42]"),
        Some(InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 0x42,
            device: None,
            uuid: None,
        }))
    );
}

#[test]
fn parse_binding_token_trims_whitespace() {
    assert_eq!(
        parse_binding_token("  KeyCode::Enter  "),
        Some(InputBinding::Key(KeyCode::Enter))
    );
}

#[test]
fn parse_binding_token_rejects_garbage() {
    assert_eq!(parse_binding_token("garbage"), None);
    assert_eq!(parse_binding_token(""), None);
}

#[test]
fn round_trip_keyboard_bindings() {
    let keys = [
        KeyCode::Enter,
        KeyCode::Escape,
        KeyCode::Period,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::Space,
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::ArrowUp,
        KeyCode::KeyA,
        KeyCode::KeyZ,
        KeyCode::Digit0,
        KeyCode::Digit9,
        KeyCode::Numpad0,
        KeyCode::Numpad2,
        KeyCode::NumpadEnter,
        KeyCode::NumpadDecimal,
        KeyCode::F1,
        KeyCode::F12,
        KeyCode::F35,
        KeyCode::ControlLeft,
        KeyCode::ShiftRight,
        KeyCode::SuperLeft,
        KeyCode::PrintScreen,
        KeyCode::Comma,
        KeyCode::Minus,
        KeyCode::Slash,
        KeyCode::Backquote,
        KeyCode::BracketLeft,
        KeyCode::AudioVolumeMute,
    ];
    for key in keys {
        let binding = InputBinding::Key(key);
        let token = binding_to_token(binding);
        assert_eq!(
            parse_binding_token(&token),
            Some(binding),
            "round-trip failed for {key:?}: token was {token:?}"
        );
    }
}

#[test]
fn round_trip_pad_dir() {
    for dir in [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right] {
        let binding = InputBinding::PadDir(dir);
        let token = binding_to_token(binding);
        assert_eq!(
            parse_binding_token(&token),
            Some(binding),
            "round-trip failed for {dir:?}"
        );
    }
}

#[test]
fn round_trip_pad_dir_on() {
    for device in [0, 1, 5] {
        for dir in [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right] {
            let binding = InputBinding::PadDirOn { device, dir };
            let token = binding_to_token(binding);
            assert_eq!(
                parse_binding_token(&token),
                Some(binding),
                "round-trip failed for device={device}, dir={dir:?}"
            );
        }
    }
}

#[test]
fn round_trip_gamepad_code() {
    let cases = [
        GamepadCodeBinding {
            code_u32: 0xDEADBEEF,
            device: None,
            uuid: None,
        },
        GamepadCodeBinding {
            code_u32: 42,
            device: Some(0),
            uuid: None,
        },
        GamepadCodeBinding {
            code_u32: 0xFF,
            device: None,
            uuid: Some([
                0x00, 0x11, 0x22, 0x33, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33,
                0x44, 0x55,
            ]),
        },
        GamepadCodeBinding {
            code_u32: 0x01,
            device: Some(3),
            uuid: Some([0xAB; 16]),
        },
    ];
    for binding in cases {
        let input = InputBinding::GamepadCode(binding);
        let token = binding_to_token(input);
        assert_eq!(
            parse_binding_token(&token),
            Some(input),
            "round-trip failed for {binding:?}: token was {token:?}"
        );
    }
}
