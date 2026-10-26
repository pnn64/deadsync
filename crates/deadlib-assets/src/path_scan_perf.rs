//! Exact-output checks and CPU/allocation measurements for filename metadata.
use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "path_scan_original.rs"]
mod original;

fn shipped_names() -> Vec<String> {
    include_str!("../tests/support/asset_names.txt")
        .lines()
        .map(str::to_owned)
        .collect()
}

fn check(raw: &str) {
    assert_eq!(
        parse_texture_resolution_hint(raw),
        original::parse_texture_resolution_hint(raw),
        "resolution: {raw:?}"
    );
    assert_eq!(
        parse_sprite_sheet_dims(raw),
        original::parse_sprite_sheet_dims(raw),
        "sheet: {raw:?}"
    );
    assert_eq!(
        texture_hint_doubleres(raw),
        original::texture_hint_doubleres(raw),
        "doubleres: {raw:?}"
    );
}

#[test]
fn shipped_names_match_frozen_metadata() {
    let names = shipped_names();
    assert!(names.len() > 100);
    for name in names {
        check(&name);
        check(&name.replace('/', "\\"));
        check(&name.to_ascii_uppercase());
    }
}

#[test]
fn metadata_scans_preserve_nested_malformed_overflow_and_unicode_inputs() {
    let pieces = [
        "",
        "x",
        "X",
        "(",
        ")",
        "(res)",
        "(RES ",
        "(res 1x2)",
        "(res (3x4) 5x6)",
        "(res 7x8",
        "(res 0x2)",
        "(res 4294967296x1)",
        " 3x4",
        "_5X6",
        " 7x8x9",
        " 0x1",
        " 1x0",
        " 4294967295x1",
        " 4294967296x1",
        "doubleres",
        "DOUBLEres",
        "doubleredoubleres",
        "doubler",
        "default",
        "\u{2003}",
        "\u{3000}",
        "é🦀中文",
        "\t\r\n",
        "\0",
    ];
    for a in pieces {
        for b in pieces {
            for c in ["", ".png", " 9x10 (res 11x12) doubleres", ") 13x14"] {
                check(&format!("{a}{b}{c}"));
            }
        }
    }
    let mut state = 0x1234_5678u32;
    let mut raw = String::new();
    for _ in 0..20_000 {
        raw.clear();
        for _ in 0..(state as usize % 32) {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            raw.push_str(pieces[state as usize % pieces.len()]);
        }
        // Advance even when the generated length is zero.
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        check(&raw);
    }
}

#[test]
fn candidate_offsets_and_non_ascii_bytes_match_frozen_metadata() {
    for offset in 0..128 {
        let prefix = "a".repeat(offset);
        for tail in [
            "(res 17x19)",
            "(res 17x19",
            "(res (1x2) 3x4) 5x6",
            "_17x19.png",
            " 17X19 2x3.png",
            "doubleres",
            "DOUBLERES",
            "doubleredoubleres",
        ] {
            for byte in 0..=255u8 {
                check(&format!("{prefix}{}{tail}", char::from(byte)));
            }
        }
    }
}

fn workloads() -> Vec<(&'static str, Vec<String>)> {
    vec![
        ("shipped", shipped_names()),
        (
            "short",
            [
                "",
                "x",
                "a.png",
                " 1x2",
                "(res 1x2)",
                "doubleres",
                " dd",
                "🦀.png",
            ]
            .map(str::to_owned)
            .to_vec(),
        ),
        (
            "long-absent",
            vec![format!("{}.png", "assets/textures/banner/".repeat(12))],
        ),
        (
            "long-present",
            vec![format!(
                "{} 16x7 (res 512x224) (doubleres).png",
                "assets/textures/banner/".repeat(12)
            )],
        ),
        (
            "dense",
            [
                "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
                "((((((((((((((",
                "dddddddddddddddddddddddddddddd",
                " 1x2 3X4 (res 5x6) 7x8",
            ]
            .map(str::to_owned)
            .to_vec(),
        ),
    ]
}

fn measure<T>(label: &str, names: &[String], parse: fn(&str) -> T) {
    let parse = black_box(parse);
    let work = || {
        for name in names {
            black_box(parse(black_box(name)));
        }
    };
    let (_, churn) = crate::perf::measure(work);
    assert_eq!(churn, crate::perf::Churn::default());
    println!("{label}: items {}; allocation {churn:?}", names.len());
    bench::measure(label, (100_000 / names.len()).max(128), work);
}

#[test]
#[ignore = "release CPU-cycle, throughput, and allocation benchmark"]
fn benchmark_metadata_scans() {
    for (label, names) in workloads() {
        measure(
            &format!("paths/res/{label}"),
            &names,
            parse_texture_resolution_hint,
        );
        measure(
            &format!("paths/sheet/{label}"),
            &names,
            parse_sprite_sheet_dims,
        );
        measure(
            &format!("paths/double/{label}"),
            &names,
            texture_hint_doubleres,
        );
        measure(&format!("paths/combined/{label}"), &names, |name| {
            (
                texture_source_frame_dims_from_real(name, 2048, 1024),
                parse_texture_hints(name),
            )
        });
    }

    let names = shipped_names();
    let mut store = TextureStore::new();
    for name in &names {
        store.insert_texture(name.clone(), (), 2048, 1024);
    }
    let work = || {
        for name in &names {
            black_box(black_box(&store).bind_texture(black_box(name)));
        }
    };
    let (_, churn) = crate::perf::measure(work);
    assert_eq!(churn, crate::perf::Churn::default());
    println!(
        "paths/bind/shipped: items {}; allocation {churn:?}",
        names.len()
    );
    bench::measure("paths/bind/shipped", 128, work);
}
