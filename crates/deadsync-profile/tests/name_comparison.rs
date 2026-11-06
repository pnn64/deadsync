//! Differential name-order checks and paired sorting CPU measurements.
use deadsync_profile::{cmp_profile_ids_case_insensitive, favorites_view};
use std::cmp::Ordering;
use std::hint::black_box;
use std::io::Write;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/name_comparison_original.rs"]
mod original;
#[path = "../../../tests/support/perf.rs"]
mod perf;
type Compare = fn(&str, &str) -> Ordering;
const FUNCTIONS: [(&str, Compare, Compare); 3] = [
    (
        "ascii",
        original::ascii_case_insensitive_cmp,
        favorites_view::ascii_case_insensitive_cmp,
    ),
    (
        "unicode",
        original::unicode_case_insensitive_cmp,
        favorites_view::unicode_case_insensitive_cmp,
    ),
    (
        "profile",
        original::cmp_profile_ids_case_insensitive,
        cmp_profile_ids_case_insensitive,
    ),
];
fn next(seed: &mut u64) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed as u32
}
fn check(a: &str, b: &str) {
    for (name, old, new) in FUNCTIONS {
        assert_eq!(old(a, b), new(a, b), "{name}: {a:?} / {b:?}");
    }
}
fn pairs(kind: &str, n: usize) -> Vec<(String, String)> {
    (0..n)
        .map(|i| match kind {
            "short" => (
                format!("Pack {:03}", i % 121),
                format!("pack {:03}", (i * 37) % 121),
            ),
            "early" => (
                format!(
                    "{}{}",
                    char::from(b'A' + (i % 25) as u8),
                    "same long song prefix ".repeat(4)
                ),
                format!(
                    "{}{}",
                    char::from(b'b' + (i % 25) as u8),
                    "same long song prefix ".repeat(4)
                ),
            ),
            "near" => (
                format!("AA{}", "same long song prefix ".repeat(4)),
                format!("AB{}", "same long song prefix ".repeat(4)),
            ),
            "random" => {
                let mut seed = (i as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
                let a = (0..40)
                    .map(|_| char::from(b'A' + (next(&mut seed) % 26) as u8))
                    .collect();
                let b = (0..40)
                    .map(|_| char::from(b'a' + (next(&mut seed) % 26) as u8))
                    .collect();
                (a, b)
            }
            "shared" => (
                format!("SUPER EUROBEAT compilation volume {:05}", i),
                format!("super eurobeat COMPILATION VOLUME {:05}", i + 1),
            ),
            "uuid" => (
                format!("550E8400-E29B-41D4-A716-{i:012X}"),
                format!(
                    "550e8400-e29b-41d4-a716-{:012x}",
                    if i % 3 == 0 { i } else { i + 1 }
                ),
            ),
            "unicode" => (
                format!("The Same ASCII Prefix / ΣÄİ東京 {i:05}"),
                format!("the same ascii prefix / σäİ東京 {:05}", i + 1),
            ),
            "equal" => {
                let s = format!("{} {i:05}", "identical ASCII common prefix ".repeat(4));
                (s.clone(), s)
            }
            _ => unreachable!(),
        })
        .collect()
}
#[test]
fn block_boundaries_ascii_bytes_and_unicode_expansions_match() {
    for size in [0, 1, 7, 8, 14, 15, 16, 17, 30, 31, 32, 33, 64] {
        let prefix = "a".repeat(size);
        for a in 0..128u8 {
            for b in 0..128u8 {
                check(
                    &format!("{prefix}{}suffix", char::from(a)),
                    &format!("{prefix}{}suffix", char::from(b)),
                );
            }
        }
        for a in [
            "", "A", "z", "é", "Σ", "σ", "ς", "İ", "i\u{307}", "ß", "ẞ", "東京", "😀",
        ] {
            for b in [
                "", "a", "Z", "É", "Σ", "σ", "ς", "İ", "i\u{307}", "ß", "ẞ", "東京", "😀",
            ] {
                for tail in ["", "abc", "x".repeat(33).as_str()] {
                    check(
                        &format!("{prefix}{a}{tail}"),
                        &format!("{}{b}{tail}", prefix.to_ascii_uppercase()),
                    );
                }
            }
        }
    }
}
#[test]
fn random_utf8_pairs_prefixes_and_profile_ties_match() {
    let alphabet = [
        'a', 'A', 'z', 'Z', '0', '-', ' ', 'é', 'Σ', 'σ', 'ς', 'İ', '\u{307}', 'ß', 'ẞ', '東',
        '京', '😀', '\0',
    ];
    let mut seed = 0x1234_5678_9abc_def0;
    for _ in 0..32768 {
        let prefix: String = (0..next(&mut seed) % 64)
            .map(|_| alphabet[next(&mut seed) as usize % alphabet.len()])
            .collect();
        let a = format!(
            "{prefix}{}",
            char::from_u32(next(&mut seed) % 0x110000).unwrap_or('x')
        );
        let b = match next(&mut seed) % 3 {
            0 => a.clone(),
            1 => a.to_lowercase(),
            _ => format!(
                "{prefix}{}",
                char::from_u32(next(&mut seed) % 0x110000).unwrap_or('y')
            ),
        };
        check(&a, &b);
        check(&b, &a);
    }
    for n in 0..1024 {
        let a = format!("ABCDEF00-1234-5678-9ABC-{n:012X}");
        let b = a.to_ascii_lowercase();
        check(&a, &b);
        assert_eq!(cmp_profile_ids_case_insensitive(&a, &b), a.cmp(&b));
    }
}
#[test]
fn complete_sort_orders_and_allocation_behavior_match() {
    for kind in [
        "short", "early", "near", "shared", "uuid", "unicode", "equal", "random",
    ] {
        let pairs = pairs(kind, 512);
        let names: Vec<_> = pairs
            .iter()
            .flat_map(|(a, b)| [a.as_str(), b.as_str()])
            .collect();
        for (_, old, new) in FUNCTIONS {
            let mut a = names.clone();
            let mut b = names.clone();
            a.sort_unstable_by(|a, b| old(a, b));
            b.sort_unstable_by(|a, b| new(a, b));
            assert_eq!(a, b);
            a.clone_from(&names);
            b.clone_from(&names);
            a.sort_by(|a, b| old(a, b));
            b.sort_by(|a, b| new(a, b));
            assert_eq!(a, b);
        }
    }
    for (_, _, new) in FUNCTIONS {
        perf::assert_no_churn(|| {
            black_box(new(
                black_box("SAME LONG ASCII PREFIX 123456789"),
                black_box("same long ascii prefix 123456790"),
            ));
        });
    }
}
fn compare(label: &str, iterations: usize, mut f: impl FnMut(bool)) {
    for (current, name) in [(false, "original"), (true, "current")] {
        let (_, c) = perf::measure(|| f(current));
        assert_eq!(c, perf::Churn::default());
        println!("{label}/{name}: allocation {c:?}");
    }
    bench::compare(label, iterations, f);
}
#[test]
#[ignore = "paired comparator and full-sort CPU measurements"]
fn benchmark_name_comparison() {
    for (family, old, new) in FUNCTIONS {
        for kind in [
            "short", "early", "near", "shared", "uuid", "unicode", "equal", "random",
        ] {
            let inputs = pairs(kind, 256);
            let functions = [old, new];
            compare(&format!("compare/{family}/{kind}"), 500, |current| {
                let f = black_box(functions[usize::from(current)]);
                for (a, b) in black_box(&inputs) {
                    black_box(f(black_box(a), black_box(b)));
                }
            });
        }
        for kind in ["shared", "uuid", "unicode", "random"] {
            let inputs = pairs(kind, 512);
            let names: Vec<_> = inputs
                .iter()
                .flat_map(|(a, b)| [a.as_str(), b.as_str()])
                .collect();
            let mut work = names.clone();
            let functions = [old, new];
            compare(&format!("sort/{family}/{kind}"), 10, |current| {
                work.copy_from_slice(&names);
                let f = black_box(functions[usize::from(current)]);
                work.sort_unstable_by(|a, b| f(a, b));
                black_box(&work);
            });
        }
    }
}
#[test]
#[ignore = "deterministic ordering and complete-sort trace"]
fn dump_name_trace() {
    block_boundaries_ascii_bytes_and_unicode_expansions_match();
    random_utf8_pairs_prefixes_and_profile_ties_match();
    complete_sort_orders_and_allocation_behavior_match();
    let mut out = std::io::BufWriter::with_capacity(
        1 << 20,
        std::fs::File::create(std::env::var_os("DEADSYNC_NAME_TRACE_OUT").unwrap()).unwrap(),
    );
    for kind in [
        "short", "early", "near", "shared", "uuid", "unicode", "equal", "random",
    ] {
        let inputs = pairs(kind, 1024);
        for (_, old, new) in FUNCTIONS {
            for (a, b) in &inputs {
                let x = new(a, b);
                assert_eq!(x, old(a, b));
                out.write_all(&[match x {
                    Ordering::Less => 0,
                    Ordering::Equal => 1,
                    Ordering::Greater => 2,
                }])
                .unwrap();
            }
            let mut indices: Vec<_> = (0..inputs.len() * 2).collect();
            let names: Vec<_> = inputs
                .iter()
                .flat_map(|(a, b)| [a.as_str(), b.as_str()])
                .collect();
            let mut before = indices.clone();
            before.sort_unstable_by(|&a, &b| old(names[a], names[b]));
            indices.sort_unstable_by(|&a, &b| new(names[a], names[b]));
            assert_eq!(before, indices);
            for index in indices {
                out.write_all(&(index as u64).to_le_bytes()).unwrap();
            }
        }
    }
    out.flush().unwrap();
}
