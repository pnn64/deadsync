use super::*;
use crate::golf::{GolfPars, par, replace_pack};
use crate::notes::{parse_chart_notes, step_type_lanes};
use crate::perf_alloc as alloc;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "metadata_dispatch_original.rs"]
mod original;
#[path = "metadata_golf_original.rs"]
mod original_golf;

const STEP_TYPES: [&str; 6] = [
    "pump-double",
    "pump_double",
    "dance-double",
    "dance_double",
    "pump-single",
    "pump_single",
];
const VIDEO_EXTENSIONS: [&str; 12] = [
    "mp4", "avi", "f4v", "flv", "m4v", "mov", "ogv", "webm", "mkv", "mpg", "mpeg", "wmv",
];

fn check_lanes(text: &str) {
    let expected = original::step_type_lanes(text);
    assert_eq!(step_type_lanes(text), expected, "{text:?}");
    let data = b"2000000001\n0000100000\n3000000000\n,\n1000000000\n";
    assert_eq!(
        parse_chart_notes(data, step_type_lanes(text)),
        parse_chart_notes(data, expected)
    );
}

#[test]
fn lane_dispatch_preserves_case_aliases_whitespace_and_note_layout() {
    for name in STEP_TYPES {
        for mask in 0..1usize << name.len() {
            let text: String = name
                .bytes()
                .enumerate()
                .map(|(i, byte)| {
                    char::from(if mask & (1 << i) != 0 {
                        byte.to_ascii_uppercase()
                    } else {
                        byte
                    })
                })
                .collect();
            check_lanes(&text);
            check_lanes(&format!("\u{2003}{text}\r\n"));
        }
        for position in 0..name.len() {
            for byte in 0..=127 {
                let mut bytes = name.as_bytes().to_vec();
                bytes[position] = byte;
                check_lanes(std::str::from_utf8(&bytes).unwrap());
            }
        }
        for extra in ["\u{e9}", "\u{130}", "\u{2003}", "\u{96ea}", "\0"] {
            for position in 0..=name.len() {
                check_lanes(&format!(
                    "{}{extra}{}",
                    &name[..position],
                    &name[position..]
                ));
            }
        }
    }
    for name in [
        "",
        "dance-single",
        "pump-halfdouble",
        "dance__double",
        "dance -double",
        "PUMP-DOUBLÉ",
    ] {
        check_lanes(name);
    }
    let mut state = 0x243f6a8885a308d3u64;
    for _ in 0..100_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bytes: Vec<_> = (0..state as usize % 24)
            .map(|i| (state.rotate_left(i as u32) & 127) as u8)
            .collect();
        check_lanes(std::str::from_utf8(&bytes).unwrap());
    }
}

fn check_video(path: &Path) {
    assert_eq!(
        SongData::is_video_path(path),
        original::is_video_path(path),
        "{path:?}"
    );
}

#[test]
fn video_paths_preserve_extensions_controls_unicode_and_invalid_os_strings() {
    for extension in VIDEO_EXTENSIONS {
        for mask in 0..1usize << extension.len() {
            let text: String = extension
                .bytes()
                .enumerate()
                .map(|(i, byte)| {
                    char::from(if mask & (1 << i) != 0 {
                        byte.to_ascii_uppercase()
                    } else {
                        byte
                    })
                })
                .collect();
            for stem in [
                "file",
                ".hidden",
                "folder.with.dots/file",
                "\u{96ea}/\u{1f600}",
            ] {
                check_video(Path::new(&format!("{stem}.{text}")));
            }
        }
        for position in 0..extension.len() {
            for byte in 0..=127 {
                let mut bytes = extension.as_bytes().to_vec();
                bytes[position] = byte;
                check_video(Path::new(&format!(
                    "file.{}",
                    std::str::from_utf8(&bytes).unwrap()
                )));
            }
        }
        for suffix in ["\u{e9}", "\u{130}", "\u{212a}", "\u{96ea}", "\0", " ", "."] {
            check_video(Path::new(&format!("file.{extension}{suffix}")));
            check_video(Path::new(&format!("file.{suffix}{extension}")));
        }
    }
    for path in [
        "",
        ".",
        "..",
        "mp4",
        ".mp4",
        "file.",
        "file.png",
        "file.jpeg",
        "file.tar.mp4",
        "folder.mp4/file",
        "file.mp\u{14}",
        "file.m\u{14}v",
        "file.f\u{14}v",
    ] {
        check_video(Path::new(path));
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        for surrogate in 0xd800..=0xdfff {
            for units in [
                vec![b'f' as u16, b'.' as u16, surrogate],
                vec![
                    surrogate,
                    b'.' as u16,
                    b'm' as u16,
                    b'p' as u16,
                    b'4' as u16,
                ],
                vec![b'f' as u16, b'.' as u16, surrogate, b'x' as u16],
            ] {
                check_video(Path::new(&std::ffi::OsString::from_wide(&units)));
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        for byte in 128..=255 {
            check_video(Path::new(std::ffi::OsStr::from_bytes(&[b'f', b'.', byte])));
            check_video(Path::new(std::ffi::OsStr::from_bytes(&[
                byte, b'.', b'm', b'p', b'4',
            ])));
        }
    }
}

#[test]
fn pack_registry_preserves_path_equivalence_replacement_misses_and_score_bits() {
    for count in [1, 16, 256] {
        let packs: Vec<_> = (0..count)
            .map(|i| PathBuf::from(format!("dispatch-check-{count}/Pack {i} - \u{96ea}")))
            .collect();
        for text in [
            "[Song]\nPar=-0\n[Song:Hard]\nPar=2.5\n",
            "[Song:Edit]\nPar=7.25\n",
            "",
        ] {
            for pack in &packs {
                replace_pack(pack, GolfPars::parse(text));
                original_golf::replace_pack(pack, original_golf::GolfPars::parse(text));
            }
            for pack in &packs {
                for song in ["Song", "Absent", "Song/."] {
                    for difficulty in ["Hard", "HARD", "Edit", "Challenge", "", " Hard"] {
                        let path = pack.join(song).join("chart.ssc");
                        assert_eq!(
                            par(&path, difficulty).map(f64::to_bits),
                            original_golf::par(&path, difficulty).map(f64::to_bits)
                        );
                    }
                }
            }
        }
    }
    for path in [
        "",
        "chart.ssc",
        "Song/chart.ssc",
        "unregistered/Song/chart.ssc",
    ] {
        assert_eq!(
            par(Path::new(path), "Hard").map(f64::to_bits),
            original_golf::par(Path::new(path), "Hard").map(f64::to_bits)
        );
    }
}

fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    let (_, counts) = alloc::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, iterations, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_metadata_dispatch() {
    for (label, choices) in [
        ("pumpdouble", vec!["pump-double", "PUMP_DOUBLE"]),
        ("pumpsingle", vec!["pump-single", "PUMP_SINGLE"]),
        ("dancedouble", vec!["dance-double", "DANCE_DOUBLE"]),
        ("default", vec!["dance-single", "DANCE_SINGLE"]),
        ("mixed", STEP_TYPES.to_vec()),
        (
            "unknown",
            vec!["", "unknown", "pump-halfdouble", "dance__double"],
        ),
        (
            "trimmed",
            vec!["\u{2003}pump-single\t", "  DANCE_DOUBLE\r\n"],
        ),
        (
            "unicode",
            vec!["\u{96ea}", "pump-\u{130}ingle", "dance-doubl\u{e9}"],
        ),
    ] {
        let inputs: Vec<_> = (0..256)
            .map(|i| choices[i % choices.len()].to_owned())
            .collect();
        measure(&format!("lanes/{label}"), 128, || {
            for input in black_box(&inputs) {
                black_box(step_type_lanes(black_box(input)));
            }
        });
    }
    for (label, extensions, prefix) in [
        (
            "video",
            VIDEO_EXTENSIONS.to_vec(),
            "Backgrounds/Pack".to_owned(),
        ),
        (
            "image",
            vec!["png", "jpg", "jpeg", "gif", "bmp", "svg"],
            "Backgrounds/Pack".to_owned(),
        ),
        (
            "mixed",
            vec!["mp4", "png", "webm", "jpeg", "MPG", ""],
            "Backgrounds/Pack".to_owned(),
        ),
        (
            "unicode",
            vec!["mp4", "WEBM", "png", "jpeg"],
            "\u{96ea}/\u{1f600}/".repeat(12),
        ),
        (
            "rejected",
            vec![
                "mp\u{14}",
                "f\u{14}v",
                "\u{96ea}",
                "\u{130}ab",
                "unknownlongextension",
            ],
            "Backgrounds/Pack".to_owned(),
        ),
    ] {
        let paths: Vec<_> = (0..256)
            .map(|i| {
                PathBuf::from(format!(
                    "{prefix}/file-{i:04}.{}",
                    extensions[i % extensions.len()]
                ))
            })
            .collect();
        measure(&format!("video/{label}"), 64, || {
            for path in black_box(&paths) {
                black_box(SongData::is_video_path(black_box(path)));
            }
        });
    }
    for (label, count, padding) in [
        ("one", 1, 0),
        ("small", 64, 0),
        ("large", 1024, 0),
        ("long", 128, 128),
    ] {
        let prefix = format!("dispatch-bench-{label}/{}", "folder-".repeat(padding / 7));
        let packs: Vec<_> = (0..count)
            .map(|i| PathBuf::from(format!("{prefix}Pack {i:04}")))
            .collect();
        for pack in &packs {
            replace_pack(
                pack,
                GolfPars::parse("[Song]\nPar=-0\n[Song:Hard]\nPar=2.5\n"),
            );
        }
        for (kind, missing) in [("hit", false), ("miss", true)] {
            let paths: Vec<_> = (0..256)
                .map(|i| {
                    let pack = &packs[i % count];
                    if missing {
                        pack.with_file_name(format!("Absent {i}"))
                            .join("Song/chart.ssc")
                    } else {
                        pack.join("Song/chart.ssc")
                    }
                })
                .collect();
            measure(&format!("golf/{label}{kind}"), 32, || {
                for path in black_box(&paths) {
                    black_box(par(black_box(path), black_box("Hard")));
                }
            });
        }
    }
}
