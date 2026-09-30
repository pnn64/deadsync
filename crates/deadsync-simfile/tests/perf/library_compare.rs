use super::*;
use crate::perf::{assert_no_churn, measure_sampled, measure_sampled_with_setup};
use std::hint::black_box;
#[path = "library_compare/baseline.rs"]
mod baseline;

#[test]
fn library_comparison_preserves_byte_order_at_every_block_boundary() {
    for len in [0, 1, 15, 16, 31, 32, 33, 47, 48, 63, 64, 129] {
        for offset in 0..=len {
            for (a, b) in [
                ('A', 'a'),
                ('Z', 'b'),
                ('\0', ' '),
                ('é', 'É'),
                ('日', '語'),
            ] {
                let left = format!("{}{}{}", "x".repeat(offset), a, "Y".repeat(len - offset));
                let right = format!("{}{}{}", "x".repeat(offset), b, "Y".repeat(len - offset));
                let shorter = &right[..right.char_indices().next_back().unwrap().0];
                for right in [&right[..], shorter, &left[..]] {
                    assert_eq!(
                        cmp_ignore_ascii_case(&left, right),
                        left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
                    );
                    assert_no_churn(|| {
                        black_box(cmp_ignore_ascii_case(&left, right));
                    });
                }
            }
        }
    }
    let alphabet = ['a', 'B', '\0', ' ', '日', 'é', 'Σ', '\u{301}'];
    let mut seed = 71_u64;
    for _ in 0..4096 {
        let mut text = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let len = (seed >> 32) as usize % 128;
            (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    alphabet[(seed >> 32) as usize % alphabet.len()]
                })
                .collect::<String>()
        };
        let left = text();
        let right = text();
        assert_eq!(
            cmp_ignore_ascii_case(&left, &right),
            baseline::cmp_ignore_ascii_case(&left, &right)
        );
    }
}

fn fixture(count: usize, mode: &str) -> Vec<Arc<SongData>> {
    let mut songs: Vec<_> = (0..count)
        .map(|i| {
            let mut song = test_song();
            song.title = match mode {
                "short" => format!("Song {:05}", i / 3),
                "diverse" => format!(
                    "{} {} distinct song in an unrelated community pack {:05}",
                    (b'A' + (i % 26) as u8) as char,
                    i % 17,
                    i / 3
                ),
                "case" => format!(
                    "{} {:05}",
                    if i % 2 == 0 {
                        "dance dance revolution community mix"
                    } else {
                        "DANCE DANCE REVOLUTION COMMUNITY MIX"
                    },
                    i / 3
                ),
                _ => format!("Dance Dance Revolution Community Mix {:05}", i / 3),
            };
            song.subtitle = if i % 7 == 0 { "Extended Mix" } else { "" }.into();
            song.simfile_path = format!("Songs/Community Pack 2026/{:05}/chart.ssc", i / 3).into();
            song.display_bpm = "150".into();
            Arc::new(song)
        })
        .collect();
    let mut seed = 0x9187_u64;
    for i in (1..count).rev() {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        songs.swap(i, (seed >> 32) as usize % (i + 1));
    }
    songs
}

#[test]
fn library_sort_keeps_transliteration_tiebreakers_groups_and_stable_identity() {
    for count in [0, 1, 32, 1024] {
        for mode in ["short", "prefix", "diverse", "case"] {
            let mut songs = fixture(count, mode);
            if count > 7 {
                for (i, title) in ["日語", "éclair", "", "\u{2003}", "Translated"]
                    .into_iter()
                    .enumerate()
                {
                    Arc::make_mut(&mut songs[i]).translit_title = title.into();
                }
            }
            for (old, new) in [
                (
                    baseline::title_grouped_songs(songs.clone()),
                    title_grouped_songs(songs.clone()),
                ),
                (
                    baseline::bpm_grouped_songs(songs.clone()),
                    bpm_grouped_songs(songs.clone()),
                ),
            ] {
                assert_eq!(old.len(), new.len());
                for (old, new) in old.iter().zip(new) {
                    assert_eq!(old.group, new.group);
                    assert_eq!(old.songs.len(), new.songs.len());
                    for (old, new) in old.songs.iter().zip(new.songs) {
                        assert!(Arc::ptr_eq(old, &new), "stable song identity changed");
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "manual release benchmark"]
fn benchmark_library_compare() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for mode in ["short", "prefix", "diverse", "case"] {
        let songs = fixture(4096, mode);
        for kind in ["cmp", "title", "bpm"] {
            for old in if reverse {
                [false, true]
            } else {
                [true, false]
            } {
                let name = format!("library_{kind}_{mode}/{}", if old { "old" } else { "new" });
                if kind == "cmp" {
                    measure_sampled(&name, 64, songs.len() - 1, || {
                        songs
                            .windows(2)
                            .map(|pair| {
                                (if old {
                                    baseline::song_title_cmp(
                                        black_box(&pair[0]),
                                        black_box(&pair[1]),
                                    )
                                } else {
                                    song_title_cmp(black_box(&pair[0]), black_box(&pair[1]))
                                }) as i32
                            })
                            .sum::<i32>()
                    });
                } else {
                    measure_sampled_with_setup(
                        &name,
                        32,
                        songs.len(),
                        || songs.clone(),
                        |songs| {
                            let songs = std::mem::take(songs);
                            black_box(match (kind, old) {
                                ("title", true) => baseline::title_grouped_songs(songs),
                                ("title", false) => title_grouped_songs(songs),
                                ("bpm", true) => baseline::bpm_grouped_songs(songs),
                                ("bpm", false) => bpm_grouped_songs(songs),
                                _ => unreachable!(),
                            });
                        },
                    );
                }
            }
        }
    }
}
