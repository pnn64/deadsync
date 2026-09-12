use deadsync_chart::SongData;
use rustc_hash::FxHashMap;
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
    by_path: FxHashMap<String, Arc<SongData>>,
    by_pack_song: FxHashMap<String, FxHashMap<String, Arc<SongData>>>,
    by_group: FxHashMap<String, Vec<Arc<SongData>>>,
}

#[must_use]
pub fn normalize_song_path(song_path: &str) -> String {
    normalize_song_path_with(song_path, false)
}

fn normalize_song_path_ascii_lowercase(song_path: &str) -> String {
    let mut normalized = String::with_capacity(song_path.trim().len());
    normalize_song_path_ascii_lowercase_into(song_path, &mut normalized);
    normalized
}

fn normalize_song_path_with(song_path: &str, ascii_lowercase: bool) -> String {
    let song_path = song_path.trim();
    let mut normalized = String::with_capacity(song_path.len());
    append_normalized_song_path(song_path, &mut normalized);
    if ascii_lowercase {
        normalized.make_ascii_lowercase();
    }
    normalized
}

fn normalize_song_path_ascii_lowercase_into(song_path: &str, normalized: &mut String) {
    let song_path = song_path.trim();
    normalized.clear();
    normalized.reserve(song_path.len());
    append_normalized_song_path(song_path, normalized);
    normalized.make_ascii_lowercase();
}

fn ascii_lowercase_into(value: &str, lowercase: &mut String) {
    lowercase.clear();
    lowercase.reserve(value.len());
    lowercase.push_str(value);
    lowercase.make_ascii_lowercase();
}

fn append_normalized_song_path(song_path: &str, normalized: &mut String) {
    for segment in song_path
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty())
    {
        if !normalized.is_empty() {
            normalized.push('/');
        }
        normalized.push_str(segment);
    }
}

#[must_use]
pub fn pack_and_song_name_from_path(song_path: &str) -> Option<(String, String)> {
    let mut parts = song_path
        .trim()
        .rsplit(['/', '\\'])
        .filter(|segment| !segment.is_empty());
    let song = parts.next()?;
    let pack = parts.next()?;
    Some((pack.to_string(), song.to_string()))
}

#[must_use]
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
    let sources = sources.into_iter();
    let path_capacity = sources.size_hint().0;
    let mut lookup = PlaylistSongLookup::default();
    let mut group_key = String::new();
    let mut pack_key = String::new();
    let mut song_key = String::new();

    for source in sources {
        if let Some(path) = source.lobby_path.as_deref() {
            if lookup.by_path.capacity() == 0 {
                lookup.by_path.reserve(path_capacity);
            }
            lookup
                .by_path
                .entry(normalize_song_path_ascii_lowercase(path))
                .or_insert_with(|| source.song.clone());
        }
        let group = source
            .group_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        if let Some(group) = group {
            ascii_lowercase_into(group, &mut group_key);
        }
        let has_pack = if let Some((pack, song)) = song_pack_and_dir_name(&source.song) {
            ascii_lowercase_into(pack.trim(), &mut pack_key);
            ascii_lowercase_into(song.trim(), &mut song_key);
            true
        } else {
            false
        };
        if has_pack {
            if group.is_some() {
                insert_pack_song(&mut lookup, &group_key, &song_key, &source.song);
            }
            // Equal display/folder aliases already inserted the same song.
            if group.is_none() || group_key != pack_key {
                insert_pack_song(&mut lookup, &pack_key, &song_key, &source.song);
            }
        }
        if group.is_some() {
            playlist_group_mut(&mut lookup.by_group, &group_key).push(source.song.clone());
        }
        if has_pack
            && source
                .group_name
                .as_deref()
                .is_none_or(|group| !group.trim().eq_ignore_ascii_case(&pack_key))
        {
            playlist_group_mut(&mut lookup.by_group, &pack_key).push(source.song);
        }
    }
    lookup
}

fn playlist_group_mut<'a, T: Default>(
    groups: &'a mut FxHashMap<String, T>,
    key: &str,
) -> &'a mut T {
    // Borrow existing keys; owning one per source would allocate for every
    // song in a pack. A second lookup is cheaper than that repeated ownership.
    if !groups.contains_key(key) {
        groups.insert(key.to_owned(), T::default());
    }
    groups.get_mut(key).expect("playlist group was inserted")
}

fn insert_pack_song(
    lookup: &mut PlaylistSongLookup,
    pack: &str,
    song_key: &str,
    song: &Arc<SongData>,
) {
    let songs = playlist_group_mut(&mut lookup.by_pack_song, pack);
    if !songs.contains_key(song_key) {
        songs.insert(song_key.to_owned(), song.clone());
    }
}

pub fn playlist_entries_from_text(
    text: &str,
    fallback_name: &str,
    lookup: &PlaylistSongLookup,
) -> Vec<PlaylistEntry> {
    let mut entries = Vec::new();
    for_each_playlist_section(text, fallback_name, lookup, |name, songs| {
        let song_count = songs.len();
        entries.reserve(song_count + 1);
        entries.push(PlaylistEntry::Header {
            name: name.to_owned(),
            song_count,
        });
        entries.extend(songs.map(PlaylistEntry::Song));
    });
    entries
}

/// Visit each nonempty resolved section in order. Names borrow the input or
/// fallback; song handles are transferred in a batch. Dropping the drain releases
/// any unconsumed handles, and the section buffer is reused for the next section.
pub fn for_each_playlist_section(
    text: &str,
    fallback_name: &str,
    lookup: &PlaylistSongLookup,
    mut visit: impl FnMut(&str, std::vec::Drain<'_, Arc<SongData>>),
) {
    let mut section_name = None;
    let mut songs = Vec::new();
    let mut normalized = String::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix("---") {
            visit_playlist_section(&mut songs, section_name, fallback_name, &mut visit);
            section_name = Some(name.trim());
            continue;
        }
        if let Some(group) = line.strip_suffix("/*").map(str::trim)
            && !group.is_empty()
        {
            ascii_lowercase_into(group, &mut normalized);
            if let Some(group_songs) = lookup.by_group.get(normalized.as_str()) {
                songs.extend(group_songs.iter().cloned());
            }
            continue;
        }
        if let Some(song) = find_playlist_song(lookup, line, &mut normalized) {
            songs.push(song);
        }
    }
    visit_playlist_section(&mut songs, section_name, fallback_name, &mut visit);
}

fn visit_playlist_section(
    songs: &mut Vec<Arc<SongData>>,
    section_name: Option<&str>,
    fallback_name: &str,
    visit: &mut impl FnMut(&str, std::vec::Drain<'_, Arc<SongData>>),
) {
    if songs.is_empty() {
        return;
    }
    let name = section_name
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback_name);
    visit(name, songs.drain(..));
}

fn find_playlist_song(
    lookup: &PlaylistSongLookup,
    line: &str,
    normalized: &mut String,
) -> Option<Arc<SongData>> {
    normalize_song_path_ascii_lowercase_into(line, normalized);
    if normalized.is_empty() {
        return None;
    }
    if let Some(song) = lookup.by_path.get(normalized.as_str()) {
        return Some(song.clone());
    }

    let mut parts = normalized.split('/').filter(|part| !part.is_empty()).rev();
    let song = parts.next()?;
    let pack = parts.next()?;
    lookup.by_pack_song.get(pack)?.get(song).cloned()
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
            translit_artist: String::new(),
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

    fn sources() -> [PlaylistSongSource; 3] {
        [
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
        ]
    }

    fn lookup() -> PlaylistSongLookup {
        build_playlist_song_lookup(sources())
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
        assert_eq!(
            pack_and_song_name_from_path(" /Songs\\Pack//Song/ "),
            Some(("Pack".to_string(), "Song".to_string()))
        );
        assert_eq!(pack_and_song_name_from_path("SongOnly"), None);

        for (input, expected) in [
            ("////", ""),
            ("\\\\Pack\\\\Song\\", "Pack/Song"),
            (" /Pack\\Song//Chart ", "Pack/Song/Chart"),
            ("Pack/ Song Name /", "Pack/ Song Name "),
            (" Müsic\\曲 ", "Müsic/曲"),
        ] {
            assert_eq!(normalize_song_path(input), expected);
        }
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

    #[test]
    fn playlist_parser_resolves_pack_song_without_lobby_path() {
        let lookup = build_playlist_song_lookup([PlaylistSongSource {
            group_name: Some("Display Group".to_string()),
            song: song("Folder Pack", "Folder Song", "Fallback"),
            lobby_path: None,
        }]);

        let entries =
            playlist_entries_from_text("folder pack/folder song\n", "Fallback Playlist", &lookup);

        assert_eq!(song_titles(&entries), ["Fallback"]);
    }
}
