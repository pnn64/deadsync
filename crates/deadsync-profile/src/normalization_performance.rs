use super::favorites_view::ascii_case_insensitive_cmp;
use super::{ComboMode, TurnOption, with_compact_option_key};
use crate::perf;
use std::hint::black_box as bb;
use std::io::Write;
use std::str::FromStr;
#[allow(
    clippy::duplicate_mod,
    reason = "private performance modules share the CPU clock"
)]
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "normalization_original.rs"]
mod original;

fn cases(step: usize, mut visit: impl FnMut(&str)) {
    for word in (0..0x110000).step_by(step) {
        if let Some(ch) = char::from_u32(word) {
            visit(&ch.to_string());
            visit(&format!("{ch}Abc9"));
            visit(&format!("abc9{ch}"));
        }
    }
    for n in [0, 1, 31, 63, 64, 65, 66, 127, 256, 1024] {
        for word in ["abc9", "Overhead", "a_b-9", "é雪", "ΟΣΣ"] {
            visit(&format!("\u{2003}{}\t", word.repeat(n)));
        }
    }
    let mut seed = 0x12345678u32;
    let chars = [
        'a', 'B', 'z', '9', '_', '-', ' ', '\0', 'é', '雪', 'Σ', 'Ο', '\u{2003}',
    ];
    for _ in 0..16384 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let n = seed % 257;
        let s: String = (0..n)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                chars[seed as usize % chars.len()]
            })
            .collect();
        visit(&s);
    }
}
fn check_text(s: &str) {
    assert_eq!(
        original::with_compact_option_key(s, str::to_owned),
        with_compact_option_key(s, str::to_owned),
        "normalized {s:?}"
    );
    assert_eq!(original::turn(s), TurnOption::from_str(s), "turn {s:?}");
    assert_eq!(original::combo(s), ComboMode::from_str(s), "combo {s:?}");
}
#[test]
fn compact_keys_preserve_all_scalars_boundaries_errors_and_callbacks() {
    cases(1, check_text);
    for s in ["", "LRMirror", "l_r-mir-ror", "unknown", "CurrentCombo"] {
        for current in [false, true] {
            let calls = std::cell::Cell::new(0);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let callback = |_: &str| {
                    calls.set(calls.get() + 1);
                    panic!("callback sentinel")
                };
                if current {
                    with_compact_option_key(s, callback)
                } else {
                    original::with_compact_option_key(s, callback)
                }
            }));
            assert!(result.is_err());
            assert_eq!(calls.get(), 1);
        }
    }
}
fn check_pair(a: &str, b: &str) {
    assert_eq!(
        original::ascii_case_insensitive_cmp(a, b),
        ascii_case_insensitive_cmp(a, b),
        "{a:?} / {b:?}"
    );
}
#[test]
fn byte_order_preserves_scalars_pairs_boundaries_and_stable_sorts() {
    for a in 0..=255u8 {
        for b in 0..=255u8 {
            for prefix in ["", "same-prefix-", "prefixABCDEFabcdefghijkl"] {
                check_pair(
                    &format!("{prefix}{}tail", char::from(a)),
                    &format!("{prefix}{}tail", char::from(b)),
                );
            }
        }
    }
    for word in 0..0x110000 {
        if let Some(ch) = char::from_u32(word) {
            let a = ch.to_string();
            check_pair(&a, &a);
            check_pair(&a, "z");
            check_pair(&a, "É");
        }
    }
    for n in [
        0, 1, 3, 4, 15, 16, 19, 20, 31, 32, 63, 64, 67, 68, 69, 127, 128, 255, 256,
    ] {
        for prefix in ["aB9", "雪é"] {
            let a = prefix.repeat(n);
            for tail in ["", "A", "a", "Z", "雪"] {
                check_pair(&a, &format!("{a}{tail}"));
                check_pair(&format!("{a}{tail}"), &a);
            }
        }
    }
    for kind in ["short", "long", "mixed"] {
        let names = sort_names(kind);
        let mut a: Vec<_> = names.iter().map(String::as_str).collect();
        let mut b = a.clone();
        a.sort_by(|a, b| original::ascii_case_insensitive_cmp(a, b));
        b.sort_by(|a, b| ascii_case_insensitive_cmp(a, b));
        assert_eq!(a, b);
    }
}
fn key_hash(s: &str) -> u64 {
    s.bytes()
        .fold(0u64, |h, b| h.wrapping_mul(131).wrapping_add(b as u64))
}
fn key_fixtures() -> Vec<(&'static str, String)> {
    vec![
        ("lower", "overhead".into()),
        ("title", "Overhead".into()),
        ("upper", "OVERHEAD".into()),
        ("compact", "avg_error_bar".into()),
        ("space", "\u{2003} overhead \t".into()),
        ("digits", "123456".into()),
        ("unicode", "ΟΣΣ雪".into()),
        ("empty", String::new()),
        ("len64", "a".repeat(64)),
        ("len65", "a".repeat(65)),
        ("len256", "a".repeat(256)),
        ("tail-upper", format!("{}Z", "a".repeat(63))),
    ]
}
fn compare_fixtures() -> Vec<(&'static str, String, String)> {
    vec![
        ("short", "Alpha".into(), "aLPHA".into()),
        ("short-same", "Profile000123".into(), "Profile000123".into()),
        ("short-late", "Profile000123".into(), "Profile000124".into()),
        ("short-prefix", "Profile001".into(), "Profile0012".into()),
        (
            "early",
            "Apple Song Long Enough".into(),
            "Banana Song Long Enough".into(),
        ),
        ("same32", "AbcD1234".repeat(4), "AbcD1234".repeat(4)),
        ("same256", "AbcD1234".repeat(32), "AbcD1234".repeat(32)),
        ("case32", "ABCD1234".repeat(4), "abcd1234".repeat(4)),
        ("case256", "ABCD1234".repeat(32), "abcd1234".repeat(32)),
        (
            "late",
            format!("{}Z", "Shared Prefix ".repeat(32)),
            format!("{}a", "Shared Prefix ".repeat(32)),
        ),
        (
            "unicode",
            format!("{}雪", "同じ文字".repeat(32)),
            format!("{}曲", "同じ文字".repeat(32)),
        ),
        (
            "partial",
            "common name prefix A suffix".into(),
            "common name prefix B suffix".into(),
        ),
    ]
}
fn sort_names(kind: &str) -> Vec<String> {
    (0..1024)
        .map(|i| {
            let index = (i * 4051) % 1024;
            match kind {
                "short" => format!("Profile{index:06}"),
                "long" => format!("{}{index:06}", "Profile collection ".repeat(12)),
                _ => format!(
                    "{}{index:06}",
                    if i % 2 == 0 {
                        "AbCd shared collection "
                    } else {
                        "abcd SHARED collection "
                    }
                ),
            }
        })
        .collect()
}
fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    for (c, variant) in [(false, "original"), (true, "current")] {
        let (_, v) = perf::measure(|| work(c));
        println!("{label}/{variant}: allocation {v:?}");
    }
    bench::compare(label, iterations, work);
}
#[test]
#[ignore]
fn benchmark() {
    for (name, s) in key_fixtures() {
        compare(&format!("key/{name}"), 20000, |c| {
            bb(if c {
                with_compact_option_key(bb(&s), key_hash)
            } else {
                original::with_compact_option_key(bb(&s), key_hash)
            });
        });
    }
    for (name, a, b) in compare_fixtures() {
        compare(&format!("ascii/{name}"), 20000, |c| {
            bb(if c {
                ascii_case_insensitive_cmp(bb(&a), bb(&b))
            } else {
                original::ascii_case_insensitive_cmp(bb(&a), bb(&b))
            });
        });
    }
    for (name, s) in [
        ("canonical", "LRMirror"),
        ("punctuation", "l_r-mir-ror"),
        ("invalid", "unknown"),
    ] {
        compare(&format!("turn/{name}"), 20000, |c| {
            bb(if c {
                TurnOption::from_str(bb(s))
            } else {
                original::turn(bb(s))
            })
            .ok();
        });
    }
    for (name, s) in [
        ("canonical", "CurrentCombo"),
        ("punctuation", " current_combo "),
    ] {
        compare(&format!("combo/{name}"), 20000, |c| {
            bb(if c {
                ComboMode::from_str(bb(s))
            } else {
                original::combo(bb(s))
            })
            .ok();
        });
    }
    for kind in ["short", "long", "mixed"] {
        let names = sort_names(kind);
        let source: Vec<_> = names.iter().map(String::as_str).collect();
        let mut values = source.clone();
        compare(&format!("sort/{kind}"), 64, |c| {
            values.copy_from_slice(bb(&source));
            if c {
                values.sort_by(|a, b| ascii_case_insensitive_cmp(a, b));
            } else {
                values.sort_by(|a, b| original::ascii_case_insensitive_cmp(a, b));
            }
            bb(&values);
        });
    }
}
#[test]
#[ignore = "deterministic normalized strings and ordering trace"]
fn dump_trace() {
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_PROFILE_KERNEL_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    fn text(out: &mut impl Write, s: &str) {
        out.write_all(&(s.len() as u64).to_le_bytes()).unwrap();
        out.write_all(s.as_bytes()).unwrap();
    }
    cases(17, |s| {
        text(&mut out, s);
        let normalized = with_compact_option_key(s, str::to_owned);
        text(&mut out, &normalized);
        for other in [s, "z", "É"] {
            let order = ascii_case_insensitive_cmp(s, other);
            out.write_all(&[match order {
                std::cmp::Ordering::Less => 0,
                std::cmp::Ordering::Equal => 1,
                std::cmp::Ordering::Greater => 2,
            }])
            .unwrap();
        }
    });
    for (_, a, b) in compare_fixtures() {
        text(&mut out, &a);
        text(&mut out, &b);
        let order = ascii_case_insensitive_cmp(&a, &b);
        out.write_all(&[match order {
            std::cmp::Ordering::Less => 0,
            std::cmp::Ordering::Equal => 1,
            std::cmp::Ordering::Greater => 2,
        }])
        .unwrap();
    }
    for kind in ["short", "long", "mixed"] {
        let mut names = sort_names(kind);
        names.sort_by(|a, b| ascii_case_insensitive_cmp(a, b));
        for name in names {
            text(&mut out, &name);
        }
    }
    out.flush().unwrap();
}
