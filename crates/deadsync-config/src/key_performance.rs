use super::{MachinePreferredPlayStyle, NormalizedAsciiKey, ThemeFlag};
use crate::dirs::resource_perf::{support as perf, text_codecs::bench};
use crate::null_or_die::parse_null_or_die_kernel_target;
use crate::streaming::TwitchChannel;
use std::hint::black_box as bb;
use std::io::Write;
use std::str::FromStr;
#[path = "key_original.rs"]
mod original;
fn key(raw: &str) -> Option<([u8; 32], u8)> {
    NormalizedAsciiKey::parse(raw).map(|key| (key.bytes, key.len))
}
fn check(raw: &str) {
    assert_eq!(original::key(raw), key(raw), "key {raw:?}");
    match (
        TwitchChannel::parse(raw),
        original::TwitchChannel::parse(raw),
    ) {
        (None, None) => {}
        (Some(a), Some(b)) => {
            assert_eq!(a.as_str(), b.as_str());
            assert_eq!(a.is_empty(), b.is_empty());
            assert_eq!(
                format!("{a:?}"),
                format!("{b:?}"),
                "complete record {raw:?}"
            );
        }
        _ => panic!("channel presence {raw:?}"),
    }
    assert_eq!(
        parse_null_or_die_kernel_target(raw),
        original::target(raw),
        "target {raw:?}"
    );
    assert_eq!(
        ThemeFlag::from_str(raw),
        original::theme(raw),
        "theme {raw:?}"
    );
    assert_eq!(
        MachinePreferredPlayStyle::from_str(raw),
        original::style(raw),
        "style {raw:?}"
    );
}
fn cases(step: usize, mut visit: impl FnMut(&str)) {
    for word in (0..0x110000).step_by(step) {
        if let Some(ch) = char::from_u32(word) {
            visit(&ch.to_string());
            visit(&format!("{ch}Abc9"));
            visit(&format!("accumulator{ch}"));
        }
    }
    for name in [
        "simplylove",
        "digest",
        "accumulator",
        "twoplayer",
        "channel_09",
    ] {
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
    for n in [0, 1, 24, 25, 26, 31, 32, 33, 64, 4096] {
        for word in ["a", "A", "_", "雪", "\u{2003}"] {
            let s = word.repeat(n);
            visit(&s);
            visit(&format!("#{s}"));
            visit(&format!(" \u{2003}{s}\t "));
            visit(&format!("d_i-g.e s t{s}"));
        }
    }
    for s in [
        "#",
        "##abc",
        "# abc",
        " #abc ",
        "#abc def",
        "\0",
        "\u{feff}abc",
        "-_.",
        "d-i g_雪e s t",
        "ACCU.MULATOR",
        "ACCUMULATORx",
        "unknown",
    ] {
        visit(s);
    }
    let mut seed = 12345678u32;
    let chars = [
        'a', 'B', '9', '_', '-', ' ', '.', '#', '\0', 'é', '雪', '\u{2003}',
    ];
    for _ in 0..16384 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let n = seed % 80;
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
#[test]
fn byte_parsers_preserve_scalars_records_bounds_aliases_and_callers() {
    cases(1, check);
    for name in [
        "simplylove",
        "twoplayer",
        "channel_09",
        "digest",
        "accumulator",
    ] {
        for mask in 0..1usize << name.len() {
            let raw: String = name
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
            check(&raw);
            check(&format!("\u{2003}{raw}\t"));
        }
    }
}
fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    for (c, name) in [(false, "original"), (true, "current")] {
        let (_, v) = perf::measure(|| work(c));
        println!("{label}/{name}: allocation {v:?}");
    }
    bench::compare(label, iterations, work);
}
#[test]
#[ignore]
fn benchmark() {
    for (label, s) in [
        ("short", "SimplyLove".into()),
        ("punct", "S_i-m.p l y Love".into()),
        ("unicode", "雪-_SimplyLove😀".into()),
        ("empty", String::new()),
        ("len32", "A".repeat(32)),
        ("len33", "A".repeat(33)),
        (
            "space",
            format!("{}SimplyLove{}", "\u{2003}".repeat(8), " ".repeat(8)),
        ),
        ("ignored", "_-雪".repeat(64)),
    ] {
        compare(&format!("key/{label}"), 20000, |c| {
            bb(if c {
                key(bb(&s))
            } else {
                original::key(bb(&s))
            });
        });
    }
    for (label, s) in [
        ("lower", "channel09".into()),
        ("upper", "CHANNEL09".into()),
        ("hash", " #Mixed_Channel09 ".into()),
        ("len25", "A".repeat(25)),
        ("len26", "A".repeat(26)),
        ("invalid", "#bad channel".into()),
        ("unicode", "雪".into()),
        ("empty", String::new()),
    ] {
        compare(&format!("channel/{label}"), 20000, |c| {
            if c {
                bb(TwitchChannel::parse(bb(&s)));
            } else {
                bb(original::TwitchChannel::parse(bb(&s)));
            }
        });
    }
    for (label, s) in [
        ("digest", "Digest".into()),
        ("accumulator", "ACCUMULATOR".into()),
        ("punct", "d-i g_雪e s t".into()),
        ("invalid", "unknown".into()),
        ("late", "accumulatorX".into()),
        ("tail", format!("Digest{}", "_雪".repeat(64))),
        ("empty", String::new()),
    ] {
        compare(&format!("target/{label}"), 20000, |c| {
            bb(if c {
                parse_null_or_die_kernel_target(bb(&s))
            } else {
                original::target(bb(&s))
            });
        });
    }
    for (label, s) in [("plain", "SimplyLove"), ("punct", "s_i-m.p l y LoVe")] {
        compare(&format!("caller/theme-{label}"), 20000, |c| {
            let _ = bb(if c {
                ThemeFlag::from_str(bb(s))
            } else {
                original::theme(bb(s))
            });
        });
    }
    for (label, s) in [
        ("plain", "Versus"),
        ("punct", "2-Players"),
        ("invalid", "3players"),
    ] {
        compare(&format!("caller/style-{label}"), 20000, |c| {
            let _ = bb(if c {
                MachinePreferredPlayStyle::from_str(bb(s))
            } else {
                original::style(bb(s))
            });
        });
    }
}
#[test]
#[ignore = "deterministic key/channel/target record trace"]
fn dump_trace() {
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_CONFIG_KEY_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    cases(17, |raw| {
        out.write_all(&(raw.len() as u64).to_le_bytes()).unwrap();
        out.write_all(raw.as_bytes()).unwrap();
        match key(raw) {
            None => out.write_all(&[0]).unwrap(),
            Some((bytes, len)) => {
                out.write_all(&[1, len]).unwrap();
                out.write_all(&bytes).unwrap();
            }
        }
        match TwitchChannel::parse(raw) {
            None => out.write_all(&[0]).unwrap(),
            Some(channel) => {
                out.write_all(&[1]).unwrap();
                let record = format!("{channel:?}");
                out.write_all(&(record.len() as u64).to_le_bytes()).unwrap();
                out.write_all(record.as_bytes()).unwrap();
            }
        }
        let target = match parse_null_or_die_kernel_target(raw) {
            None => 0,
            Some(::null_or_die::KernelTarget::Digest) => 1,
            Some(::null_or_die::KernelTarget::Accumulator) => 2,
        };
        out.write_all(&[target]).unwrap();
    });
    out.flush().unwrap();
}
