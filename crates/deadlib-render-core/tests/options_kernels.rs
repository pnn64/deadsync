use deadlib_render_core::{BackendType, DenseSlotMap, PresentModePolicy};
use std::hint::black_box;
use std::str::FromStr;

#[path = "../../../tests/perf/core_alloc.rs"]
mod alloc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/options_original.rs"]
#[allow(clippy::default_constructed_unit_structs)]
mod original;

const ALIASES: &[&str] = &[
    "vulkan",
    "vulkan-wgpu",
    "vulkan_wgpu",
    "wgpu-vulkan",
    "vulkan (wgpu)",
    "metal",
    "metal-native",
    "metal_native",
    "native-metal",
    "metal-wgpu",
    "metal_wgpu",
    "wgpu-metal",
    "metal (wgpu)",
    "opengl",
    "opengl-wgpu",
    "opengl_wgpu",
    "wgpu-opengl",
    "opengl (wgpu)",
    "software",
    "cpu",
    "directx",
    "dx12",
    "directx (wgpu)",
    "mailbox",
    "balanced",
    "immediate",
    "unhinged",
];

fn check(text: &str) {
    assert_eq!(
        BackendType::from_str(text),
        original::backend(text),
        "{text:?}"
    );
    assert_eq!(
        PresentModePolicy::from_str(text),
        original::policy(text),
        "{text:?}"
    );
}

#[test]
fn parsers_preserve_all_case_patterns_platform_aliases_and_error_text() {
    for &alias in ALIASES {
        let letters = alias.bytes().filter(u8::is_ascii_alphabetic).count();
        for mask in 0..1usize << letters {
            let mut bit = 0;
            let case: String = alias
                .bytes()
                .map(|c| {
                    let upper = c.is_ascii_alphabetic() && mask & (1 << bit) != 0;
                    bit += usize::from(c.is_ascii_alphabetic());
                    char::from(if upper { c.to_ascii_uppercase() } else { c })
                })
                .collect();
            check(&case);
            check(&format!("\u{2003}\t{case}\r\n"));
        }
        for i in 0..alias.len() {
            for byte in 0..=127 {
                let mut bytes = alias.as_bytes().to_vec();
                bytes[i] = byte;
                check(std::str::from_utf8(&bytes).unwrap());
            }
            for ch in ["\u{e9}", "\u{130}", "\u{212a}", "\u{96ea}", "\u{1f600}"] {
                check(&format!("{}{ch}{}", &alias[..i], &alias[i + 1..]));
            }
        }
        for input in [
            format!(".{alias}"),
            format!("{alias}!"),
            format!("{alias}\0"),
        ] {
            check(&input);
        }
    }
    for input in [
        "",
        " ",
        "\u{2003}",
        "CPU\u{200b}",
        "mail box",
        "\0",
        "wgpu",
        "VULKAN-WGPU!",
    ] {
        check(input);
    }
    check(&"MiXeD Case Invalid \u{96ea}".repeat(1000));
}

#[test]
fn dense_slots_preserve_invalid_handles_replacement_clear_and_drain() {
    for key in [
        0,
        1,
        2,
        u32::MAX as u64,
        u32::MAX as u64 + 1,
        u32::MAX as u64 + 2,
        u64::MAX,
    ] {
        assert_eq!(
            key.checked_sub(1)
                .and_then(|index| u32::try_from(index).ok()),
            u32::try_from(key.wrapping_sub(1)).ok()
        );
    }
    let mut current = DenseSlotMap::with_capacity(64);
    let mut before = original::DenseSlotMap::with_capacity(64);
    for round in 0..4 {
        for key in 1..=64 {
            assert_eq!(
                current.insert(key, key + round),
                before.insert(key, key + round)
            );
        }
        for key in (0..=70).chain([u32::MAX as u64, u32::MAX as u64 + 1, u64::MAX]) {
            assert_eq!(current.get(key), before.get(key));
            assert_eq!(current.get_slot(key), before.get_slot(key));
        }
        assert_eq!(current.len(), before.len());
        assert_eq!(current.is_empty(), before.is_empty());
    }
    assert_eq!(format!("{current:?}"), format!("{before:?}"));
    assert_eq!(
        current.drain().take(3).collect::<Vec<_>>(),
        before.drain().take(3).collect::<Vec<_>>()
    );
    for key in 0..=70 {
        assert_eq!(current.get_slot(key), before.get_slot(key));
    }
    assert_eq!(current.insert(99, 77), before.insert(99, 77));
    current.clear();
    before.clear();
    assert_eq!(current.get_slot(1), before.get_slot(1));
    let current: DenseSlotMap<()> = DenseSlotMap::default();
    let before: original::DenseSlotMap<()> = original::DenseSlotMap::default();
    assert_eq!(current.get_slot(u64::MAX), before.get_slot(u64::MAX));
}

fn compare(label: &str, mut work: impl FnMut(bool)) {
    for variant in [false, true] {
        let (_, counts) = alloc::measure(|| work(variant));
        if label.starts_with("slots/") {
            assert_eq!(counts, alloc::Churn::default());
        }
        println!(
            "{label}/{}: allocation {counts:?}",
            if variant { "current" } else { "original" }
        );
    }
    bench::compare(label, 3000, work);
}

#[test]
#[ignore = "release paired renderer CPU/allocation benchmark"]
fn benchmark_options_kernels() {
    let backends: Vec<_> = ALIASES
        .iter()
        .copied()
        .filter(|alias| original::backend(alias).is_ok())
        .enumerate()
        .map(|(i, alias)| match i % 3 {
            0 => alias.to_owned(),
            1 => alias.to_ascii_uppercase(),
            _ => format!("\u{2003}\t{alias}\r\n"),
        })
        .collect();
    for valid in [true, false] {
        let inputs: Vec<_> = (0..256)
            .map(|i| {
                if valid {
                    backends[i % backends.len()].as_str()
                } else {
                    ["", "wrong renderer", "\u{96ea}bad-RENDERER", "not CPU"][i % 4]
                }
            })
            .collect();
        compare(
            &format!("backend/{}/256", if valid { "valid" } else { "invalid" }),
            |updated| {
                let parse: fn(&str) -> Result<BackendType, String> = black_box(if updated {
                    BackendType::from_str
                } else {
                    original::backend
                });
                for text in black_box(&inputs) {
                    black_box(parse(black_box(text))).ok();
                }
            },
        );
        let inputs: Vec<_> = (0..256)
            .map(|i| {
                if valid {
                    ["Mailbox", " BALANCED ", "IMMEDIATE", "\u{2003}unhinged\t"][i % 4]
                } else {
                    ["", "wrong policy", "\u{96ea}bad-POLICY", "not mailbox"][i % 4]
                }
            })
            .collect();
        compare(
            &format!("policy/{}/256", if valid { "valid" } else { "invalid" }),
            |updated| {
                let parse: fn(&str) -> Result<PresentModePolicy, String> = black_box(if updated {
                    PresentModePolicy::from_str
                } else {
                    original::policy
                });
                for text in black_box(&inputs) {
                    black_box(parse(black_box(text))).ok();
                }
            },
        );
    }
    let mut current = DenseSlotMap::with_capacity(1024);
    let mut before = original::DenseSlotMap::with_capacity(1024);
    for key in 1..=1024 {
        current.insert(key, key * 2);
        before.insert(key, key * 2);
    }
    for kind in ["hits", "mixed"] {
        let keys: Vec<_> = (0..256)
            .map(|i| {
                if kind == "mixed" && i % 4 == 0 {
                    [0, u64::MAX, 1025][i % 3]
                } else {
                    (i * 997 % 1024 + 1) as u64
                }
            })
            .collect();
        compare(&format!("slots/{kind}/256"), |updated| {
            if updated {
                for &key in black_box(&keys) {
                    black_box(black_box(&current).get_slot(black_box(key)));
                }
            } else {
                for &key in black_box(&keys) {
                    black_box(black_box(&before).get_slot(black_box(key)));
                }
            }
        });
    }
}
