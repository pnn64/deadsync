use deadlib_platform::input::KeyCode;
use deadsync_input::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/binding_codecs_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn check_parse(text: &str) {
    assert_eq!(
        parse_gamepad_code_binding(text),
        original::parse_gamepad_code_binding(text),
        "{text:?}"
    );
}

#[test]
fn qualified_tokens_preserve_padding_case_devices_and_uuid_bytes() {
    for byte in 0_u8..=255 {
        for code in [0, 1, 0xff, 0x1234_5678, u32::MAX] {
            for device in [
                None,
                Some(0),
                Some(9),
                Some(10),
                Some(999),
                Some(usize::MAX),
            ] {
                for uuid in [
                    None,
                    Some([byte; 16]),
                    Some(std::array::from_fn(|i| byte.wrapping_add(i as u8))),
                ] {
                    let binding = GamepadCodeBinding {
                        code_u32: code,
                        device,
                        uuid,
                    };
                    let current = gamepad_code_binding_to_token(binding);
                    assert_eq!(current, original::gamepad_code_binding_to_token(binding));
                    assert_eq!(parse_gamepad_code_binding(&current), Some(binding));
                    check_parse(&current.to_ascii_lowercase().replace("padcode", "PadCode"));
                }
            }
        }
    }
}

#[test]
fn uuid_decode_preserves_malformed_lengths_repeated_suffixes_and_partial_parses() {
    let uuid = "0123456789abcdefABCDEF0123456789";
    for code in [
        "",
        "0",
        "+1",
        "4294967295",
        "4294967296",
        "0x0",
        "0XFFFFFFFF",
        "0x+1",
        "0xGG",
        "-1",
        "\u{96ea}",
    ] {
        for suffix in [
            "",
            "@0",
            "@1@2",
            "@-1",
            "@+1",
            "@9999999999999999999999999999",
            "#",
            "##",
            "#bad@3",
            "#bad#0123456789abcdefABCDEF0123456789",
            "@1#0123456789abcdefABCDEF0123456789@2",
            "#0123456789abcdefABCDEF0123456789#FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
            "junk",
            "\u{96ea}",
        ] {
            check_parse(&format!("PadCode[{code}]{suffix}"));
        }
    }
    for len in 0..=80 {
        for suffix in ["", "@4", "#FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF", "\u{2003}"] {
            check_parse(&format!("PadCode[0x123]#{}{suffix}", "a".repeat(len)));
        }
    }
    for index in 0..32 {
        for byte in 0..=127 {
            let mut candidate = uuid.as_bytes().to_vec();
            candidate[index] = byte;
            check_parse(&format!(
                "PadCode[42]@7#{}@8",
                String::from_utf8(candidate).unwrap()
            ));
        }
        for extra in ["\u{e9}", "\u{96ea}", "\u{2003}"] {
            check_parse(&format!(
                "PadCode[42]#{}{extra}{}",
                &uuid[..index],
                &uuid[index..]
            ));
        }
    }
}

fn entries(qualified: bool) -> Vec<(String, String)> {
    ALL_VIRTUAL_ACTIONS.iter().enumerate().map(|(i, &action)| {
        let value = if qualified {
            format!("KeyCode::KeyA,PadDir::Left,Pad{}::Dir::Right,PadCode[0x{:08X}]@{}#0123456789ABCDEF0123456789ABCDEF", i % 4, i, i % 4)
        } else {
            DEFAULT_KEYMAP_INI_LINES.iter().find(|(key, _)| *key == action_to_ini_key(action)).unwrap().1.to_string()
        };
        (action_to_ini_key(action).to_string(), value)
    }).collect()
}

fn check_load(entries: Option<&[(String, String)]>) {
    let current = load_keymap_from_ini_entries(
        entries.map(|pairs| pairs.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
    );
    let old = original::load_keymap_from_ini_entries(
        entries.map(|pairs| pairs.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
    );
    assert!(current.has_same_bindings(&old));
    for code in [
        KeyCode::KeyA,
        KeyCode::ArrowLeft,
        KeyCode::Enter,
        KeyCode::Escape,
        KeyCode::F35,
    ] {
        for action in ALL_VIRTUAL_ACTIONS {
            assert_eq!(
                current.keycode_has_action(code, |a| a == action),
                old.keycode_has_action(code, |a| a == action)
            );
        }
    }
}

#[test]
fn keymap_load_preserves_defaults_duplicates_empty_values_and_reverse_keys() {
    check_load(None);
    check_load(Some(&[]));
    for qualified in [false, true] {
        let mut pairs = entries(qualified);
        check_load(Some(&pairs));
        pairs.extend([
            (
                "P1_Up".into(),
                "KeyCode::KeyA,KeyCode::KeyA,bad,PadCode[9]#0123456789abcdefABCDEF0123456789"
                    .into(),
            ),
            ("P1_Start".into(), "".into()),
            (
                "P1_Down".into(),
                "\u{2003}KeyCode::Enter\t,PadCode[0x00]@7#bad".into(),
            ),
            ("unknown".into(), "KeyCode::Escape".into()),
        ]);
        check_load(Some(&pairs));
    }
}

fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, iterations, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_binding_codecs() {
    let bindings: Vec<_> = (0..1024)
        .map(|i| GamepadCodeBinding {
            code_u32: (i as u32).wrapping_mul(0x7fa9_539d),
            device: Some(i % 16),
            uuid: Some(std::array::from_fn(|j| (i + j * 17) as u8)),
        })
        .collect();
    // Frozen formatting keeps every parser fixture identical across stages.
    let tokens: Vec<_> = bindings
        .iter()
        .copied()
        .map(original::gamepad_code_binding_to_token)
        .collect();
    measure("tokens/format", 32, || {
        for &binding in black_box(&bindings) {
            black_box(gamepad_code_binding_to_token(black_box(binding)));
        }
    });
    measure("tokens/parse", 64, || {
        for token in black_box(&tokens) {
            black_box(parse_gamepad_code_binding(black_box(token)));
        }
    });
    for (label, qualified) in [("keyboard", false), ("qualified", true)] {
        let pairs = entries(qualified);
        measure(&format!("load/{label}"), 64, || {
            black_box(load_keymap_from_ini_entries(Some(
                black_box(&pairs)
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str())),
            )));
        });
        let keymap =
            load_keymap_from_ini_entries(Some(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str()))));
        let mut text = String::with_capacity(8192);
        measure(&format!("write/{label}"), 64, || {
            text.clear();
            write_keymap_ini_section(black_box(&mut text), black_box(&keymap));
            black_box(&text);
        });
        measure(&format!("load_write/{label}"), 64, || {
            let keymap = load_keymap_from_ini_entries(Some(
                black_box(&pairs)
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str())),
            ));
            text.clear();
            write_keymap_ini_section(black_box(&mut text), &keymap);
            black_box(&text);
        });
    }
}
