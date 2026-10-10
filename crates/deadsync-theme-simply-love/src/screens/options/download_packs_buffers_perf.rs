use super::*;
use crate::buffers_support::compare;
use crate::perf::measure;
use std::hint::black_box;

fn fixture(count: usize, text: &str) -> (Box<DownloadPacksOverlayData>, Snapshot) {
    let snapshot = Snapshot {
        phase: CatalogPhase::Ready,
        catalog: (0..count)
            .map(|i| {
                PackInfo::new(
                    i as u64,
                    format!("{text} {i}"),
                    10,
                    1000,
                    Some("ITG".into()),
                    Some("0.009".into()),
                    Some("Technical".into()),
                    Some("5.1".into()),
                )
            })
            .collect::<Vec<_>>()
            .into(),
        revision: 1,
        message: None,
        installs: Vec::new(),
    };
    let mut overlay = DownloadPacksOverlayState::Hidden;
    show_overlay(&mut overlay, &snapshot, &[]);
    let DownloadPacksOverlayState::Visible(data) = overlay else {
        unreachable!()
    };
    (data, snapshot)
}

fn render(old: bool, data: &DownloadPacksOverlayData, snapshot: &Snapshot, out: &mut Vec<Actor>) {
    let draw = if old {
        buffers_original::push_catalog
    } else {
        push_catalog
    };
    draw(
        out,
        data,
        snapshot,
        [0.2, 0.4, 0.8, 1.0],
        320.0,
        240.0,
        "wendy",
    );
}

#[test]
fn pack_catalog_preserves_every_actor_and_selection_boundary() {
    for text in ["", "Pack", "日本語 🎵\n\0", &"Metadata ".repeat(100)] {
        for count in [0, 1, 7, 20] {
            let (mut data, mut snapshot) = fixture(count, text);
            for selected in [0, 3, count.saturating_sub(1), usize::MAX] {
                data.selected = selected;
                for phase in [
                    InstallPhase::Queued,
                    InstallPhase::Downloading,
                    InstallPhase::Extracting,
                    InstallPhase::Installed,
                    InstallPhase::Error,
                ] {
                    snapshot.installs = vec![InstallSnapshot {
                        pack_id: 0,
                        phase,
                        downloaded_bytes: 256,
                        total_bytes: 1000,
                        message: Some(text.into()),
                    }];
                    let mut old = Vec::new();
                    let mut new = Vec::new();
                    render(true, &data, &snapshot, &mut old);
                    render(false, &data, &snapshot, &mut new);
                    crate::buffers_support::assert_actors_equal(old, new);
                }
            }
        }
    }
    for message in ["", "Loading", "日本語\0\n", &"Error ".repeat(200)] {
        let mut old = Vec::new();
        let mut new = Vec::new();
        buffers_original::push_status(&mut old, message, [1.0; 4], 320.0, 240.0, "wendy");
        push_status(&mut new, message, [1.0; 4], 320.0, 240.0, "wendy");
        crate::buffers_support::assert_actors_equal(old, new);
    }
}

#[test]
fn pack_catalog_drops_temporary_names_and_metadata_buffers() {
    for count in [1, 7, 100] {
        let (data, snapshot) = fixture(count, "Pack");
        let mut old = Vec::with_capacity(64);
        let mut new = Vec::with_capacity(64);
        render(true, &data, &snapshot, &mut old);
        old.clear();
        let (_, before) = measure(|| render(true, &data, &snapshot, &mut old));
        let (_, after) = measure(|| render(false, &data, &snapshot, &mut new));
        assert_eq!(before.allocs - after.allocs, count.min(VIEW_ROWS) + 1 + 11);
        assert!(before.allocated_bytes > after.allocated_bytes);
        assert_eq!(before.reallocs, after.reallocs);
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_view_buffers_pack_catalog() {
    for (count, label, text) in [
        (1, "short", "Pack".into()),
        (7, "short", "Pack".into()),
        (7, "long", "日本語 pack ".repeat(100)),
    ] {
        let (data, snapshot) = fixture(count, &text);
        let mut old = Vec::with_capacity(64);
        let mut new = Vec::with_capacity(64);
        compare(
            &format!("catalog/{count}-{label}"),
            || {
                render(
                    true,
                    black_box(&data),
                    black_box(&snapshot),
                    black_box(&mut old),
                );
                black_box(&old);
                old.clear();
            },
            || {
                render(
                    false,
                    black_box(&data),
                    black_box(&snapshot),
                    black_box(&mut new),
                );
                black_box(&new);
                new.clear();
            },
        );
    }
}
