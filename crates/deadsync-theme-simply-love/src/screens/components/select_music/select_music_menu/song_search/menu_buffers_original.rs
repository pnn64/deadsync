pub fn original_song_search_completion(open: &SongSearchOpen) -> Option<SongSearchCompletion> {
    if open.query.is_empty() {
        return None;
    }
    let text = parse_song_search_live(&open.query).text;
    if text.is_empty() {
        return None;
    }
    let label = song_search_focused_match(open)?.label();
    // The ghost is drawn as gray text *under* the typed text, so it can only be
    // offered when it literally extends what is on screen. Testing the raw query
    // against the query Tab would install keeps the two honest: a trailing space
    // or a trailing `[###]` token means the accepted query reorders what the user
    // typed, and no ghost can truthfully be drawn over that.
    let accepted = song_search_query_completed_with(&open.query, &label);
    let consumed = fuzzy::folded_prefix_len(&open.query, &accepted)?;
    let remainder: String = accepted.chars().skip(consumed).collect();
    if remainder.is_empty() {
        return None;
    }
    Some(SongSearchCompletion {
        display: format!("{}{remainder}", open.query),
        typed: open.query.clone(),
        accepted,
    })
}
