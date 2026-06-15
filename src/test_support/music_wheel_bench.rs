use crate::screens::components::select_music::music_wheel::{self, MusicWheelParams};
use crate::screens::select_music::MusicWheelEntry;
use deadsync_chart::SongData;
use deadsync_chart::{ArrowStats, ChartData, StaminaCounts, TechCounts};
use deadsync_present::actors::Actor;
use deadsync_score::{CachedScore, Grade};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

pub const SCENARIO_NAME: &str = "music-wheel";
pub const SCENARIO_NAME_LOADED: &str = "music-wheel-loaded";

pub struct MusicWheelBenchFixture {
    entries: Vec<MusicWheelEntry>,
    song_text_color_overrides: HashMap<usize, [f32; 4]>,
    song_has_edit_ptrs: HashSet<usize>,
    selected_index: usize,
    position_offset_from_selection: f32,
    selection_animation_timer: f32,
    selection_animation_beat: f32,
    preferred_difficulty_index: usize,
    show_grades: bool,
    show_lamps: bool,
    itl_rank_mode: crate::config::SelectMusicItlRankMode,
    itl_wheel_mode: crate::config::SelectMusicItlWheelMode,
    song_select_bg_mode: crate::config::SelectMusicSongSelectBgMode,
}

impl MusicWheelBenchFixture {
    pub fn push(&self, actors: &mut Vec<Actor>) {
        music_wheel::push(
            actors,
            MusicWheelParams {
                entries: &self.entries,
                selected_index: self.selected_index,
                position_offset_from_selection: self.position_offset_from_selection,
                selection_animation_timer: self.selection_animation_timer,
                selection_animation_beat: self.selection_animation_beat,
                color_pack_headers: true,
                selected_charts: [None, None],
                preferred_difficulty_index: [self.preferred_difficulty_index; 2],
                song_box_color: None,
                song_text_color: Some([0.95, 0.96, 1.0, 1.0]),
                song_text_color_overrides: Some(&self.song_text_color_overrides),
                song_has_edit_ptrs: Some(&self.song_has_edit_ptrs),
                show_music_wheel_grades: self.show_grades,
                show_music_wheel_lamps: self.show_lamps,
                itl_rank_mode: self.itl_rank_mode,
                itl_wheel_mode: self.itl_wheel_mode,
                song_select_bg_mode: self.song_select_bg_mode,
                expanded_pack_name: None,
                allow_online_fetch: false,
                new_pack_names: None,
                pack_sync_prefs: None,
                default_sync_offset: crate::config::DefaultSyncOffset::Null,
            },
        );
    }

    pub fn build(&self) -> Vec<Actor> {
        let mut actors = Vec::new();
        self.push(&mut actors);
        actors
    }
}

pub fn fixture() -> MusicWheelBenchFixture {
    let mut entries = Vec::with_capacity(36);
    let mut song_text_color_overrides = HashMap::with_capacity(10);
    let mut song_has_edit_ptrs = HashSet::with_capacity(12);
    let pack_names = ["Stamina Lab", "Tech Alley", "Groove Works", "Night Shift"];

    for (pack_idx, pack_name) in pack_names.iter().enumerate() {
        entries.push(MusicWheelEntry::PackHeader {
            name: (*pack_name).to_string(),
            original_index: pack_idx,
            banner_path: None,
            song_count: 7,
        });

        for song_idx in 0..7 {
            let song = bench_song(pack_idx, song_idx);
            let song_ptr = Arc::as_ptr(&song) as usize;
            if song_idx % 2 == 0 {
                song_text_color_overrides
                    .insert(song_ptr, [0.86 + song_idx as f32 * 0.01, 0.94, 1.0, 1.0]);
            }
            if song.charts.iter().any(|chart| {
                chart.chart_type.eq_ignore_ascii_case("dance-single")
                    && chart.difficulty.eq_ignore_ascii_case("edit")
            }) {
                song_has_edit_ptrs.insert(song_ptr);
            }
            entries.push(MusicWheelEntry::Song(song));
        }
    }

    MusicWheelBenchFixture {
        entries,
        song_text_color_overrides,
        song_has_edit_ptrs,
        selected_index: 11,
        position_offset_from_selection: 0.35,
        selection_animation_timer: 1.375,
        selection_animation_beat: 37.5,
        preferred_difficulty_index: 3,
        show_grades: false,
        show_lamps: false,
        itl_rank_mode: crate::config::SelectMusicItlRankMode::None,
        itl_wheel_mode: crate::config::SelectMusicItlWheelMode::Off,
        song_select_bg_mode: crate::config::SelectMusicSongSelectBgMode::Off,
    }
}

/// Feature-rich fixture that exercises the side-gated render paths (per-side
/// chart lookups, grade/lamp badges, ITL rank + wheel overlays, favorites,
/// ITL-unlock lock icons, song-select BG art). Joins P1 in Single style so
/// those blocks actually execute, and turns on every per-slot feature so the
/// relevant per-frame work is measured. Seeds local + GrooveStats grades,
/// favorites, ITL self-scores/ranks, and an "ITL Online <year> Unlocks" pack
/// so the grade/lamp, favorites, and lock-icon paths all render real actors.
pub fn loaded_fixture() -> MusicWheelBenchFixture {
    use deadsync_profile::{PlayStyle, PlayerSide};

    const BENCH_ITL_API_KEY: &str = "bench-itl-api-key";

    crate::game::profile::set_session_play_style(PlayStyle::Single);
    crate::game::profile::set_session_joined(true, false);
    crate::game::profile::set_groovestats_credentials_for_side(
        PlayerSide::P1,
        BENCH_ITL_API_KEY,
        "BenchPlayer",
    );
    let profile_id =
        crate::game::profile::active_local_profile_id_for_side(PlayerSide::P1).unwrap_or_default();

    let mut entries = Vec::with_capacity(48);
    let mut song_text_color_overrides = HashMap::with_capacity(12);
    let mut song_has_edit_ptrs = HashSet::with_capacity(12);
    // The ITL-unlock pack is placed first so its songs fall inside the visible
    // wheel window around the selected index, exercising the lock-icon path.
    let pack_names = [
        "ITL Online 2026 Unlocks",
        "Stamina Lab",
        "Tech Alley",
        "Groove Works",
        "Night Shift",
    ];

    for (pack_idx, pack_name) in pack_names.iter().enumerate() {
        entries.push(MusicWheelEntry::PackHeader {
            name: (*pack_name).to_string(),
            original_index: pack_idx,
            banner_path: Some(PathBuf::from(format!(
                "songs/Bench/P{}/banner.png",
                pack_idx + 1
            ))),
            song_count: 7,
        });

        for song_idx in 0..7 {
            let song = bench_song_loaded(pack_name, pack_idx, song_idx);
            let song_ptr = Arc::as_ptr(&song) as usize;
            if song_idx % 2 == 0 {
                song_text_color_overrides
                    .insert(song_ptr, [0.86 + song_idx as f32 * 0.01, 0.94, 1.0, 1.0]);
            }
            if song.charts.iter().any(|chart| {
                chart.chart_type.eq_ignore_ascii_case("dance-single")
                    && chart.difficulty.eq_ignore_ascii_case("edit")
            }) {
                song_has_edit_ptrs.insert(song_ptr);
            }
            for (chart_idx, chart) in song.charts.iter().enumerate() {
                let seed = pack_idx * 37 + song_idx * 5 + chart_idx;
                // Seed local + GrooveStats grades/lamps so the per-slot grade
                // badge + lamp quad actors render (and the merge in
                // get_cached_score_for_side picks a real entry).
                crate::game::scores::seed_session_local_itg_score(
                    &profile_id,
                    chart.short_hash.as_str(),
                    bench_cached_score(seed),
                );
                if chart_idx % 2 == 1 {
                    crate::game::scores::seed_session_gs_score(
                        &profile_id,
                        chart.short_hash.as_str(),
                        bench_cached_score(seed + 3),
                    );
                }
                // Seed an ITL self-score + tournament rank for every chart hash
                // so whichever chart the wheel resolves per slot renders the ITL
                // rank (Header font) and wheel-score (Numbers font) text actors.
                let ex_hundredths = 8800 + ((song_idx * 5 + chart_idx) as u32 * 53) % 1200;
                let rank = 1 + ((pack_idx * 7 + song_idx) as u32 * 11 + chart_idx as u32) % 750;
                crate::game::scores::seed_session_online_itl_self_score(
                    BENCH_ITL_API_KEY,
                    chart.short_hash.as_str(),
                    ex_hundredths,
                );
                crate::game::scores::seed_session_online_itl_self_rank(
                    BENCH_ITL_API_KEY,
                    chart.short_hash.as_str(),
                    rank,
                );
                // Favorite roughly half the songs (P1) so the heart icon path
                // runs with both hits and misses.
                if song_idx % 2 == 0 {
                    crate::game::profile::seed_session_favorite(
                        PlayerSide::P1,
                        chart.short_hash.as_str(),
                    );
                }
            }
            entries.push(MusicWheelEntry::Song(song));
        }
    }

    // Mark a subset of the ITL-unlock pack's song folders as unlocked so the
    // lock-icon path exercises both the locked (icon emitted) and unlocked
    // (skipped) branches.
    let unlock_song_dirs: Vec<String> = (0..7)
        .filter(|song_idx| song_idx % 3 == 0)
        .map(|song_idx| song_base(0, song_idx))
        .collect();
    let unlock_refs: Vec<&str> = unlock_song_dirs.iter().map(String::as_str).collect();
    crate::game::scores::seed_session_itl_unlock_folders(&profile_id, &unlock_refs);

    MusicWheelBenchFixture {
        entries,
        song_text_color_overrides,
        song_has_edit_ptrs,
        selected_index: 11,
        position_offset_from_selection: 0.35,
        selection_animation_timer: 1.375,
        selection_animation_beat: 37.5,
        preferred_difficulty_index: 3,
        show_grades: true,
        show_lamps: true,
        itl_rank_mode: crate::config::SelectMusicItlRankMode::Chart,
        itl_wheel_mode: crate::config::SelectMusicItlWheelMode::PointsAndScore,
        song_select_bg_mode: crate::config::SelectMusicSongSelectBgMode::Banner,
    }
}

/// Deterministic per-chart cached score with varied grade/lamp/judge fields so
/// the grade-badge and lamp-quad render branches are all exercised.
fn bench_cached_score(seed: usize) -> CachedScore {
    let grade = match seed % 6 {
        0 => Grade::Quint,
        1 => Grade::Tier01,
        2 => Grade::Tier02,
        3 => Grade::Tier04,
        4 => Grade::Tier07,
        _ => Grade::Tier11,
    };
    let lamp_index = match seed % 5 {
        0 => Some(0u8),
        1 => Some(2u8),
        2 => Some(4u8),
        3 => Some(7u8),
        _ => None,
    };
    let lamp_judge_count = match seed % 4 {
        0 => Some(1u8),
        1 => Some(6u8),
        _ => None,
    };
    CachedScore {
        grade,
        score_percent: 0.50 + (seed % 50) as f64 / 100.0,
        lamp_index,
        lamp_judge_count,
    }
}

fn bench_song(pack_idx: usize, song_idx: usize) -> Arc<SongData> {
    let has_subtitle = !song_idx.is_multiple_of(3);
    let has_edit = song_idx.is_multiple_of(3);
    let base = format!("P{}-{:02}", pack_idx + 1, song_idx + 1);
    let title = format!("Benchmark {base} Velocity");
    let subtitle = if has_subtitle {
        format!("Phase {}", (song_idx % 4) + 1)
    } else {
        String::new()
    };
    Arc::new(SongData {
        simfile_path: PathBuf::from(format!("songs/Bench/{base}.ssc")),
        title: title.clone(),
        subtitle: subtitle.clone(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: format!("Bench Artist {}", pack_idx + 1),
        genre: String::new(),
        banner_path: None,
        background_path: None,
        background_changes: Vec::new(),
        background_layer2_changes: Vec::new(),
        foreground_changes: Vec::new(),
        background_lua_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: song_idx.is_multiple_of(4),
        cdtitle_path: None,
        music_path: None,
        display_bpm: String::from("160"),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 160.0,
        max_bpm: 160.0,
        normalized_bpms: String::from("0.000=160.000"),
        music_length_seconds: 92.0 + song_idx as f32,
        first_second: 0.0,
        total_length_seconds: 92 + song_idx as i32,
        precise_last_second_seconds: 92.0 + song_idx as f32,
        charts: bench_charts(&base, has_edit),
    })
}

fn bench_charts(base: &str, has_edit: bool) -> Vec<ChartData> {
    let mut charts = Vec::with_capacity(2 + usize::from(has_edit));
    charts.push(bench_chart(base, "hard", 9));
    charts.push(bench_chart(base, "challenge", 13));
    if has_edit {
        charts.push(bench_chart(base, "edit", 14));
    }
    charts
}

fn song_base(pack_idx: usize, song_idx: usize) -> String {
    format!("P{}-{:02}", pack_idx + 1, song_idx + 1)
}

fn bench_song_loaded(pack_name: &str, pack_idx: usize, song_idx: usize) -> Arc<SongData> {
    let has_subtitle = !song_idx.is_multiple_of(3);
    let has_edit = song_idx.is_multiple_of(3);
    let base = song_base(pack_idx, song_idx);
    let title = format!("Benchmark {base} Velocity");
    let subtitle = if has_subtitle {
        format!("Phase {}", (song_idx % 4) + 1)
    } else {
        String::new()
    };
    // Three-level path (`songs/<pack>/<song>/<file>.ssc`) so that
    // `song_pack_and_dir_name` resolves the real pack + song folder, letting the
    // ITL-unlock lock-icon path activate for the unlock pack.
    Arc::new(SongData {
        simfile_path: PathBuf::from(format!("songs/{pack_name}/{base}/{base}.ssc")),
        title,
        subtitle,
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: format!("Bench Artist {}", pack_idx + 1),
        genre: String::new(),
        banner_path: Some(PathBuf::from(format!("songs/Bench/{base}/banner.png"))),
        background_path: Some(PathBuf::from(format!("songs/Bench/{base}/bg.png"))),
        background_changes: Vec::new(),
        background_layer2_changes: Vec::new(),
        foreground_changes: Vec::new(),
        background_lua_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: true,
        cdtitle_path: None,
        music_path: None,
        display_bpm: String::from("160"),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 160.0,
        max_bpm: 160.0,
        normalized_bpms: String::from("0.000=160.000"),
        music_length_seconds: 92.0 + song_idx as f32,
        first_second: 0.0,
        total_length_seconds: 92 + song_idx as i32,
        precise_last_second_seconds: 92.0 + song_idx as f32,
        charts: bench_charts_loaded(&base, has_edit),
    })
}

fn bench_charts_loaded(base: &str, has_edit: bool) -> Vec<ChartData> {
    let mut charts = Vec::with_capacity(5 + usize::from(has_edit));
    charts.push(bench_chart(base, "beginner", 2));
    charts.push(bench_chart(base, "easy", 5));
    charts.push(bench_chart(base, "medium", 7));
    charts.push(bench_chart(base, "hard", 9));
    charts.push(bench_chart(base, "challenge", 13));
    if has_edit {
        charts.push(bench_chart(base, "edit", 14));
    }
    for chart in &mut charts {
        chart.chart_name = String::from("7500 (P) + 12000 (S)");
    }
    charts
}

fn bench_chart(base: &str, difficulty: &str, meter: u32) -> ChartData {
    ChartData {
        chart_type: String::from("dance-single"),
        difficulty: difficulty.to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter,
        step_artist: String::new(),
        music_path: None,
        short_hash: format!("{base}-{difficulty}"),
        stats: ArrowStats {
            total_arrows: 0,
            left: 0,
            down: 0,
            up: 0,
            right: 0,
            total_steps: 0,
            jumps: 0,
            hands: 0,
            mines: 0,
            holds: 0,
            rolls: 0,
            lifts: 0,
            fakes: 0,
            holding: 0,
        },
        tech_counts: TechCounts {
            crossovers: 0,
            half_crossovers: 0,
            full_crossovers: 0,
            footswitches: 0,
            up_footswitches: 0,
            down_footswitches: 0,
            sideswitches: 0,
            jacks: 0,
            brackets: 0,
            doublesteps: 0,
        },
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
        min_bpm: 150.0,
        max_bpm: 150.0,
    }
}
