use super::state::{self, State};
use deadsync_online::pack_page::{PackPage, PagePhase, PageSnapshot, SongRow};
use deadsync_online::smo_details::{DetailsPhase, DetailsSnapshot, PackDetails};
use deadsync_online::stepmaniaonline::{CatalogPhase, PackInfo, Snapshot};
use std::collections::HashMap;
use std::sync::Arc;


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
