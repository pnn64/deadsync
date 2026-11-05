use super::{MachineFont, normalize_locale};
use crate::dirs::resource_perf::{support as perf, text_codecs::bench};
use std::{hint::black_box as bb, io::Write, str::FromStr};
#[path = "preference_original.rs"]
mod original;

fn next(seed: &mut u64) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed as u32
}
fn locale_cases(step: usize, mut visit: impl FnMut(&str)) {
    for word in (0..0x110000).step_by(step) {
        let Some(ch) = char::from_u32(word) else {
            continue;
        };
        for prefix in ["zh-", "ZH_", ""] {
            visit(&format!("{prefix}{ch}tw.UTF-8@modifier"));
        }
    }
    for n in 0..80 {
        for token in ["a", "_", "é", "東", "😀", "tw", "hk", "hant", "hans"] {
            visit(&format!(" zh{} ", token.repeat(n)));
        }
    }
    let mut seed = 0x1234_5678_9abc_def0;
    let chars = [
        'z', 'h', 'Z', 'H', 'a', 'n', 't', 'w', 'k', '_', '-', '.', '@', 'é', '東', '😀',
        '\u{2003}', '\0',
    ];
    for _ in 0..16384 {
        let s: String = (0..next(&mut seed) % 100)
            .map(|_| chars[next(&mut seed) as usize % chars.len()])
            .collect();
        visit(&s);
    }
    for s in [
        "zh-Hans-HANT",
        "zh-hk-cn",
        "zhang",
        "zh.@tw",
        "zh@tw.cn",
        "\u{feff}zh",
        "zh_tw",
        "ZH_HK",
        "en_US.UTF-8",
        "",
        "\0",
    ] {
        visit(s);
    }
}
fn font_cases(step: usize, mut visit: impl FnMut(&str)) {
    for name in ["common", "wendy", "mega", "unprofessional"] {
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
            visit(&format!("\u{2003}{s}\t"));
        }
        for position in 0..=name.len() {
            for byte in 0..128u8 {
                visit(&format!(
                    "{}{}{}",
                    &name[..position],
                    char::from(byte),
                    &name[position..]
                ));
            }
        }
    }
    for word in (0..0x110000).step_by(step) {
        if let Some(ch) = char::from_u32(word) {
            visit(&format!("{ch}mega"));
        }
    }
    for s in [
        "",
        "mega!",
        "me-ga",
        "Ｍｅｇａ",
        "un professional",
        "\0",
        "\u{feff}",
    ] {
        visit(s);
    }
}
#[test]
fn locale_scalars_aliases_boundaries_and_capacities() {
    locale_cases(1, |s| {
        let old = original::normalize_locale(s);
        let new = normalize_locale(s);
        assert_eq!(old, new, "{s:?}");
        assert_eq!(old.capacity(), new.capacity(), "{s:?}");
    });
}
#[test]
fn font_preserves_case_whitespace_unicode_and_rejections() {
    font_cases(1, |s| {
        assert_eq!(original::font(s), MachineFont::from_str(s), "{s:?}")
    });
}
fn compare(label: &str, repetitions: usize, mut work: impl FnMut(bool)) {
    for (c, name) in [(false, "original"), (true, "current")] {
        let (_, churn) = perf::measure(|| work(c));
        println!("{label}/{name}: allocation {churn:?}");
    }
    bench::compare(label, repetitions, work);
}
#[test]
#[ignore = "paired thread-cycle and allocation measurements"]
fn benchmark() {
    let mut locales: Vec<(String, String)> = [
        ("en", "en_US.UTF-8"),
        ("zh-tw", "zh_TW.UTF-8"),
        ("zh-hant", "ZH_hAnt_CN@x"),
        ("zh-default", "zh-anything"),
        ("unicode", "zh-繁體"),
        ("empty", ""),
    ]
    .into_iter()
    .map(|(a, b)| (a.into(), b.into()))
    .collect();
    for n in [62, 63, 64, 65, 66, 4096] {
        locales.push((format!("len{n}"), format!("zh{}", "a".repeat(n - 2))));
    }
    for (label, s) in locales {
        compare(&format!("locale/{label}"), 10000, |c| {
            drop(bb(if c {
                normalize_locale(bb(&s))
            } else {
                original::normalize_locale(bb(&s))
            }))
        });
    }
    for (label, s) in [
        ("valid", "UnProfessional"),
        ("short", "MEGA"),
        ("spaced", "\u{2003} Wendy \t"),
        ("invalid", "me-ga"),
        ("empty", ""),
    ] {
        compare(&format!("font/{label}"), 20000, |c| {
            let _ = bb(if c {
                MachineFont::from_str(bb(s))
            } else {
                original::font(bb(s))
            });
        });
    }
}
fn emit(w: &mut impl Write, bytes: &[u8]) {
    w.write_all(&(bytes.len() as u64).to_le_bytes()).unwrap();
    w.write_all(bytes).unwrap();
}
#[test]
#[ignore = "deterministic locale bytes/capacities and font discriminants"]
fn dump_trace() {
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_PREFERENCE_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    locale_cases(17, |s| {
        emit(&mut out, s.as_bytes());
        let value = normalize_locale(s);
        emit(&mut out, value.as_bytes());
        out.write_all(&(value.capacity() as u64).to_le_bytes())
            .unwrap();
    });
    font_cases(17, |s| {
        emit(&mut out, s.as_bytes());
        let code = match MachineFont::from_str(s) {
            Ok(MachineFont::Wendy) => 0,
            Ok(MachineFont::Mega) => 1,
            Ok(MachineFont::Unprofessional) => 2,
            Err(()) => 3,
        };
        out.write_all(&[code]).unwrap();
    });
    out.flush().unwrap();
}
