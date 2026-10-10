// Frozen from main 16a06a2619a6cdcc34c647603d5dfa186e2f3e73; visibility only.

pub(super) fn itl_event_intro_name(pack_group: &str) -> Option<String> {
    let name = pack_group.trim();
    let bytes = name.as_bytes();
    if !is_itl_event_group(name) {
        return None;
    }

    // Personal ITL unlock packs are named "ITL Online <year> Unlocks - <username>".
    // Cut everything from the " Unlocks" marker onward (including any trailing
    // "- <username>") so the footer shows just the event name, e.g. "ITL Online 2026".
    const UNLOCKS_MARKER: &str = " unlocks";
    let name = match ascii_case_insensitive_find(bytes, UNLOCKS_MARKER.as_bytes()) {
        Some(idx) => &name[..idx],
        None => name,
    };
    Some(name.trim().to_string())
}

pub(super) fn event_intro_name_for_pack(pack_group: &str) -> Option<String> {
    let name = pack_group.trim();
    let bytes = name.as_bytes();
    if ascii_case_insensitive_find(bytes, b"stamina rpg 10").is_some()
        || ascii_case_insensitive_find(bytes, b"srpg10").is_some()
    {
        return Some("Stamina RPG 10".to_string());
    }
    if ascii_case_insensitive_find(bytes, b"stamina rpg 9").is_some()
        || ascii_case_insensitive_find(bytes, b"srpg9").is_some()
    {
        return Some("Stamina RPG 9".to_string());
    }
    itl_event_intro_name(name)
}

pub(super) fn gameplay_event_intro_text(song: &SongData) -> Arc<str> {
    song_pack_group(song)
        .and_then(event_intro_name_for_pack)
        .map(Arc::from)
        .unwrap_or_else(|| Arc::from("EVENT"))
}
