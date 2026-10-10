use super::*;
use crate::buffers_support::compare;
use crate::perf::measure;
use std::hint::black_box;

fn fixture(text: &str, phase: crate::views::SimplyLoveContentReloadPhase) -> ReloadUiState {
    let mut state = ReloadUiState::new();
    state.phase = phase;
    state.line2 = text.into();
    state.line3 = text.into();
    // Keep the elapsed-time speed label deterministic in both variants.
    state.phase_started_at = Instant::now() + Duration::from_secs(3600);
    state
}

#[test]
fn reload_render_preserves_text_geometry_and_progress_boundaries() {
    use crate::views::SimplyLoveContentReloadPhase as Phase;
    for phase in [
        Phase::Songs,
        Phase::Courses,
        Phase::Artwork,
        Phase::Noteskins,
        Phase::ReplayGain,
    ] {
        for text in [
            "",
            "Pack / Song",
            "日本語 🎵\n\0 name",
            &"Long metadata ".repeat(80),
        ] {
            for (done, total) in [
                (0, 0),
                (0, 10),
                (5, 10),
                (10, 10),
                (11, 10),
                (usize::MAX, usize::MAX),
            ] {
                let mut state = fixture(text, phase);
                state.songs_done = done;
                state.songs_total = total;
                state.replaygain_done = done;
                state.replaygain_total = total;
                for finished in [false, true] {
                    state.done = finished;
                    let mut old = Vec::new();
                    let mut new = Vec::new();
                    buffers_original::push_reload_overlay_actors_unreserved(&mut old, &state, 3);
                    push_reload_overlay_actors_unreserved(&mut new, &state, 3);
                    crate::buffers_support::assert_actors_equal(old, new);
                }
            }
        }
    }
}

#[test]
fn reload_render_removes_only_temporary_detail_buffers() {
    for text in ["", "Pack", "音楽\0\n", &"long ".repeat(300)] {
        let state = fixture(text, crate::views::SimplyLoveContentReloadPhase::Songs);
        let mut old = Vec::with_capacity(8);
        let mut new = Vec::with_capacity(8);
        buffers_original::push_reload_overlay_actors_unreserved(&mut old, &state, 3);
        old.clear();
        let (_, before) = measure(|| {
            buffers_original::push_reload_overlay_actors_unreserved(&mut old, &state, 3)
        });
        let (_, after) = measure(|| push_reload_overlay_actors_unreserved(&mut new, &state, 3));
        let removed =
            if text.is_empty() || text.len() > deadlib_present::actors::InlineText::CAPACITY {
                0
            } else {
                2
            };
        assert_eq!(before.allocs - after.allocs, removed);
        assert_eq!(
            before.allocated_bytes - after.allocated_bytes,
            if text.len() <= deadlib_present::actors::InlineText::CAPACITY {
                2 * text.len()
            } else {
                0
            }
        );
        assert_eq!(before.reallocs, after.reallocs);
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_view_buffers_reload() {
    for (label, text) in [
        ("short", "Pack / Song".into()),
        ("long", "日本語 metadata ".repeat(100)),
    ] {
        let mut state = fixture(&text, crate::views::SimplyLoveContentReloadPhase::Songs);
        state.songs_done = 50;
        state.songs_total = 100;
        let mut old = Vec::with_capacity(8);
        let mut new = Vec::with_capacity(8);
        compare(
            &format!("reload/{label}"),
            || {
                buffers_original::push_reload_overlay_actors_unreserved(
                    black_box(&mut old),
                    black_box(&state),
                    3,
                );
                black_box(&old);
                old.clear();
            },
            || {
                push_reload_overlay_actors_unreserved(black_box(&mut new), black_box(&state), 3);
                black_box(&new);
                new.clear();
            },
        );
    }
}
