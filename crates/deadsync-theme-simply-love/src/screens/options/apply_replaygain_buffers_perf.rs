use super::*;
use crate::buffers_support::compare;
use crate::perf::measure;
use std::hint::black_box;

fn fixture(text: &str) -> ApplyReplayGainUiState {
    let mut state = ApplyReplayGainUiState::new();
    state.line2 = text.into();
    state.line3 = text.into();
    state.started_at = Instant::now() + Duration::from_secs(3600);
    state
}

#[test]
fn replaygain_render_preserves_details_cancellation_and_progress() {
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
            let mut state = fixture(text);
            state.done = done;
            state.total = total;
            state.displayed_done = done as f32;
            for flags in 0..8 {
                state.finished = flags & 1 != 0;
                state.cancelled = flags & 2 != 0;
                state.cancel_requested = flags & 4 != 0;
                let mut old = Vec::new();
                let mut new = Vec::new();
                buffers_original::push_apply_replaygain_overlay_actors_unreserved(
                    &mut old, &state, 3,
                );
                push_apply_replaygain_overlay_actors_unreserved(&mut new, &state, 3);
                crate::buffers_support::assert_actors_equal(old, new);
            }
        }
    }
}

#[test]
fn replaygain_render_removes_only_temporary_detail_buffers() {
    for text in ["", "Pack", "音楽\0\n", &"long ".repeat(300)] {
        let state = fixture(text);
        let mut old = Vec::with_capacity(8);
        let mut new = Vec::with_capacity(8);
        buffers_original::push_apply_replaygain_overlay_actors_unreserved(&mut old, &state, 3);
        old.clear();
        let (_, before) = measure(|| {
            buffers_original::push_apply_replaygain_overlay_actors_unreserved(&mut old, &state, 3)
        });
        let (_, after) =
            measure(|| push_apply_replaygain_overlay_actors_unreserved(&mut new, &state, 3));
        assert_eq!(
            before.allocs - after.allocs,
            if text.is_empty() || text.len() > deadlib_present::actors::InlineText::CAPACITY {
                0
            } else {
                2
            }
        );
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
fn benchmark_view_buffers_replaygain() {
    for (label, text) in [
        ("short", "Pack / Song".into()),
        ("long", "日本語 metadata ".repeat(100)),
    ] {
        let mut state = fixture(&text);
        state.done = 50;
        state.total = 100;
        state.displayed_done = 49.5;
        let mut old = Vec::with_capacity(8);
        let mut new = Vec::with_capacity(8);
        compare(
            &format!("replaygain/{label}"),
            || {
                buffers_original::push_apply_replaygain_overlay_actors_unreserved(
                    black_box(&mut old),
                    black_box(&state),
                    3,
                );
                black_box(&old);
                old.clear();
            },
            || {
                push_apply_replaygain_overlay_actors_unreserved(
                    black_box(&mut new),
                    black_box(&state),
                    3,
                );
                black_box(&new);
                new.clear();
            },
        );
    }
}
