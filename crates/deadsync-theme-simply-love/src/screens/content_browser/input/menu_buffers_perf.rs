use super::*;

use crate::screens::content_browser::state;
use deadsync_online::stepmaniaonline::{CatalogPhase, PackInfo, Snapshot};
use std::sync::Arc;


fn browser(query: &str, catalog_size: usize) -> State {
    let mut state = state::init();
    state.snapshot = Arc::new(Snapshot {
        phase: CatalogPhase::Ready,
        catalog: (0..catalog_size)
            .map(|n| PackInfo::new(n as u64, format!("Pack {n}"), 10, 1, None, None, None, None))
            .collect::<Vec<_>>()
            .into(),
        revision: 1,
        message: None,
        installs: Vec::new(),
    });
    focus_search(&mut state);
    state.query = String::with_capacity(512);
    state.query.push_str(query);
    state.query_idle = 7.0;
    state.caret_elapsed = 0.75;
    state.search_why.insert(1, "old reason".into());
    state.cursor = 2;
    state
}

#[test]
fn menu_buffers_browser_full_query_and_rejected_text_do_not_allocate() {
    let mut state = browser(&"🎵".repeat(64), 8);
    let mut effects = Vec::new();
    let paste = "a🎵\n".repeat(4096);
    crate::perf::assert_no_churn(|| {
        assert!(text_typed(&mut state, &paste, None, false, &mut effects));
    });
    assert_eq!(state.query.chars().count(), 64);
    assert_eq!(state.query_idle, 7.0);
    assert!(effects.is_empty());
    state.zone = Zone::Detail;
    crate::perf::assert_no_churn(|| {
        assert!(!text_typed(&mut state, &paste, None, false, &mut effects));
    });
}
