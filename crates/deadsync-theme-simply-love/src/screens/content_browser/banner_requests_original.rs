// Frozen from 374c24c4c3c41631d3a8e50962c7fec3ed94bad3 for paired regression/benchmark checks.
use super::*;

pub(super) fn wanted_banners(state: &State) -> Vec<(u64, String)> {
    /// Rows above and below the window to fetch ahead, so paging does not
    /// start from nothing.
    const LOOKAHEAD: usize = lo::ROWS;

    let mut out = Vec::new();
    let mut want = |state: &State, index: usize| {
        let Some(pack) = state.snapshot.catalog.get(index) else {
            return;
        };
        if let Some(url) = banner_url_for(state, pack.id) {
            out.push((pack.id, url.to_owned()));
        }
    };

    // The grid is the first thing anyone sees, so its artwork is asked for
    // whether or not the grid is currently showing.
    for index in state.featured.clone() {
        want(state, index);
    }

    let start = window_start(state).saturating_sub(LOOKAHEAD);
    let end = (window_start(state) + visible_rows() + LOOKAHEAD).min(state.results.len());
    for index in state.results.get(start..end).unwrap_or_default().to_vec() {
        want(state, index);
    }

    // Both doubles columns, since both are on screen at once.
    for column in 0..2 {
        let list = doubles_column(state, column);
        let first = state.doubles_window[column].saturating_sub(2);
        let last = (state.doubles_window[column] + lo::DBL_ROWS + 2).min(list.len());
        for index in list.get(first..last).unwrap_or_default().to_vec() {
            want(state, index);
        }
    }

    // The open detail page's song jackets, which are not pack banners at all
    // but go through the same fetcher.
    if state.zone == Zone::Detail
        && let Some(page) = state.page.page.as_ref()
    {
        let last = (state.song_window + lo::SONG_ROWS + 2).min(page.songs.len());
        for song in page.songs.get(state.song_window..last).unwrap_or_default() {
            if let Some(url) = song.image_url.as_deref() {
                out.push((song_art_key(url), url.to_owned()));
            }
        }
    }
    out
}
