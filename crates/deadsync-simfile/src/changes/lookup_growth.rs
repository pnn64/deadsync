use super::{normalized_background_lua_color, resolve_background_changes_from_values};
use crate::metadata_perf::measure as allocations;
use deadsync_chart::{SongBackgroundChange, SongBackgroundChangeTarget};
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "../../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "lookup_growth_original.rs"]
mod original;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "deadsync-lookup-growth-{:010}-{unique:032}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for file in ["background.png", "single.mp4"] {
            let _ = fs::remove_file(self.0.join(file));
        }
        let _ = fs::remove_dir(&self.0);
    }
}

fn current(path: &Path, text: &str, fallback: bool) -> Vec<SongBackgroundChange> {
    resolve_background_changes_from_values(
        path,
        std::iter::once(text.as_bytes()),
        &[],
        &[],
        &[],
        false,
        fallback,
    )
}

fn previous(path: &Path, text: &str, fallback: bool) -> Vec<SongBackgroundChange> {
    original::resolve_background_changes_from_values(
        path,
        std::iter::once(text.as_bytes()),
        &[],
        &[],
        &[],
        false,
        fallback,
    )
}

fn assert_changes(actual: &[SongBackgroundChange], expected: &[SongBackgroundChange]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.start_beat.to_bits(), expected.start_beat.to_bits());
        assert_eq!(actual.rate.to_bits(), expected.rate.to_bits());
        match (&actual.target, &expected.target) {
            (SongBackgroundChangeTarget::File(a), SongBackgroundChangeTarget::File(b)) => {
                assert_eq!(a, b)
            }
            (
                SongBackgroundChangeTarget::Animation(a),
                SongBackgroundChangeTarget::Animation(b),
            ) => assert_eq!(a, b),
            (SongBackgroundChangeTarget::Random, SongBackgroundChangeTarget::Random)
            | (SongBackgroundChangeTarget::NoSongBg, SongBackgroundChangeTarget::NoSongBg) => {}
            _ => panic!("background target changed"),
        }
        assert_eq!(actual.effect, expected.effect);
        assert_eq!(actual.file2, expected.file2);
        assert_eq!(actual.transition, expected.transition);
        assert_eq!(
            actual.color1.map(|v| v.map(f32::to_bits)),
            expected.color1.map(|v| v.map(f32::to_bits))
        );
        assert_eq!(
            actual.color2.map(|v| v.map(f32::to_bits)),
            expected.color2.map(|v| v.map(f32::to_bits))
        );
    }
}

fn values(count: usize, kind: &str) -> String {
    (0..count)
        .map(|i| {
            let beat = match kind {
                "few" => ((i % 3) * 4).to_string(),
                "duplicates" => ((i % 37) * 4).to_string(),
                "reverse" => ((count - i) * 4).to_string(),
                "nonfinite" => ["NaN", "-NaN", "inf", "-inf", "0", "-0"][i % 6].to_owned(),
                "nan" => "NaN".to_owned(),
                _ => (i * 4).to_string(),
            };
            format!(
                "{beat}=-random-={}={}={}={}={}=\u{2003}={}=0.5^0.25^1^0.75=#12345678",
                i as f32 / 7.0 + 1.0,
                i % 2,
                i % 2,
                i % 2,
                ["StretchNoLoop", "UpperLeft", "StretchRewind"][i % 3],
                ["CrossFade", "FadeLeft"][i % 2]
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn background_replacement_matches_original_around_thresholds_and_nonfinite_beats() {
    let fixture = Fixture::new();
    for count in [0, 1, 16, 31, 32, 33, 64, 128, 512] {
        for kind in ["unique", "reverse", "few", "duplicates", "nonfinite", "nan"] {
            let text = values(count, kind);
            for fallback in [false, true] {
                assert_changes(
                    &current(&fixture.0, &text, fallback),
                    &previous(&fixture.0, &text, fallback),
                );
            }
        }
    }
    for text in [
        "0=-random-=1,-0=-random-=2",
        "-0=-random-=1,0=-random-=2",
        "NaN=-random-=1,NaN=-random-=2",
        "inf=-random-=1,inf=-random-=2",
        "-inf=-random-=1,-inf=-random-=2",
    ] {
        assert_changes(
            &current(&fixture.0, text, false),
            &previous(&fixture.0, text, false),
        );
    }
}

#[test]
fn background_fallback_preserves_first_slots_and_last_replacement_contents() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("background.png"), b"path fixture").unwrap();
    fs::write(fixture.0.join("single.mp4"), b"path fixture").unwrap();
    for count in [0, 31, 32, 33, 128] {
        let prefix = values(count, "unique");
        for suffix in [
            "-4=background.png=1,-8=background.png=2,0=-nosongbg-",
            "-8=background.png=1,-4=background.png=2,-8=background.png=3,0=-nosongbg-",
            "0=background.png=1,-0=-random-=2,0=background.png=3,-1=-nosongbg-",
            "NaN=background.png=1,NaN=background.png=2,0=-nosongbg-",
        ] {
            let text = if prefix.is_empty() {
                suffix.to_owned()
            } else {
                format!("{prefix},{suffix}")
            };
            for fallback in [false, true] {
                assert_changes(
                    &current(&fixture.0, &text, fallback),
                    &previous(&fixture.0, &text, fallback),
                );
            }
        }
    }
}

#[test]
fn color_output_keeps_bytes_capacities_and_failure_diagnostics() {
    for component in 0..4 {
        for byte in 0..=u8::MAX {
            let mut rgba = [17, 85, 170, 255];
            rgba[component] = byte;
            let [r, g, b, a] = rgba;
            let text = format!("#{r:02x}{g:02x}{b:02x}{a:02x}");
            let before = original::normalized_background_lua_color(Some(&text));
            let after = normalized_background_lua_color(Some(&text));
            assert_eq!(after, before);
            assert_eq!(after.capacity(), before.capacity());
        }
    }
    for value in [
        None,
        Some(""),
        Some("bad"),
        Some("NaN^inf^-inf^-0"),
        Some("\u{2003}0.5^0.25^1^0.75\u{2003}"),
        Some("#1\u{e9}^00"),
    ] {
        let before = std::panic::catch_unwind(|| original::normalized_background_lua_color(value));
        let after = std::panic::catch_unwind(|| normalized_background_lua_color(value));
        match (before, after) {
            (Ok(before), Ok(after)) => {
                assert_eq!(after, before);
                assert_eq!(after.capacity(), before.capacity());
            }
            (Err(before), Err(after)) => assert_eq!(
                before.downcast_ref::<String>(),
                after.downcast_ref::<String>()
            ),
            _ => panic!("color outcome changed"),
        }
    }
}

fn measure(label: &str, repetitions: usize, mut work: impl FnMut()) {
    let (_, counts) = allocations(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, repetitions, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_lookup_growth() {
    for (label, values) in [
        (
            "rgb",
            (0..256)
                .map(|i| format!("#{i:02x}80ff"))
                .collect::<Vec<_>>(),
        ),
        (
            "rgba",
            (0..256).map(|i| format!("#1280ff{i:02x}")).collect(),
        ),
        ("caret", vec!["0.5^0.25^1^0.75".to_owned(); 256]),
        ("mixed", vec!["0.5^0.25,1^0.75".to_owned(); 256]),
        (
            "unicode",
            vec!["\u{2003}0.5^0.25^1^0.75\u{2003}".to_owned(); 256],
        ),
        ("invalid", vec!["not-a-color".to_owned(); 256]),
    ] {
        measure(&format!("color/{label}"), 32, || {
            for value in black_box(&values) {
                black_box(normalized_background_lua_color(Some(black_box(value))));
            }
        });
    }
    let fixture = Fixture::new();
    for count in [0, 16, 32, 33, 64, 256, 1024] {
        for kind in ["unique", "few", "reverse"] {
            let text = values(count, kind);
            measure(
                &format!("beats/{kind}{count}"),
                if count >= 256 { 4 } else { 16 },
                || {
                    black_box(current(black_box(&fixture.0), black_box(&text), false));
                },
            );
        }
    }
    for count in [64, 256, 1024] {
        let text = values(count, "duplicates");
        measure(&format!("beats/duplicates{count}"), 8, || {
            black_box(current(black_box(&fixture.0), black_box(&text), false));
        });
    }
    for kind in ["nonfinite", "nan"] {
        let text = values(512, kind);
        measure(&format!("beats/{kind}512"), 8, || {
            black_box(current(black_box(&fixture.0), black_box(&text), false));
        });
    }
}
