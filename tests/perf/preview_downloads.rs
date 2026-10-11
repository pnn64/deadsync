use super::*;
use crate::preview_dataflows_support::compare;
use std::hint::black_box;

#[allow(dead_code)]
mod original {
    include!("preview_downloads_original.rs");
    pub(super) fn state(scroll_index: usize) -> DownloadsOverlayState {
        DownloadsOverlayState::Visible(DownloadsOverlayStateData {
            scroll_index,
            presentation: RefCell::new(None),
        })
    }
}

fn snapshots(count: usize) -> Vec<SelectMusicDownloadView> {
    (0..count)
        .map(|i| SelectMusicDownloadView {
            name: format!("Download {i}: Artist - Song title"),
            current_bytes: 500,
            total_bytes: 1000,
            complete: i % 3 == 0,
            error_message: (i % 7 == 0).then(|| "Connection interrupted".to_owned()),
        })
        .collect()
}

fn actors(
    state: &DownloadsOverlayState,
    rows: &[SelectMusicDownloadView],
    color: i32,
    font: MachineFont,
) -> Vec<Actor> {
    let mut result = Vec::new();
    assert!(push_downloads_overlay(
        &mut result,
        state,
        color,
        rows,
        font
    ));
    result
}

fn assert_original(
    state: &DownloadsOverlayState,
    rows: &[SelectMusicDownloadView],
    color: i32,
    font: MachineFont,
) -> Vec<Actor> {
    let DownloadsOverlayState::Visible(overlay) = state else {
        panic!()
    };
    let original = original::state(overlay.scroll_index);
    let mut before = Vec::new();
    original::push_downloads_overlay(&mut before, &original, color, rows, font);
    let after = actors(state, rows, color, font);
    assert_eq!(format!("{before:?}"), format!("{after:?}"));
    after
}

fn children(actors: &[Actor]) -> &Arc<[Actor]> {
    let Actor::SharedFrame { children, .. } = &actors[0] else {
        panic!("retained overlay")
    };
    children
}

#[test]
fn download_projection_preserves_scrolling_counts_and_visible_changes() {
    for count in [0, 1, 6, 7, 64] {
        let mut state = show_downloads_overlay();
        let mut rows = snapshots(count);
        for scroll in [0, 1, count, usize::MAX] {
            let DownloadsOverlayState::Visible(overlay) = &mut state else {
                panic!()
            };
            overlay.scroll_index = scroll;
            for font in [MachineFont::Wendy, MachineFont::Mega] {
                assert_original(&state, &rows, 2, font);
                if let Some(row) = rows.last_mut() {
                    row.name.push_str(" (updated)");
                    row.current_bytes += 1;
                    row.complete = !row.complete;
                    row.error_message = Some("New error".to_owned());
                }
                assert_original(&state, &rows, 3, font);
            }
        }
    }
    let mut out = Vec::new();
    assert!(!push_downloads_overlay(
        &mut out,
        &DownloadsOverlayState::Hidden,
        0,
        &[],
        MachineFont::default()
    ));
    assert!(out.is_empty());
}

#[test]
fn download_projection_reuses_offscreen_updates_but_tracks_global_status() {
    let state = show_downloads_overlay();
    let mut rows = snapshots(64);
    for row in &mut rows {
        row.complete = false;
        row.error_message = None;
    }
    let font = MachineFont::default();
    let before = assert_original(&state, &rows, 0, font);
    rows[40].name.push_str(" invisible");
    rows[40].current_bytes += 1;
    rows[40].error_message = Some("Pending error".to_owned());
    let unchanged = assert_original(&state, &rows, 0, font);
    assert!(Arc::ptr_eq(children(&before), children(&unchanged)));
    rows[40].complete = true;
    let completed = assert_original(&state, &rows, 0, font);
    assert!(!Arc::ptr_eq(children(&unchanged), children(&completed)));
    rows[40].error_message = None;
    let no_retry = assert_original(&state, &rows, 0, font);
    assert!(!Arc::ptr_eq(children(&completed), children(&no_retry)));
    rows[0].current_bytes += 1;
    let visible = assert_original(&state, &rows, 0, font);
    assert!(!Arc::ptr_eq(children(&no_retry), children(&visible)));
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        854.0, 480.0,
    ));
    let resized = assert_original(&state, &rows, 0, font);
    assert!(!Arc::ptr_eq(children(&visible), children(&resized)));
    rows.truncate(6);
    let shortened = assert_original(&state, &rows, 0, font);
    assert!(!Arc::ptr_eq(children(&resized), children(&shortened)));
    rows[0].complete = true;
    assert_original(&state, &rows, 0, font);
    let DownloadsOverlayState::Visible(overlay) = &state else {
        panic!()
    };
    assert_eq!(
        overlay
            .presentation
            .borrow()
            .as_ref()
            .unwrap()
            .snapshots
            .len(),
        6
    );
}

#[test]
#[ignore = "paired release benchmark; run alone"]
fn benchmark_preview_dataflows_downloads() {
    for count in [0, 6, 64, 1024] {
        let rows = snapshots(count);
        let font = MachineFont::default();
        compare(
            &format!("downloads/cold-{count}"),
            || {
                let state = original::show_downloads_overlay();
                let mut out = Vec::with_capacity(1);
                original::push_downloads_overlay(&mut out, &state, 0, black_box(&rows), font);
                black_box(out);
            },
            || {
                let state = show_downloads_overlay();
                let mut out = Vec::with_capacity(1);
                push_downloads_overlay(&mut out, &state, 0, black_box(&rows), font);
                black_box(out);
            },
        );
        for change in ["stable", "visible", "offscreen"] {
            if count == 0 && change != "stable" || count <= 6 && change == "offscreen" {
                continue;
            }
            let mut changed = rows.clone();
            if count > 0 {
                let index = if change == "offscreen" { count - 1 } else { 0 };
                changed[index].current_bytes += 100;
            }
            let variants = [
                &rows[..],
                if change == "stable" {
                    &rows[..]
                } else {
                    &changed[..]
                },
            ];
            let before = original::show_downloads_overlay();
            let after = show_downloads_overlay();
            let mut old_out = Vec::with_capacity(1);
            let mut new_out = Vec::with_capacity(1);
            let mut a = 0;
            let mut b = 0;
            compare(
                &format!("downloads/{change}-{count}"),
                || {
                    a ^= 1;
                    old_out.clear();
                    original::push_downloads_overlay(
                        &mut old_out,
                        &before,
                        0,
                        black_box(variants[a]),
                        font,
                    );
                    black_box(&old_out);
                },
                || {
                    b ^= 1;
                    new_out.clear();
                    push_downloads_overlay(&mut new_out, &after, 0, black_box(variants[b]), font);
                    black_box(&new_out);
                },
            );
        }
    }
}
