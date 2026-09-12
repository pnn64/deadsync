use super::{MusicWheelEntry, SelectMusicPlaylistView};
use deadsync_chart::SongData;
use deadsync_simfile::playlist::{
    PlaylistEntry, PlaylistSongLookup, PlaylistSongSource, song_pack_and_dir_name,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PlaylistMenuEntry {
    pub(super) id: String,
    pub(super) top_label: String,
    pub(super) bottom_label: String,
}

#[derive(Debug)]
pub(super) struct PlaylistCacheEntry {
    pub(super) menu_entry: PlaylistMenuEntry,
    pub(super) entries: Arc<[MusicWheelEntry]>,
}

fn playlist_song_sources(
    grouped_entries: &[MusicWheelEntry],
    song_scan_roots: &[PathBuf],
) -> Vec<PlaylistSongSource> {
    let mut sources = Vec::new();
    let mut current_group = None;

    for entry in grouped_entries {
        match entry {
            MusicWheelEntry::PackHeader { name, pack_key, .. } => {
                current_group = Some(
                    pack_key
                        .as_deref()
                        .unwrap_or_else(|| name.as_ref())
                        .to_owned(),
                );
            }
            MusicWheelEntry::Song(song) => sources.push(PlaylistSongSource {
                group_name: current_group.clone(),
                song: song.clone(),
                lobby_path: lobby_song_path(song.as_ref(), song_scan_roots),
            }),
        }
    }

    sources
}

pub(super) fn build_playlist_song_lookup(
    grouped_entries: &[MusicWheelEntry],
    song_scan_roots: &[PathBuf],
) -> PlaylistSongLookup {
    deadsync_simfile::playlist::build_playlist_song_lookup(playlist_song_sources(
        grouped_entries,
        song_scan_roots,
    ))
}

fn playlist_music_entries(
    entries: impl IntoIterator<Item = PlaylistEntry>,
) -> Vec<MusicWheelEntry> {
    let mut header_idx = 0usize;
    entries
        .into_iter()
        .map(|entry| match entry {
            PlaylistEntry::Header { name, song_count } => {
                let original_index = header_idx;
                header_idx += 1;
                MusicWheelEntry::PackHeader {
                    name: Arc::from(name),
                    original_index,
                    banner_path: None,
                    song_count,
                    pack_key: None,
                    parent_series: None,
                }
            }
            PlaylistEntry::Song(song) => MusicWheelEntry::Song(song),
        })
        .collect()
}

pub(super) fn build_playlist_entries_from_text(
    text: &str,
    fallback_name: &str,
    lookup: &PlaylistSongLookup,
) -> Vec<MusicWheelEntry> {
    // A single section exposes its full size before output construction.
    // Keep whole-playlist sizing for multiple sections: growing the larger
    // wheel-entry vector can otherwise cost more than the intermediate vector.
    let first_line = text.trim_start();
    let later_text = if first_line.starts_with("---") {
        first_line.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        text
    };
    if later_text.contains("---") {
        return playlist_music_entries(deadsync_simfile::playlist::playlist_entries_from_text(
            text,
            fallback_name,
            lookup,
        ));
    }
    let mut entries = Vec::new();
    let mut header_index = 0;
    deadsync_simfile::playlist::for_each_playlist_section(
        text,
        fallback_name,
        lookup,
        |name, songs| {
            let song_count = songs.len();
            entries.reserve(song_count + 1);
            entries.push(MusicWheelEntry::PackHeader {
                name: Arc::from(name),
                original_index: header_index,
                banner_path: None,
                song_count,
                pack_key: None,
                parent_series: None,
            });
            header_index += 1;
            entries.extend(songs.map(MusicWheelEntry::Song));
        },
    );
    entries
}

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

pub(super) fn lobby_song_path(song: &SongData, _song_scan_roots: &[PathBuf]) -> Option<String> {
    let (pack, song) = song_pack_and_dir_name(song)?;
    Some(format!("{pack}/{song}"))
}
