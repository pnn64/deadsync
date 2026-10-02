// Frozen from 5abf1dd0b (0.5.1684).
#[inline(always)]
#[must_use]
fn old_song_lua_message_command_index(
    indices: &SongLuaMessageCommandIndices,
    message: &str,
) -> Option<usize> {
    let found = if message.bytes().any(|byte| byte.is_ascii_uppercase()) {
        indices.entries.binary_search_by(|entry| {
            song_lua_message_key(&indices.keys, entry)
                .iter()
                .copied()
                .cmp(message.bytes().map(|byte| byte.to_ascii_lowercase()))
        })
    } else {
        indices.entries.binary_search_by(|entry| {
            song_lua_message_key(&indices.keys, entry).cmp(message.as_bytes())
        })
    };
    found.ok().map(|entry| indices.entries[entry].index)
}
