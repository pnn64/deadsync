use deadsync_chart::song::{standard_difficulty_index, sync_pref_offset};
use deadsync_chart::{SongData, SongPack, SyncPref};
use deadsync_config::app_config::Config;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::course::CourseFile;

pub type CourseData = (PathBuf, CourseFile);

static SONG_CACHE: std::sync::LazyLock<Mutex<Vec<SongPack>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));
static SONG_CACHE_GENERATION: AtomicU64 = AtomicU64::new(1);

static COURSE_CACHE: std::sync::LazyLock<Mutex<Vec<CourseData>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

/// Provides safe, read-only access to the global song cache.
pub fn get_song_cache() -> std::sync::MutexGuard<'static, Vec<SongPack>> {
    SONG_CACHE.lock().unwrap()
}

pub fn song_cache_generation() -> u64 {
    SONG_CACHE_GENERATION.load(Ordering::Relaxed)
}

/// A public function to allow the parser to populate the cache.
pub fn set_song_cache(packs: Vec<SongPack>) {
    let mut cache = SONG_CACHE.lock().unwrap();
    *cache = packs;
    SONG_CACHE_GENERATION.fetch_add(1, Ordering::Relaxed);
}

pub fn song_pack_group_for_simfile_path<'a>(
    packs: &'a [SongPack],
    simfile_path: &Path,
) -> Option<&'a str> {
    packs
        .iter()
        .find(|pack| {
            pack.songs
                .iter()
                .any(|song| song.simfile_path == simfile_path)
        })
        .map(|pack| pack.group_name.as_str())
}

pub fn song_pack_group_for_song(song: &SongData) -> Option<String> {
    let song_cache = get_song_cache();
    song_pack_group_for_simfile_path(&song_cache, &song.simfile_path).map(str::to_string)
}

pub fn pack_sync_offset_for_song(
    song: &SongData,
    enabled: bool,
    default_sync_pref: SyncPref,
) -> f32 {
    let song_cache = get_song_cache();
    pack_sync_offset_for_song_in_packs(song, song_cache.as_slice(), enabled, default_sync_pref)
}

pub fn pack_sync_offset_for_song_config(song: &SongData, cfg: &Config) -> f32 {
    pack_sync_offset_for_song(
        song,
        cfg.machine_pack_ini_offsets,
        cfg.machine_default_sync_offset.sync_pref(),
    )
}

pub fn pack_sync_offset_for_song_in_packs(
    song: &SongData,
    packs: &[SongPack],
    enabled: bool,
    default_sync_pref: SyncPref,
) -> f32 {
    if !enabled {
        return 0.0;
    }
    let Some(pack_group) = crate::event_intro::song_pack_group(song) else {
        return 0.0;
    };
    let pack_sync_pref = packs
        .iter()
        .find(|pack| pack.group_name == pack_group)
        .map(|pack| pack.sync_pref)
        .unwrap_or(SyncPref::Default);
    sync_pref_offset(pack_sync_pref, default_sync_pref)
}

#[inline(always)]
pub fn replace_song_arc_if_same_simfile(
    current_song: &mut Arc<SongData>,
    updated_song: &Arc<SongData>,
) -> bool {
    if current_song.simfile_path != updated_song.simfile_path {
        return false;
    }
    *current_song = updated_song.clone();
    true
}

#[inline(always)]
pub fn can_reuse_quick_restart_payload(
    current_song: &SongData,
    current_chart_hashes: [&str; 2],
    next_song: &SongData,
    next_chart_hashes: [&str; 2],
) -> bool {
    current_song.simfile_path == next_song.simfile_path
        && (current_song.offset - next_song.offset).abs() < 0.000_001_f32
        && current_chart_hashes == next_chart_hashes
}

pub fn reloaded_chart_hashes_for_restart(
    old_song: &SongData,
    updated_song: &SongData,
    chart_type: &str,
    old_hashes: [&str; 2],
    old_difficulties: [&str; 2],
) -> [String; 2] {
    std::array::from_fn(|slot| {
        let steps_index = old_song
            .steps_index_for_chart_hash(chart_type, old_hashes[slot])
            .or_else(|| standard_difficulty_index(old_difficulties[slot]))
            .unwrap_or(0);
        updated_song
            .chart_for_steps_index(chart_type, steps_index)
            .map(|chart| chart.short_hash.clone())
            .unwrap_or_default()
    })
}

pub fn reload_song_in_cache_with<F>(
    simfile_path: &std::path::Path,
    parse_song: F,
) -> Result<Arc<SongData>, String>
where
    F: FnOnce(&std::path::Path) -> Result<SongData, String>,
{
    let updated = Arc::new(parse_song(simfile_path)?);
    let mut song_cache = get_song_cache();
    let mut replaced = false;
    for pack in song_cache.iter_mut() {
        for song in &mut pack.songs {
            if song.simfile_path == simfile_path {
                *song = updated.clone();
                replaced = true;
            }
        }
    }
    if !replaced {
        return Err(format!(
            "Song '{}' not found in song cache",
            simfile_path.display()
        ));
    }
    Ok(updated)
}

pub fn get_course_cache() -> std::sync::MutexGuard<'static, Vec<CourseData>> {
    COURSE_CACHE.lock().unwrap()
}

pub fn set_course_cache(courses: Vec<CourseData>) {
    *COURSE_CACHE.lock().unwrap() = courses;
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{
        ArrowStats, ChartData, SongBackgroundChange, SongBackgroundChangeTarget, StaminaCounts,
        TechCounts,
    };

    fn song(path: &str) -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from(path),
            title: "Song".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: vec![SongBackgroundChange::new(
                0.0,
                SongBackgroundChangeTarget::NoSongBg,
            )],
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
            min_bpm: 120.0,
            max_bpm: 120.0,
            normalized_bpms: "120.000".to_string(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::<ChartData>::new(),
        })
    }

    fn pack(group_name: &str, sync_pref: SyncPref, songs: Vec<Arc<SongData>>) -> SongPack {
        SongPack {
            group_name: group_name.to_string(),
            name: group_name.to_string(),
            sort_title: group_name.to_string(),
            translit_title: String::new(),
            series: String::new(),
            year: 0,
            sync_pref,
            directory: PathBuf::from("Songs").join(group_name),
            banner_path: None,
            songs,
        }
    }

    fn chart(chart_type: &str, difficulty: &str, hash: &str) -> ChartData {
        ChartData {
            chart_type: chart_type.to_string(),
            difficulty: difficulty.to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 9,
            step_artist: String::new(),
            music_path: None,
            short_hash: hash.to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
        }
    }

    fn song_with_charts(path: &str, offset: f32, charts: Vec<ChartData>) -> Arc<SongData> {
        let mut song = song(path);
        Arc::make_mut(&mut song).offset = offset;
        Arc::make_mut(&mut song).charts = charts;
        song
    }

    #[test]
    fn pack_sync_offset_uses_matching_pack_pref() {
        let song = song("Songs/Test Pack/Song/song.ssc");
        let packs = [pack("Test Pack", SyncPref::Itg, vec![song.clone()])];

        assert_eq!(
            pack_sync_offset_for_song_in_packs(&song, &packs, true, SyncPref::Null),
            deadsync_chart::song::ITG_SYNC_OFFSET_SECONDS,
        );
    }

    #[test]
    fn pack_sync_offset_uses_default_for_default_pack_pref() {
        let song = song("Songs/Test Pack/Song/song.ssc");
        let packs = [pack("Test Pack", SyncPref::Default, vec![song.clone()])];

        assert_eq!(
            pack_sync_offset_for_song_in_packs(&song, &packs, true, SyncPref::Itg),
            deadsync_chart::song::ITG_SYNC_OFFSET_SECONDS,
        );
    }

    #[test]
    fn pack_sync_offset_disabled_is_zero() {
        let song = song("Songs/Test Pack/Song/song.ssc");
        let packs = [pack("Test Pack", SyncPref::Itg, vec![song.clone()])];

        assert_eq!(
            pack_sync_offset_for_song_in_packs(&song, &packs, false, SyncPref::Itg),
            0.0,
        );
    }

    #[test]
    fn replace_song_arc_swaps_matching_simfile() {
        let mut current = song_with_charts(
            "Songs/Test/song.ssc",
            0.0,
            vec![
                chart("dance-single", "Hard", "a"),
                chart("dance-single", "Challenge", "b"),
            ],
        );
        let updated = song_with_charts(
            "Songs/Test/song.ssc",
            -0.003,
            vec![
                chart("dance-single", "Hard", "a"),
                chart("dance-single", "Challenge", "b"),
            ],
        );

        assert!(replace_song_arc_if_same_simfile(&mut current, &updated));
        assert!(Arc::ptr_eq(&current, &updated));
        assert!((current.offset + 0.003).abs() < 0.000_001_f32);
    }

    #[test]
    fn replace_song_arc_ignores_other_simfile() {
        let original = song_with_charts("Songs/Test/a.ssc", 0.0, vec![]);
        let mut current = original.clone();
        let updated = song_with_charts("Songs/Test/b.ssc", -0.003, vec![]);

        assert!(!replace_song_arc_if_same_simfile(&mut current, &updated));
        assert!(Arc::ptr_eq(&current, &original));
    }

    #[test]
    fn quick_restart_payload_reuse_rejects_offset_mismatch() {
        let current_song = song_with_charts("Songs/Test/song.ssc", 0.0, vec![]);
        let updated_song = song_with_charts("Songs/Test/song.ssc", -0.003, vec![]);

        assert!(!can_reuse_quick_restart_payload(
            &current_song,
            ["a", "b"],
            &updated_song,
            ["a", "b"],
        ));
    }

    #[test]
    fn quick_restart_payload_reuse_accepts_matching_song_and_charts() {
        let current_song = song_with_charts("Songs/Test/song.ssc", -0.003, vec![]);
        let next_song = song_with_charts("Songs/Test/song.ssc", -0.003, vec![]);

        assert!(can_reuse_quick_restart_payload(
            &current_song,
            ["a", "b"],
            &next_song,
            ["a", "b"],
        ));
    }

    #[test]
    fn reloaded_chart_hashes_keep_matching_steps_index() {
        let old_song = song_with_charts(
            "Songs/Test/song.ssc",
            0.0,
            vec![
                chart("dance-single", "Easy", "old-easy"),
                chart("dance-single", "Hard", "old-hard"),
                chart("dance-single", "Challenge", "old-challenge"),
            ],
        );
        let updated_song = song_with_charts(
            "Songs/Test/song.ssc",
            0.0,
            vec![
                chart("dance-single", "Easy", "new-easy"),
                chart("dance-single", "Hard", "new-hard"),
                chart("dance-single", "Challenge", "new-challenge"),
            ],
        );

        assert_eq!(
            reloaded_chart_hashes_for_restart(
                &old_song,
                &updated_song,
                "dance-single",
                ["old-hard", "old-challenge"],
                ["Hard", "Challenge"],
            ),
            ["new-hard".to_string(), "new-challenge".to_string()],
        );
    }

    #[test]
    fn reloaded_chart_hashes_fall_back_to_difficulty_index() {
        let old_song = song_with_charts("Songs/Test/song.ssc", 0.0, vec![]);
        let updated_song = song_with_charts(
            "Songs/Test/song.ssc",
            0.0,
            vec![
                chart("dance-single", "Easy", "new-easy"),
                chart("dance-single", "Hard", "new-hard"),
                chart("dance-single", "Challenge", "new-challenge"),
            ],
        );

        assert_eq!(
            reloaded_chart_hashes_for_restart(
                &old_song,
                &updated_song,
                "dance-single",
                ["missing-hard", "missing-challenge"],
                ["Hard", "Challenge"],
            ),
            ["new-hard".to_string(), "new-challenge".to_string()],
        );
    }
}
