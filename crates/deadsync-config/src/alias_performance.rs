use super::{BreakdownStyle, DefaultSyncOffset, VersionOverlaySide};
use crate::dirs::resource_perf::{support as perf, text_codecs::bench};
use std::io::Write;
use std::str::FromStr;
#[path = "alias_original.rs"]
mod original;
fn check(s: &str) {
    assert_eq!(
        original::breakdown(s),
        BreakdownStyle::from_str(s),
        "breakdown {s:?}"
    );
    assert_eq!(
        original::sync(s),
        DefaultSyncOffset::from_str(s),
        "sync {s:?}"
    );
    assert_eq!(
        original::side(s),
        VersionOverlaySide::from_str(s),
        "side {s:?}"
    );
}
fn cases(step: usize, mut visit: impl FnMut(&str)) {
    for name in [
        "sl", "sn", "null", "none", "off", "itg", "left", "l", "lhs", "right", "r", "rhs",
    ] {
        for mask in 0..1usize << name.len() {
            let s: String = name
                .bytes()
                .enumerate()
                .map(|(i, b)| {
                    char::from(if mask & (1 << i) == 0 {
                        b
                    } else {
                        b.to_ascii_uppercase()
                    })
                })
                .collect();
            visit(&s);
            visit(&format!("\u{2003}{s}\t\n"));
        }
        for at in 0..=name.len() {
            for byte in 0..128u8 {
                visit(&format!(
                    "{}{}{}",
                    &name[..at],
                    char::from(byte),
                    &name[at..]
                ));
            }
        }
    }
    for word in (0..0x110000).step_by(step) {
        if let Some(ch) = char::from_u32(word) {
            visit(&ch.to_string());
            visit(&format!("{ch}sl"));
            visit(&format!("right{ch}"));
        }
    }
    let mut seed = 0x1234_5678u32;
    let chars = [
        's', 'l', 'n', 'u', 'o', 'f', 'i', 't', 'g', 'e', 'h', 'r', 'L', 'R', '_', '-', ' ', '.',
        '\0', 'é', '雪', '\u{2003}',
    ];
    for _ in 0..16384 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let len = seed % 25;
        let s: String = (0..len)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                chars[seed as usize % chars.len()]
            })
            .collect();
        visit(&s);
    }
    for s in [
        "",
        "s-l",
        "l_h_s",
        "no-ne",
        "OFF!",
        "Ｌ",
        "\u{feff}right",
        "\0",
        "s l",
    ] {
        visit(s);
    }
}
#[test]
fn strict_aliases_case_whitespace_unicode_and_rejections_match() {
    cases(1, check);
}
fn compare(label: &str, mut work: impl FnMut(bool)) {
    for (c, name) in [(false, "original"), (true, "current")] {
        let (_, churn) = perf::measure(|| work(c));
        println!("{label}/{name}: allocation {churn:?}");
    }
    bench::compare(label, 50000, work);
}
#[test]
#[ignore]
fn benchmark() {
    use std::hint::black_box as bb;
    for (label, s) in [
        ("sl", "SL"),
        ("sn", "sN"),
        ("space", "\u{2003} sL\t"),
        ("invalid", "s-l"),
        ("empty", ""),
    ] {
        compare(&format!("breakdown/{label}"), |c| {
            let _ = bb(if c {
                BreakdownStyle::from_str(bb(s))
            } else {
                original::breakdown(bb(s))
            });
        });
    }
    for (label, s) in [
        ("itg", "iTG"),
        ("null", "NuLL"),
        ("none", "NONE"),
        ("off", "oFf"),
        ("invalid", "no-ne"),
        ("empty", ""),
    ] {
        compare(&format!("sync/{label}"), |c| {
            let _ = bb(if c {
                DefaultSyncOffset::from_str(bb(s))
            } else {
                original::sync(bb(s))
            });
        });
    }
    for (label, s) in [
        ("left", "LEFT"),
        ("right", "RiGhT"),
        ("l", "L"),
        ("r", "r"),
        ("lhs", "LhS"),
        ("rhs", "RHS"),
        ("space", "\u{2003} LEFT \t"),
        ("invalid", "l_h_s"),
        ("empty", ""),
    ] {
        compare(&format!("side/{label}"), |c| {
            let _ = bb(if c {
                VersionOverlaySide::from_str(bb(s))
            } else {
                original::side(bb(s))
            });
        });
    }
}

#[test]
#[ignore = "deterministic strict-alias input/result trace"]
fn dump_trace() {
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_CONFIG_ALIAS_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    cases(17, |s| {
        out.write_all(&(s.len() as u64).to_le_bytes()).unwrap();
        out.write_all(s.as_bytes()).unwrap();
        let a = match BreakdownStyle::from_str(s) {
            Ok(BreakdownStyle::Sl) => 0,
            Ok(BreakdownStyle::Sn) => 1,
            Err(()) => 2,
        };
        let b = match DefaultSyncOffset::from_str(s) {
            Ok(DefaultSyncOffset::Null) => 0,
            Ok(DefaultSyncOffset::Itg) => 1,
            Err(()) => 2,
        };
        let c = match VersionOverlaySide::from_str(s) {
            Ok(VersionOverlaySide::Left) => 0,
            Ok(VersionOverlaySide::Right) => 1,
            Err(()) => 2,
        };
        out.write_all(&[a, b, c]).unwrap();
    });
    out.flush().unwrap();
}
