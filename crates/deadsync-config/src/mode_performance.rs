use super::{GameFlag, MachinePreferredPlayMode, SelectMusicPatternInfoMode};
use crate::dirs::resource_perf::{support as perf, text_codecs::bench};
use std::io::Write;
use std::str::FromStr;
#[path = "mode_original.rs"]
mod original;
fn check(s: &str) {
    assert_eq!(
        original::pattern(s),
        SelectMusicPatternInfoMode::from_str(s),
        "pattern {s:?}"
    );
    assert_eq!(
        original::play(s),
        MachinePreferredPlayMode::from_str(s),
        "play {s:?}"
    );
    assert_eq!(original::game(s), GameFlag::from_str(s), "game {s:?}");
}
fn cases(step: usize, mut visit: impl FnMut(&str)) {
    for name in [
        "tech", "stamina", "auto", "regular", "marathon", "dance", "pump",
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
            visit(&format!("{ch}tech"));
            visit(&format!("dance{ch}"));
        }
    }
    let mut seed = 0x1234_5678u32;
    let chars = [
        'p', 'a', 'c', 'k', 'i', 'n', 's', 'r', 'g', 'h', 'o', 'f', 'l', 'd', 'e', 't', 'y', 'x',
        'M', 'R', '_', '-', ' ', '.', '\0', 'é', '雪', '\u{2003}',
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
        "sta-mina",
        "mara_thon",
        "dan ce",
        "pump!",
        "ＴＥＣＨ",
        "ＤＡＮＣＥ",
        "\u{feff}dance",
        "\0",
        "sta\tmina",
    ] {
        visit(s);
    }
}
#[test]
fn strict_modes_case_whitespace_unicode_and_rejections_match() {
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
        ("tech", "tECH"),
        ("stamina", "StaMINa"),
        ("auto", "AuTO"),
        ("space", "  TeCH\t"),
        ("invalid", "sta-mina"),
        ("empty", ""),
    ] {
        compare(&format!("pattern/{label}"), |c| {
            let _ = bb(if c {
                SelectMusicPatternInfoMode::from_str(bb(s))
            } else {
                original::pattern(bb(s))
            });
        });
    }
    for (label, s) in [
        ("regular", "ReGULar"),
        ("marathon", "MaraTHON"),
        ("space", "  Regular \t"),
        ("invalid", "mara-thon"),
        ("empty", ""),
    ] {
        compare(&format!("play/{label}"), |c| {
            let _ = bb(if c {
                MachinePreferredPlayMode::from_str(bb(s))
            } else {
                original::play(bb(s))
            });
        });
    }
    for (label, s) in [
        ("dance", "DANce"),
        ("pump", "pUMP"),
        ("space", "  Dance \t"),
        ("invalid", "d_ance"),
        ("empty", ""),
    ] {
        compare(&format!("game/{label}"), |c| {
            let _ = bb(if c {
                GameFlag::from_str(bb(s))
            } else {
                original::game(bb(s))
            });
        });
    }
}

#[test]
#[ignore = "deterministic strict-mode input/result trace"]
fn dump_trace() {
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_CONFIG_MODE_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    cases(17, |s| {
        out.write_all(&(s.len() as u64).to_le_bytes()).unwrap();
        out.write_all(s.as_bytes()).unwrap();
        let a = match SelectMusicPatternInfoMode::from_str(s) {
            Ok(SelectMusicPatternInfoMode::Tech) => 0,
            Ok(SelectMusicPatternInfoMode::Stamina) => 1,
            Ok(SelectMusicPatternInfoMode::Auto) => 2,
            Err(()) => 3,
        };
        let b = match MachinePreferredPlayMode::from_str(s) {
            Ok(MachinePreferredPlayMode::Regular) => 0,
            Ok(MachinePreferredPlayMode::Marathon) => 1,
            Err(()) => 2,
        };
        let c = match GameFlag::from_str(s) {
            Ok(GameFlag::Dance) => 0,
            Ok(GameFlag::Pump) => 1,
            Err(()) => 2,
        };
        out.write_all(&[a, b, c]).unwrap();
    });
    out.flush().unwrap();
}
