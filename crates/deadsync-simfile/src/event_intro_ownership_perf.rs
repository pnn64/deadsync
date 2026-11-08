use super::*;
use crate::metadata_perf::{compare, measure};
use std::hint::black_box;
mod original {
    use super::*;
    include!("event_intro_original.rs");
}

#[test]
fn borrowed_intro_preserves_public_names_and_gameplay_text() {
    let groups = [
        "",
        "Regular Pack",
        "Stamina RPG 10",
        "SRPG9 Unlocks",
        "ITL Online 2026",
        "  itl ONLINE 2026 UnLoCkS - player  ",
        "ITL Community 17",
        "ITL Online",
        "\u{00c9}t\u{00e9} ITL Online 2026 Unlocks",
        "\u{2003}ITL 2025\u{2003}",
        "SRPG10 SRPG9 ITL Online 2026",
    ];
    for group in groups {
        assert_eq!(
            event_intro_name_for_pack(group),
            original::event_intro_name_for_pack(group)
        );
        let song = test_song(&format!("Songs/{group}/Song/song.ssc"), ["a", "b"]);
        assert_eq!(
            gameplay_event_intro_text(&song),
            original::gameplay_event_intro_text(&song)
        );
    }
    for path in ["", "song.ssc", "Song/song.ssc", "/"] {
        let song = test_song(path, ["a", "b"]);
        assert_eq!(
            gameplay_event_intro_text(&song),
            original::gameplay_event_intro_text(&song)
        );
    }
}

#[test]
fn event_intro_removes_the_temporary_string_allocation() {
    for group in [
        "Stamina RPG 10",
        "SRPG9",
        "ITL Online 2026 Unlocks - player",
    ] {
        let song = test_song(&format!("Songs/{group}/Song/song.ssc"), ["a", "b"]);
        let (old, a) = measure(|| original::gameplay_event_intro_text(&song));
        let (new, b) = measure(|| gameplay_event_intro_text(&song));
        assert_eq!(old, new);
        assert_eq!(a.allocs, 2);
        assert_eq!(b.allocs, 1);
        assert_eq!(a.allocated_bytes - b.allocated_bytes, old.len());
    }
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture"]
fn benchmark_metadata_intro() {
    for (label, group) in [
        ("srpg10", "Stamina RPG 10"),
        ("srpg9", "SRPG9 Unlocks"),
        ("itl", "ITL Online 2026"),
        ("unlocks", "ITL Online 2026 Unlocks - player"),
        ("unicode", "\u{00c9}t\u{00e9} ITL Online 2026 Unlocks"),
        ("fallback", "Ordinary Pack"),
    ] {
        let song = test_song(&format!("Songs/{group}/Song/song.ssc"), ["a", "b"]);
        let (_, a) = measure(|| original::gameplay_event_intro_text(&song));
        let (_, b) = measure(|| gameplay_event_intro_text(&song));
        println!("ALLOC intro/{label}: original {a:?}, current {b:?}");
        compare(
            &format!("intro/{label}"),
            32,
            || {
                black_box(original::gameplay_event_intro_text(black_box(&song)));
            },
            || {
                black_box(gameplay_event_intro_text(black_box(&song)));
            },
        );
    }
}
