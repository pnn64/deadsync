use super::*;
use crate::perf;
use std::hint::black_box;

#[path = "smo_extensions_original.rs"]
mod original;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

fn index(files: &[String]) -> PackIndex {
    let entries: Vec<_> = files
        .iter()
        .map(|file| pack_archive::ZipEntry {
            name: format!("Pack/Song/{file}"),
            compressed: 0,
            uncompressed: 0,
            local_header: 0,
            end: 0,
        })
        .collect();
    let audio = (0..entries.len())
        .filter(|&i| original::is_audio(&entries[i].name))
        .collect();
    PackIndex {
        pack_id: 1,
        total: 0,
        etag: None,
        tail_start: 0,
        tail: Arc::from([]),
        folders: vec![pack_archive::SongFolder {
            name: "Song".to_owned(),
            entries: (0..entries.len()).collect(),
            simfile: None,
            audio,
        }],
        entries,
    }
}

fn song(music: &str, clip: &str) -> song_preview::SongPreviewData {
    song_preview::SongPreviewData {
        title: String::new(),
        artist: String::new(),
        music: music.to_owned(),
        preview_clip: clip.to_owned(),
        sample_start: 0.0,
        sample_length: 10.0,
        bpm: 120.0,
        charts: Vec::new(),
    }
}

#[test]
fn extension_classification_matches_original_for_case_unicode_and_paths() {
    for extension in [
        "",
        "ogg",
        "mp3",
        "wav",
        "flac",
        "opus",
        "oga",
        "sm",
        "ssc",
        "png",
        "o\u{11f}g",
        "\u{212a}",
        "\u{130}",
        "\u{e9}",
        "ogg/",
        "ogg\\",
        "long.extension",
    ] {
        for prefix in [
            "",
            ".",
            "song.",
            "Pack/Song/sub/",
            "Pack.ogg/no-extension",
            "Pack\\Song\\file.",
            "._metadata.",
        ] {
            for suffix in [extension.to_owned(), extension.to_uppercase()] {
                let name = format!("{prefix}{suffix}");
                assert_eq!(is_audio(&name), original::is_audio(&name), "{name:?}");
                assert_eq!(
                    is_chart_simfile(&name),
                    original::is_chart_simfile(&name),
                    "{name:?}"
                );
                assert_eq!(
                    extension_of(&name).to_ascii_lowercase(),
                    original::extension_of(&name)
                );
            }
        }
    }
    for byte in 0..=127u8 {
        for suffix in ["ogg", "OGG", "sm", "SSC"] {
            for position in 0..=suffix.len() {
                let mut name = suffix.to_owned();
                name.insert(position, char::from(byte));
                let name = format!("Pack/Song/a.{name}");
                assert_eq!(is_audio(&name), original::is_audio(&name));
                assert_eq!(is_chart_simfile(&name), original::is_chart_simfile(&name));
            }
        }
    }
}

#[test]
fn audio_selection_preserves_precedence_fallback_and_first_match() {
    let files = [
        "a.ssc",
        "bg.png",
        "sub/Music.OGG",
        "other/music.ogg",
        "preview.MP3",
        "Caf\u{e9}.opus",
        "\u{39f}\u{3a3}.ogg",
        "._metadata.ogg",
        "no-extension",
        "folder.ogg/file",
        "clip.wav/",
    ]
    .map(str::to_owned);
    let index = index(&files);
    let names = [
        "",
        " ",
        "missing.ogg",
        "music.ogg",
        "SUB\\Music.ogg",
        "other/music.ogg",
        "preview.mp3",
        "caf\u{c9}.opus",
        "\u{3bf}\u{3c2}.ogg",
        "._metadata.ogg",
        "bg.png",
        "clip.wav/",
    ];
    for music in names {
        for clip in names {
            let song = song(music, clip);
            assert_eq!(
                audio_entry(&index, 0, &song),
                original::audio_entry(&index, 0, &song),
                "{music:?}, {clip:?}"
            );
        }
    }
    assert_eq!(
        audio_entry(&index, 0, &song("music.ogg", "preview.mp3")),
        Some((4, true))
    );
    assert_eq!(
        audio_entry(&index, 0, &song("other/music.ogg", "")),
        Some((2, false)),
        "first basename match still wins"
    );
    let empty = self::index(&[]);
    assert_eq!(
        audio_entry(&empty, 0, &song("music.ogg", "preview.mp3")),
        None
    );
}

#[test]
fn predicates_allocate_nothing_and_preview_scans_allocate_less() {
    perf::assert_no_churn(|| {
        for name in ["a.OGG", "a.png", "a", "a.SSC", "a.\u{e9}"] {
            black_box(is_audio(black_box(name)));
            black_box(is_chart_simfile(black_box(name)));
        }
    });
    let mut files: Vec<_> = (0..200).map(|id| format!("image-{id}.png")).collect();
    files.push("music.ogg".to_owned());
    let index = index(&files);
    let song = song("music.ogg", "missing.ogg");
    let (old, before) = perf::measure(|| original::audio_entry(&index, 0, &song));
    let (new, after) = perf::measure(|| audio_entry(&index, 0, &song));
    assert_eq!(old, new);
    assert_eq!(before.allocs - after.allocs, 402);
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_preview_and_chart_classification() {
    for count in [0, 4, 32, 256] {
        let mut files: Vec<_> = (0..count).map(|id| format!("image-{id}.png")).collect();
        if count > 0 {
            files.push("music.OGG".to_owned());
            files.push("preview.mp3".to_owned());
        }
        let index = index(&files);
        for (case, song) in [
            ("clip", song("music.ogg", "preview.mp3")),
            ("fallback", song("missing.ogg", "absent.ogg")),
        ] {
            let label = format!("preview/{count}/{case}");
            let (old, before) = perf::measure(|| original::audio_entry(&index, 0, &song));
            let (new, after) = perf::measure(|| audio_entry(&index, 0, &song));
            assert_eq!(old, new);
            println!("{label}: churn original {before:?}, current {after:?}");
            paired::compare(&label, 100, |current| {
                black_box(if current {
                    audio_entry(black_box(&index), 0, black_box(&song))
                } else {
                    original::audio_entry(black_box(&index), 0, black_box(&song))
                });
            });
        }
    }
    for count in [200, 9000] {
        let names: Vec<_> = (0..count)
            .map(|id| {
                format!(
                    "Pack/Song-{id}/file.{}",
                    ["SSC", "sm", "png", "OGG"][id % 4]
                )
            })
            .collect();
        let old = || {
            names
                .iter()
                .filter(|name| original::is_chart_simfile(name))
                .count()
        };
        let new = || names.iter().filter(|name| is_chart_simfile(name)).count();
        let (before_result, before) = perf::measure(old);
        let (after_result, after) = perf::measure(new);
        assert_eq!(before_result, after_result);
        let label = format!("chart-files/{count}");
        println!("{label}: churn original {before:?}, current {after:?}");
        paired::compare(&label, 100, |current| {
            black_box(if current { new() } else { old() });
        });
    }
}
