use super::state::{self, State};
use deadlib_present::actors::{Actor, TextContent};
use deadsync_online::pack_page::{PackPage, PagePhase, PageSnapshot, SongRow};
use deadsync_online::smo_details::{DetailsPhase, DetailsSnapshot, PackDetails};
use deadsync_online::stepmaniaonline::{CatalogPhase, PackInfo, Snapshot};
use std::collections::HashMap;
use std::sync::Arc;

#[path = "../../../../../tests/support/paired_bench.rs"]
pub mod paired_bench;

pub fn fixture(count: usize, banners: bool) -> State {
    let mut state = state::init();
    let catalog = (0..count)
        .map(|i| {
            PackInfo::new(
                1_000_000 + i as u64,
                format!("Pack {i}: \u{6771}\u{4eac} \u{e9}lan"),
                42,
                700 * 1024 * 1024,
                Some("NULL".into()),
                Some("itg".into()),
                Some("technical".into()),
                None,
            )
        })
        .collect::<Vec<_>>();
    let by_id = catalog
        .iter()
        .map(|pack| {
            (
                pack.id,
                PackDetails {
                    banner_url: banners.then(|| format!("https://example.test/{}.jpg", pack.id)),
                    date_added: Some("2025-03-04".into()),
                    chart_types: vec!["dance".into()],
                },
            )
        })
        .collect::<HashMap<_, _>>();
    state.snapshot = Arc::new(Snapshot {
        phase: CatalogPhase::Ready,
        catalog: Arc::from(catalog),
        revision: 1,
        message: None,
        installs: Vec::new(),
    });
    state.details = Arc::new(DetailsSnapshot {
        phase: DetailsPhase::Ready,
        by_id: Arc::new(by_id),
        has_more: false,
        ..DetailsSnapshot::default()
    });
    state.featured = (0..count.min(12)).collect();
    state.results = (0..count).collect();
    state.doubles_left = (0..count).step_by(2).collect();
    state.doubles_right = (1..count).step_by(2).collect();
    state
}

pub fn page(state: &mut State, phase: PagePhase, long_authors: bool) {
    let pack_id = state.snapshot.catalog.first().map_or(0, |pack| pack.id);
    let data = (phase == PagePhase::Ready).then(|| {
        Arc::new(PackPage {
            songs: (0..20)
                .map(|i| SongRow {
                    title: format!("Song {i}"),
                    image_url: (i % 3 != 0)
                        .then(|| format!("https://example.test/jacket-{}.png", i % 4)),
                    ..SongRow::default()
                })
                .collect(),
            meter_labels: vec![3, 7, 12],
            meter_counts: vec![5, 9, 3],
            chart_count: Some("123 charts".into()),
            authors: if long_authors {
                vec![
                    "Charter \u{6771}\u{4eac} ".repeat(80),
                    "Another charter ".repeat(80),
                    "Third".into(),
                ]
            } else {
                vec!["Charter A".into(), "Charter B".into(), "Charter C".into()]
            },
            styles: vec!["dance-single".into(), "dance-double".into()],
            banner_url: Some("https://example.test/detail.png".into()),
        })
    });
    state.page = Arc::new(PageSnapshot {
        phase,
        pack_id,
        page: data,
        message: None,
        revision: 1,
    });
}

/// Compare all actor fields. Text storage is the intentional representation
/// change, so compare its bytes after normalizing only that storage variant.
pub fn assert_actors(before: &[Actor], after: &[Actor]) {
    fn canonical(actors: &[Actor]) -> String {
        let mut actors = actors.to_vec();
        for actor in &mut actors {
            match actor {
                Actor::Text { content, .. } => {
                    *content = TextContent::Owned(content.as_str().to_owned());
                }
                Actor::Sprite { .. } => {}
                other => panic!("extend comparison for unexpected actor: {other:?}"),
            }
        }
        format!("{actors:?}")
    }
    assert_eq!(canonical(after), canonical(before));
}

/// Both renderers read the real spinner clock. Capture within one clock cell;
/// retry only when the clock crossed a boundary, never for a mismatched output.
pub fn compare_draws(mut draw: impl FnMut(bool) -> Vec<Actor>) -> (Vec<Actor>, Vec<Actor>) {
    for _ in 0..10 {
        let frame = super::spinner::frame();
        let before = draw(false);
        let after = draw(true);
        if frame == super::spinner::frame() {
            assert_actors(&before, &after);
            return (before, after);
        }
    }
    panic!("could not capture the two renderers within one spinner frame");
}

pub fn retained_text_bytes(actors: &[Actor]) -> usize {
    actors
        .iter()
        .map(|actor| match actor {
            Actor::Text {
                content: TextContent::Owned(text),
                ..
            } => text.capacity(),
            Actor::Text {
                content: TextContent::Static(_),
                ..
            }
            | Actor::Sprite { .. } => 0,
            other => panic!("unexpected actor storage: {other:?}"),
        })
        .sum()
}
