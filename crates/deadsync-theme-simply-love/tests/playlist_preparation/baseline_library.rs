// Frozen from 9541f9eeb. Imports/visibility changed; lookup/parser calls rebound to the frozen baseline.
#![allow(dead_code, clippy::all)]
use super::baseline_song::{PlaylistSongLookup, song_pack_and_dir_name};
use super::{MusicWheelEntry, SelectMusicPlaylistView};
use super::{PlaylistEntry, PlaylistSongSource, baseline_song};
use deadsync_chart::SongData;
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
    baseline_song::build_playlist_song_lookup(playlist_song_sources(
        grouped_entries,
        song_scan_roots,
    ))
}

fn playlist_music_entries(entries: Vec<PlaylistEntry>) -> Vec<MusicWheelEntry> {
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
    playlist_music_entries(baseline_song::playlist_entries_from_text(
        text,
        fallback_name,
        lookup,
    ))
}

pub(super) fn build_playlist_library(
    grouped_entries: &[MusicWheelEntry],
    playlist_views: &[SelectMusicPlaylistView],
    song_scan_roots: &[PathBuf],
) -> Vec<PlaylistCacheEntry> {
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
