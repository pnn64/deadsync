//! Differential identity policy checks and paired CPU/allocation measurements.
use super::*;
use crate::perf;
use std::hint::black_box;
use std::io::Write;
#[allow(
    clippy::duplicate_mod,
    reason = "private performance modules share timing helpers"
)]
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "identity_original.rs"]
mod original;

fn next(seed: &mut u64) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed as u32
}

fn same_string(a: String, b: String) {
    assert_eq!(a, b);
    assert_eq!(a.capacity(), b.capacity());
}
fn check_derived(s: &str) {
    match (
        original::profile_guid_from_itgmania_guid(s),
        profile_guid_from_itgmania_guid(s),
    ) {
        (Some(a), Some(b)) => same_string(a, b),
        (None, None) => {}
        _ => panic!("derived GUID presence changed"),
    }
}

#[test]
fn initials_preserve_all_scalars_capacity_and_stop_position() {
    for word in 0..0x110000 {
        let Some(ch) = char::from_u32(word) else {
            continue;
        };
        for prefix in ["", "abc", "abcd"] {
            let s = format!("{prefix}{ch}Z!?5");
            same_string(
                original::sanitize_player_initials(&s),
                sanitize_player_initials(&s),
            );
        }
    }
    for n in [0, 1, 2, 3, 4, 31, 64, 1024] {
        for token in ["a", "é", "😀", "  -_", "!?", "ß", "\0"] {
            let s = format!("{}Ab!9Z", token.repeat(n));
            same_string(
                original::sanitize_player_initials(&s),
                sanitize_player_initials(&s),
            );
        }
    }
}

#[test]
fn guid_validation_preserves_every_byte_at_every_position() {
    let canonical = b"01234567-89ab-cdef-ABCD-0123456789EF";
    assert!(is_valid_profile_guid(
        std::str::from_utf8(canonical).unwrap()
    ));
    for position in 0..canonical.len() {
        for byte in 0..128 {
            let mut bytes = *canonical;
            bytes[position] = byte;
            let s = std::str::from_utf8(&bytes).unwrap();
            assert_eq!(
                original::is_valid_profile_guid(s),
                is_valid_profile_guid(s),
                "{s:?}"
            );
        }
        for ch in ['é', '東', '😀', '\u{2003}', 'Ａ'] {
            let mut s = String::from_utf8(canonical.to_vec()).unwrap();
            s.replace_range(position..position + 1, &ch.to_string());
            assert_eq!(
                original::is_valid_profile_guid(&s),
                is_valid_profile_guid(&s)
            );
        }
    }
    for n in 0..80 {
        for c in ['a', '-', '0', 'é'] {
            let s = c.to_string().repeat(n);
            assert_eq!(
                original::is_valid_profile_guid(&s),
                is_valid_profile_guid(&s)
            );
        }
    }
}

#[test]
fn derived_guids_preserve_ascii_folding_trim_and_stack_boundary() {
    assert_eq!(
        profile_guid_from_itgmania_guid("99f55b745304ebcf").as_deref(),
        Some("f0b6ee3d-8535-5826-96eb-15fd8ab16ab0")
    );
    for n in 0..140 {
        for text in ["aB09", "éZ", "😀A", "\0Az", "\u{2003}", "ßİAZ"] {
            for s in [text.repeat(n), format!("\u{2003}{}\t", text.repeat(n))] {
                check_derived(&s);
            }
        }
    }
    for ch in 0..128u8 {
        let s = format!("A{}Z", char::from(ch));
        check_derived(&s);
    }
    let mut seed = 0x1c72_aa39_9834_65bd;
    let alphabet = [
        'a', 'Z', '0', '-', '?', '!', 'é', '東', '😀', 'ß', 'İ', '\u{2003}', '\0', '\t',
    ];
    for _ in 0..16384 {
        let s: String = (0..next(&mut seed) % 150)
            .map(|_| alphabet[next(&mut seed) as usize % alphabet.len()])
            .collect();
        check_derived(&s);
        same_string(
            original::sanitize_player_initials(&s),
            sanitize_player_initials(&s),
        );
        assert_eq!(
            original::is_valid_profile_guid(&s),
            is_valid_profile_guid(&s)
        );
    }
}

fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    for (current, name) in [(false, "original"), (true, "current")] {
        let (_, churn) = perf::measure(|| work(current));
        println!("{label}/{name}: allocation {churn:?}");
    }
    bench::compare(label, iterations, work);
}

fn initials_fixtures() -> Vec<(&'static str, String)> {
    vec![
        ("empty", String::new()),
        ("short", "ab?c!de".into()),
        ("dirty", " a b-c_d!".into()),
        ("unicode", "éß😀東a İb?c!".into()),
        ("invalid", "😀é東ß".repeat(256)),
        ("ascii-invalid", " _-+ .".repeat(256)),
        ("early", format!("abcd{}", "😀é東ß".repeat(256))),
    ]
}
fn validation_fixtures() -> Vec<(&'static str, String)> {
    vec![
        ("lower", "01234567-89ab-cdef-abcd-0123456789ef".into()),
        ("upper", "01234567-89AB-CDEF-ABCD-0123456789EF".into()),
        ("early", "x1234567-89ab-cdef-abcd-0123456789ef".into()),
        ("late", "01234567-89ab-cdef-abcd-0123456789ex".into()),
        ("hyphen", "01234567_89ab-cdef-abcd-0123456789ef".into()),
        ("empty", String::new()),
        ("short", "01234567".into()),
        ("long", "a".repeat(4096)),
        ("unicode", "é123456-89ab-cdef-abcd-0123456789ef".into()),
    ]
}
fn derivation_fixtures() -> Vec<(&'static str, String)> {
    vec![
        ("lower16", "99f55b745304ebcf".into()),
        ("upper16", "99F55B745304EBCF".into()),
        ("trimmed", "\u{2003}99F55B745304EBCF \t".into()),
        ("empty", String::new()),
        ("blank", " \t\u{2003}".into()),
        ("unicode", "éİ😀ßAZ".into()),
        ("len63", "Ab9".repeat(21)),
        ("len64", "Ab09".repeat(16)),
        ("len65", format!("{}Z", "Ab09".repeat(16))),
        ("long", "Ab09é".repeat(256)),
    ]
}
#[test]
#[ignore = "paired identity CPU and allocation measurements"]
fn benchmark_identity() {
    for (label, s) in initials_fixtures() {
        compare(
            &format!("initials/{label}"),
            if label.ends_with("invalid") {
                2000
            } else {
                20000
            },
            |current| {
                let s = black_box(s.as_str());
                drop(black_box(if current {
                    sanitize_player_initials(s)
                } else {
                    original::sanitize_player_initials(s)
                }));
            },
        );
    }
    for (label, s) in validation_fixtures() {
        compare(&format!("validate/{label}"), 200000, |current| {
            let s = black_box(s.as_str());
            black_box(if current {
                is_valid_profile_guid(s)
            } else {
                original::is_valid_profile_guid(s)
            });
        });
    }
    for (label, s) in derivation_fixtures() {
        compare(&format!("derive/{label}"), 2000, |current| {
            let s = black_box(s.as_str());
            drop(black_box(if current {
                profile_guid_from_itgmania_guid(s)
            } else {
                original::profile_guid_from_itgmania_guid(s)
            }));
        });
    }
}

fn emit(writer: &mut impl Write, s: &str) {
    let initials = sanitize_player_initials(s);
    writer
        .write_all(&(initials.capacity() as u64).to_le_bytes())
        .unwrap();
    writer
        .write_all(&(initials.len() as u64).to_le_bytes())
        .unwrap();
    writer.write_all(initials.as_bytes()).unwrap();
    writer
        .write_all(&[u8::from(is_valid_profile_guid(s))])
        .unwrap();
    match profile_guid_from_itgmania_guid(s) {
        None => writer.write_all(&[0]).unwrap(),
        Some(guid) => {
            writer.write_all(&[1]).unwrap();
            writer
                .write_all(&(guid.capacity() as u64).to_le_bytes())
                .unwrap();
            writer
                .write_all(&(guid.len() as u64).to_le_bytes())
                .unwrap();
            writer.write_all(guid.as_bytes()).unwrap();
        }
    }
}
#[test]
#[ignore = "deterministic public identity output trace"]
fn dump_identity_trace() {
    let file =
        std::fs::File::create(std::env::var_os("DEADSYNC_IDENTITY_TRACE_OUT").expect("trace path"))
            .unwrap();
    let mut out = std::io::BufWriter::with_capacity(1024 * 1024, file);
    for fixtures in [
        initials_fixtures(),
        validation_fixtures(),
        derivation_fixtures(),
    ] {
        for (_, s) in fixtures {
            emit(&mut out, &s);
        }
    }
    let mut seed = 0x1c72_aa39_9834_65bd;
    let alphabet = [
        'a', 'Z', '0', '-', '?', '!', 'é', '東', '😀', 'ß', 'İ', '\u{2003}', '\0', '\t',
    ];
    for _ in 0..16384 {
        let s: String = (0..next(&mut seed) % 150)
            .map(|_| alphabet[next(&mut seed) as usize % alphabet.len()])
            .collect();
        emit(&mut out, &s);
    }
    for word in 0..0x110000 {
        let Some(ch) = char::from_u32(word) else {
            continue;
        };
        let mut bytes = [0; 4];
        let s = ch.encode_utf8(&mut bytes);
        let initials = sanitize_player_initials(s);
        out.write_all(&(initials.capacity() as u64).to_le_bytes())
            .unwrap();
        out.write_all(&(initials.len() as u64).to_le_bytes())
            .unwrap();
        out.write_all(initials.as_bytes()).unwrap();
    }
    out.flush().unwrap();
}
