use deadlib_input_native::{PAD_ORDER_CAP, PadOrderBackend, pad_order};
use deadsync_config::ini::SimpleIni;
use deadsync_config::pad_order::{
    load_order_from_ini, pad_order_ini_lines, push_pad_order_option_lines,
};
use deadsync_input::write_default_keymap_ini_section;
use std::cell::Cell;
use std::fmt::{self, Display, Write as _};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/ini_uuid_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

const FIELDS: [(PadOrderBackend, &str); 6] = [
    (PadOrderBackend::RawInput, "PadOrderRawInput"),
    (PadOrderBackend::Wgi, "PadOrderWGI"),
    (PadOrderBackend::IoHid, "PadOrderIoHid"),
    (PadOrderBackend::Hidraw, "PadOrderHidraw"),
    (PadOrderBackend::LinuxEvdev, "PadOrderLinuxEvdev"),
    (PadOrderBackend::FreeBsdEvdev, "PadOrderFreeBsdEvdev"),
];

fn check_line(value: impl Display + Copy) {
    for key in ["", "ConfigKey", "key=value", "key\nline", "\u{96ea}\0key"] {
        for capacity in [0, 8, 64, 1024] {
            for prefix in ["", "prefix\n", "\u{2003}\0"] {
                let mut expected = String::with_capacity(capacity);
                expected.push_str(prefix);
                let mut actual = expected.clone();
                original::push_line(&mut expected, key, value);
                push_pad_order_option_lines(&mut actual, [(key, value)]);
                assert_eq!(actual, expected);
            }
        }
    }
}

#[test]
fn ini_lines_preserve_strings_scalars_and_display_options() {
    for value in [
        "",
        "plain",
        "a=b\nnext",
        "\0",
        "{}",
        "\u{96ea}\u{1f600}",
        "\u{2003}",
    ] {
        check_line(value);
    }
    for length in [1, 7, 31, 256, 8192] {
        let value = "x".repeat(length);
        check_line(value.as_str());
    }
    for value in [i64::MIN, -1, 0, 1, i64::MAX] {
        check_line(value);
    }
    for value in [0, 1, u64::MAX] {
        check_line(value);
    }
    for value in [
        0.0,
        -0.0,
        1.25,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        check_line(value);
    }
    check_line(true);
    check_line(false);
    let calls = Cell::new(0);
    check_line(Probe {
        calls: &calls,
        fail: false,
    });
}

#[derive(Clone, Copy)]
struct Probe<'a> {
    calls: &'a Cell<usize>,
    fail: bool,
}

impl Display for Probe<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.calls.set(self.calls.get() + 1);
        let flags = (
            f.width(),
            f.precision(),
            f.align(),
            f.fill(),
            f.sign_plus(),
            f.sign_minus(),
            f.alternate(),
            f.sign_aware_zero_pad(),
        );
        write!(f, "{flags:?}:partial")?;
        if self.fail {
            Err(fmt::Error)
        } else {
            f.write_str(":complete")
        }
    }
}

#[test]
fn failing_display_retains_partial_output_panic_and_single_invocation() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let calls = Cell::new(0);
    let value = Probe {
        calls: &calls,
        fail: true,
    };
    let mut expected = "prefix\n".to_owned();
    let mut actual = expected.clone();
    let before = catch_unwind(AssertUnwindSafe(|| {
        original::push_line(&mut expected, "Key", value)
    }));
    assert_eq!(calls.replace(0), 1);
    let after = catch_unwind(AssertUnwindSafe(|| {
        push_pad_order_option_lines(&mut actual, [("Key", value)])
    }));
    assert_eq!(calls.get(), 1);
    assert_eq!(actual, expected);
    let payload = |result: Result<(), Box<dyn std::any::Any + Send>>| {
        let error = result.expect_err("Display failure must retain the original panic");
        error
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| error.downcast_ref::<&str>().map(|s| (*s).to_owned()))
    };
    assert_eq!(payload(after), payload(before));
}

#[test]
fn default_keymaps_preserve_all_bytes_order_prefixes_and_repeated_appends() {
    for prefix in ["", "prefix\n", "\0", "\u{96ea}\u{1f600}"] {
        for capacity in [0, 8, 128, 4096] {
            let mut expected = String::with_capacity(capacity);
            expected.push_str(prefix);
            let mut actual = expected.clone();
            for _ in 0..3 {
                original::write_default_keymap_ini_section(&mut expected);
                write_default_keymap_ini_section(&mut actual);
                assert_eq!(actual, expected);
            }
        }
    }
}

fn uuid(index: usize) -> [u8; 16] {
    std::array::from_fn(|i| index.wrapping_mul(73).wrapping_add(i * 19) as u8)
}

fn hex(value: [u8; 16]) -> String {
    format!("{:032x}", u128::from_be_bytes(value))
}

fn check_ini(text: &str) {
    let mut ini = SimpleIni::new();
    ini.load_str(text);
    original::load_order_from_ini(&ini);
    let expected_order = FIELDS.map(|(backend, _)| pad_order(backend));
    load_order_from_ini(&ini);
    assert_eq!(
        FIELDS.map(|(backend, _)| pad_order(backend)),
        expected_order
    );
    let before = original::pad_order_ini_lines();
    let after = pad_order_ini_lines();
    assert_eq!(after, before);
    assert_eq!(after.capacity(), before.capacity());
    for ((_, actual), (_, expected)) in after.iter().zip(&before) {
        assert_eq!(actual.capacity(), expected.capacity());
    }
    let mut actual = "[Options]\n".to_owned();
    let mut expected = actual.clone();
    push_pad_order_option_lines(&mut actual, after);
    for (key, value) in before {
        original::push_line(&mut expected, key, value);
    }
    assert_eq!(actual, expected);
    let mut roundtrip = SimpleIni::new();
    roundtrip.load_str(&actual);
    load_order_from_ini(&roundtrip);
    assert_eq!(
        FIELDS.map(|(backend, _)| pad_order(backend)),
        expected_order
    );
}

#[test]
fn uuid_loading_and_saving_preserve_bytes_rejections_order_and_capacities() {
    for byte in 0..=u8::MAX {
        for position in 0..16 {
            let mut value = uuid(position);
            value[position] = byte;
            let text = hex(value);
            check_ini(&format!("[Options]\nPadOrderRawInput={text}\n"));
            check_ini(&format!(
                "[Options]\nPadOrderRawInput=\u{2003}{}\t\n",
                text.to_ascii_uppercase()
            ));
        }
    }
    let text = hex(uuid(17));
    for position in 0..32 {
        for byte in 0..=127 {
            let mut candidate = text.as_bytes().to_vec();
            candidate[position] = byte;
            check_ini(&format!(
                "[Options]\nPadOrderRawInput={}\n",
                String::from_utf8(candidate).unwrap()
            ));
        }
        for extra in ["\u{e9}", "\u{130}", "\u{2003}", "\u{96ea}"] {
            let end = (position + extra.len()).min(32);
            check_ini(&format!(
                "[Options]\nPadOrderRawInput={}{}{}\n",
                &text[..position],
                extra,
                &text[end..]
            ));
        }
    }
    for count in [0, 1, 16, PAD_ORDER_CAP, PAD_ORDER_CAP + 7] {
        let values = (0..count)
            .map(|i| hex(uuid(i)))
            .collect::<Vec<_>>()
            .join(",");
        let mut text = "[Options]\n".to_owned();
        for (_, key) in FIELDS {
            writeln!(text, "{key}=bad,{values},{values},bad").unwrap();
        }
        check_ini(&text);
    }
    let mut state = 0x243f6a8885a308d3u64;
    for i in 0..10_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let text = format!("{state:016x}{:016x}", state.rotate_left(29));
        let text = if i % 3 == 0 {
            text.to_ascii_uppercase()
        } else {
            text
        };
        check_ini(&format!("[Options]\nPadOrderHidraw={text}\n"));
    }
    check_ini("[Options]\nPadOrderRawInput=00000000000000000000000000000000\n");
    check_ini("");
}

fn random_uuid(index: usize) -> [u8; 16] {
    let mut state = (index as u64)
        .wrapping_mul(0x9e3779b97f4a7c15)
        .wrapping_add(0x243f6a8885a308d3);
    std::array::from_fn(|_| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 32) as u8
    })
}

fn order_ini(count: usize, kind: usize) -> SimpleIni {
    let mut text = "[Options]\n".to_owned();
    for (_, key) in FIELDS {
        let values: Vec<_> = (0..count)
            .map(|i| {
                let text = hex(uuid(i));
                match kind {
                    4 => hex(random_uuid(i)),
                    1 => text.to_ascii_uppercase(),
                    2 => match i % 4 {
                        0 => format!("\u{2003}{text}\t"),
                        1 => "not-a-uuid".to_owned(),
                        2 => hex(uuid(i - 2)),
                        _ => format!("{}g", &text[..31]),
                    },
                    3 => format!("{}g", &text[..31]),
                    _ => text,
                }
            })
            .collect();
        writeln!(text, "{key}={}", values.join(",")).unwrap();
    }
    let mut ini = SimpleIni::new();
    ini.load_str(&text);
    ini
}

fn measure(label: &str, repetitions: usize, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, repetitions, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_ini_uuid() {
    for (label, unicode, owned) in [
        ("strings", false, false),
        ("unicode", true, false),
        ("owned", false, true),
    ] {
        let values: Vec<_> = (0..256)
            .map(|i| {
                if unicode {
                    format!("\u{96ea}\u{1f600} value-{i}")
                } else {
                    format!("value-{i}")
                }
            })
            .collect();
        let mut out = String::with_capacity(values.iter().map(|v| v.len() + 11).sum());
        measure(&format!("lines/{label}"), 128, || {
            if owned {
                let mut out = String::new();
                for value in black_box(&values) {
                    push_pad_order_option_lines(
                        &mut out,
                        [(black_box("ConfigKey"), black_box(value))],
                    );
                }
                black_box(out);
            } else {
                out.clear();
                for value in black_box(&values) {
                    push_pad_order_option_lines(
                        &mut out,
                        [(black_box("ConfigKey"), black_box(value))],
                    );
                }
                black_box(&out);
            }
        });
    }
    let mut out = String::with_capacity(16384);
    measure("lines/numbers", 128, || {
        out.clear();
        for i in 0..256u32 {
            push_pad_order_option_lines(&mut out, [(black_box("ConfigKey"), black_box(i))]);
        }
        black_box(&out);
    });
    measure("lines/floats", 64, || {
        out.clear();
        for i in 0..256 {
            push_pad_order_option_lines(
                &mut out,
                [(black_box("ConfigKey"), black_box(i as f64 / 7.0))],
            );
        }
        black_box(&out);
    });
    for (label, count) in [
        ("empty", 0),
        ("one", 1),
        ("sixteen", 16),
        ("cap", PAD_ORDER_CAP),
    ] {
        let ini = order_ini(count, 0);
        load_order_from_ini(&ini);
        measure(&format!("load/{label}"), 128, || {
            load_order_from_ini(black_box(&ini))
        });
        measure(&format!("save/{label}"), 128, || {
            black_box(pad_order_ini_lines());
        });
    }
    for (label, kind) in [("upper", 1), ("mixed", 2), ("invalid", 3)] {
        let ini = order_ini(16, kind);
        measure(&format!("load/{label}"), 128, || {
            load_order_from_ini(black_box(&ini))
        });
    }
    for (label, count) in [("random", 16), ("randomcap", PAD_ORDER_CAP)] {
        let ini = order_ini(count, 4);
        measure(&format!("load/{label}"), 128, || {
            load_order_from_ini(black_box(&ini))
        });
    }
    let mut defaults = String::with_capacity(1024);
    measure("defaults/reused", 4096, || {
        defaults.clear();
        write_default_keymap_ini_section(black_box(&mut defaults));
        black_box(&defaults);
    });
    measure("defaults/owned", 1024, || {
        let mut text = String::new();
        write_default_keymap_ini_section(black_box(&mut text));
        black_box(text);
    });
    let ini = order_ini(16, 0);
    measure("compose/lines", 64, || {
        load_order_from_ini(black_box(&ini));
        let mut text = String::new();
        push_pad_order_option_lines(&mut text, pad_order_ini_lines());
        black_box(text);
    });
    measure("compose/defaults", 64, || {
        load_order_from_ini(black_box(&ini));
        let mut text = "[Options]\n".to_owned();
        push_pad_order_option_lines(&mut text, pad_order_ini_lines());
        write_default_keymap_ini_section(&mut text);
        black_box(text);
    });
    measure("roundtrip/ini", 32, || {
        load_order_from_ini(black_box(&ini));
        let mut text = "[Options]\n".to_owned();
        push_pad_order_option_lines(&mut text, pad_order_ini_lines());
        let mut reparsed = SimpleIni::new();
        reparsed.load_str(black_box(&text));
        load_order_from_ini(black_box(&reparsed));
        black_box(reparsed);
    });
}
