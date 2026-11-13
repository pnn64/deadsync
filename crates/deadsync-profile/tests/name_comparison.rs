//! Name ordering and allocation regressions.
use deadsync_profile::{cmp_profile_ids_case_insensitive, favorites_view};
use std::cmp::Ordering;
use std::hint::black_box;
#[path = "../../../tests/support/allocations.rs"]
mod perf;
type Compare = fn(&str, &str) -> Ordering;
const FUNCTIONS: [(&str, Compare); 3] = [
    ("ascii", favorites_view::ascii_case_insensitive_cmp),
    ("unicode", favorites_view::unicode_case_insensitive_cmp),
    ("profile", cmp_profile_ids_case_insensitive),
];
fn next(seed: &mut u64) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed as u32
}
fn check(a: &str, b: &str) {
    for (name, compare) in FUNCTIONS {
        assert_eq!(compare(a, b), compare(b, a).reverse(), "{name}: {a:?} / {b:?}");
        assert_eq!(compare(a, a), Ordering::Equal, "{name}: {a:?}");
    }
    assert_eq!(favorites_view::ascii_case_insensitive_cmp(a, b), a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
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
fn name_comparisons_do_not_allocate_for_ascii_inputs() {
    for (_, compare) in FUNCTIONS {
        perf::assert_no_churn(|| {
            assert_eq!(
                compare(black_box("SAME LONG ASCII PREFIX 123456789"), black_box("same long ascii prefix 123456790")),
                Ordering::Less,
            );
        });
    }
}
