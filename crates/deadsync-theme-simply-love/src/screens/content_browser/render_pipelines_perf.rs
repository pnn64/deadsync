use super::*;
use crate::perf::measure;
use crate::pipelines_support::compare;
use deadsync_online::smo_songs::{SongInstall, SongInstallPhase, SongInstallsSnapshot};
use std::hint::black_box;
use std::sync::Arc;

fn fixture(packs: usize, songs: usize) -> State {
    let mut state = super::super::state::init();
    state.installed_dirs = (0..packs)
        .map(|i| std::path::PathBuf::from(format!("Songs/Pack {i}")))
        .collect();
    // The singles group must not be counted as a pack.
    state.installed_dirs.push(std::path::PathBuf::from(
        deadsync_online::smo_songs::SINGLES_GROUP,
    ));
    state.song_installs = Arc::new(SongInstallsSnapshot {
        revision: 1,
        installs: (0..songs)
            .map(|i| SongInstall {
                pack_id: 7,
                title: format!("Song Å猫 {i}"),
                artist: String::new(),
                group: deadsync_online::smo_songs::SINGLES_GROUP.into(),
                phase: SongInstallPhase::Installed,
                downloaded_bytes: 100,
                total_bytes: None,
                message: None,
            })
            .collect(),
    });
    assert_eq!(super::super::preview::songs_added(&state), songs);
    state
}

#[test]
fn reload_dialog_preserves_counts_pluralization_and_all_actor_properties() {
    for packs in [0, 1, 2, 12] {
        for songs in [0, 1, 2, 12] {
            let state = fixture(packs, songs);
            for choice in 0..=2 {
                let prompt = ReloadPrompt {
                    choice,
                    ..Default::default()
                };
                for (w, h) in [(854., 480.), (1920., 1080.)] {
                    let mut old = Vec::new();
                    let mut new = Vec::new();
                    pipelines_original::push_reload_dialog(&mut old, &state, &prompt, w, h);
                    push_reload_dialog(&mut new, &state, &prompt, w, h);
                    assert_eq!(format!("{old:?}"), format!("{new:?}"));
                }
            }
        }
    }
}

#[test]
fn reload_dialog_removes_temporary_summary_vectors_and_strings() {
    for (packs, songs) in [(0, 0), (1, 0), (0, 1), (1, 1), (12, 12)] {
        let state = fixture(packs, songs);
        let prompt = ReloadPrompt::default();
        let mut old = Vec::with_capacity(32);
        let mut new = Vec::with_capacity(32);
        let (_, before) = measure(|| {
            pipelines_original::push_reload_dialog(&mut old, &state, &prompt, 854., 480.)
        });
        let (_, after) = measure(|| push_reload_dialog(&mut new, &state, &prompt, 854., 480.));
        assert_eq!(format!("{old:?}"), format!("{new:?}"));
        assert!(after.allocs < before.allocs);
        assert!(after.allocated_bytes < before.allocated_bytes);
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_string_pipelines_reload() {
    for (packs, songs) in [(0, 0), (1, 0), (0, 1), (1, 1), (12, 12)] {
        let state = fixture(packs, songs);
        let prompt = ReloadPrompt::default();
        let mut old = Vec::with_capacity(32);
        let mut new = Vec::with_capacity(32);
        compare(
            &format!("reload/{packs}-{songs}"),
            || {
                old.clear();
                pipelines_original::push_reload_dialog(
                    &mut old,
                    black_box(&state),
                    black_box(&prompt),
                    854.,
                    480.,
                );
                black_box(&old);
            },
            || {
                new.clear();
                push_reload_dialog(&mut new, black_box(&state), black_box(&prompt), 854., 480.);
                black_box(&new);
            },
        );
    }
}
