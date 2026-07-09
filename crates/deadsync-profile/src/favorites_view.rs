use deadsync_chart::SongData;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FavoriteCatalogHeader {
    pub name: String,
    pub pack_key: Option<String>,
}

#[derive(Clone, Debug)]
pub enum FavoriteCatalogEntry {
    Header(FavoriteCatalogHeader),
    Song(Arc<SongData>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FavoritePackRange {
    pub header_index: usize,
    pub song_start: usize,
    pub song_end: usize,
}

#[derive(Clone, Debug, Default)]
pub struct FavoriteViewPlan {
    pub loose_songs: Vec<Arc<SongData>>,
    pub pack_ranges: Vec<FavoritePackRange>,
}

pub fn favorite_view_plan<K: Ord>(
    entries: &[FavoriteCatalogEntry],
    mut pack_is_favorited: impl FnMut(&str) -> bool,
    mut song_is_favorited: impl FnMut(&SongData) -> bool,
    mut song_sort_key: impl FnMut(&SongData) -> K,
) -> FavoriteViewPlan {
    let mut current_header_idx = None;
    let mut current_pack_is_fav = false;
    let mut song_start = 0usize;
    let mut pack_ranges = Vec::new();
    let mut loose_songs = Vec::new();

    for (idx, entry) in entries.iter().enumerate() {
        match entry {
            FavoriteCatalogEntry::Header(header) => {
                close_pack(
                    &mut pack_ranges,
                    current_header_idx,
                    current_pack_is_fav,
                    song_start,
                    idx,
                );
                current_header_idx = Some(idx);
                current_pack_is_fav = header
                    .pack_key
                    .as_deref()
                    .is_some_and(&mut pack_is_favorited);
                song_start = idx + 1;
            }
            FavoriteCatalogEntry::Song(song) => {
                if !current_pack_is_fav && song_is_favorited(song) {
                    loose_songs.push(song.clone());
                }
            }
        }
    }

    close_pack(
        &mut pack_ranges,
        current_header_idx,
        current_pack_is_fav,
        song_start,
        entries.len(),
    );

    loose_songs.sort_by_cached_key(|song| song_sort_key(song));
    pack_ranges.sort_by_cached_key(|range| header_sort_key(entries, range.header_index));

    FavoriteViewPlan {
        loose_songs,
        pack_ranges,
    }
}

fn close_pack(
    pack_ranges: &mut Vec<FavoritePackRange>,
    header_index: Option<usize>,
    pack_is_favorited: bool,
    song_start: usize,
    song_end: usize,
) {
    if let Some(header_index) = header_index
        && pack_is_favorited
    {
        pack_ranges.push(FavoritePackRange {
            header_index,
            song_start,
            song_end,
        });
    }
}

fn header_sort_key(entries: &[FavoriteCatalogEntry], header_index: usize) -> String {
    match entries.get(header_index) {
        Some(FavoriteCatalogEntry::Header(header)) => header.name.to_ascii_lowercase(),
        Some(FavoriteCatalogEntry::Song(_)) | None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_song(title: &str) -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from(format!("{title}.ssc")),
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

    fn header(name: &str, pack_key: Option<&str>) -> FavoriteCatalogEntry {
        FavoriteCatalogEntry::Header(FavoriteCatalogHeader {
            name: name.to_string(),
            pack_key: pack_key.map(str::to_string),
        })
    }

    fn entries() -> Vec<FavoriteCatalogEntry> {
        vec![
            header("Pack A", Some("Pack A")),
            FavoriteCatalogEntry::Song(test_song("Song A1")),
            FavoriteCatalogEntry::Song(test_song("Song A2")),
            header("Pack B", Some("Pack B")),
            FavoriteCatalogEntry::Song(test_song("Song B1")),
        ]
    }

    fn song_titles(songs: &[Arc<SongData>]) -> Vec<&str> {
        songs.iter().map(|song| song.title.as_str()).collect()
    }

    #[test]
    fn includes_favorited_pack_without_loose_bucket() {
        let plan = favorite_view_plan(
            &entries(),
            |key| key == "Pack B",
            |_| false,
            |song| song.title.clone(),
        );

        assert!(plan.loose_songs.is_empty());
        assert_eq!(
            plan.pack_ranges,
            [FavoritePackRange {
                header_index: 3,
                song_start: 4,
                song_end: 5,
            }]
        );
    }

    #[test]
    fn dedupes_song_favorites_inside_favorited_pack() {
        let plan = favorite_view_plan(
            &entries(),
            |key| key == "Pack A",
            |song| song.title == "Song A1",
            |song| song.title.clone(),
        );

        assert!(plan.loose_songs.is_empty());
        assert_eq!(
            plan.pack_ranges,
            [FavoritePackRange {
                header_index: 0,
                song_start: 1,
                song_end: 3,
            }]
        );
    }

    #[test]
    fn ignores_synthetic_headers_for_pack_favorites() {
        let entries = vec![
            header("Pack A", None),
            FavoriteCatalogEntry::Song(test_song("Song")),
        ];
        let plan = favorite_view_plan(
            &entries,
            |key| key == "Pack A",
            |_| false,
            |song| song.title.clone(),
        );

        assert!(plan.loose_songs.is_empty());
        assert!(plan.pack_ranges.is_empty());
    }

    #[test]
    fn keeps_loose_song_favorites_sorted() {
        let plan = favorite_view_plan(
            &entries(),
            |_| false,
            |song| song.title == "Song A2" || song.title == "Song A1",
            |song| song.title.clone(),
        );

        assert_eq!(song_titles(&plan.loose_songs), ["Song A1", "Song A2"]);
        assert!(plan.pack_ranges.is_empty());
    }
}
