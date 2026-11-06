use super::*;
use crate::audio::*;
use crate::bools::{parse_bool_str, parse_u8_bool_or_default, parse_u8_bool_str};
use crate::dirs::resource_perf::{support as perf, text_codecs::bench};
use crate::ini::SimpleIni;
use std::hint::black_box as bb;
use std::str::FromStr;
#[path = "audio_kernel_original.rs"]
mod original;
fn check(raw: &str) {
    assert_eq!(
        original::clock(raw),
        NoteScrollClock::from_str(raw),
        "clock {raw:?}"
    );
    assert_eq!(
        original::anchor(raw),
        canonical_frame_stats_overlay_anchor(raw),
        "anchor {raw:?}"
    );
    assert_eq!(
        original::numeric(raw),
        parse_u8_bool_str(raw),
        "numeric {raw:?}"
    );
    for default in [false, true] {
        assert_eq!(
            original::numeric_or_default(Some(raw), default),
            parse_u8_bool_or_default(Some(raw), default),
            "default {raw:?}"
        );
    }
}
fn fixture(raw: Option<&str>, populated: bool) -> SimpleIni {
    let mut text = String::from("[Options]\n");
    if populated {
        text.push_str("VisualDelaySeconds=-0.0\nMasterVolume=95\nMusicVolume=75\nMusicWheelSwitchSpeed=23\nSFXVolume=39\nAssistTickVolume=17\nAudioOutputDevice=2\nAudioSampleRateHz=48000\nWriteCurrentScreen=1\nTabAcceleration=off\n");
    }
    if let Some(raw) = raw {
        for key in [
            "NoteScrollClock",
            "MenuMusic",
            "CustomSoundsEnabled",
            "RateModPreservesPitch",
            "ReplayGain",
        ] {
            text.push_str(key);
            text.push('=');
            text.push_str(raw);
            text.push('\n');
        }
    }
    let mut ini = SimpleIni::new();
    ini.load_str(&text);
    ini
}
fn defaults(flag: bool) -> AudioOptions {
    AudioOptions {
        visual_delay_seconds: -0.0,
        note_scroll_clock: if flag {
            NoteScrollClock::FrameStable
        } else {
            NoteScrollClock::RawAudio
        },
        menu_music: flag,
        custom_sounds_enabled: flag,
        rate_mod_preserves_pitch: flag,
        enable_replaygain: flag,
        ..AudioOptions::default()
    }
}
fn check_loaded(raw: Option<&str>, populated: bool) {
    let ini = fixture(raw, populated);
    for flag in [false, true] {
        let old = original::audio(&ini, defaults(flag));
        let new = load_audio_options(&ini, defaults(flag));
        assert_eq!(old, new, "{raw:?}/{populated}/{flag}");
        assert_eq!(
            old.visual_delay_seconds.to_bits(),
            new.visual_delay_seconds.to_bits()
        );
    }
}
#[test]
fn audio_and_machine_word_kernels_preserve_every_alias_and_full_audio_output() {
    each_case(1, check);
    numeric_edges();
    for &word in WORDS {
        for at in 0..word.len() {
            for byte in 0..128u8 {
                let mut raw = word.to_owned();
                raw.replace_range(at..at + 1, &char::from(byte).to_string());
                check(&raw);
            }
        }
        check_loaded(Some(word), false);
        check_loaded(Some(&word.to_ascii_uppercase()), true);
    }
    each_case(257, |raw| check_loaded(Some(raw), false));
    for populated in [false, true] {
        check_loaded(None, populated);
        for raw in [
            "",
            "unknown",
            "+0001",
            "-0",
            "256",
            "RAWaudio",
            "FrameStable",
            "ITG-de-stepped",
            "\u{96ea}",
            "\u{2003}1\t",
        ] {
            check_loaded(Some(raw), populated);
        }
    }
    for flag in [false, true] {
        assert_eq!(
            original::numeric_or_default(None, flag),
            parse_u8_bool_or_default(None, flag)
        );
    }
}
fn compare(label: &str, n: usize, mut work: impl FnMut(bool)) {
    for (current, name) in [(false, "original"), (true, "current")] {
        let (_, v) = perf::measure(|| work(current));
        println!("{label}/{name}: allocation {v:?}");
    }
    bench::compare(label, n, work);
}
#[test]
#[ignore]
fn benchmark() {
    for (label, raw) in [
        ("clock/raw", "RawAudio"),
        ("clock/stable", "FrameStable"),
        ("clock/separated", "ITG-de-stepped"),
        ("clock/smooth", "Smoothed"),
        ("clock/one", "1"),
        ("clock/invalid", "nonsense"),
        ("clock/unicode", "\u{2003}FrameStable\t"),
        ("clock/empty", ""),
    ] {
        compare(label, 50000, |new| {
            bb(if new {
                NoteScrollClock::from_str(bb(raw))
            } else {
                original::clock(bb(raw))
            });
        });
    }
    for (label, raw) in [
        ("anchor/first", "top-left"),
        ("anchor/last", "bottom-center"),
        ("anchor/middle", "bottom-right"),
        ("anchor/invalid", "top-right!"),
        ("anchor/unicode", "\u{2003}TOP-CENTER\t"),
        ("anchor/empty", ""),
    ] {
        compare(label, 50000, |new| {
            bb(if new {
                canonical_frame_stats_overlay_anchor(bb(raw))
            } else {
                original::anchor(bb(raw))
            });
        });
    }
    for (label, raw) in [
        ("numeric/zero", "0"),
        ("numeric/one", "1"),
        ("numeric/other", "8"),
        ("numeric/three", "255"),
        ("numeric/overflow", "256"),
        ("numeric/plus", "+1"),
        ("numeric/zeros", "00000001"),
        ("numeric/unicode", "\u{2003}1\t"),
        ("numeric/invalid", "true"),
        ("numeric/empty", ""),
    ] {
        compare(label, 50000, |new| {
            bb(if new {
                parse_u8_bool_str(bb(raw))
            } else {
                original::numeric(bb(raw))
            });
        });
    }

    for (label, raw, populated) in [
        ("raw", Some("RawAudio"), false),
        ("stable", Some("FrameStable"), false),
        ("populated", Some("1"), true),
        ("zeros", Some("00000001"), true),
        ("invalid", Some("unknown"), false),
        ("missing", None, false),
    ] {
        let ini = fixture(raw, populated);
        let default = defaults(false);
        compare(&format!("caller/{label}"), 10000, |new| {
            bb(if new {
                load_audio_options(bb(&ini), bb(default))
            } else {
                original::audio(bb(&ini), bb(default))
            });
        });
    }
    for (label, raw) in [("present", Some("1")), ("missing", None)] {
        compare(&format!("default/{label}"), 50000, |new| {
            bb(if new {
                parse_u8_bool_or_default(bb(raw), bb(false))
            } else {
                original::numeric_or_default(bb(raw), bb(false))
            });
        });
    }
}
#[test]
#[ignore = "deterministic audio and anchor output trace"]
fn dump_trace() {
    use std::io::Write;
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(
            std::env::var_os("DEADSYNC_AUDIO_KERNEL_TRACE_OUT").expect("trace path"),
        )
        .unwrap(),
    );
    each_case(17, |raw| {
        out.write_all(&(raw.len() as u64).to_le_bytes()).unwrap();
        out.write_all(raw.as_bytes()).unwrap();
        let clock = match NoteScrollClock::from_str(raw) {
            Err(()) => 0,
            Ok(NoteScrollClock::RawAudio) => 1,
            Ok(NoteScrollClock::FrameStable) => 2,
        };
        let numeric = match parse_u8_bool_str(raw) {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        };
        let anchor = canonical_frame_stats_overlay_anchor(raw);
        out.write_all(&[clock, numeric, anchor.len() as u8])
            .unwrap();
        out.write_all(anchor.as_bytes()).unwrap();
    });
    for populated in [false, true] {
        for raw in [
            None,
            Some(""),
            Some("unknown"),
            Some("RawAudio"),
            Some("FrameStable"),
            Some("1"),
            Some("00000001"),
            Some("255"),
            Some("256"),
            Some("\u{2003}0\t"),
        ] {
            for flag in [false, true] {
                let new = load_audio_options(&fixture(raw, populated), defaults(flag));
                let record = format!(
                    "{raw:?}/{populated}/{flag}/{new:?}/{:08x}",
                    new.visual_delay_seconds.to_bits()
                );
                out.write_all(&(record.len() as u64).to_le_bytes()).unwrap();
                out.write_all(record.as_bytes()).unwrap();
            }
        }
    }
    out.flush().unwrap();
}

const WORDS: &[&str] = &[
    "rawaudio",
    "raw",
    "off",
    "0",
    "false",
    "framestable",
    "frame-stable",
    "frame_stable",
    "frame stable",
    "smooth",
    "smoothed",
    "itgdestep",
    "itg-destep",
    "itgdestepped",
    "itg-de-stepped",
    "itg_de_stepped",
    "itg",
    "on",
    "1",
    "true",
    "top-left",
    "top-right",
    "bottom-left",
    "bottom-right",
    "top-center",
    "bottom-center",
];

fn each_case(step: usize, mut work: impl FnMut(&str)) {
    for n in (0..=0x10ffff).step_by(step) {
        if let Some(ch) = char::from_u32(n) {
            let raw = ch.to_string();
            work(&raw);
            work(&format!(" {ch}\t"));
            work(&format!("raw{ch}audio"));
        }
    }
    for &word in WORDS {
        for mask in 0..1usize << word.len() {
            let raw: String = word
                .bytes()
                .enumerate()
                .map(|(i, b)| {
                    if mask & (1 << i) != 0 {
                        b.to_ascii_uppercase() as char
                    } else {
                        b as char
                    }
                })
                .collect();
            work(&raw);
            work(&format!("\u{2003}{raw}\u{3000}"));
        }
        for byte in 0..128u8 {
            for i in 0..=word.len() {
                let mut raw = word.to_owned();
                raw.insert(i, byte as char);
                work(&raw);
            }
        }
    }
    for n in 0..=1024 {
        for raw in [
            n.to_string(),
            format!("+{n}"),
            format!("{n:07}"),
            format!("\u{2003}{n}\u{3000}"),
            format!("-{n}"),
        ] {
            work(&raw);
        }
    }
    for a in 0..128u8 {
        for b in 0..128u8 {
            let bytes = [a, b];
            work(std::str::from_utf8(&bytes).unwrap());
        }
    }
    let mut seed = 0xfade3419u64;
    for _ in 0..32768 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let mut raw = String::new();
        for i in 0..(seed as usize & 63) {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let n = (seed >> 32) as u32;
            raw.push(if i % 7 == 0 {
                char::from_u32(n % 0x110000).unwrap_or('\u{fffd}')
            } else {
                (n as u8 & 127) as char
            });
        }
        work(&raw);
    }
}
fn numeric_edges() {
    for a in 0..128u8 {
        for b in 0..128u8 {
            for c in 0..128u8 {
                let bytes = [a, b, c];
                let raw = std::str::from_utf8(&bytes).unwrap();
                assert_eq!(original::numeric(raw), parse_u8_bool_str(raw), "{raw:?}");
            }
        }
    }
    for n in 0..=1024 {
        for prefix in ["", "+"] {
            for zeros in [0, 1, 3, 32, 1024] {
                let raw = format!("{prefix}{}{n}", "0".repeat(zeros));
                assert_eq!(original::numeric(&raw), parse_u8_bool_str(&raw), "{raw:?}");
            }
        }
    }
}
