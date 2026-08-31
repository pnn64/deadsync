use deadsync_chart::SongData;
use rustc_hash::{FxBuildHasher, FxHashMap};
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
    let mut lookup = PlaylistSongLookup {
        by_path: FxHashMap::with_capacity_and_hasher(path_capacity, FxBuildHasher),
        by_pack_song: FxHashMap::default(),
        by_group: FxHashMap::default(),
    };

    for source in sources {
        if let Some(path) = source.lobby_path.as_deref() {
            lookup
                .by_path
                .entry(normalize_song_path_ascii_lowercase(path))
                .or_insert_with(|| source.song.clone());
        }

        let group_key = source
            .group_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_ascii_lowercase);
        let (pack_dir_key, song_dir_key) = song_pack_and_dir_name(source.song.as_ref()).map_or(
            (None, None),
            |(pack_dir, song_dir)| {
                (
                    Some(pack_dir.trim().to_ascii_lowercase()),
                    Some(song_dir.trim().to_ascii_lowercase()),
                )
            },
        );

        if let Some(song_dir) = song_dir_key {
            if let Some(group_key) = group_key.as_ref() {
                lookup
                    .by_pack_song
                    .entry(group_key.clone())
                    .or_default()
                    .entry(song_dir.clone())
                    .or_insert_with(|| source.song.clone());
            }
            if let Some(pack_dir) = pack_dir_key.as_ref() {
                lookup
                    .by_pack_song
                    .entry(pack_dir.clone())
                    .or_default()
                    .entry(song_dir)
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
    let mut normalized = String::new();

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
            ascii_lowercase_into(group_name, &mut normalized);
            if let Some(songs) = lookup.by_group.get(normalized.as_str()) {
                current_songs.extend(songs.iter().cloned());
            }
            continue;
        }
        if let Some(song) = find_playlist_song(lookup, line, &mut normalized) {
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

#[cfg(any(test, feature = "bench-support"))]
pub mod bench_support {
    use super::*;
    use std::collections::HashMap;
    use std::hash::BuildHasher;

    #[must_use]
    pub fn normalize_song_path_ascii_lowercase_reference(song_path: &str) -> String {
        let song_path = song_path.trim();
        let mut normalized = String::with_capacity(song_path.len());
        for segment in song_path
            .split(['/', '\\'])
            .filter(|segment| !segment.is_empty())
        {
            if !normalized.is_empty() {
                normalized.push('/');
            }
            normalized.extend(segment.chars().map(|ch| ch.to_ascii_lowercase()));
        }
        normalized
    }

    #[must_use]
    pub fn normalize_song_path_ascii_lowercase_current(song_path: &str) -> String {
        normalize_song_path_ascii_lowercase(song_path)
    }

    #[must_use]
    pub fn playlist_entries_from_text_reference(
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
            if let Some(song) = find_playlist_song_reference(lookup, line) {
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

    fn find_playlist_song_reference(
        lookup: &PlaylistSongLookup,
        line: &str,
    ) -> Option<Arc<SongData>> {
        let normalized = normalize_song_path_ascii_lowercase(line);
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

    #[must_use]
    pub fn playlist_lookup_reference_checksum(
        sources: impl IntoIterator<Item = PlaylistSongSource>,
    ) -> u64 {
        let mut by_path = HashMap::new();
        let mut by_pack_song: HashMap<String, HashMap<String, Arc<SongData>>> = HashMap::new();
        let mut by_group: HashMap<String, Vec<Arc<SongData>>> = HashMap::new();

        for source in sources {
            if let Some(path) = source.lobby_path.as_deref() {
                by_path
                    .entry(normalize_song_path_ascii_lowercase(path))
                    .or_insert_with(|| source.song.clone());
            }

            let group_key = source
                .group_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_ascii_lowercase);
            let (pack_dir_key, song_dir_key) = song_pack_and_dir_name(source.song.as_ref()).map_or(
                (None, None),
                |(pack_dir, song_dir)| {
                    (
                        Some(pack_dir.trim().to_ascii_lowercase()),
                        Some(song_dir.trim().to_ascii_lowercase()),
                    )
                },
            );

            if let Some(song_dir) = song_dir_key {
                if let Some(group_key) = group_key.as_ref() {
                    by_pack_song
                        .entry(group_key.clone())
                        .or_default()
                        .entry(song_dir.clone())
                        .or_insert_with(|| source.song.clone());
                }
                if let Some(pack_dir) = pack_dir_key.as_ref() {
                    by_pack_song
                        .entry(pack_dir.clone())
                        .or_default()
                        .entry(song_dir)
                        .or_insert_with(|| source.song.clone());
                }
            }

            if let Some(group_key) = group_key {
                by_group
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
                by_group.entry(pack_dir).or_default().push(source.song);
            }
        }

        playlist_lookup_checksum(&by_path, &by_pack_song, &by_group)
    }

    #[must_use]
    pub fn playlist_lookup_current_checksum(
        sources: impl IntoIterator<Item = PlaylistSongSource>,
    ) -> u64 {
        let lookup = build_playlist_song_lookup(sources);
        playlist_lookup_checksum(&lookup.by_path, &lookup.by_pack_song, &lookup.by_group)
    }

    fn playlist_lookup_checksum<S: BuildHasher>(
        by_path: &HashMap<String, Arc<SongData>, S>,
        by_pack_song: &HashMap<String, HashMap<String, Arc<SongData>, S>, S>,
        by_group: &HashMap<String, Vec<Arc<SongData>>, S>,
    ) -> u64 {
        let mut checksum = (by_path.len() as u64)
            ^ (by_pack_song.len() as u64).rotate_left(11)
            ^ (by_group.len() as u64).rotate_left(23);
        for (path, song) in by_path {
            checksum = checksum.wrapping_add(
                text_checksum(path).rotate_left(5) ^ text_checksum(&song.title).rotate_left(19),
            );
        }
        for (pack, songs) in by_pack_song {
            let pack_hash = text_checksum(pack);
            for (song_dir, song) in songs {
                checksum = checksum.wrapping_add(
                    pack_hash.rotate_left(7)
                        ^ text_checksum(song_dir).rotate_left(17)
                        ^ text_checksum(&song.title).rotate_left(29),
                );
            }
        }
        for (group, songs) in by_group {
            let group_hash = text_checksum(group);
            for (index, song) in songs.iter().enumerate() {
                checksum = checksum.wrapping_add(
                    group_hash.rotate_left(13)
                        ^ text_checksum(&song.title).rotate_left((index % 63) as u32 + 1),
                );
            }
        }
        checksum
    }

    fn text_checksum(text: &str) -> u64 {
        text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }
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
    fn byte_lowercase_normalization_matches_scalar_reference() {
        for input in [
            "",
            "////",
            " /Songs\\Pack//Song/ ",
            "MIXED/Ascii/Path.SSC",
            " MÃ¼sic\\æ›²/Ä°STANBUL ",
            "\\\\Pack\\\\Song\\",
        ] {
            assert_eq!(
                bench_support::normalize_song_path_ascii_lowercase_current(input),
                bench_support::normalize_song_path_ascii_lowercase_reference(input),
                "input={input:?}",
            );
        }
    }

    #[test]
    fn fast_playlist_lookup_preserves_default_map_contents() {
        let sources = sources();
        assert_eq!(
            bench_support::playlist_lookup_current_checksum(sources.clone()),
            bench_support::playlist_lookup_reference_checksum(sources),
        );
    }

    #[test]
    fn reused_playlist_line_buffer_preserves_entry_sequence() {
        fn signature(entries: &[PlaylistEntry]) -> Vec<String> {
            entries
                .iter()
                .map(|entry| match entry {
                    PlaylistEntry::Header { name, song_count } => {
                        format!("header:{name}:{song_count}")
                    }
                    PlaylistEntry::Song(song) => format!("song:{}", song.title),
                })
                .collect()
        }

        let lookup = lookup();
        let text = "\n--- Warmup \nPACK A/*\nmissing/song\n---Finale\n\\Pack B\\Song B1\\\n";
        assert_eq!(
            signature(&playlist_entries_from_text(text, "Night Shift", &lookup)),
            signature(&bench_support::playlist_entries_from_text_reference(
                text,
                "Night Shift",
                &lookup,
            )),
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
