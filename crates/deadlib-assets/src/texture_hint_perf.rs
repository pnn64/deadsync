//! Keyword scanner exact-flag checks and CPU/allocation measurements.
use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "texture_hint_original.rs"]
mod original;

#[test]
fn hint_flags_match_every_byte_boundary_overlap_and_shipped_name() {
    let words: &[&[u8]] = &[
        b"16bpp",
        b"32bpp",
        b"alphamap",
        b"clamp",
        b"dither",
        b"doubleres",
        b"grayscale",
        b"linear",
        b"mipmaps",
        b"nearest",
        b"nomipmaps",
        b"point",
        b"repeat",
        b"stretch",
        b"wrap",
    ];
    let check = |bytes: &[u8]| {
        assert_eq!(
            texture_hint_flags(bytes),
            original::texture_hint_flags(bytes),
            "{bytes:?}"
        )
    };
    for word in words {
        for byte in 0..=255u8 {
            for offset in 0..=word.len() {
                let mut bytes = word.to_vec();
                bytes.insert(offset, byte);
                check(&bytes);
                bytes.make_ascii_uppercase();
                check(&bytes);
            }
        }
        for other in words {
            let mut bytes = word.to_vec();
            bytes.extend_from_slice(other);
            check(&bytes);
        }
    }
    for raw in include_str!("../tests/support/asset_names.txt").lines() {
        check(raw.as_bytes());
        check(raw.to_ascii_uppercase().as_bytes());
    }
    for raw in [
        b"clampoint".as_slice(),
        b"nomipmaps",
        b"repeatstretchwrap",
        b"\x116bpp\x132bpp",
    ] {
        check(raw);
    }
    let mut state = 0x9182_56abu32;
    for _ in 0..20_000 {
        let mut bytes = Vec::new();
        for _ in 0..(state as usize % 100) {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            bytes.push((state >> 24) as u8);
        }
        check(&bytes);
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    }
}

#[test]
#[ignore = "optimized CPU-cycle and allocation benchmark"]
fn benchmark_hint_flags() {
    let shipped: Vec<String> = include_str!("../tests/support/asset_names.txt")
        .lines()
        .map(str::to_owned)
        .collect();
    let workloads = [
        ("shipped", shipped),
        ("short", ["", "x", "default", "(doubleres)", "nearest mipmaps", "nomipmaps clamp"].map(str::to_owned).to_vec()),
        ("long", vec![format!("{} (doubleres)", "a".repeat(1024)), "z".repeat(1024)]),
        ("keywords", vec!["16bpp 32BPP alphamap clamp dither doubleres grayscale linear mipmaps nearest nomipmaps point repeat stretch wrap".to_owned(), "clampoint mipmipmaps nomipmapsnomipmaps nearestnearest".to_owned()]),
    ];
    for (name, names) in workloads {
        let work = || {
            let scanner = black_box(texture_hint_flags as fn(&[u8]) -> u16);
            for name in &names {
                black_box(scanner(black_box(name.as_bytes())));
            }
        };
        let (_, churn) = crate::perf::measure(work);
        let label = format!("metadata/flags/{name}");
        println!("{label}: allocation {churn:?}");
        bench::measure(&label, 128, work);
    }
}
