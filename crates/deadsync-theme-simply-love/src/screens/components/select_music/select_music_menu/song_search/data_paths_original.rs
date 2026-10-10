// Frozen from b4d4a8b49c21b96b3be66cf05cdfda37b74f9b2f; only names changed.
pub fn original_song_search_delete_word(open: &mut SongSearchOpen) -> bool {
    let mut chars: Vec<char> = open.query.chars().collect();
    let before = chars.len();
    while chars.last().is_some_and(|c| c.is_whitespace()) {
        chars.pop();
    }
    while chars.last().is_some_and(|c| !c.is_whitespace()) {
        chars.pop();
    }
    if chars.len() == before {
        return false;
    }
    open.query = chars.into_iter().collect();
    true
}
