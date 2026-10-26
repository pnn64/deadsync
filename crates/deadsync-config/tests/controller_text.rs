use deadlib_input_native::{PAD_ORDER_CAP, PadOrderBackend, pad_order, set_pad_order};
use deadsync_config::ini::SimpleIni;
use deadsync_config::pad_order::{load_order_from_ini, pad_order_ini_lines};
use deadsync_config::theme::auto_screenshot_mask_to_str;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/controller_text_original.rs"]
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

fn uuid(index: usize) -> [u8; 16] {
    std::array::from_fn(|byte| (index.wrapping_mul(37) + byte * 19) as u8)
}

fn hex(uuid: [u8; 16]) -> String {
    format!("{:032x}", u128::from_be_bytes(uuid))
}

fn check_ini(text: &str) {
    let mut ini = SimpleIni::new();
    ini.load_str(text);
    original::load_order_from_ini(&ini);
    let expected = FIELDS.map(|(backend, _)| pad_order(backend));
    load_order_from_ini(&ini);
    assert_eq!(FIELDS.map(|(backend, _)| pad_order(backend)), expected);
    let actual = pad_order_ini_lines();
    let expected = original::pad_order_ini_lines();
    assert_eq!(actual, expected);
    assert_eq!(actual.capacity(), expected.capacity());
    for ((_, a), (_, b)) in actual.iter().zip(&expected) {
        assert_eq!(a.capacity(), b.capacity());
    }
}

#[test]
fn controller_text_preserves_uuid_bytes_rejections_order_capacity_and_duplicates() {
    for byte in 0..=u8::MAX {
        for index in 0..16 {
            let mut value = uuid(index);
            value[index] = byte;
            let text = hex(value);
            for text in [
                text.clone(),
                format!("\u{2003}{}\u{a0}", text.to_ascii_uppercase()),
            ] {
                check_ini(&format!("[Options]\nPadOrderRawInput=bad,{text},bad\n"));
            }
        }
    }
    let text = hex(uuid(17));
    for index in 0..32 {
        for byte in 0..=127 {
            let mut candidate = text.clone().into_bytes();
            candidate[index] = byte;
            let candidate = String::from_utf8(candidate).unwrap();
            check_ini(&format!("[Options]\nPadOrderRawInput={candidate}\n"));
        }
        for extra in ["\u{e9}", "\u{96ea}", "\u{2003}", "\u{130}"] {
            let end = (index + extra.len()).min(text.len());
            let candidate = format!("{}{extra}{}", &text[..index], &text[end..]);
            check_ini(&format!("[Options]\nPadOrderRawInput={candidate}\n"));
        }
    }
    for count in [0, 1, 3, 16, PAD_ORDER_CAP, PAD_ORDER_CAP + 7] {
        let values: Vec<_> = (0..count).map(|index| hex(uuid(index))).collect();
        let mut text = String::from("[Options]\n");
        for (_, key) in FIELDS {
            text.push_str(&format!(
                "{key}={},garbage,{},bad,\n",
                values.join(","),
                values.join(",")
            ));
        }
        check_ini(&text);
    }
    check_ini(
        "[Other]\nPadOrderRawInput=00000000000000000000000000000000\n[Options]\nPadOrderrawinput=11111111111111111111111111111111\n",
    );
    check_ini("");
}

#[test]
fn screenshot_flags_preserve_all_masks_text_and_string_capacity() {
    for mask in 0..=u8::MAX {
        let expected = original::auto_screenshot_mask_to_str(mask);
        let actual = auto_screenshot_mask_to_str(mask);
        assert_eq!(actual, expected, "mask {mask:#04x}");
        assert_eq!(actual.capacity(), expected.capacity());
    }
}

fn measure(label: &str, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, 32, work);
}

fn order_ini(count: usize, mixed: bool) -> SimpleIni {
    let mut text = String::from("[Options]\n");
    for (_, key) in FIELDS {
        let values: Vec<_> = (0..count)
            .map(|index| {
                let text = hex(uuid(index));
                if !mixed {
                    text
                } else {
                    match index % 4 {
                        0 => format!("\u{2003}{}\t", text.to_ascii_uppercase()),
                        1 => "not-a-uuid".to_owned(),
                        2 => hex(uuid(index - 2)),
                        _ => format!("{}\u{e9}", &text[..30]),
                    }
                }
            })
            .collect();
        text.push_str(&format!("{key}={}\n", values.join(",")));
    }
    let mut ini = SimpleIni::new();
    ini.load_str(&text);
    ini
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_controller_text() {
    for count in [0, 1, 16, PAD_ORDER_CAP] {
        let ini = order_ini(count, false);
        load_order_from_ini(&ini);
        measure(&format!("load/{count}"), || {
            load_order_from_ini(black_box(&ini))
        });
        measure(&format!("save/{count}"), || {
            black_box(pad_order_ini_lines());
        });
    }
    let mixed = order_ini(32, true);
    measure("load/mixed", || load_order_from_ini(black_box(&mixed)));
    for (label, masks) in [
        ("all_masks", (0..=u8::MAX).collect::<Vec<_>>()),
        ("single", (0..256).map(|i| 1 << (i % 5)).collect()),
        (
            "combinations",
            (0..256).map(|i| (i as u8 & 31) | 3).collect(),
        ),
        ("off", vec![0; 256]),
    ] {
        measure(&format!("flags/{label}"), || {
            for &mask in black_box(&masks) {
                black_box(auto_screenshot_mask_to_str(black_box(mask)));
            }
        });
    }
    for (backend, _) in FIELDS {
        set_pad_order(backend, []);
    }
}
