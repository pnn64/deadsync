//! Exact folder-name policy checks and CPU/allocation comparisons.
use super::*;
use crate::perf;
use std::hint::black_box;
use std::io::Write;
#[path = "folder_paths_ablation.rs"]
mod ablation;
#[allow(
    clippy::duplicate_mod,
    reason = "private profile performance modules share timing helpers"
)]
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "folder_paths_original.rs"]
mod original;
fn next(seed: &mut u64) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed as u32
}
fn compare_string(a: Option<String>, b: Option<String>) {
    match (a, b) {
        (Some(a), Some(b)) => {
            assert_eq!(a, b);
            assert_eq!(a.capacity(), b.capacity());
        }
        (None, None) => {}
        _ => panic!("sanitizer presence changed"),
    }
}
#[test]
fn reserved_names_preserve_ascii_case_and_exact_set() {
    for a in 0..128u8 {
        for b in 0..128u8 {
            for c in 0..128u8 {
                let bytes = [a, b, c];
                let s = std::str::from_utf8(&bytes).unwrap();
                assert_eq!(
                    original::is_windows_reserved_name(s),
                    is_windows_reserved_name(s)
                );
            }
        }
    }
    for prefix in ["COM", "LPT", "CON", "PRN", "AUX", "NUL"] {
        for mask in 0..8 {
            let prefix: String = prefix
                .bytes()
                .enumerate()
                .map(|(i, b)| char::from(if mask & (1 << i) == 0 { b } else { b + 32 }))
                .collect();
            for byte in 0..128u8 {
                let s = format!("{prefix}{}", char::from(byte));
                assert_eq!(
                    original::is_windows_reserved_name(&s),
                    is_windows_reserved_name(&s)
                );
            }
        }
    }
    for s in [
        "",
        "CON.",
        "COM0",
        "COM10",
        " LPT1",
        "LPT1 ",
        "COM¹",
        "LPT²",
        "éé",
        "ＣＯＮ",
        "nul.txt",
    ] {
        assert_eq!(
            original::is_windows_reserved_name(s),
            is_windows_reserved_name(s)
        );
    }
}
#[test]
fn sanitizer_preserves_every_unicode_scalar_limits_and_capacity() {
    for word in 0..0x110000 {
        let Some(ch) = char::from_u32(word) else {
            continue;
        };
        let mut s = String::from(" .A");
        s.push(ch);
        s.push_str(" B. ");
        compare_string(original::sanitize_folder_base(&s), sanitize_folder_base(&s));
    }
    for n in 0..55 {
        for tail in [
            "é",
            "東",
            "😀",
            "\u{2003}",
            "\u{85}",
            " .",
            "/\\:*?\"<>|\0",
            "  /  ",
        ] {
            let s = format!(".{}{} .", "x".repeat(n), tail);
            compare_string(original::sanitize_folder_base(&s), sanitize_folder_base(&s));
        }
    }
    let mut seed = 0x1234_5678_9abc_def0;
    let chars = [
        'a', 'A', '.', ' ', '/', '\\', ':', '*', '?', '"', '<', '>', '|', '\0', '\r', '\t',
        '\u{85}', '\u{2003}', 'é', '東', '😀',
    ];
    for _ in 0..8192 {
        let s: String = (0..next(&mut seed) % 100)
            .map(|_| chars[next(&mut seed) as usize % chars.len()])
            .collect();
        compare_string(original::sanitize_folder_base(&s), sanitize_folder_base(&s));
    }
}
fn existing(base: &str, n: usize) -> Vec<String> {
    (1..=n)
        .map(|i| {
            if i == 1 {
                base.to_string()
            } else {
                format!("{base}-{i}")
            }
        })
        .collect()
}
fn check_folder(display: &str, names: &[String]) {
    let a = original::folder_name_for_display(display, "fallback", names);
    let b = folder_name_for_display(display, "fallback", names);
    assert_eq!(a, b);
    assert_eq!(a.capacity(), b.capacity());
}
#[test]
fn collision_gaps_canonical_suffixes_and_exhaustion_match() {
    for base in ["Player", "é東", "A-B", "x . x"] {
        for n in [0, 1, 2, 8, 31, 32, 33, 63, 64, 127, 256] {
            let mut names = existing(base, n);
            check_folder(base, &names);
            names.reverse();
            check_folder(base, &names);
            if names.len() > 2 {
                names.remove(names.len() / 2);
                check_folder(base, &names);
            }
        }
    }
    let base = "Player";
    let mut names = existing(base, 9999);
    check_folder(base, &names);
    for gap in [2, 31, 32, 63, 64, 127, 128, 9984, 9999] {
        let saved = std::mem::take(&mut names[gap - 1]);
        check_folder(base, &names);
        names[gap - 1] = saved;
    }
    let mut names = vec!["player".to_string(); 64];
    for suffix in [
        "2", "02", "+2", "2 ", " 2", "2x", "-2", "1", "0", "10000", "99999", "٢", "２", "",
    ] {
        names.push(format!("PLAYER-{suffix}"));
    }
    check_folder(base, &names);
    for bad in ["CON", "   ", "../", "COM1", "...", "\u{85}"] {
        check_folder(bad, &names);
    }
}
fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    for (current, name) in [(false, "original"), (true, "current")] {
        let (_, churn) = perf::measure(|| work(current));
        println!("{label}/{name}: allocation {churn:?}");
    }
    bench::compare(label, iterations, work);
}
#[test]
#[ignore = "paired folder-path CPU and allocation measurements"]
fn benchmark_folder_paths() {
    for (label, names) in [
        (
            "reserved",
            vec!["CON", "PRN", "AUX", "NUL", "COM1", "com9", "LPT1", "lpt9"],
        ),
        ("short", vec!["A", "Bob", "Alice", "Player", "Base"]),
        (
            "long",
            vec!["Player Profile Name", "ordinary long nonreserved name"],
        ),
        ("near", vec!["COM0", "COMA", "LPT0", "LPTX", "COP", "NUN"]),
        ("unicode", vec!["東", "éé", "ＣＯＮ", "COM¹", "😀"]),
        ("empty", vec!["", " ", "  ", "    "]),
    ] {
        let functions = [
            original::is_windows_reserved_name as fn(&str) -> bool,
            is_windows_reserved_name,
        ];
        compare(&format!("reserved/{label}"), 100000, |current| {
            let f = black_box(functions[usize::from(current)]);
            for s in black_box(&names) {
                black_box(f(black_box(s)));
            }
        });
    }
    for (label, text) in [
        ("clean", "Player"),
        ("spaces", "  Player  "),
        ("dirty", "./A:/  B\\  ."),
        ("unicode", "é東😀"),
        ("long", &"abcdefghij".repeat(16)),
        ("limit", &format!("{}😀", "x".repeat(47))),
        ("reserved", "cOm1"),
        ("empty", " / .\0 "),
    ] {
        let functions = [
            original::sanitize_folder_base as fn(&str) -> Option<String>,
            sanitize_folder_base,
        ];
        compare(&format!("sanitize/{label}"), 20000, |current| {
            black_box(functions[usize::from(current)](black_box(text)));
        });
        let functions = [
            ablation::sanitize_with_copy as fn(&str) -> Option<String>,
            sanitize_folder_base,
        ];
        compare(&format!("sanitize/reuse/{label}"), 20000, |current| {
            black_box(functions[usize::from(current)](black_box(text)));
        });
    }
    for (label, second) in [("sparse", false), ("gap3", true)] {
        let mut names: Vec<_> = (0..2048).map(|i| format!("Other {i}")).collect();
        names.push("Player".to_string());
        if second {
            names.push("Player-2".to_string());
        }
        for isolated in [false, true] {
            let functions = [
                if isolated {
                    ablation::folder_name_for_display as fn(&str, &str, &[String]) -> String
                } else {
                    original::folder_name_for_display
                },
                folder_name_for_display,
            ];
            compare(
                &format!(
                    "folder/{label}/{}",
                    if isolated { "index" } else { "combined" }
                ),
                20,
                |current| {
                    black_box(functions[usize::from(current)](
                        black_box("Player"),
                        black_box("fallback"),
                        black_box(&names),
                    ));
                },
            );
        }
    }
    for n in [0, 8, 31, 32, 64, 256, 2048, 9999usize] {
        let names = existing("Player", n);
        let iterations = (10000 / n.max(1).saturating_mul(n.max(1))).clamp(1, 100);
        let functions = [
            original::folder_name_for_display as fn(&str, &str, &[String]) -> String,
            folder_name_for_display,
        ];
        compare(&format!("folder/{n}"), iterations, |current| {
            black_box(functions[usize::from(current)](
                black_box("Player"),
                black_box("fallback"),
                black_box(&names),
            ));
        });
        let functions = [
            ablation::folder_name_for_display as fn(&str, &str, &[String]) -> String,
            folder_name_for_display,
        ];
        compare(&format!("folder/index/{n}"), iterations, |current| {
            black_box(functions[usize::from(current)](
                black_box("Player"),
                black_box("fallback"),
                black_box(&names),
            ));
        });
    }
}
#[test]
#[ignore = "deterministic folder-name policy and capacity trace"]
fn dump_folder_trace() {
    reserved_names_preserve_ascii_case_and_exact_set();
    sanitizer_preserves_every_unicode_scalar_limits_and_capacity();
    collision_gaps_canonical_suffixes_and_exhaustion_match();
    let mut out = std::io::BufWriter::with_capacity(
        1 << 20,
        std::fs::File::create(std::env::var_os("DEADSYNC_FOLDER_TRACE_OUT").unwrap()).unwrap(),
    );
    for word in 0..0x110000 {
        let Some(ch) = char::from_u32(word) else {
            continue;
        };
        let s = format!(" .A{ch} B. ");
        if let Some(s) = sanitize_folder_base(&s) {
            out.write_all(&(s.capacity() as u64).to_le_bytes()).unwrap();
            out.write_all(&(s.len() as u64).to_le_bytes()).unwrap();
            out.write_all(s.as_bytes()).unwrap();
        } else {
            out.write_all(&u64::MAX.to_le_bytes()).unwrap();
        }
    }
    for n in [0, 1, 8, 31, 32, 33, 63, 64, 256, 2048, 9999] {
        let names = existing("Player", n);
        let s = folder_name_for_display("Player", "fallback", &names);
        out.write_all(&(s.capacity() as u64).to_le_bytes()).unwrap();
        out.write_all(&(s.len() as u64).to_le_bytes()).unwrap();
        out.write_all(s.as_bytes()).unwrap();
        out.write_all(&[0]).unwrap();
    }
    out.flush().unwrap();
}
