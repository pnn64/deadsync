use super::support as perf;
use crate::coin::{CoinMode, CoinOptions, load_coin_options};
use crate::ini::SimpleIni;
use crate::null_or_die::*;
use crate::writer::push_bool;
use std::hint::black_box;
use std::str::FromStr;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "text_codecs_original.rs"]
mod original;

#[test]
fn graph_choices_preserve_ascii_case_whitespace_and_rejections() {
    let check = |raw: &str| {
        assert_eq!(
            parse_null_or_die_graph_orientation(raw),
            original::parse_null_or_die_graph_orientation(raw),
            "orientation {raw:?}"
        );
        assert_eq!(
            parse_null_or_die_graph_origin(raw),
            original::parse_null_or_die_graph_origin(raw),
            "origin {raw:?}"
        );
    };
    check("");
    for name in ["vertical", "horizontal", "bottom", "top"] {
        for mask in 0..1 << name.len() {
            let text: String = name
                .bytes()
                .enumerate()
                .map(|(i, b)| char::from(if mask & (1 << i) == 0 { b } else { b - 32 }))
                .collect();
            check(&text);
            check(&format!("\u{2003}{text}\r\n"));
        }
        for index in 0..=name.len() {
            for byte in 0..=127 {
                check(&format!(
                    "{}{}{}",
                    &name[..index],
                    char::from(byte),
                    &name[index..]
                ));
            }
            for extra in ["\u{e9}", "\u{2003}", "\u{96ea}"] {
                check(&format!("{}{extra}{}", &name[..index], &name[index..]));
            }
        }
        check(&format!("{name}{}", "x".repeat(4096)));
    }
}

#[test]
fn coin_mode_preserves_case_trimming_and_invalid_values() {
    for name in ["pay", "free", "home", "", "unknown"] {
        for mask in 0..1 << name.len() {
            let text: String = name
                .bytes()
                .enumerate()
                .map(|(i, b)| char::from(if mask & (1 << i) == 0 { b } else { b - 32 }))
                .collect();
            for text in [text.clone(), format!("\u{2003}{text}\t")] {
                assert_eq!(
                    CoinMode::from_str(&text),
                    original::coin_mode_from_str(&text),
                    "{text:?}"
                );
            }
        }
        for index in 0..=name.len() {
            for byte in 0..=127 {
                let text = format!("{}{}{}", &name[..index], char::from(byte), &name[index..]);
                assert_eq!(
                    CoinMode::from_str(&text),
                    original::coin_mode_from_str(&text),
                    "{text:?}"
                );
            }
            for extra in ["\u{e9}", "\u{2003}", "\u{96ea}"] {
                let text = format!("{}{extra}{}", &name[..index], &name[index..]);
                assert_eq!(
                    CoinMode::from_str(&text),
                    original::coin_mode_from_str(&text),
                    "{text:?}"
                );
            }
        }
    }
}

fn check_alias(raw: &str) {
    assert_eq!(
        parse_null_or_die_kernel_target(raw),
        original::parse_null_or_die_kernel_target(raw),
        "target {raw:?}"
    );
    assert_eq!(
        parse_null_or_die_kernel_type(raw),
        original::parse_null_or_die_kernel_type(raw),
        "type {raw:?}"
    );
}

#[test]
fn kernel_aliases_preserve_case_filtering_unicode_and_rejections() {
    for name in ["digest", "accumulator", "rising", "loudest"] {
        for mask in 0..1 << name.len() {
            let varied: String = name
                .bytes()
                .enumerate()
                .map(|(i, b)| char::from(if mask & (1 << i) == 0 { b } else { b - 32 }))
                .collect();
            check_alias(&varied);
            check_alias(&format!(
                "\u{2003}{}\t",
                varied
                    .chars()
                    .map(|c| format!("{c}-\u{96ea}_"))
                    .collect::<String>()
            ));
        }
        for index in 0..=name.len() {
            for byte in 0..=127 {
                check_alias(&format!(
                    "{}{}{}",
                    &name[..index],
                    char::from(byte),
                    &name[index..]
                ));
                if index < name.len() {
                    check_alias(&format!(
                        "{}{}{}",
                        &name[..index],
                        char::from(byte),
                        &name[index + 1..]
                    ));
                }
            }
            for extra in ["\u{e9}", "\u{200b}", "\u{2003}", "\u{1f63a}"] {
                check_alias(&format!("{}{extra}{}", &name[..index], &name[index..]));
            }
        }
        for len in [0, 1, 32, 256, 4096] {
            check_alias(&format!("{name}{}", "a".repeat(len)));
            check_alias(&format!(
                "{}{name}{}",
                "_".repeat(len),
                "\u{96ea}".repeat(len)
            ));
        }
    }
    check_alias("");
    for scalar in (0..=0x10ffff).step_by(131) {
        if let Some(c) = char::from_u32(scalar) {
            check_alias(&format!("d{c}igest"));
            check_alias(&format!("lo{c}udest"));
            check_alias(&c.to_string());
        }
    }
}

#[test]
fn boolean_lines_preserve_existing_content_and_arbitrary_keys() {
    for prefix in ["", "[Options]\n", "\u{96ea}\0\r\n="] {
        for key in ["", "Enabled", "x=y\n", "\u{e9}\u{96ea}\0"] {
            for enabled in [false, true] {
                let mut current = prefix.to_owned();
                let mut old = current.clone();
                push_bool(&mut current, key, enabled);
                original::push_bool(&mut old, key, enabled);
                assert_eq!(current, old);
            }
        }
    }
}

fn check_options(options: NullOrDieOptions) {
    let mut current = String::from("prefix\n");
    let mut old = current.clone();
    push_null_or_die_option_lines(&mut current, options);
    original::push_null_or_die_option_lines(&mut old, options);
    assert_eq!(current, old, "{options:?}");
    let mut ini = SimpleIni::new();
    ini.load_str(&format!("[Options]\n{current}"));
    assert_eq!(
        load_null_or_die_options(&ini, NullOrDieOptions::default()),
        original::load_null_or_die_options(&ini, NullOrDieOptions::default())
    );
}

#[test]
fn option_text_preserves_quantization_precision_and_loading() {
    for value in [
        f64::NEG_INFINITY,
        -1000.0,
        -100.0,
        -10.005,
        -0.005,
        -0.0,
        0.0,
        0.005,
        0.05,
        0.1,
        10.005,
        10.05,
        99.95,
        100.0,
        1000.0,
        f64::INFINITY,
        f64::NAN,
    ] {
        check_options(NullOrDieOptions {
            fingerprint_ms: value,
            window_ms: value,
            step_ms: value,
            magic_offset_ms: value,
            ..NullOrDieOptions::default()
        });
    }
    for i in 0_u64..4096 {
        let bits = i.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        check_options(NullOrDieOptions {
            fingerprint_ms: f64::from_bits(bits),
            window_ms: f64::from_bits(bits.rotate_left(17)),
            step_ms: i as f64 / 20.0,
            magic_offset_ms: i as f64 / 200.0 - 10.0,
            cache_results: i & 1 != 0,
            full_spectrogram: i & 2 != 0,
            confidence_percent: i as u8,
            ..NullOrDieOptions::default()
        });
    }
}

fn aliases(kind: usize) -> Vec<(String, String)> {
    (0..512)
        .map(|i| {
            let (target, kernel) = if i & 1 == 0 {
                ("Digest", "Rising")
            } else {
                ("Accumulator", "Loudest")
            };
            let decorate = |s: &str| match kind {
                0 => s.to_owned(),
                1 => format!(
                    "\u{2003}{}\t",
                    s.chars()
                        .map(|c| format!("{c}-\u{96ea}_"))
                        .collect::<String>()
                ),
                _ if i & 1 == 0 => format!("unknown{}", "a".repeat(64)),
                _ => format!("{s}0"),
            };
            (decorate(target), decorate(kernel))
        })
        .collect()
}

fn option_fixtures() -> Vec<NullOrDieOptions> {
    (0..32)
        .map(|i| NullOrDieOptions {
            fingerprint_ms: 0.1 + i as f64 * 0.3,
            window_ms: 8.0 + i as f64 / 10.0,
            step_ms: 0.1 + i as f64 / 20.0,
            magic_offset_ms: i as f64 / 100.0 - 0.15,
            cache_results: i & 1 != 0,
            full_spectrogram: i & 2 != 0,
            ..NullOrDieOptions::default()
        })
        .collect()
}

fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, iterations, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_config_text() {
    for (kind, label) in ["plain", "decorated", "rejected"].iter().enumerate() {
        let pairs = aliases(kind);
        measure(&format!("aliases/{label}"), 32, || {
            for (target, kernel) in black_box(&pairs) {
                black_box(parse_null_or_die_kernel_target(black_box(target)));
                black_box(parse_null_or_die_kernel_type(black_box(kernel)));
            }
        });
    }
    let graph_pairs: Vec<_> = (0..512)
        .map(|i| {
            let (orientation, origin) = [
                ("Vertical", "Bottom"),
                ("horizontal", "top"),
                ("\u{2003}vErTiCaL\t", "\u{2003}bOtToM\t"),
                ("unknown", "unknown"),
            ][i % 4];
            (orientation.to_owned(), origin.to_owned())
        })
        .collect();
    measure("parse/graph", 32, || {
        for (orientation, origin) in black_box(&graph_pairs) {
            black_box(parse_null_or_die_graph_orientation(black_box(orientation)));
            black_box(parse_null_or_die_graph_origin(black_box(origin)));
        }
    });
    let modes: Vec<_> = (0..512)
        .map(|i| ["Pay", "Free", "Home", "\u{2003}pAy\t", "unknown"][i % 5].to_owned())
        .collect();
    measure("parse/coin", 32, || {
        for mode in black_box(&modes) {
            let _ = black_box(CoinMode::from_str(black_box(mode)));
        }
    });
    let coin_inputs: Vec<_> = modes
        .iter()
        .take(32)
        .map(|mode| {
            let mut ini = SimpleIni::new();
            ini.load_str(&format!("[Options]\nCoinMode={mode}\n"));
            ini
        })
        .collect();
    measure("load/coin_options", 32, || {
        for ini in black_box(&coin_inputs) {
            black_box(load_coin_options(black_box(ini), CoinOptions::default()));
        }
    });
    let keys: Vec<_> = (0..512).map(|i| format!("ConfigFlag{i}")).collect();
    let mut text = String::with_capacity(32768);
    measure("write/bools", 64, || {
        text.clear();
        for (i, key) in black_box(&keys).iter().enumerate() {
            push_bool(&mut text, black_box(key), black_box(i & 1 != 0));
        }
        black_box(&text);
    });
    let options = option_fixtures();
    measure("write/null_options", 16, || {
        text.clear();
        for &options in black_box(&options) {
            push_null_or_die_option_lines(&mut text, black_box(options));
        }
        black_box(&text);
    });
    let inputs: Vec<_> = options
        .iter()
        .map(|&options| {
            let mut content = String::from("[Options]\n");
            original::push_null_or_die_option_lines(&mut content, options);
            let mut ini = SimpleIni::new();
            ini.load_str(&content);
            ini
        })
        .collect();
    measure("load/null_options", 32, || {
        for ini in black_box(&inputs) {
            black_box(load_null_or_die_options(
                black_box(ini),
                NullOrDieOptions::default(),
            ));
        }
    });
    measure("write/default_config", 16, || {
        black_box(crate::save::build_default_app_config_file());
    });
}
