use super::*;
use crate::perf::assert_no_churn;


fn open(query: &str) -> SongSearchOpen {
    let SongSearchState::Open(mut open) = begin_song_search() else {
        unreachable!()
    };
    open.query = query.to_owned();
    open.selected_index = 7;
    open.blink_t = 0.25;
    open
}


#[test]
fn data_paths_delete_word_retains_query_allocation() {
    let mut state = open("日本語 alpha   ");
    let pointer = state.query.as_ptr();
    let capacity = state.query.capacity();
    let mut changed = false;
    assert_no_churn(|| changed = song_search_delete_word(&mut state));
    assert!(changed);
    assert_eq!(state.query, "日本語 ");
    assert_eq!(state.query.as_ptr(), pointer);
    assert_eq!(state.query.capacity(), capacity);
}
