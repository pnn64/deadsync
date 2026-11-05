use super::*;
use crate::bools::parse_bool_str;
use crate::dirs::resource_perf::{support as perf, text_codecs::bench};
use crate::numbers::parse_auto_threads_u8;
use std::hint::black_box as bb;
#[path = "field_original.rs"]
mod original;
fn current_step(value: &str) -> Option<bool> {
    match value.trim() {
        v if v.eq_ignore_ascii_case("show") => Some(true),
        v if v.eq_ignore_ascii_case("hide") => Some(false),
        _ => original::parse_loose_bool_str(value),
    }
}
fn number_cases(step: usize, mut visit: impl FnMut(&str)) {
    for scalar in (0..0x110000).step_by(step) {
        if let Some(c) = char::from_u32(scalar) {
            visit(&c.to_string());
            visit(&format!("1{c}2"));
            visit(&format!("{c}42{c}"));
        }
    }
    for n in 0..=1024 {
        let s = n.to_string();
        for x in [
            &s,
            &format!("+{s}"),
            &format!("-{s}"),
            &format!("000{s}"),
            &format!("\u{2003}{s}\t"),
        ] {
            visit(x);
        }
    }
    for name in [
        "auto", "AUTO", "AuTo", "", " ", "+", "-", "1_0", "1.0", "0x10", "++1", "-0", "+0001",
    ] {
        visit(name);
    }
    for a in 0..128u8 {
        for b in 0..128u8 {
            visit(&format!("{}{}", char::from(a), char::from(b)));
        }
    }
    let mut seed = 12345678u32;
    let chars = [
        '0', '1', '2', '5', '9', '+', '-', 'a', ' ', '\0', '\u{96ea}', '\u{2003}',
    ];
    for _ in 0..32768 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let s: String = (0..seed % 40)
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
fn check_raw(raw: &str) {
    assert_eq!(
        original::parse_bool_str(raw),
        parse_bool_str(raw),
        "bool {raw:?}"
    );
    assert_eq!(
        original::parse_auto_threads_u8(raw),
        parse_auto_threads_u8(raw),
        "threads {raw:?}"
    );
    assert_eq!(original::step(raw), current_step(raw), "step {raw:?}");
}
fn fixture(raw: Option<&str>, populated: bool) -> SimpleIni {
    let mut text = String::from("[Theme]\n");
    if populated {
        text.push_str("SimplyLoveColor=4\nShowSelectMusicGameplayTimer=0\nKeyboardFeatures=1\nVisualStyle=hearts\nSrpgVariant=srpg9\nVideoBackgrounds=1\nRandomBackgroundMode=off\nZmodRatingBoxText=0\nShowBpmDecimal=1\nGameplayBpmPosition=topcenter\nGameplayBannerMode=loop\nEnableTournamentMode=1\nScoringSystem=EX\nEnforceNoCmod=0\n");
    }
    if let Some(raw) = raw {
        text.push_str("StepStats=");
        text.push_str(raw);
        text.push('\n');
    }
    let mut ini = SimpleIni::new();
    ini.load_str(&text);
    ini
}
fn check_loaded(raw: Option<&str>, populated: bool) {
    let ini = fixture(raw, populated);
    for show in [false, true] {
        let mut defaults = ThemePresentationOptions::default();
        defaults.simply_love_color = -37;
        defaults.keyboard_features = false;
        defaults.tournament.enabled = true;
        defaults.tournament.show_step_stats = show;
        assert_eq!(
            original::load_theme_presentation_options(&ini, defaults),
            load_theme_presentation_options(&ini, defaults),
            "{raw:?}/{populated}/{show}"
        );
    }
}
#[test]
fn field_parsers_and_complete_theme_outputs_preserve_aliases_defaults() {
    number_cases(1, check_raw);
    for a in 0..128u8 {
        for b in 0..128u8 {
            for c in 0..128u8 {
                let bytes = [a, b, c];
                let raw = std::str::from_utf8(&bytes).unwrap();
                assert_eq!(
                    original::parse_auto_threads_u8(raw),
                    parse_auto_threads_u8(raw),
                    "{bytes:?}"
                );
            }
        }
    }
    for word in [
        "true", "false", "yes", "no", "on", "off", "show", "hide", "auto", "0", "1", "255",
    ] {
        for mask in 0..1usize << word.len() {
            let raw: String = word
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
            check_raw(&raw);
            check_loaded(Some(&raw), false);
            check_loaded(Some(&format!("\u{2003}{raw}\t")), true);
        }
        for at in 0..=word.len() {
            for b in 0..128u8 {
                let raw = format!("{}{}{}", &word[..at], char::from(b), &word[at..]);
                check_raw(&raw);
            }
        }
    }
    number_cases(257, |raw| check_loaded(Some(raw), false));
    for populated in [false, true] {
        check_loaded(None, populated);
        for raw in [
            "", "unknown", "+0001", "-0", "256", "SHOW", "hide", "off", "\u{96ea}",
        ] {
            check_loaded(Some(raw), populated);
        }
    }
}
fn compare(label: &str, n: usize, mut work: impl FnMut(bool)) {
    for (c, name) in [(false, "original"), (true, "current")] {
        let (_, v) = perf::measure(|| work(c));
        println!("{label}/{name}: allocation {v:?}");
    }
    bench::compare(label, n, work);
}
#[test]
#[ignore]
fn benchmark() {
    for (label, raw) in [
        ("one", "1"),
        ("zero", "0"),
        ("true", "TRUE"),
        ("false", "false"),
        ("yes", "Yes"),
        ("no", "NO"),
        ("on", "On"),
        ("off", "OFF"),
        ("space", "\u{2003}FALSE\t"),
        ("invalid", "unknown"),
        ("unicode", "\u{96ea}"),
        ("empty", ""),
    ] {
        compare(&format!("bool/{label}"), 50000, |c| {
            bb(if c {
                parse_bool_str(bb(raw))
            } else {
                original::parse_bool_str(bb(raw))
            });
        });
    }
    for (label, raw) in [
        ("one", "4"),
        ("three", "255"),
        ("overflow", "256"),
        ("plus", "+8"),
        ("zeros", "0004"),
        ("auto", "AuTo"),
        ("empty", ""),
        ("unicode", "\u{96ea}"),
    ] {
        compare(&format!("number/{label}"), 50000, |c| {
            bb(if c {
                parse_auto_threads_u8(bb(raw))
            } else {
                original::parse_auto_threads_u8(bb(raw))
            });
        });
    }
    for (label, raw) in [
        ("show", "SHOW"),
        ("hide", "hide"),
        ("bool", "true"),
        ("number", "255"),
        ("invalid", "unknown"),
        ("empty", ""),
    ] {
        compare(&format!("step/{label}"), 50000, |c| {
            bb(if c {
                current_step(bb(raw))
            } else {
                original::step(bb(raw))
            });
        });
    }
    for (label, raw, populated) in [
        ("show", Some("SHOW"), false),
        ("hide", Some("hide"), false),
        ("populated", Some("SHOW"), true),
        ("invalid", Some("unknown"), false),
        ("empty", Some(""), false),
        ("missing", None, false),
    ] {
        let ini = fixture(raw, populated);
        let defaults = ThemePresentationOptions::default();
        compare(&format!("caller/{label}"), 10000, |c| {
            bb(if c {
                load_theme_presentation_options(bb(&ini), bb(defaults))
            } else {
                original::load_theme_presentation_options(bb(&ini), bb(defaults))
            });
        });
    }
}
#[test]
#[ignore = "deterministic field parser and complete theme output trace"]
fn dump_trace() {
    use std::io::Write;
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_CONFIG_FIELDS_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    number_cases(17, |raw| {
        out.write_all(&(raw.len() as u64).to_le_bytes()).unwrap();
        out.write_all(raw.as_bytes()).unwrap();
        let b = match parse_bool_str(raw) {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        };
        let s = match current_step(raw) {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        };
        out.write_all(&[b, s]).unwrap();
        match parse_auto_threads_u8(raw) {
            None => out.write_all(&[0]).unwrap(),
            Some(value) => out.write_all(&[1, value]).unwrap(),
        }
    });
    for populated in [false, true] {
        for raw in [
            None,
            Some("SHOW"),
            Some("hide"),
            Some("true"),
            Some("255"),
            Some("0"),
            Some(""),
            Some("unknown"),
            Some("\u{2003}OFF\t"),
            Some("\u{96ea}"),
        ] {
            let ini = fixture(raw, populated);
            for show in [false, true] {
                let mut defaults = ThemePresentationOptions::default();
                defaults.tournament.show_step_stats = show;
                let record = format!(
                    "{raw:?}/{populated}/{show}/{:?}",
                    load_theme_presentation_options(&ini, defaults)
                );
                out.write_all(&(record.len() as u64).to_le_bytes()).unwrap();
                out.write_all(record.as_bytes()).unwrap();
            }
        }
    }
    out.flush().unwrap();
}
