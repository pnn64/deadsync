use super::*;

pub(super) fn build_playlist_library(
    grouped_entries: &[MusicWheelEntry],
    playlist_views: &[SelectMusicPlaylistView],
    song_scan_roots: &[PathBuf],
) -> Vec<PlaylistCacheEntry> {
    if playlist_views.is_empty() {
        return Vec::new();
    }
    let lookup = build_playlist_song_lookup(grouped_entries, song_scan_roots);
    let mut playlists: Vec<PlaylistCacheEntry> = playlist_views
        .iter()
        .map(|playlist| PlaylistCacheEntry {
            menu_entry: PlaylistMenuEntry {
                id: playlist.id.clone(),
                top_label: playlist.owner.as_ref().map_or_else(
                    || "Machine Playlist".to_string(),
                    |owner| format!("{owner} Playlist"),
                ),
                bottom_label: playlist.name.clone(),
            },
            entries: build_playlist_entries_from_text(&playlist.text, &playlist.name, &lookup)
                .into(),
        })
        .collect();

    playlists.sort_by_cached_key(|playlist| {
        (
            playlist.menu_entry.top_label.to_ascii_lowercase(),
            playlist.menu_entry.bottom_label.to_ascii_lowercase(),
        )
    });
    playlists
}
