use deadsync_config::audio::{AudioOptions, NoteScrollClock, load_audio_options};
use deadsync_config::ini::SimpleIni;
use deadsync_config::options::{SystemOptions, load_system_options};
use deadsync_config::theme::{DefaultFailType, LogLevel, MachineFont};
use std::fmt::Debug;
use std::hint::black_box;
use std::str::FromStr;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/preference_codecs_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn check_spellings<T: Debug + PartialEq>(
    names: &[&str],
    current: impl Fn(&str) -> Result<T, ()>,
    reference: impl Fn(&str) -> Result<T, ()>,
) {
    let check = |raw: &str| assert_eq!(current(raw), reference(raw), "{raw:?}");
    for name in names {
        for mask in 0..1 << name.len() {
            let text: String = name
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
            check(&text);
            check(&format!("\u{2003}{text}\r\n"));
        }
        for index in 0..=name.len() {
            for byte in 0..=127 {
                check(&format!(
                    "{}{}{}",
                    &name[..index],
                    char::from(byte),
                    &name[index..]
                ));
            }
            for extra in ["\u{e9}", "\u{2003}", "\u{96ea}", "\u{130}", "\u{212a}"] {
                check(&format!("{}{extra}{}", &name[..index], &name[index..]));
            }
        }
        check(&format!("{name}{}", "a".repeat(4096)));
    }
    for raw in ["", " ", "unknown", "\0", "\u{2003}", "\u{feff}", "\u{a0}"] {
        check(raw);
    }
}

const CLOCK_NAMES: &[&str] = &[
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
];
const LOG_NAMES: &[&str] = &["error", "err", "warn", "warning", "info", "debug", "trace"];
const FAIL_NAMES: &[&str] = &["immediate", "immediatecontinue", "immediate_continue"];

#[test]
fn preference_parsers_preserve_case_whitespace_aliases_and_rejections() {
    check_spellings(CLOCK_NAMES, NoteScrollClock::from_str, original::clock);
    check_spellings(LOG_NAMES, LogLevel::from_str, original::log);
    check_spellings(FAIL_NAMES, DefaultFailType::from_str, original::fail);
}

#[test]
fn preference_loaders_preserve_defaults_and_complete_option_values() {
    for raw in CLOCK_NAMES
        .iter()
        .chain(["unknown", "", "\u{2003}", "RAW-AUDIO"].iter())
    {
        let mut ini = SimpleIni::new();
        ini.load_str(&format!("[Options]\nNoteScrollClock={raw}\n"));
        let input = ini.get("Options", "NoteScrollClock").unwrap();
        for note_scroll_clock in [NoteScrollClock::RawAudio, NoteScrollClock::FrameStable] {
            let defaults = AudioOptions {
                note_scroll_clock,
                ..AudioOptions::default()
            };
            let expected = AudioOptions {
                note_scroll_clock: original::clock(input).unwrap_or(note_scroll_clock),
                ..defaults
            };
            assert_eq!(load_audio_options(&ini, defaults), expected);
        }
    }
    for raw_log in LOG_NAMES.iter().chain(["unknown", "", "InFo"].iter()) {
        for raw_fail in FAIL_NAMES.iter().chain(["unknown", "", "IMMEDIATE"].iter()) {
            let mut ini = SimpleIni::new();
            ini.load_str(&format!(
                "[Options]\nLogLevel={raw_log}\nDefaultFailType={raw_fail}\n"
            ));
            for log_level in [LogLevel::Error, LogLevel::Trace] {
                for default_fail_type in [
                    DefaultFailType::Immediate,
                    DefaultFailType::ImmediateContinue,
                ] {
                    let defaults = SystemOptions {
                        log_level,
                        default_fail_type,
                        ..SystemOptions::default()
                    };
                    let expected = SystemOptions {
                        log_level: original::log(ini.get("Options", "LogLevel").unwrap())
                            .unwrap_or(log_level),
                        default_fail_type: original::fail(
                            ini.get("Options", "DefaultFailType").unwrap(),
                        )
                        .unwrap_or(default_fail_type),
                        ..defaults
                    };
                    assert_eq!(load_system_options(&ini, defaults), expected);
                }
            }
        }
    }
    let empty = SimpleIni::new();
    assert_eq!(
        load_audio_options(&empty, AudioOptions::default()),
        AudioOptions::default()
    );
    assert_eq!(
        load_system_options(&empty, SystemOptions::default()),
        SystemOptions::default()
    );
}

fn inputs(names: &[&str]) -> Vec<String> {
    (0..512)
        .map(|i| {
            let text = names[i / 4 % names.len()];
            match i % 4 {
                0 => text.to_owned(),
                1 => text.to_ascii_uppercase(),
                2 => format!("\u{2003}{text}\t"),
                _ => ["unknown", "", "not-a-mode", "\u{96ea}"][i / 4 % 4].to_owned(),
            }
        })
        .collect()
}

fn measure(label: &str, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, 32, work);
}

fn parser_benchmark<T>(label: &str, data: &[String], parse: impl Fn(&str) -> T) {
    measure(label, || {
        for raw in black_box(data) {
            black_box(parse(black_box(raw)));
        }
    });
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_preference_codecs() {
    let clocks = inputs(CLOCK_NAMES);
    let logs = inputs(LOG_NAMES);
    let fails = inputs(FAIL_NAMES);
    parser_benchmark("parse/clock", &clocks, NoteScrollClock::from_str);
    parser_benchmark("parse/log", &logs, LogLevel::from_str);
    parser_benchmark("parse/fail", &fails, DefaultFailType::from_str);
    let rejected: Vec<_> = (0..64)
        .map(|i| format!("{}{}", CLOCK_NAMES[i % CLOCK_NAMES.len()], "x".repeat(4096)))
        .collect();
    parser_benchmark("parse/long_clock", &rejected, NoteScrollClock::from_str);
    let fonts = inputs(&["Wendy", "Mega", "Common"]);
    parser_benchmark("parse/font_control", &fonts, MachineFont::from_str);
    let audio_inputs: Vec<_> = clocks.iter().take(32).map(|clock| {
        let mut ini = SimpleIni::new();
        ini.load_str(&format!("[Options]\nNoteScrollClock={clock}\nMasterVolume=83\nMusicVolume=92\nMenuMusic=1\nTabAcceleration=true\n"));
        ini
    }).collect();
    measure("load/audio", || {
        for ini in black_box(&audio_inputs) {
            black_box(load_audio_options(black_box(ini), AudioOptions::default()));
        }
    });
    let system_inputs: Vec<_> = logs.iter().zip(&fails).take(32).map(|(log, fail)| {
        let mut ini = SimpleIni::new();
        ini.load_str(&format!("[Options]\nLogLevel={log}\nDefaultFailType={fail}\nShowConsole=true\nLogToFile=false\nBackgroundBrightness=0.75\n"));
        ini
    }).collect();
    measure("load/system", || {
        for ini in black_box(&system_inputs) {
            black_box(load_system_options(
                black_box(ini),
                SystemOptions::default(),
            ));
        }
    });
}
