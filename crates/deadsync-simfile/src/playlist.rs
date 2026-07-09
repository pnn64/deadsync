use deadsync_chart::SongData;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct PlaylistSongSource {
    pub group_name: Option<String>,
    pub song: Arc<SongData>,
    pub lobby_path: Option<String>,
}

#[derive(Clone, Debug)]
pub enum PlaylistEntry {
    Header { name: String, song_count: usize },
    Song(Arc<SongData>),
}

#[derive(Clone, Debug, Default)]
pub struct PlaylistSongLookup {
    by_path: HashMap<String, Arc<SongData>>,
    by_pack_song: HashMap<(String, String), Arc<SongData>>,
    by_group: HashMap<String, Vec<Arc<SongData>>>,
}

pub fn normalize_song_path(song_path: &str) -> String {
    song_path
        .trim()
        .trim_matches('/')
        .replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn pack_and_song_name_from_path(song_path: &str) -> Option<(String, String)> {
    let normalized = normalize_song_path(song_path);
    let mut parts = normalized
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let song = parts.pop()?.to_string();
    let pack = parts.pop()?.to_string();
    Some((pack, song))
}

pub fn song_pack_and_dir_name(song: &SongData) -> Option<(&str, &str)> {
    let song_dir = song.simfile_path.parent()?.file_name()?.to_str()?;
    let pack_dir = song
        .simfile_path
        .parent()?
        .parent()?
        .file_name()?
        .to_str()?;
    Some((pack_dir, song_dir))
}

pub fn build_playlist_song_lookup(
    sources: impl IntoIterator<Item = PlaylistSongSource>,
) -> PlaylistSongLookup {
    let mut lookup = PlaylistSongLookup::default();

    for source in sources {
        if let Some(path) = source.lobby_path.as_deref() {
            lookup
                .by_path
                .entry(normalize_song_path(path).to_ascii_lowercase())
                .or_insert_with(|| source.song.clone());
        }

        let group_key = source
            .group_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_ascii_lowercase);
        let pack_dir_key = song_pack_and_dir_name(source.song.as_ref())
            .map(|(pack_dir, _)| pack_dir.trim().to_ascii_lowercase());
        let song_dir_key = song_pack_and_dir_name(source.song.as_ref())
            .map(|(_, song_dir)| song_dir.trim().to_ascii_lowercase());

        if let Some(song_dir) = song_dir_key {
            if let Some(group_key) = group_key.as_ref() {
                lookup
                    .by_pack_song
                    .entry((group_key.clone(), song_dir.clone()))
                    .or_insert_with(|| source.song.clone());
            }
            if let Some(pack_dir) = pack_dir_key.as_ref() {
                lookup
                    .by_pack_song
                    .entry((pack_dir.clone(), song_dir))
                    .or_insert_with(|| source.song.clone());
            }
        }

        if let Some(group_key) = group_key {
            lookup
                .by_group
                .entry(group_key)
                .or_default()
                .push(source.song.clone());
        }
        if let Some(pack_dir) = pack_dir_key
            && source
                .group_name
                .as_deref()
                .is_none_or(|group| !group.trim().eq_ignore_ascii_case(pack_dir.as_str()))
        {
            lookup
                .by_group
                .entry(pack_dir)
                .or_default()
                .push(source.song);
        }
    }

    lookup
}

pub fn playlist_entries_from_text(
    text: &str,
    fallback_name: &str,
    lookup: &PlaylistSongLookup,
) -> Vec<PlaylistEntry> {
    let mut entries = Vec::new();
    let mut current_section = None;
    let mut current_songs = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(section_name) = line.strip_prefix("---") {
            push_playlist_section(
                &mut entries,
                current_section.as_deref(),
                fallback_name,
                &mut current_songs,
            );
            current_section = Some(section_name.trim().to_string());
            continue;
        }
        if let Some(group_name) = line.strip_suffix("/*").map(str::trim)
            && !group_name.is_empty()
        {
            if let Some(songs) = lookup
                .by_group
                .get(group_name.to_ascii_lowercase().as_str())
            {
                current_songs.extend(songs.iter().cloned());
            }
            continue;
        }
        if let Some(song) = find_playlist_song(lookup, line) {
            current_songs.push(song);
        }
    }

    push_playlist_section(
        &mut entries,
        current_section.as_deref(),
        fallback_name,
        &mut current_songs,
    );
    entries
}

fn find_playlist_song(lookup: &PlaylistSongLookup, line: &str) -> Option<Arc<SongData>> {
    let normalized = normalize_song_path(line).to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }
    if let Some(song) = lookup.by_path.get(normalized.as_str()) {
        return Some(song.clone());
    }

    let mut parts = normalized.split('/').filter(|part| !part.is_empty()).rev();
    let song = parts.next()?;
    let pack = parts.next()?;
    lookup
        .by_pack_song
        .get(&(pack.to_string(), song.to_string()))
        .cloned()
}

fn push_playlist_section(
    entries: &mut Vec<PlaylistEntry>,
    section_name: Option<&str>,
    fallback_name: &str,
    songs: &mut Vec<Arc<SongData>>,
) {
    if songs.is_empty() {
        return;
    }
    let name = section_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback_name)
        .to_string();
    entries.push(PlaylistEntry::Header {
        name,
        song_count: songs.len(),
    });
    entries.extend(songs.drain(..).map(PlaylistEntry::Song));
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::SongData;
    use std::path::PathBuf;

    fn song(pack: &str, song_dir: &str, title: &str) -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from(format!("/songs/{pack}/{song_dir}/song.ssc")),
            title: title.to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        })
    }

    fn lookup() -> PlaylistSongLookup {
        build_playlist_song_lookup([
            PlaylistSongSource {
                group_name: Some("Pack A".to_string()),
                song: song("Pack A", "Song A1", "Alpha"),
                lobby_path: Some("Pack A/Song A1".to_string()),
            },
            PlaylistSongSource {
                group_name: Some("Pack A".to_string()),
                song: song("Pack A", "Song A2", "Beta"),
                lobby_path: Some("Pack A/Song A2".to_string()),
            },
            PlaylistSongSource {
                group_name: Some("Pack B".to_string()),
                song: song("Pack B", "Song B1", "Gamma"),
                lobby_path: Some("Pack B/Song B1".to_string()),
            },
        ])
    }

    fn song_titles(entries: &[PlaylistEntry]) -> Vec<&str> {
        entries
            .iter()
            .filter_map(|entry| match entry {
                PlaylistEntry::Song(song) => Some(song.title.as_str()),
                PlaylistEntry::Header { .. } => None,
            })
            .collect()
    }

    #[test]
    fn normalizes_paths_and_extracts_pack_song_names() {
        assert_eq!(
            normalize_song_path(" /Songs\\Pack//Song/ "),
            "Songs/Pack/Song"
        );
        assert_eq!(
            pack_and_song_name_from_path("Songs/Pack/Song"),
            Some(("Pack".to_string(), "Song".to_string()))
        );
    }

    #[test]
    fn playlist_parser_supports_sections_and_pack_wildcards() {
        let entries = playlist_entries_from_text(
            "---Warmup\nPack A/*\n---Finale\nPack B/Song B1\n",
            "Night Shift",
            &lookup(),
        );

        assert!(matches!(
            entries[0],
            PlaylistEntry::Header { ref name, song_count: 2 } if name == "Warmup"
        ));
        assert_eq!(song_titles(&entries), ["Alpha", "Beta", "Gamma"]);
        assert!(matches!(
            entries[3],
            PlaylistEntry::Header { ref name, song_count: 1 } if name == "Finale"
        ));
    }

    #[test]
    fn playlist_parser_uses_playlist_name_when_no_header_exists() {
        let entries = playlist_entries_from_text(
            "Pack A/Song A2\nPack B/Song B1\n",
            "Night Shift",
            &lookup(),
        );

        assert!(matches!(
            entries[0],
            PlaylistEntry::Header { ref name, song_count: 2 } if name == "Night Shift"
        ));
        assert_eq!(song_titles(&entries), ["Beta", "Gamma"]);
    }
}
