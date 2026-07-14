use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::tr;
use crate::assets::{FontRole, current_machine_font_key};
use crate::effects::{sfx, sfx_then};
use crate::rgba_const;
use crate::screens::components::{
    select_music::{music_wheel, screen_bars, select_pane, step_artist_bar},
    shared::{
        banner as shared_banner, gs_scorebox, mode_pads, timers, transitions, visual_style_bg,
    },
};
use crate::screens::input as screen_input;
use crate::screens::{Screen, ThemeEffect};
pub use crate::views::{CourseStagePlan, SelectedCoursePlan};
use crate::views::{
    MusicWheelRankSource, MusicWheelRuntimeRequest, MusicWheelRuntimeView, SelectCourseInitView,
    SelectCourseRuntimeView, SelectCourseScoreRequest, SelectCourseScoreView,
};
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::cache::{TextCache, cached_text};
use deadlib_present::color;
use deadlib_present::space::{
    is_wide, screen_center_x, screen_center_y, screen_height, screen_width,
};
use deadsync_chart::song::standard_difficulty_index;
use deadsync_chart::{ChartData, SongData};
use deadsync_input::{InputEvent, PadDir, VirtualAction};
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use deadsync_simfile::course::{
    self, COURSE_RATING_ORDER, CourseEntry, CourseFile, CourseSong, CourseTotals, Difficulty,
    SongSort, StepsSpec, add_chart_totals, course_difficulty_from_meters, course_meter,
    nearest_filled_slot, push_song_bpm_range, resolve_course_chart, resolve_entry_song,
    song_unique_key,
};
use deadsync_simfile::runtime_cache::get_course_cache;
use deadsync_simfile::runtime_cache::get_song_cache;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use twox_hash::XxHash64;

use super::select_music::MusicWheelEntry;

const TRANSITION_IN_DURATION: f32 = 0.5;
const TRANSITION_OUT_DURATION: f32 = 0.3;
const SHOW_OPTIONS_MESSAGE_SECONDS: f32 = 1.5;
const ENTERING_OPTIONS_FADE_OUT_SECONDS: f32 = 0.125;
const ENTERING_OPTIONS_HIBERNATE_SECONDS: f32 = 0.1;
const ENTERING_OPTIONS_FADE_IN_SECONDS: f32 = 0.125;
const ENTERING_OPTIONS_HOLD_SECONDS: f32 = 1.0;
const ENTERING_OPTIONS_TOTAL_SECONDS: f32 = ENTERING_OPTIONS_FADE_OUT_SECONDS
    + ENTERING_OPTIONS_HIBERNATE_SECONDS
    + ENTERING_OPTIONS_FADE_IN_SECONDS
    + ENTERING_OPTIONS_HOLD_SECONDS;
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(250);
const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(300);
const MUSIC_WHEEL_SWITCH_SECONDS: f32 = 0.10;
const MUSIC_WHEEL_SETTLE_MIN_SPEED: f32 = 0.2;
const MUSIC_WHEEL_HOLD_SPIN_SPEED_DEFAULT: f32 = 15.0;
const MUSIC_WHEEL_STOP_SPINDOWN_THRESHOLD: f32 = 0.25;
const BANNER_NATIVE_WIDTH: f32 = 418.0;
const BANNER_NATIVE_HEIGHT: f32 = 164.0;
const BANNER_UPDATE_DELAY_SECONDS: f32 = 0.01;
const COURSE_TRACKLIST_ROW_SPACING: f32 = 23.0;
const COURSE_TRACKLIST_SCROLL_STEP_SECONDS: f32 = 0.5;
const COURSE_TRACKLIST_SCROLL_END_PAUSE_SECONDS: f32 = 0.5;
const COURSE_TRACKLIST_TARGET_VISIBLE_ROWS: usize = 6;
const COURSE_TRACKLIST_SCROLL_MIN_ENTRIES: usize = 6;
const COURSE_RATING_VISIBLE_SLOTS: usize = 5;
const COURSE_TRACKLIST_RATING_BOX_W: f32 = 32.0;
const COURSE_TRACKLIST_RATING_BOX_H: f32 = 152.0;
// Manual tune knob for the whole course tracklist text block.
// Negative moves up, positive moves down.
const COURSE_TRACKLIST_TEXT_Y_OFFSET: f32 = 0.0;
const COURSE_TRACKLIST_TEXT_HEIGHT: f32 = 15.0;
const SL_EXIT_PROMPT_BG_ALPHA: f32 = 0.925;
const SL_EXIT_PROMPT_CHOICE_Y: f32 = 250.0;
const SL_EXIT_PROMPT_CHOICE_X_OFFSET: f32 = 100.0;
const SL_EXIT_PROMPT_PROMPT_Y_OFFSET: f32 = -70.0;
const SL_EXIT_PROMPT_PROMPT_ZOOM: f32 = 1.3;
const SL_EXIT_PROMPT_LABEL_ZOOM: f32 = 1.1;
const SL_EXIT_PROMPT_INFO_ZOOM: f32 = 0.825;
const SL_EXIT_PROMPT_INFO_Y_OFFSET: f32 = 30.0;
const SL_EXIT_PROMPT_ACTIVE_ZOOM: f32 = 1.1;
const SL_EXIT_PROMPT_INACTIVE_ZOOM: f32 = 0.5;
const SL_EXIT_PROMPT_CHOICE_TWEEN_SECONDS: f32 = 0.1;
const SL_EXIT_PROMPT_CHOICES_DELAY_SECONDS: f32 = 0.0;
const SL_EXIT_PROMPT_CHOICES_FADE_SECONDS: f32 = 0.15;

rgba_const!(UI_BOX_BG_COLOR, "#1E282F");
rgba_const!(COURSE_WHEEL_SONG_TEXT_COLOR, "#D77272");
rgba_const!(COURSE_WHEEL_RANDOM_TEXT_COLOR, "#FFFF00");
const TEXT_CACHE_LIMIT: usize = 4096;

thread_local! {
    static SCORE_PERCENT_CACHE: RefCell<TextCache<u64>> = RefCell::new(HashMap::with_capacity(1024));
    static UINT_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(1024));
}

#[inline(always)]
fn cached_u32_text(value: u32) -> Arc<str> {
    cached_text(&UINT_TEXT_CACHE, value, TEXT_CACHE_LIMIT, || {
        value.to_string()
    })
}

#[inline(always)]
fn unknown_text() -> Arc<str> {
    static UNKNOWN: OnceLock<Arc<str>> = OnceLock::new();
    UNKNOWN.get_or_init(|| Arc::<str>::from("?")).clone()
}

#[inline(always)]
fn placeholder_score_percent() -> Arc<str> {
    static UNKNOWN: OnceLock<Arc<str>> = OnceLock::new();
    UNKNOWN.get_or_init(|| Arc::<str>::from("??.??%")).clone()
}

#[inline(always)]
fn cached_score_percent_text(score_percent: f64) -> Arc<str> {
    let score = if score_percent.is_finite() {
        score_percent.clamp(0.0, 1.0) * 100.0
    } else {
        0.0
    };
    cached_text(
        &SCORE_PERCENT_CACHE,
        score.to_bits(),
        TEXT_CACHE_LIMIT,
        || format!("{score:.2}%"),
    )
}

#[derive(Clone, Debug)]
struct CourseSongEntry {
    title: String,
    difficulty: String,
    meter: Option<u32>,
    step_artist: String,
}

#[derive(Clone, Debug)]
struct CourseMeta {
    path: PathBuf,
    score_hash: String,
    name: String,
    scripter: String,
    description: String,
    banner_path: Option<PathBuf>,
    ratings: Vec<Option<CourseRatingMeta>>,
    default_rating_index: usize,
    min_bpm: Option<f64>,
    max_bpm: Option<f64>,
    total_length_seconds: i32,
    has_random_entries: bool,
    has_most_played_entries: bool,
}

#[derive(Clone, Debug)]
struct CourseRatingMeta {
    entries: Vec<CourseSongEntry>,
    totals: CourseTotals,
    rated_entry_count: usize,
    course_difficulty_name: String,
    course_stepchart_label: String,
    course_meter: Option<u32>,
    meter_sum: u32,
    meter_count: usize,
    min_bpm: Option<f64>,
    max_bpm: Option<f64>,
    total_length_seconds: i32,
    runtime_stages: Vec<CourseStagePlan>,
}

struct InitData {
    all_entries: Vec<MusicWheelEntry>,
    course_meta_by_path: HashMap<PathBuf, Arc<CourseMeta>>,
    course_text_color_overrides: HashMap<usize, [f32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum NavDirection {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum OutPromptState {
    None,
    PressStartForOptions { elapsed: f32 },
    EnteringOptions { elapsed: f32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ThreeKeyFocus {
    #[default]
    Wheel,
    Rating,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ExitPromptState {
    None,
    Active {
        elapsed: f32,
        active_choice: u8,
        switch_from: Option<u8>,
        switch_elapsed: f32,
    },
}

pub struct State {
    pub entries: Vec<MusicWheelEntry>,
    pub selected_index: usize,
    pub active_color_index: i32,
    pub selection_animation_timer: f32,
    pub wheel_offset_from_selection: f32,
    pub current_banner_key: String,
    pub session_elapsed: f32,
    music_wheel: MusicWheelRuntimeView,
    score_view: SelectCourseScoreView,

    all_entries: Vec<MusicWheelEntry>,
    course_meta_by_path: HashMap<PathBuf, Arc<CourseMeta>>,
    course_text_color_overrides: HashMap<usize, [f32; 4]>,
    bg: visual_style_bg::State,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    last_requested_banner_path: Option<PathBuf>,
    pub banner_high_quality_requested: bool,
    prev_selected_index: usize,
    time_since_selection_change: f32,
    out_prompt: OutPromptState,
    exit_prompt: ExitPromptState,
    selected_rating_index_by_path: HashMap<PathBuf, usize>,
    last_rating_nav_dir_p1: Option<PadDir>,
    last_rating_nav_time_p1: Option<Instant>,
    last_rating_nav_dir_p2: Option<PadDir>,
    last_rating_nav_time_p2: Option<Instant>,
    menu_lr_chord: screen_input::MenuLrChordTracker,
    menu_lr_undo: i8,
    three_key_focus: ThreeKeyFocus,
}

#[inline(always)]
fn song_dir_key(song: &SongData) -> Option<String> {
    song.simfile_path
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .map(|s| s.trim().to_ascii_lowercase())
}

fn build_song_lookup(
    played_chart_counts: &[(String, u32)],
) -> (
    HashMap<(String, String), Arc<SongData>>,
    HashMap<String, Arc<SongData>>,
    HashMap<String, Vec<Arc<SongData>>>,
    Vec<Arc<SongData>>,
    HashMap<String, u32>,
) {
    let song_cache = get_song_cache();
    let mut by_group_song: HashMap<(String, String), Arc<SongData>> = HashMap::new();
    let mut by_song: HashMap<String, Arc<SongData>> = HashMap::new();
    let mut songs_by_group: HashMap<String, Vec<Arc<SongData>>> = HashMap::new();
    let mut all_songs = Vec::new();
    let mut chart_to_song_key: HashMap<String, String> = HashMap::new();

    for pack in song_cache.iter() {
        let group_key = pack.group_name.trim().to_ascii_lowercase();
        for song in &pack.songs {
            let unique_song_key = song_unique_key(song);
            all_songs.push(song.clone());
            songs_by_group
                .entry(group_key.clone())
                .or_default()
                .push(song.clone());
            for chart in &song.charts {
                chart_to_song_key
                    .entry(chart.short_hash.clone())
                    .or_insert_with(|| unique_song_key.clone());
            }
            let Some(song_key) = song_dir_key(song) else {
                continue;
            };
            by_group_song.insert((group_key.clone(), song_key.clone()), song.clone());
            by_song.entry(song_key).or_insert_with(|| song.clone());
        }
    }

    drop(song_cache);

    let mut song_play_counts: HashMap<String, u32> = HashMap::new();
    for (chart_hash, plays) in played_chart_counts {
        if let Some(song_key) = chart_to_song_key.get(chart_hash.as_str()) {
            song_play_counts
                .entry(song_key.clone())
                .and_modify(|count| *count = count.saturating_add(*plays))
                .or_insert(*plays);
        }
    }

    (
        by_group_song,
        by_song,
        songs_by_group,
        all_songs,
        song_play_counts,
    )
}

#[inline(always)]
fn course_group_name(path: &Path) -> String {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| tr("SelectCourse", "CoursesGroup").to_string())
}

#[inline(always)]
fn course_name(path: &Path, course: &CourseFile) -> String {
    if course.name.trim().is_empty() {
        path.file_stem()
            .and_then(|n| n.to_str())
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| tr("SelectCourse", "UntitledCourse").to_string())
    } else {
        course.name.clone()
    }
}

#[inline(always)]
pub fn course_score_hash(course_path: &Path) -> String {
    let mut hasher = XxHash64::with_seed(0xC0_01_53_42_0A);
    hasher.write(course_path.to_string_lossy().as_bytes());
    format!("course-{:016x}", hasher.finish())
}

#[inline(always)]
fn course_steps_label(steps: &StepsSpec) -> String {
    match steps {
        StepsSpec::Difficulty(diff) => course::difficulty_label(*diff)
            .to_ascii_lowercase()
            .to_string(),
        StepsSpec::MeterRange { low, high } => format!("{low}-{high}"),
        StepsSpec::Unknown { raw } => {
            if raw.trim().is_empty() {
                "?".to_string()
            } else {
                raw.trim().to_string()
            }
        }
    }
}

#[inline(always)]
fn course_entry_song_label(entry: &CourseEntry) -> String {
    match &entry.song {
        CourseSong::Fixed { song, .. } => song.clone(),
        CourseSong::RandomAny => tr("SelectCourse", "RandomLabel").to_string(),
        CourseSong::RandomWithinGroup { group } => format!("{group}/*"),
        CourseSong::SortPick { sort, index } => {
            let rank = index.saturating_add(1).max(1);
            let prefix = match sort {
                SongSort::MostPlays => tr("SelectCourse", "BestPrefix"),
                SongSort::FewestPlays => tr("SelectCourse", "WorstPrefix"),
                SongSort::TopGrades => tr("SelectCourse", "TopGradesPrefix"),
                SongSort::LowestGrades => tr("SelectCourse", "LowestGradesPrefix"),
            };
            format!("{prefix}{rank}")
        }
        CourseSong::Unknown { raw } => raw.clone(),
    }
}

#[inline(always)]
fn course_stepchart_label(difficulty_name: &str, meter: Option<u32>) -> String {
    let idx = standard_difficulty_index(difficulty_name).unwrap_or(2);
    let display = color::DISPLAY_DIFFICULTY_NAMES[idx];
    if let Some(meter) = meter {
        format!("{display} {meter}")
    } else {
        display.to_string()
    }
}

#[inline(always)]
fn chart_step_artist(chart: &ChartData) -> String {
    if chart.difficulty.eq_ignore_ascii_case("edit") && !chart.description.trim().is_empty() {
        chart.description.clone()
    } else if !chart.step_artist.trim().is_empty() {
        chart.step_artist.clone()
    } else {
        tr("SelectCourse", "UnknownStepArtist").to_string()
    }
}

fn make_course_song(meta: &CourseMeta) -> SongData {
    SongData {
        simfile_path: meta.path.clone(),
        title: meta.name.clone(),
        subtitle: String::new(),
        translit_title: meta.name.clone(),
        translit_subtitle: String::new(),
        artist: if meta.scripter.trim().is_empty() {
            tr("SelectCourse", "CourseScripter").to_string()
        } else {
            meta.scripter.clone()
        },
        genre: String::new(),
        banner_path: meta.banner_path.clone(),
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
        min_bpm: meta.min_bpm.unwrap_or(0.0),
        max_bpm: meta.max_bpm.unwrap_or(meta.min_bpm.unwrap_or(0.0)),
        normalized_bpms: String::new(),
        music_length_seconds: meta.total_length_seconds.max(0) as f32,
        first_second: 0.0,
        total_length_seconds: meta.total_length_seconds.max(0),
        precise_last_second_seconds: meta.total_length_seconds.max(0) as f32,
        charts: Vec::new(),
    }
}

fn build_init_data(init_view: &SelectCourseInitView) -> InitData {
    let translated_titles = crate::config::get().translated_titles;
    let target_chart_type = profile::get_session_play_style().chart_type();
    let (by_group_song, by_song, songs_by_group, all_songs, song_play_counts) =
        build_song_lookup(&init_view.played_chart_counts);
    let course_cache = get_course_cache();

    let mut grouped: HashMap<String, Vec<Arc<CourseMeta>>> = HashMap::new();
    let mut course_meta_by_path: HashMap<PathBuf, Arc<CourseMeta>> = HashMap::new();

    for (path, course) in course_cache.iter() {
        let mut total_seconds = 0i32;
        let mut min_bpm = None;
        let mut max_bpm = None;
        let mut used_song_keys = HashSet::new();
        let mut has_random_entries = false;
        let mut has_most_played_entries = false;
        let random_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0_u64, |d| d.as_nanos() as u64);

        for (entry_idx, entry) in course.entries.iter().enumerate() {
            if matches!(
                &entry.song,
                CourseSong::RandomAny | CourseSong::RandomWithinGroup { .. }
            ) {
                has_random_entries = true;
            }
            if matches!(
                &entry.song,
                CourseSong::SortPick {
                    sort: SongSort::MostPlays,
                    ..
                }
            ) {
                has_most_played_entries = true;
            }

            let resolved = resolve_entry_song(
                path,
                entry_idx,
                random_seed,
                entry,
                &by_group_song,
                &by_song,
                &all_songs,
                &songs_by_group,
                &song_play_counts,
                &used_song_keys,
                target_chart_type,
                Difficulty::Medium,
            );

            if let Some(song_data) = resolved.as_ref() {
                used_song_keys.insert(song_unique_key(song_data));
                let len = if song_data.music_length_seconds > 0.0 {
                    song_data.music_length_seconds.round() as i32
                } else {
                    song_data.total_length_seconds.max(0)
                };
                total_seconds = total_seconds.saturating_add(len.max(0));
                push_song_bpm_range(&mut min_bpm, &mut max_bpm, song_data);
            }
        }

        let preferred_default_idx = course_difficulty_from_meters(course)
            .and_then(|(difficulty_name, _)| {
                COURSE_RATING_ORDER.iter().position(|diff| {
                    course::difficulty_label(*diff).eq_ignore_ascii_case(difficulty_name)
                })
            })
            .unwrap_or(Difficulty::Medium as usize);
        let preferred_default_diff = COURSE_RATING_ORDER[preferred_default_idx];
        let mut available_course_diffs: Vec<Difficulty> = COURSE_RATING_ORDER
            .iter()
            .copied()
            .filter(|diff| course_meter(course, *diff).is_some_and(|meter| meter >= 0))
            .collect();
        if has_random_entries && available_course_diffs.len() <= 1 {
            available_course_diffs = COURSE_RATING_ORDER.to_vec();
        }
        if available_course_diffs.is_empty() {
            available_course_diffs.push(preferred_default_diff);
        }

        let mut ratings: Vec<Option<CourseRatingMeta>> = vec![None; COURSE_RATING_ORDER.len()];
        for course_diff in available_course_diffs {
            let mut entries = Vec::with_capacity(course.entries.len());
            let mut runtime_stages = Vec::with_capacity(course.entries.len());
            let mut totals = CourseTotals::default();
            let mut rated_entry_count = 0usize;
            let mut meter_sum = 0u32;
            let mut meter_count = 0usize;
            let mut rating_used_song_keys = HashSet::new();
            let mut rating_total_seconds = 0i32;
            let mut rating_min_bpm = None;
            let mut rating_max_bpm = None;

            for (entry_idx, entry) in course.entries.iter().enumerate() {
                let mut title = course_entry_song_label(entry);
                let mut difficulty = course_steps_label(&entry.steps);
                let mut meter = None;
                let mut step_artist = if course.scripter.trim().is_empty() {
                    tr("SelectCourse", "UnknownStepArtist").to_string()
                } else {
                    course.scripter.clone()
                };
                let resolved = resolve_entry_song(
                    path,
                    entry_idx,
                    random_seed,
                    entry,
                    &by_group_song,
                    &by_song,
                    &all_songs,
                    &songs_by_group,
                    &song_play_counts,
                    &rating_used_song_keys,
                    target_chart_type,
                    course_diff,
                );
                if let Some(song_data) = resolved.as_ref() {
                    rating_used_song_keys.insert(song_unique_key(song_data));
                    title = song_data.display_full_title(translated_titles);
                    let len = if song_data.music_length_seconds > 0.0 {
                        song_data.music_length_seconds.round() as i32
                    } else {
                        song_data.total_length_seconds.max(0)
                    };
                    rating_total_seconds = rating_total_seconds.saturating_add(len.max(0));
                    push_song_bpm_range(&mut rating_min_bpm, &mut rating_max_bpm, song_data);
                    if let Some(chart) =
                        resolve_course_chart(song_data, entry, target_chart_type, course_diff)
                    {
                        difficulty = chart.difficulty.to_ascii_lowercase();
                        meter = Some(chart.meter);
                        step_artist = chart_step_artist(chart);
                        runtime_stages.push(CourseStagePlan {
                            song: song_data.clone(),
                            chart_hash: chart.short_hash.clone(),
                        });
                        add_chart_totals(&mut totals, chart);
                        rated_entry_count = rated_entry_count.saturating_add(1);
                        meter_sum = meter_sum.saturating_add(chart.meter);
                        meter_count = meter_count.saturating_add(1);
                    }
                }
                entries.push(CourseSongEntry {
                    title,
                    difficulty,
                    meter,
                    step_artist,
                });
            }

            let explicit_meter = course_meter(course, course_diff)
                .filter(|v| *v >= 0)
                .map(|v| v as u32);
            if rated_entry_count == 0
                && explicit_meter.is_none()
                && course_diff != Difficulty::Medium
            {
                continue;
            }

            let course_meter = explicit_meter.or_else(|| {
                if meter_count > 0 {
                    Some((meter_sum as f32 / meter_count as f32).round() as u32)
                } else {
                    None
                }
            });
            let course_difficulty_name = course::difficulty_label(course_diff).to_string();
            let course_stepchart_label =
                course_stepchart_label(course_difficulty_name.as_str(), course_meter);

            ratings[course_diff as usize] = Some(CourseRatingMeta {
                entries,
                totals,
                rated_entry_count,
                course_difficulty_name,
                course_stepchart_label,
                course_meter,
                meter_sum,
                meter_count,
                min_bpm: rating_min_bpm,
                max_bpm: rating_max_bpm,
                total_length_seconds: rating_total_seconds.max(0),
                runtime_stages,
            });
        }

        let group_name = course_group_name(path);
        let default_rating_index =
            nearest_filled_slot(&ratings, preferred_default_idx).unwrap_or(preferred_default_idx);
        let (meta_min_bpm, meta_max_bpm, meta_total_length_seconds) = ratings
            .get(default_rating_index)
            .and_then(Option::as_ref)
            .map(|rating| {
                (
                    rating.min_bpm,
                    rating.max_bpm,
                    rating.total_length_seconds.max(0),
                )
            })
            .unwrap_or((min_bpm, max_bpm, total_seconds.max(0)));
        let meta = Arc::new(CourseMeta {
            path: path.clone(),
            score_hash: course_score_hash(path),
            name: course_name(path, course),
            scripter: course.scripter.clone(),
            description: course.description.clone(),
            banner_path: course::resolve_course_banner_path(path, &course.banner),
            ratings,
            default_rating_index,
            min_bpm: meta_min_bpm,
            max_bpm: meta_max_bpm,
            total_length_seconds: meta_total_length_seconds,
            has_random_entries,
            has_most_played_entries,
        });

        grouped.entry(group_name).or_default().push(meta.clone());
        course_meta_by_path.insert(meta.path.clone(), meta);
    }

    let mut all_courses: Vec<Arc<CourseMeta>> = grouped.into_values().flatten().collect();
    all_courses.sort_by_cached_key(|c| c.name.to_ascii_lowercase());

    let mut all_entries = Vec::with_capacity(all_courses.len());
    let mut course_text_color_overrides = HashMap::with_capacity(all_courses.len());
    for meta in all_courses {
        let song_stub = Arc::new(make_course_song(&meta));
        if meta.has_random_entries {
            course_text_color_overrides.insert(
                Arc::as_ptr(&song_stub) as usize,
                COURSE_WHEEL_RANDOM_TEXT_COLOR,
            );
        }
        all_entries.push(MusicWheelEntry::Song(song_stub));
    }

    InitData {
        all_entries,
        course_meta_by_path,
        course_text_color_overrides,
    }
}

fn rebuild_displayed_entries(state: &mut State) {
    let selected_path = match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(song)) => Some(song.simfile_path.clone()),
        _ => None,
    };
    let cfg = crate::config::get();
    state.entries.clear();
    state.entries.reserve(state.all_entries.len());
    for entry in &state.all_entries {
        let include = match entry {
            MusicWheelEntry::Song(song) => state
                .course_meta_by_path
                .get(&song.simfile_path)
                .is_none_or(|meta| {
                    (cfg.show_random_courses || !meta.has_random_entries)
                        && (cfg.show_most_played_courses || !meta.has_most_played_entries)
                }),
            _ => true,
        };
        if include {
            state.entries.push(entry.clone());
        }
    }
    if state.entries.is_empty() {
        state.selected_index = 0;
        state.prev_selected_index = 0;
        state.wheel_offset_from_selection = 0.0;
        state.time_since_selection_change = 0.0;
        state.last_requested_banner_path = None;
        state.banner_high_quality_requested = false;
        state.last_rating_nav_dir_p1 = None;
        state.last_rating_nav_time_p1 = None;
        state.last_rating_nav_dir_p2 = None;
        state.last_rating_nav_time_p2 = None;
        return;
    }
    if let Some(path) = selected_path
        && let Some(index) = state.entries.iter().position(
            |entry| matches!(entry, MusicWheelEntry::Song(song) if song.simfile_path == path),
        )
    {
        state.selected_index = index;
    }
    state.selected_index = state
        .selected_index
        .min(state.entries.len().saturating_sub(1));
    state.prev_selected_index = state.selected_index;
    state.wheel_offset_from_selection = 0.0;
    state.time_since_selection_change = 0.0;
    state.last_requested_banner_path = None;
    state.banner_high_quality_requested = false;
    state.last_rating_nav_dir_p1 = None;
    state.last_rating_nav_time_p1 = None;
    state.last_rating_nav_dir_p2 = None;
    state.last_rating_nav_time_p2 = None;
}

fn selected_course_meta(state: &State) -> Option<Arc<CourseMeta>> {
    let MusicWheelEntry::Song(song) = state.entries.get(state.selected_index)? else {
        return None;
    };
    state.course_meta_by_path.get(&song.simfile_path).cloned()
}

pub fn restore_selection_for_course(
    state: &mut State,
    course_path: &Path,
    course_difficulty_name: Option<&str>,
) -> bool {
    let Some(index) = state.entries.iter().position(
        |entry| matches!(entry, MusicWheelEntry::Song(song) if song.simfile_path == course_path),
    ) else {
        return false;
    };
    state.selected_index = index;
    state.prev_selected_index = index;
    state.wheel_offset_from_selection = 0.0;
    state.time_since_selection_change = 0.0;

    if let Some(meta) = selected_course_meta(state) {
        if let Some(diff_name) = course_difficulty_name
            && let Some(slot_idx) = meta.ratings.iter().position(|slot| {
                slot.as_ref().is_some_and(|rating| {
                    rating
                        .course_difficulty_name
                        .eq_ignore_ascii_case(diff_name)
                })
            })
        {
            set_selected_course_rating_index(state, &meta, slot_idx);
        } else {
            let idx = selected_course_rating_index(state, &meta);
            state
                .selected_rating_index_by_path
                .insert(meta.path.clone(), idx);
        }
    }

    state.last_rating_nav_dir_p1 = None;
    state.last_rating_nav_time_p1 = None;
    state.last_rating_nav_dir_p2 = None;
    state.last_rating_nav_time_p2 = None;
    true
}

#[inline(always)]
fn selected_course_rating_index(state: &State, meta: &CourseMeta) -> usize {
    let len = meta.ratings.len();
    if len == 0 {
        return 0;
    }
    let preferred = state
        .selected_rating_index_by_path
        .get(meta.path.as_path())
        .copied()
        .unwrap_or(meta.default_rating_index)
        .min(len.saturating_sub(1));
    nearest_filled_slot(&meta.ratings, preferred).unwrap_or(preferred)
}

#[inline(always)]
fn selected_course_rating<'a>(state: &State, meta: &'a CourseMeta) -> Option<&'a CourseRatingMeta> {
    meta.ratings
        .get(selected_course_rating_index(state, meta))
        .and_then(Option::as_ref)
}

#[inline(always)]
fn set_selected_course_rating_index(state: &mut State, meta: &CourseMeta, idx: usize) {
    if meta.ratings.is_empty() {
        return;
    }
    let preferred = idx.min(meta.ratings.len().saturating_sub(1));
    let selected = nearest_filled_slot(&meta.ratings, preferred).unwrap_or(preferred);
    state
        .selected_rating_index_by_path
        .insert(meta.path.clone(), selected);
}

pub fn selected_course_plan(state: &State) -> Option<SelectedCoursePlan> {
    let meta = selected_course_meta(state)?;
    let rating = selected_course_rating(state, &meta)?;
    if rating.runtime_stages.is_empty() {
        return None;
    }
    Some(SelectedCoursePlan {
        path: meta.path.clone(),
        name: meta.name.clone(),
        banner_path: meta.banner_path.clone(),
        score_hash: meta.score_hash.clone(),
        song_stub: Arc::new(make_course_song(&meta)),
        course_difficulty_name: rating.course_difficulty_name.clone(),
        course_meter: rating.course_meter,
        course_stepchart_label: rating.course_stepchart_label.clone(),
        stages: rating.runtime_stages.clone(),
    })
}

#[inline(always)]
fn selected_banner_path(state: &State) -> Option<PathBuf> {
    match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(song)) => song.banner_path.clone(),
        Some(MusicWheelEntry::PackHeader { banner_path, .. }) => banner_path.clone(),
        None => None,
    }
}

fn restore_last_course(state: &mut State) {
    let profile_data = profile::get();
    let last_played = profile_data.last_played_course(profile::get_session_play_style());
    let Some(path) = last_played.course_path.as_deref() else {
        return;
    };
    restore_selection_for_course(
        state,
        Path::new(path),
        last_played.difficulty_name.as_deref(),
    );
}

pub fn init(init_view: SelectCourseInitView) -> State {
    let init = build_init_data(&init_view);
    let mut state = State {
        entries: Vec::new(),
        selected_index: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        selection_animation_timer: 0.0,
        wheel_offset_from_selection: 0.0,
        current_banner_key: "banner1.png".to_string(),
        session_elapsed: 0.0,
        music_wheel: MusicWheelRuntimeView::default(),
        score_view: SelectCourseScoreView::default(),
        all_entries: init.all_entries,
        course_meta_by_path: init.course_meta_by_path,
        course_text_color_overrides: init.course_text_color_overrides,
        bg: visual_style_bg::State::new(),
        nav_key_held_direction: None,
        nav_key_held_since: None,
        last_requested_banner_path: None,
        banner_high_quality_requested: false,
        prev_selected_index: 0,
        time_since_selection_change: 0.0,
        out_prompt: OutPromptState::None,
        exit_prompt: ExitPromptState::None,
        selected_rating_index_by_path: HashMap::new(),
        last_rating_nav_dir_p1: None,
        last_rating_nav_time_p1: None,
        last_rating_nav_dir_p2: None,
        last_rating_nav_time_p2: None,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        menu_lr_undo: 0,
        three_key_focus: ThreeKeyFocus::Wheel,
    };
    rebuild_displayed_entries(&mut state);
    restore_last_course(&mut state);
    state
}

#[inline(always)]
fn music_wheel_settle_offset(state: &mut State, dt: f32) {
    if dt <= 0.0 || state.wheel_offset_from_selection == 0.0 {
        return;
    }
    let off = state.wheel_offset_from_selection;
    let speed = MUSIC_WHEEL_SETTLE_MIN_SPEED + off.abs() / MUSIC_WHEEL_SWITCH_SECONDS;
    if off > 0.0 {
        state.wheel_offset_from_selection = (off - speed * dt).max(0.0);
    } else {
        state.wheel_offset_from_selection = (off + speed * dt).min(0.0);
    }
}

#[inline(always)]
fn music_wheel_hold_spin_speed() -> f32 {
    let configured = crate::config::get().music_wheel_switch_speed;
    if configured == 0 {
        MUSIC_WHEEL_HOLD_SPIN_SPEED_DEFAULT
    } else {
        configured.max(1) as f32
    }
}

#[inline(always)]
fn music_wheel_change(state: &mut State, dist: isize) {
    if dist == 0 {
        return;
    }
    let len = state.entries.len();
    if len == 0 {
        state.selected_index = 0;
        state.wheel_offset_from_selection = 0.0;
        state.time_since_selection_change = 0.0;
        return;
    }
    if dist > 0 {
        state.selected_index = (state.selected_index + 1) % len;
        state.wheel_offset_from_selection += 1.0;
    } else {
        state.selected_index = (state.selected_index + len - 1) % len;
        state.wheel_offset_from_selection -= 1.0;
    }
    state.time_since_selection_change = 0.0;
}

#[inline(always)]
fn music_wheel_update_hold_scroll(state: &mut State, dt: f32, dir: NavDirection) {
    if dt <= 0.0 {
        return;
    }
    let moving = match dir {
        NavDirection::Left => -1.0,
        NavDirection::Right => 1.0,
    };
    let hold_spin_speed = music_wheel_hold_spin_speed();
    state.wheel_offset_from_selection -= hold_spin_speed * moving * dt;
    state.wheel_offset_from_selection = state.wheel_offset_from_selection.clamp(-1.0, 1.0);

    let off = state.wheel_offset_from_selection;
    let passed = (moving < 0.0 && off >= 0.0) || (moving > 0.0 && off <= 0.0);
    if passed {
        music_wheel_change(state, if moving < 0.0 { -1 } else { 1 });
    }
}

fn handle_wheel_dir(state: &mut State, dir: PadDir, pressed: bool, ts: Instant) -> ThemeEffect {
    match (dir, pressed) {
        (PadDir::Left, true) => {
            if state.nav_key_held_direction == Some(NavDirection::Left) {
                return ThemeEffect::None;
            }
            music_wheel_change(state, -1);
            state.nav_key_held_direction = Some(NavDirection::Left);
            state.nav_key_held_since = Some(ts);
        }
        (PadDir::Right, true) => {
            if state.nav_key_held_direction == Some(NavDirection::Right) {
                return ThemeEffect::None;
            }
            music_wheel_change(state, 1);
            state.nav_key_held_direction = Some(NavDirection::Right);
            state.nav_key_held_since = Some(ts);
        }
        (PadDir::Left, false) => {
            if state.nav_key_held_direction == Some(NavDirection::Left) {
                let moving_started = state
                    .nav_key_held_since
                    .is_some_and(|t| ts.duration_since(t) >= NAV_INITIAL_HOLD_DELAY);
                if moving_started
                    && state.wheel_offset_from_selection.abs() < MUSIC_WHEEL_STOP_SPINDOWN_THRESHOLD
                {
                    music_wheel_change(state, -1);
                }
                state.nav_key_held_direction = None;
                state.nav_key_held_since = None;
            }
        }
        (PadDir::Right, false) => {
            if state.nav_key_held_direction == Some(NavDirection::Right) {
                let moving_started = state
                    .nav_key_held_since
                    .is_some_and(|t| ts.duration_since(t) >= NAV_INITIAL_HOLD_DELAY);
                if moving_started
                    && state.wheel_offset_from_selection.abs() < MUSIC_WHEEL_STOP_SPINDOWN_THRESHOLD
                {
                    music_wheel_change(state, 1);
                }
                state.nav_key_held_direction = None;
                state.nav_key_held_since = None;
            }
        }
        _ => {}
    }
    ThemeEffect::None
}

fn handle_rating_dir(
    state: &mut State,
    side: profile_data::PlayerSide,
    dir: PadDir,
    pressed: bool,
    timestamp: Instant,
) -> ThemeEffect {
    if !pressed || !matches!(dir, PadDir::Up | PadDir::Down) {
        return ThemeEffect::None;
    }
    let (last_dir, last_time) = match side {
        profile_data::PlayerSide::P1 => (
            &mut state.last_rating_nav_dir_p1,
            &mut state.last_rating_nav_time_p1,
        ),
        profile_data::PlayerSide::P2 => (
            &mut state.last_rating_nav_dir_p2,
            &mut state.last_rating_nav_time_p2,
        ),
    };
    if *last_dir != Some(dir)
        || last_time.is_none_or(|t| timestamp.duration_since(t) >= DOUBLE_TAP_WINDOW)
    {
        *last_dir = Some(dir);
        *last_time = Some(timestamp);
        return ThemeEffect::None;
    }
    *last_dir = None;
    *last_time = None;

    let Some(meta) = selected_course_meta(state) else {
        return ThemeEffect::None;
    };
    let available = meta.ratings.iter().filter(|r| r.is_some()).count();
    if available <= 1 {
        return ThemeEffect::None;
    }
    let current = selected_course_rating_index(state, &meta);
    let next = match dir {
        PadDir::Up => (0..current).rev().find(|&idx| meta.ratings[idx].is_some()),
        PadDir::Down => {
            ((current + 1)..meta.ratings.len()).find(|&idx| meta.ratings[idx].is_some())
        }
        _ => None,
    };
    if let Some(next) = next {
        set_selected_course_rating_index(state, &meta, next);
        return sfx(if matches!(dir, PadDir::Up) {
            "assets/sounds/easier.ogg"
        } else {
            "assets/sounds/harder.ogg"
        });
    }
    ThemeEffect::None
}

#[inline(always)]
fn clear_wheel_hold(state: &mut State) {
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
}

#[inline(always)]
fn selected_course_has_multiple_ratings(state: &State) -> bool {
    selected_course_meta(state)
        .map(|meta| meta.ratings.iter().filter(|r| r.is_some()).count() > 1)
        .unwrap_or(false)
}

fn shift_selected_course_rating(state: &mut State, delta: isize) -> Option<&'static str> {
    if delta == 0 {
        return None;
    }
    let Some(meta) = selected_course_meta(state) else {
        return None;
    };
    let available = meta.ratings.iter().filter(|r| r.is_some()).count();
    if available <= 1 {
        return None;
    }
    let current = selected_course_rating_index(state, &meta);
    let next = if delta < 0 {
        (0..current).rev().find(|&idx| meta.ratings[idx].is_some())
    } else {
        ((current + 1)..meta.ratings.len()).find(|&idx| meta.ratings[idx].is_some())
    };
    let Some(next) = next else {
        return None;
    };
    set_selected_course_rating_index(state, &meta, next);
    Some(if delta < 0 {
        "assets/sounds/easier.ogg"
    } else {
        "assets/sounds/harder.ogg"
    })
}

pub fn handle_confirm(state: &mut State) -> ThemeEffect {
    if state.out_prompt != OutPromptState::None {
        return ThemeEffect::None;
    }
    if state.entries.is_empty() {
        return sfx("assets/sounds/expand.ogg");
    }
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.menu_lr_undo = 0;
    state.three_key_focus = ThreeKeyFocus::Wheel;

    match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(_)) => {
            state.out_prompt = OutPromptState::PressStartForOptions { elapsed: 0.0 };
            sfx("assets/sounds/start.ogg")
        }
        _ => ThemeEffect::None,
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    let three_key_action = screen_input::three_key_menu_action(&mut state.menu_lr_chord, ev);
    if screen_input::dedicated_three_key_nav_enabled()
        && matches!(state.three_key_focus, ThreeKeyFocus::Wheel)
    {
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left
                if !ev.pressed =>
            {
                state.menu_lr_undo = 0;
                return handle_wheel_dir(state, PadDir::Left, false, ev.timestamp);
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right
                if !ev.pressed =>
            {
                state.menu_lr_undo = 0;
                return handle_wheel_dir(state, PadDir::Right, false, ev.timestamp);
            }
            _ => {}
        }
    }
    if state.exit_prompt != ExitPromptState::None {
        if let Some((_, nav)) = three_key_action {
            return match nav {
                screen_input::ThreeKeyMenuAction::Prev | screen_input::ThreeKeyMenuAction::Next => {
                    let ExitPromptState::Active {
                        active_choice,
                        switch_from,
                        switch_elapsed,
                        ..
                    } = &mut state.exit_prompt
                    else {
                        return ThemeEffect::None;
                    };
                    let prev = *active_choice;
                    *active_choice = 1 - prev;
                    *switch_from = Some(prev);
                    *switch_elapsed = 0.0;
                    sfx("assets/sounds/change.ogg")
                }
                screen_input::ThreeKeyMenuAction::Cancel => {
                    state.exit_prompt = ExitPromptState::None;
                    sfx("assets/sounds/start.ogg")
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    let ExitPromptState::Active { active_choice, .. } = state.exit_prompt else {
                        return ThemeEffect::None;
                    };
                    state.exit_prompt = ExitPromptState::None;
                    if active_choice == 1 {
                        sfx_then(
                            "assets/sounds/start.ogg",
                            ThemeEffect::Navigate(Screen::Menu),
                        )
                    } else {
                        sfx("assets/sounds/start.ogg")
                    }
                }
            };
        }
        return handle_exit_prompt_input(state, ev);
    }

    if state.out_prompt != OutPromptState::None {
        let start_pressed = matches!(
            three_key_action,
            Some((_, screen_input::ThreeKeyMenuAction::Confirm))
        ) || (ev.pressed
            && matches!(ev.action, VirtualAction::p1_start | VirtualAction::p2_start));
        if start_pressed
            && matches!(
                state.out_prompt,
                OutPromptState::PressStartForOptions { .. }
            )
        {
            state.out_prompt = OutPromptState::EnteringOptions { elapsed: 0.0 };
            return sfx("assets/sounds/start.ogg");
        }
        return ThemeEffect::None;
    }

    if screen_input::dedicated_three_key_nav_enabled() {
        if let Some((_, nav)) = three_key_action {
            return match nav {
                screen_input::ThreeKeyMenuAction::Prev => {
                    if matches!(state.three_key_focus, ThreeKeyFocus::Rating) {
                        let sound = shift_selected_course_rating(state, -1);
                        state.menu_lr_undo = if sound.is_some() { 1 } else { 0 };
                        sound.map(sfx).unwrap_or(ThemeEffect::None)
                    } else {
                        state.menu_lr_undo = 1;
                        handle_wheel_dir(state, PadDir::Left, true, ev.timestamp)
                    }
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    if matches!(state.three_key_focus, ThreeKeyFocus::Rating) {
                        let sound = shift_selected_course_rating(state, 1);
                        state.menu_lr_undo = if sound.is_some() { -1 } else { 0 };
                        sound.map(sfx).unwrap_or(ThemeEffect::None)
                    } else {
                        state.menu_lr_undo = -1;
                        handle_wheel_dir(state, PadDir::Right, true, ev.timestamp)
                    }
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    state.menu_lr_undo = 0;
                    if matches!(state.three_key_focus, ThreeKeyFocus::Wheel)
                        && selected_course_has_multiple_ratings(state)
                    {
                        clear_wheel_hold(state);
                        state.three_key_focus = ThreeKeyFocus::Rating;
                        sfx("assets/sounds/start.ogg")
                    } else {
                        state.three_key_focus = ThreeKeyFocus::Wheel;
                        handle_confirm(state)
                    }
                }
                screen_input::ThreeKeyMenuAction::Cancel => {
                    if matches!(state.three_key_focus, ThreeKeyFocus::Rating) {
                        let undo_sound = if state.menu_lr_undo != 0 {
                            shift_selected_course_rating(state, -(state.menu_lr_undo as isize))
                        } else {
                            None
                        };
                        if state.menu_lr_undo != 0 {
                            state.menu_lr_undo = 0;
                        }
                        state.three_key_focus = ThreeKeyFocus::Wheel;
                        if let Some(path) = undo_sound {
                            ThemeEffect::Batch(vec![sfx(path), sfx("assets/sounds/change.ogg")])
                        } else {
                            sfx("assets/sounds/change.ogg")
                        }
                    } else {
                        if state.menu_lr_undo != 0 {
                            music_wheel_change(state, state.menu_lr_undo as isize);
                            state.menu_lr_undo = 0;
                        }
                        clear_wheel_hold(state);
                        begin_exit_prompt(state);
                        ThemeEffect::None
                    }
                }
            };
        }
    }

    let play_style = profile::get_session_play_style();
    if play_style == profile_data::PlayStyle::Versus {
        return match ev.action {
            VirtualAction::p1_left | VirtualAction::p1_menu_left => {
                handle_wheel_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_right | VirtualAction::p1_menu_right => {
                handle_wheel_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_left | VirtualAction::p2_menu_left => {
                handle_wheel_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_right | VirtualAction::p2_menu_right => {
                handle_wheel_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_up | VirtualAction::p1_menu_up => handle_rating_dir(
                state,
                profile_data::PlayerSide::P1,
                PadDir::Up,
                ev.pressed,
                ev.timestamp,
            ),
            VirtualAction::p2_up | VirtualAction::p2_menu_up => handle_rating_dir(
                state,
                profile_data::PlayerSide::P2,
                PadDir::Up,
                ev.pressed,
                ev.timestamp,
            ),
            VirtualAction::p1_down | VirtualAction::p1_menu_down => handle_rating_dir(
                state,
                profile_data::PlayerSide::P1,
                PadDir::Down,
                ev.pressed,
                ev.timestamp,
            ),
            VirtualAction::p2_down | VirtualAction::p2_menu_down => handle_rating_dir(
                state,
                profile_data::PlayerSide::P2,
                PadDir::Down,
                ev.pressed,
                ev.timestamp,
            ),
            VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
                handle_confirm(state)
            }
            VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
                begin_exit_prompt(state);
                ThemeEffect::None
            }
            _ => ThemeEffect::None,
        };
    }

    match profile::get_session_player_side() {
        profile_data::PlayerSide::P1 => match ev.action {
            VirtualAction::p1_left | VirtualAction::p1_menu_left => {
                handle_wheel_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_right | VirtualAction::p1_menu_right => {
                handle_wheel_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p1_up | VirtualAction::p1_menu_up => handle_rating_dir(
                state,
                profile_data::PlayerSide::P1,
                PadDir::Up,
                ev.pressed,
                ev.timestamp,
            ),
            VirtualAction::p1_down | VirtualAction::p1_menu_down => handle_rating_dir(
                state,
                profile_data::PlayerSide::P1,
                PadDir::Down,
                ev.pressed,
                ev.timestamp,
            ),
            VirtualAction::p1_start if ev.pressed => handle_confirm(state),
            VirtualAction::p1_back if ev.pressed => {
                begin_exit_prompt(state);
                ThemeEffect::None
            }
            _ => ThemeEffect::None,
        },
        profile_data::PlayerSide::P2 => match ev.action {
            VirtualAction::p2_left | VirtualAction::p2_menu_left => {
                handle_wheel_dir(state, PadDir::Left, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_right | VirtualAction::p2_menu_right => {
                handle_wheel_dir(state, PadDir::Right, ev.pressed, ev.timestamp)
            }
            VirtualAction::p2_up | VirtualAction::p2_menu_up => handle_rating_dir(
                state,
                profile_data::PlayerSide::P2,
                PadDir::Up,
                ev.pressed,
                ev.timestamp,
            ),
            VirtualAction::p2_down | VirtualAction::p2_menu_down => handle_rating_dir(
                state,
                profile_data::PlayerSide::P2,
                PadDir::Down,
                ev.pressed,
                ev.timestamp,
            ),
            VirtualAction::p2_start if ev.pressed => handle_confirm(state),
            VirtualAction::p2_back if ev.pressed => {
                begin_exit_prompt(state);
                ThemeEffect::None
            }
            _ => ThemeEffect::None,
        },
    }
}

pub fn update(state: &mut State, dt: f32) -> ThemeEffect {
    let dt = dt.max(0.0);

    match state.out_prompt {
        OutPromptState::PressStartForOptions { elapsed } => {
            let elapsed = elapsed + dt;
            if elapsed >= SHOW_OPTIONS_MESSAGE_SECONDS {
                state.out_prompt = OutPromptState::None;
                return ThemeEffect::NavigateNoFade(Screen::Gameplay);
            }
            state.out_prompt = OutPromptState::PressStartForOptions { elapsed };
            return ThemeEffect::None;
        }
        OutPromptState::EnteringOptions { elapsed } => {
            let elapsed = elapsed + dt;
            if elapsed >= ENTERING_OPTIONS_TOTAL_SECONDS {
                state.out_prompt = OutPromptState::None;
                return ThemeEffect::NavigateNoFade(Screen::PlayerOptions);
            }
            state.out_prompt = OutPromptState::EnteringOptions { elapsed };
            return ThemeEffect::None;
        }
        OutPromptState::None => {}
    }

    if let ExitPromptState::Active {
        elapsed,
        switch_from,
        switch_elapsed,
        ..
    } = &mut state.exit_prompt
    {
        *elapsed += dt;
        if switch_from.is_some() {
            *switch_elapsed += dt;
            if *switch_elapsed >= SL_EXIT_PROMPT_CHOICE_TWEEN_SECONDS {
                *switch_from = None;
                *switch_elapsed = 0.0;
            }
        }
    }

    state.selection_animation_timer += dt;
    state.time_since_selection_change += dt;

    let now = Instant::now();
    let moving = state
        .nav_key_held_since
        .is_some_and(|t| now.duration_since(t) >= NAV_INITIAL_HOLD_DELAY);
    if moving {
        match state.nav_key_held_direction.clone() {
            Some(dir) => music_wheel_update_hold_scroll(state, dt, dir),
            None => music_wheel_settle_offset(state, dt),
        }
    } else {
        music_wheel_settle_offset(state, dt);
    }

    let selection_changed = state.selected_index != state.prev_selected_index;
    if selection_changed {
        state.prev_selected_index = state.selected_index;
        state.time_since_selection_change = 0.0;
        state.menu_lr_undo = 0;
        state.three_key_focus = ThreeKeyFocus::Wheel;
        state.last_rating_nav_dir_p1 = None;
        state.last_rating_nav_time_p1 = None;
        state.last_rating_nav_dir_p2 = None;
        state.last_rating_nav_time_p2 = None;
        if let Some(meta) = selected_course_meta(state) {
            let idx = selected_course_rating_index(state, &meta);
            state
                .selected_rating_index_by_path
                .insert(meta.path.clone(), idx);
        }
    }

    if state.time_since_selection_change >= BANNER_UPDATE_DELAY_SECONDS {
        let banner = selected_banner_path(state);
        if banner != state.last_requested_banner_path {
            state.last_requested_banner_path.clone_from(&banner);
            state.banner_high_quality_requested = false;
            let effect = ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Media(
                crate::SimplyLoveMediaRequest::Banner(banner),
            ));
            return if selection_changed {
                sfx_then("assets/sounds/change.ogg", effect)
            } else {
                effect
            };
        }
        if banner.is_some()
            && !state.banner_high_quality_requested
            && state.nav_key_held_direction.is_none()
            && state.wheel_offset_from_selection.abs() < 0.0001
        {
            state.banner_high_quality_requested = true;
            let effect = ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Media(
                crate::SimplyLoveMediaRequest::Banner(banner),
            ));
            return if selection_changed {
                sfx_then("assets/sounds/change.ogg", effect)
            } else {
                effect
            };
        }
    }

    if selection_changed {
        sfx("assets/sounds/change.ogg")
    } else {
        ThemeEffect::None
    }
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

#[inline(always)]
pub fn trigger_immediate_refresh(state: &mut State) {
    rebuild_displayed_entries(state);
    state.time_since_selection_change = BANNER_UPDATE_DELAY_SECONDS;
    state.last_requested_banner_path = None;
    state.banner_high_quality_requested = false;
    state.out_prompt = OutPromptState::None;
    state.exit_prompt = ExitPromptState::None;
}

#[inline(always)]
pub fn allows_late_join(_state: &State) -> bool {
    true
}

fn format_session_time(seconds: f32) -> String {
    if seconds < 0.0 {
        return "00:00".to_string();
    }
    let s = seconds as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn format_len(seconds: i32) -> String {
    let s = seconds.max(0) as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[inline(always)]
fn format_bpm_value(bpm: f64) -> String {
    if !bpm.is_finite() || bpm <= 0.0 {
        return "?".to_string();
    }
    let rounded = bpm.round();
    if (bpm - rounded).abs() < 0.05 {
        format!("{}", rounded as i32)
    } else {
        format!("{bpm:.1}")
    }
}

fn format_bpm_range(min_bpm: Option<f64>, max_bpm: Option<f64>) -> String {
    let (Some(min_bpm), Some(max_bpm)) = (min_bpm, max_bpm) else {
        return "?".to_string();
    };
    let lo = min_bpm.min(max_bpm);
    let hi = min_bpm.max(max_bpm);
    let lo_txt = format_bpm_value(lo);
    let hi_txt = format_bpm_value(hi);
    if (hi - lo).abs() < 0.05 {
        lo_txt
    } else {
        format!("{lo_txt}-{hi_txt}")
    }
}

#[inline(always)]
fn course_selection_anim_beat(state: &State) -> f32 {
    // Keep course wheel pulse speed aligned with SelectMusic fallback (150 BPM).
    state.session_elapsed * 2.5
}

#[inline(always)]
fn course_arrow_bounce01(selection_beat: f32) -> f32 {
    // Match SelectMusic arrow timing: effectperiod(1) + effectoffset(-10*GlobalOffsetSeconds).
    let effect_offset = -10.0 * crate::config::get().global_offset_seconds;
    let t = (selection_beat + effect_offset).rem_euclid(1.0);
    (t * std::f32::consts::PI).sin().clamp(0.0, 1.0)
}

#[inline(always)]
fn course_tracklist_scroll(
    entry_count: usize,
    visible_rows: usize,
    elapsed: f32,
) -> (usize, f32, usize) {
    if entry_count == 0
        || visible_rows == 0
        || entry_count <= COURSE_TRACKLIST_SCROLL_MIN_ENTRIES
        || entry_count <= visible_rows
    {
        return (0, 0.0, 0);
    }
    let max_start = entry_count - visible_rows;
    let step = COURSE_TRACKLIST_SCROLL_STEP_SECONDS.max(1e-3);
    let pause = COURSE_TRACKLIST_SCROLL_END_PAUSE_SECONDS.max(0.0);
    let sweep = max_start as f32 * step;
    let cycle = pause + sweep + pause + sweep;
    if cycle <= f32::EPSILON {
        return (0, 0.0, 0);
    }

    let mut t = elapsed.max(0.0).rem_euclid(cycle);
    let pos = if t < pause {
        0.0
    } else {
        t -= pause;
        if t < sweep {
            t / step
        } else {
            t -= sweep;
            if t < pause {
                max_start as f32
            } else {
                t -= pause;
                (max_start as f32 - t / step).max(0.0)
            }
        }
    }
    .clamp(0.0, max_start as f32);

    let start = pos.floor() as usize;
    let frac = pos - start as f32;
    let focus = pos.round().clamp(0.0, max_start as f32) as usize;
    (start, frac, focus)
}

fn sl_select_music_bg_flash() -> Actor {
    act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(-98):
        sleep(0.6):
        linear(0.5): alpha(0.0):
        linear(0.0): visible(false)
    )
}

pub fn music_wheel_runtime_request(state: &State) -> MusicWheelRuntimeRequest<'_> {
    let play_style = profile::get_session_play_style();
    MusicWheelRuntimeRequest {
        read_scores: true,
        rank_source: MusicWheelRankSource::None,
        read_itl_scores: false,
        sides: Default::default(),
        slots: music_wheel::runtime_slot_requests(
            &state.entries,
            state.selected_index,
            [None, None],
            [0, 0],
            play_style,
        ),
    }
}

#[inline(always)]
pub fn score_runtime_request(state: &State) -> SelectCourseScoreRequest<'_> {
    let course_hash = state
        .entries
        .get(state.selected_index)
        .and_then(|entry| match entry {
            MusicWheelEntry::Song(song) => state
                .course_meta_by_path
                .get(song.simfile_path.as_path())
                .map(|meta| meta.score_hash.as_str()),
            MusicWheelEntry::PackHeader { .. } => None,
        });
    SelectCourseScoreRequest { course_hash }
}

#[inline(always)]
pub fn sync_runtime_view(state: &mut State, view: SelectCourseRuntimeView) {
    state.music_wheel = view.music_wheel;
    state.score_view = view.score;
}

pub fn push_actors(actors: &mut Vec<Actor>, state: &State, _asset_manager: &AssetManager) {
    actors.reserve(256);
    let side = profile::get_session_player_side();
    let play_style = profile::get_session_play_style();
    let is_p2_single = profile_data::is_single_p2_side(play_style, side);
    let selected_entry = state.entries.get(state.selected_index);
    let selected_meta = selected_course_meta(state);
    let selected_rating = selected_meta
        .as_ref()
        .and_then(|meta| selected_course_rating(state, meta));
    let selected_rating_index = selected_meta
        .as_ref()
        .map_or(0, |meta| selected_course_rating_index(state, meta));
    let selection_animation_beat = course_selection_anim_beat(state);
    let selected_diff_col = selected_rating.map(|rating| {
        color::difficulty_rgba(
            rating.course_difficulty_name.as_str(),
            state.active_color_index,
        )
    });

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
        },
    );
    actors.push(sl_select_music_bg_flash());
    screen_bars::push(actors, &tr("ScreenTitles", "SelectCourse"));
    actors.push(timers::build_session(format_session_time(
        state.session_elapsed,
    )));

    let mode_text = gs_scorebox::select_music_mode_text(state.score_view.mode_show_ex_score);
    actors.push(mode_pads::build_label(mode_text));
    actors.extend(mode_pads::build());

    let (banner_zoom, banner_cx, banner_cy) = if is_wide() {
        (0.7655, screen_center_x() - 170.0, 96.0)
    } else {
        (0.75, screen_center_x() - 166.0, 96.0)
    };
    actors.push(shared_banner::sprite(
        state.current_banner_key.clone(),
        banner_cx,
        banner_cy,
        BANNER_NATIVE_WIDTH,
        BANNER_NATIVE_HEIGHT,
        banner_zoom,
        51,
    ));

    let music_rate = profile::get_session_music_rate();
    let (songs_label, songs_value, bpm_text, len_text, desc_text) =
        match (selected_entry, selected_meta.as_ref()) {
            (Some(MusicWheelEntry::Song(_)), Some(meta)) => {
                let diff_min_bpm = selected_rating
                    .and_then(|rating| rating.min_bpm)
                    .or(meta.min_bpm);
                let diff_max_bpm = selected_rating
                    .and_then(|rating| rating.max_bpm)
                    .or(meta.max_bpm);
                let diff_len_secs = selected_rating
                    .map(|rating| rating.total_length_seconds.max(0))
                    .filter(|secs| *secs > 0)
                    .unwrap_or(meta.total_length_seconds.max(0));
                (
                    tr("SelectCourse", "SongsLabel").to_string(),
                    selected_rating
                        .map_or(0, |rating| rating.entries.len())
                        .to_string(),
                    format_bpm_range(diff_min_bpm, diff_max_bpm),
                    format_len(((diff_len_secs as f32) / music_rate).round() as i32),
                    meta.description.clone(),
                )
            }
            _ => (
                tr("SelectCourse", "SongsLabel").to_string(),
                "0".to_string(),
                "?".to_string(),
                "0:00".to_string(),
                String::new(),
            ),
        };

    let (steps_text, jumps_text, holds_text, mines_text, hands_text, rolls_text, meter_text) =
        match selected_rating {
            Some(rating) => {
                let meter = if let Some(course_meter) = rating.course_meter {
                    cached_u32_text(course_meter)
                } else if rating.meter_count > 0 {
                    cached_u32_text(
                        (rating.meter_sum as f32 / rating.meter_count as f32).round() as u32,
                    )
                } else {
                    unknown_text()
                };
                if rating.rated_entry_count > 0 {
                    (
                        cached_u32_text(rating.totals.steps),
                        cached_u32_text(rating.totals.jumps),
                        cached_u32_text(rating.totals.holds),
                        cached_u32_text(rating.totals.mines),
                        cached_u32_text(rating.totals.hands),
                        cached_u32_text(rating.totals.rolls),
                        meter,
                    )
                } else {
                    (
                        unknown_text(),
                        unknown_text(),
                        unknown_text(),
                        unknown_text(),
                        unknown_text(),
                        unknown_text(),
                        meter,
                    )
                }
            }
            None => (
                unknown_text(),
                unknown_text(),
                unknown_text(),
                unknown_text(),
                unknown_text(),
                unknown_text(),
                unknown_text(),
            ),
        };

    let pane_sel_col =
        selected_diff_col.unwrap_or_else(|| color::simply_love_rgba(state.active_color_index));
    let pane_cx = if is_p2_single {
        screen_width() * 0.75 + 5.0
    } else {
        screen_width() * 0.25 - 5.0
    };
    let placeholder = ("----".to_string(), placeholder_score_percent());
    let fallback_player = state.score_view.player_score_percent.map_or_else(
        || placeholder.clone(),
        |score_percent| {
            (
                state.score_view.player_initials.clone(),
                cached_score_percent_text(score_percent),
            )
        },
    );
    let fallback_machine = match (
        state.score_view.machine_initials.as_ref(),
        state.score_view.machine_score_percent,
    ) {
        (Some(initials), Some(score_percent)) => {
            (initials.clone(), cached_score_percent_text(score_percent))
        }
        _ => placeholder,
    };
    let gs_view = gs_scorebox::SelectMusicScoreboxView {
        mode_text: gs_scorebox::select_music_mode_text(state.score_view.pane_show_ex_score),
        machine_name: fallback_machine.0,
        machine_score: fallback_machine.1,
        player_name: fallback_player.0,
        player_score: fallback_player.1,
        rivals: std::array::from_fn(|_| ("----".to_string(), placeholder_score_percent())),
        show_rivals: false,
        loading_text: None,
    };
    select_pane::push_base(
        actors,
        select_pane::StatsPaneParams {
            pane_cx,
            accent_color: pane_sel_col,
            values: select_pane::StatsValues {
                steps: steps_text,
                mines: mines_text,
                jumps: jumps_text,
                hands: hands_text,
                holds: holds_text,
                rolls: rolls_text,
            },
            meter: (!gs_view.show_rivals).then_some(meter_text),
        },
    );
    let pane_layout = select_pane::layout();
    let lines = [
        (gs_view.machine_name.clone(), gs_view.machine_score.clone()),
        (gs_view.player_name.clone(), gs_view.player_score.clone()),
    ];
    for (i, (name, pct)) in lines.into_iter().enumerate() {
        actors.push(act!(text: font("miso"): settext(name): align(0.5, 0.5): xy(pane_cx + pane_layout.cols[2] - 50.0 * pane_layout.text_zoom, pane_layout.pane_top + pane_layout.rows[i]): maxwidth(30.0): zoom(pane_layout.text_zoom): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
        actors.push(act!(text: font("miso"): settext(pct): align(1.0, 0.5): xy(pane_cx + pane_layout.cols[2] + 25.0 * pane_layout.text_zoom, pane_layout.pane_top + pane_layout.rows[i]): zoom(pane_layout.text_zoom): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
    }
    if let Some(status) = gs_view.loading_text {
        actors.push(act!(text: font("miso"): settext(status): align(0.5, 0.5): xy(pane_cx + pane_layout.cols[2] - 15.0, pane_layout.pane_top + pane_layout.rows[2]): maxwidth(90.0): zoom(pane_layout.text_zoom): z(121): diffuse(0.0, 0.0, 0.0, 1.0): horizalign(center)));
    }
    if gs_view.show_rivals {
        for i in 0..3 {
            let (name, pct) = (&gs_view.rivals[i].0, &gs_view.rivals[i].1);
            actors.push(act!(text: font("miso"): settext(name.clone()): align(0.5, 0.5): xy(pane_cx + pane_layout.cols[2] + 50.0 * pane_layout.text_zoom, pane_layout.pane_top + pane_layout.rows[i]): maxwidth(30.0): zoom(pane_layout.text_zoom): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
            actors.push(act!(text: font("miso"): settext(pct.clone()): align(1.0, 0.5): xy(pane_cx + pane_layout.cols[2] + 125.0 * pane_layout.text_zoom, pane_layout.pane_top + pane_layout.rows[i]): zoom(pane_layout.text_zoom): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
        }
    }

    let (box_w, frame_x, frame_y) = if is_wide() {
        (320.0, screen_center_x() - 170.0, screen_center_y() - 55.0)
    } else {
        (310.0, screen_center_x() - 165.0, screen_center_y() - 55.0)
    };
    actors.push(Actor::Frame {
        align: [0.0, 0.0],
        offset: [frame_x, frame_y],
        size: [SizeSpec::Px(box_w), SizeSpec::Px(50.0)],
        background: None,
        z: 51,
        children: vec![
            act!(quad:
                setsize(box_w, 50.0):
                diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])
            ),
            Actor::Frame {
                align: [0.0, 0.0],
                offset: [-110.0, -6.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                background: None,
                z: 0,
                children: vec![
                    act!(text: font("miso"): settext(songs_label): align(1.0, 0.0): y(-11.0): maxwidth(56.0): diffuse(0.5, 0.5, 0.5, 1.0): z(52)),
                    act!(text: font("miso"): settext(songs_value): align(0.0, 0.0): xy(5.0, -11.0): maxwidth(box_w - 60.0): zoomtoheight(15.0): diffuse(1.0, 1.0, 1.0, 1.0): z(52)),
                    act!(text: font("miso"): settext(tr("SelectMusic", "BPMLabel")): align(1.0, 0.0): y(10.0): diffuse(0.5, 0.5, 0.5, 1.0): z(52)),
                    act!(text: font("miso"): settext(bpm_text): align(0.0, 0.0): xy(5.0, 10.0): zoomtoheight(15.0): diffuse(1.0, 1.0, 1.0, 1.0): z(52)),
                    act!(text: font("miso"): settext(tr("SelectMusic", "LengthLabel")): align(1.0, 0.0): xy(box_w - 130.0, 10.0): diffuse(0.5, 0.5, 0.5, 1.0): z(52)),
                    act!(text: font("miso"): settext(len_text): align(0.0, 0.0): xy(box_w - 125.0, 10.0): zoomtoheight(15.0): diffuse(1.0, 1.0, 1.0, 1.0): z(52)),
                ],
            },
        ],
    });

    let panel_w = if is_wide() { 286.0 } else { 276.0 };
    let rating_box_cx = screen_center_x() - 26.0;
    let rating_box_cy = screen_center_y() + 67.0;
    let rating_box_left = rating_box_cx - COURSE_TRACKLIST_RATING_BOX_W * 0.5;
    let rating_box_top = rating_box_cy - COURSE_TRACKLIST_RATING_BOX_H * 0.5;
    let rating_box_bottom = rating_box_cy + COURSE_TRACKLIST_RATING_BOX_H * 0.5;
    let panel_right = rating_box_left - 2.0;
    let panel_h = rating_box_bottom - rating_box_top;
    let panel_cx = panel_right - panel_w * 0.5;
    let panel_top = rating_box_top;
    let panel_bottom = rating_box_bottom;
    let panel_cy = panel_top + panel_h * 0.5;
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(panel_cx, panel_cy):
        setsize(panel_w, panel_h):
        z(120):
        diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])
    ));

    let (step_idx_text, step_artist_text, step_artist_col) = match selected_rating {
        Some(rating) if !rating.entries.is_empty() => {
            let idx = ((state.session_elapsed / 2.0).floor() as usize) % rating.entries.len();
            let entry = &rating.entries[idx];
            (
                format!("#{}", idx + 1),
                entry.step_artist.clone(),
                selected_diff_col.unwrap_or([0.5, 0.5, 0.5, 1.0]),
            )
        }
        Some(_) => (
            "#-".to_string(),
            tr("SelectCourse", "StepArtistPlaceholder").to_string(),
            selected_diff_col.unwrap_or([0.5, 0.5, 0.5, 1.0]),
        ),
        _ => (
            "#-".to_string(),
            tr("SelectCourse", "StepArtistPlaceholder").to_string(),
            [0.5, 0.5, 0.5, 1.0],
        ),
    };
    let has_desc = !desc_text.trim().is_empty();
    let list_left_x = panel_cx - panel_w * 0.5 + 10.0;
    let list_title_x = list_left_x + 38.0;
    let list_start_y = panel_top + 8.0 + COURSE_TRACKLIST_TEXT_Y_OFFSET;
    let list_right_pad = 14.0;
    let list_clip = Some([panel_cx - panel_w * 0.5, panel_top, panel_w, panel_h]);
    if let Some(rating) = selected_rating
        && !rating.entries.is_empty()
    {
        let visible_rows = rating
            .entries
            .len()
            .clamp(1, COURSE_TRACKLIST_TARGET_VISIBLE_ROWS);
        let row_spacing = COURSE_TRACKLIST_ROW_SPACING;
        let (start_idx, frac, _) =
            course_tracklist_scroll(rating.entries.len(), visible_rows, state.session_elapsed);
        let rows_to_draw = visible_rows + 2;
        let title_maxwidth = (panel_w - (list_title_x - list_left_x) - list_right_pad).max(40.0);
        for row in 0..rows_to_draw {
            let idx = start_idx + row;
            if idx >= rating.entries.len() {
                break;
            }
            let entry = &rating.entries[idx];
            let y = list_start_y + row as f32 * row_spacing - frac * row_spacing;
            if y > panel_bottom + row_spacing {
                break;
            }
            let diff_text = entry
                .meter
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| "?".to_string());
            let diff_color = color::difficulty_rgba(&entry.difficulty, state.active_color_index);
            let mut meter_actor = act!(text:
                font("miso"):
                settext(diff_text):
                align(0.0, 0.0):
                xy(list_left_x, y):
                zoomtoheight(COURSE_TRACKLIST_TEXT_HEIGHT):
                maxwidth(34.0):
                z(121):
                diffuse(diff_color[0], diff_color[1], diff_color[2], 1.0)
            );
            if let Actor::Text { clip, .. } = &mut meter_actor {
                *clip = list_clip;
            }
            actors.push(meter_actor);

            let mut title_actor = act!(text:
                font("miso"):
                settext(entry.title.clone()):
                align(0.0, 0.0):
                xy(list_title_x, y):
                zoomtoheight(COURSE_TRACKLIST_TEXT_HEIGHT):
                maxwidth(title_maxwidth):
                z(121):
                diffuse(1.0, 1.0, 1.0, 1.0)
            );
            if let Actor::Text { clip, .. } = &mut title_actor {
                *clip = list_clip;
            }
            actors.push(title_actor);
        }
    } else {
        let mut no_course_actor = act!(text:
            font("miso"):
            settext(tr("SelectCourse", "SelectCourseHint")):
            align(0.0, 0.0):
            xy(list_left_x, list_start_y):
            zoom(0.72):
            maxwidth(panel_w - 16.0):
            z(121):
            diffuse(1.0, 1.0, 1.0, 1.0)
        );
        if let Actor::Text { clip, .. } = &mut no_course_actor {
            *clip = list_clip;
        }
        actors.push(no_course_actor);
    }

    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(rating_box_cx, rating_box_cy):
        setsize(COURSE_TRACKLIST_RATING_BOX_W, COURSE_TRACKLIST_RATING_BOX_H):
        z(120):
        diffuse(UI_BOX_BG_COLOR[0], UI_BOX_BG_COLOR[1], UI_BOX_BG_COLOR[2], UI_BOX_BG_COLOR[3])
    ));
    let rating_len = selected_meta.as_ref().map_or(0, |meta| meta.ratings.len());
    let rating_top_index = if rating_len > COURSE_RATING_VISIBLE_SLOTS {
        selected_rating_index
            .saturating_sub(COURSE_RATING_VISIBLE_SLOTS - 1)
            .min(rating_len - COURSE_RATING_VISIBLE_SLOTS)
    } else {
        0
    };
    if let Some(meta) = selected_meta.as_ref() {
        for slot in 0..COURSE_RATING_VISIBLE_SLOTS {
            let y = rating_box_cy + (slot as i32 - 2) as f32 * 30.0;
            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(rating_box_cx, y):
                setsize(28.0, 28.0):
                z(121):
                diffuse(0.059, 0.059, 0.059, 1.0)
            ));
            let idx = rating_top_index + slot;
            if idx >= meta.ratings.len() {
                continue;
            }
            if let Some(rating) = meta.ratings[idx].as_ref() {
                let meter_text = rating
                    .course_meter
                    .map_or_else(|| "?".to_string(), |meter| meter.to_string());
                let color = color::difficulty_rgba(
                    rating.course_difficulty_name.as_str(),
                    state.active_color_index,
                );
                actors.push(act!(text:
                    font(current_machine_font_key(FontRole::Header)):
                    settext(meter_text):
                    align(0.5, 0.5):
                    xy(rating_box_cx, y):
                    zoom(0.45):
                    z(122):
                    diffuse(color[0], color[1], color[2], 1.0)
                ));
            }
        }
    }
    if rating_len > 0 {
        let selected_slot = (selected_rating_index.saturating_sub(rating_top_index))
            .min(COURSE_RATING_VISIBLE_SLOTS - 1);
        let arrow_y = rating_box_cy + (selected_slot as i32 - 2) as f32 * 30.0 + 1.0;
        let bounce = course_arrow_bounce01(selection_animation_beat);
        let (arrow_x0, arrow_dx, arrow_rot) = if is_p2_single {
            (rating_box_cx + 8.0, 3.0 * bounce, 180.0)
        } else {
            (rating_box_cx - 27.0, -3.0 * bounce, 0.0)
        };
        actors.push(act!(sprite("meter_arrow.png"):
            align(0.0, 0.5):
            xy(arrow_x0 + arrow_dx, arrow_y):
            rotationz(arrow_rot):
            zoom(0.575):
            z(122)
        ));
    }

    let step_artist_x0 = if is_p2_single {
        screen_center_x() - 244.0
    } else if is_wide() {
        screen_center_x() - 355.5
    } else {
        screen_center_x() - 345.5
    };
    let step_artist_y = (screen_center_y() - 9.0) - 0.5 * (screen_height() / 28.0);
    step_artist_bar::push(
        actors,
        step_artist_bar::StepArtistBarParams {
            x0: step_artist_x0,
            center_y: step_artist_y,
            layout: step_artist_bar::StepArtistBarLayout::Legacy,
            expanded_line_count: 0,
            accent_color: step_artist_col,
            z_base: 122,
            label_text: step_idx_text.into(),
            label_max_width: 22.0,
            artist_text: step_artist_text.into(),
            artist_x_offset: 60.0,
            artist_max_width: 138.0,
            artist_color: [
                UI_BOX_BG_COLOR[0],
                UI_BOX_BG_COLOR[1],
                UI_BOX_BG_COLOR[2],
                1.0,
            ],
        },
    );

    if has_desc {
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(panel_cx, panel_cy + panel_h * 0.5 - 9.0):
            setsize(panel_w, 16.0):
            z(122):
            diffuse(0.0, 0.0, 0.0, 0.5)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(desc_text):
            align(0.5, 0.5):
            xy(panel_cx, panel_cy + panel_h * 0.5 - 9.0):
            zoom(0.72):
            maxwidth(panel_w - 8.0):
            z(123):
            diffuse(1.0, 1.0, 1.0, 1.0)
        ));
    }

    music_wheel::push(
        actors,
        music_wheel::MusicWheelParams {
            entries: &state.entries,
            selected_index: state.selected_index,
            position_offset_from_selection: state.wheel_offset_from_selection,
            selection_animation_timer: state.selection_animation_timer,
            selection_animation_beat,
            color_pack_headers: true,
            selected_charts: [None, None],
            preferred_difficulty_index: [0, 0],
            song_box_color: None,
            song_text_color: Some(COURSE_WHEEL_SONG_TEXT_COLOR),
            song_text_color_overrides: Some(&state.course_text_color_overrides),
            song_has_edit_ptrs: None,
            show_music_wheel_grades: true,
            show_music_wheel_lamps: true,
            itl_rank_mode: crate::config::SelectMusicItlRankMode::None,
            itl_wheel_mode: crate::config::SelectMusicItlWheelMode::Off,
            song_select_bg_mode: crate::config::SelectMusicSongSelectBgMode::Off,
            expanded_series_name: None,
            expanded_pack_name: None,
            new_pack_names: None,
            pack_sync_prefs: None,
            default_sync_offset: crate::config::DefaultSyncOffset::Null,
            runtime: &state.music_wheel,
        },
    );

    if !matches!(selected_entry, Some(MusicWheelEntry::Song(_))) {
        actors.push(act!(text:
            font("miso"):
            settext(tr("SelectCourse", "PickCoursePrompt")):
            align(0.5, 0.5):
            xy(screen_center_x() - 26.0, screen_center_y() + 67.0):
            zoom(0.8):
            z(122):
            diffuse(1.0, 1.0, 1.0, 0.8)
        ));
    }

    // Match ScreenSelectMusic out-prompt visual treatment.
    if state.out_prompt != OutPromptState::None {
        actors.push(act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 0.0):
            cropbottom(1.0):
            fadebottom(0.5):
            z(1400):
            linear(TRANSITION_OUT_DURATION): cropbottom(-0.5): alpha(1.0)
        ));

        match state.out_prompt {
            OutPromptState::PressStartForOptions { .. } => {
                actors.push(act!(text:
                    font(current_machine_font_key(FontRole::Header)):
                    settext(tr("SelectMusic", "PressStartForOptions")):
                    align(0.5, 0.5):
                    xy(screen_center_x(), screen_center_y()):
                    zoom(0.75):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(1401)
                ));
            }
            OutPromptState::EnteringOptions { .. } => {
                actors.push(act!(text:
                    font(current_machine_font_key(FontRole::Header)):
                    settext(tr("SelectMusic", "PressStartForOptions")):
                    align(0.5, 0.5):
                    xy(screen_center_x(), screen_center_y()):
                    zoom(0.75):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(1401):
                    linear(ENTERING_OPTIONS_FADE_OUT_SECONDS): alpha(0.0)
                ));
                actors.push(act!(text:
                    font(current_machine_font_key(FontRole::Header)):
                    settext(tr("SelectMusic", "EnteringOptions")):
                    align(0.5, 0.5):
                    xy(screen_center_x(), screen_center_y()):
                    zoom(0.75):
                    diffuse(1.0, 1.0, 1.0, 0.0):
                    z(1401):
                    sleep(ENTERING_OPTIONS_FADE_OUT_SECONDS + ENTERING_OPTIONS_HIBERNATE_SECONDS):
                    linear(ENTERING_OPTIONS_FADE_IN_SECONDS): alpha(1.0):
                    sleep(ENTERING_OPTIONS_HOLD_SECONDS)
                ));
            }
            OutPromptState::None => {}
        }
    }

    if let ExitPromptState::Active {
        elapsed,
        active_choice,
        switch_from,
        switch_elapsed,
    } = state.exit_prompt
    {
        let choices_alpha = if elapsed <= SL_EXIT_PROMPT_CHOICES_DELAY_SECONDS {
            0.0
        } else {
            ((elapsed - SL_EXIT_PROMPT_CHOICES_DELAY_SECONDS) / SL_EXIT_PROMPT_CHOICES_FADE_SECONDS)
                .clamp(0.0, 1.0)
        };
        let p2_color = color::simply_love_rgba(state.active_color_index - 2);

        actors.push(act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, SL_EXIT_PROMPT_BG_ALPHA):
            z(1500)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(tr("SelectMusic", "ExitGamePrompt")):
            align(0.5, 0.0):
            xy(screen_center_x(), screen_center_y() + SL_EXIT_PROMPT_PROMPT_Y_OFFSET):
            zoom(SL_EXIT_PROMPT_PROMPT_ZOOM):
            maxwidth(420.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(1501):
            horizalign(center)
        ));

        let zoom_no = exit_prompt_choice_zoom(0, active_choice, switch_from, switch_elapsed);
        let zoom_yes = exit_prompt_choice_zoom(1, active_choice, switch_from, switch_elapsed);
        let cx = screen_center_x();
        let no_label = tr("Common", "No");
        let yes_label = tr("Common", "Yes");
        let no_info = tr("SelectMusic", "KeepPlayingInfo");
        let yes_info = tr("SelectMusic", "FinishedInfo");
        push_exit_prompt_choice(
            actors,
            cx - SL_EXIT_PROMPT_CHOICE_X_OFFSET,
            SL_EXIT_PROMPT_CHOICE_Y,
            no_label,
            no_info,
            active_choice == 0,
            zoom_no,
            p2_color,
            choices_alpha,
            1502,
        );
        push_exit_prompt_choice(
            actors,
            cx + SL_EXIT_PROMPT_CHOICE_X_OFFSET,
            SL_EXIT_PROMPT_CHOICE_Y,
            yes_label,
            yes_info,
            active_choice == 1,
            zoom_yes,
            p2_color,
            choices_alpha,
            1502,
        );
    }
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(256);
    push_actors(&mut actors, state, asset_manager);
    actors
}

#[inline(always)]
fn begin_exit_prompt(state: &mut State) {
    state.exit_prompt = ExitPromptState::Active {
        elapsed: 0.0,
        active_choice: 0,
        switch_from: None,
        switch_elapsed: 0.0,
    };
    state.menu_lr_undo = 0;
    state.three_key_focus = ThreeKeyFocus::Wheel;
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
}

#[inline(always)]
fn exit_prompt_choice_zoom(
    choice: u8,
    active_choice: u8,
    switch_from: Option<u8>,
    switch_elapsed: f32,
) -> f32 {
    #[inline(always)]
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        (b - a).mul_add(t, a)
    }

    if let Some(from) = switch_from {
        let t = (switch_elapsed / SL_EXIT_PROMPT_CHOICE_TWEEN_SECONDS).clamp(0.0, 1.0);
        if choice == from {
            return lerp(SL_EXIT_PROMPT_ACTIVE_ZOOM, SL_EXIT_PROMPT_INACTIVE_ZOOM, t);
        }
        if choice == active_choice {
            return lerp(SL_EXIT_PROMPT_INACTIVE_ZOOM, SL_EXIT_PROMPT_ACTIVE_ZOOM, t);
        }
    }

    [SL_EXIT_PROMPT_INACTIVE_ZOOM, SL_EXIT_PROMPT_ACTIVE_ZOOM][(choice == active_choice) as usize]
}

#[allow(clippy::too_many_arguments)]
fn push_exit_prompt_choice(
    out: &mut Vec<Actor>,
    cx: f32,
    cy: f32,
    label: std::sync::Arc<str>,
    info: std::sync::Arc<str>,
    active: bool,
    choice_zoom: f32,
    active_rgba: [f32; 4],
    alpha: f32,
    z: i16,
) {
    let mut rgba = [1.0; 4];
    if active {
        rgba = active_rgba;
    }
    rgba[3] *= alpha;

    out.push(act!(text:
        align(0.5, 0.5):
        xy(cx, cy):
        font(current_machine_font_key(FontRole::Header)):
        zoom(SL_EXIT_PROMPT_LABEL_ZOOM * choice_zoom):
        settext(label):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
        z(z):
        horizalign(center)
    ));
    out.push(act!(text:
        align(0.5, 0.5):
        xy(cx, cy + SL_EXIT_PROMPT_INFO_Y_OFFSET * choice_zoom):
        font("miso"):
        zoom(SL_EXIT_PROMPT_INFO_ZOOM * choice_zoom):
        settext(info):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
        z(z):
        horizalign(center)
    ));
}

fn handle_exit_prompt_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    if !ev.pressed {
        return ThemeEffect::None;
    }
    let ExitPromptState::Active { active_choice, .. } = state.exit_prompt else {
        return ThemeEffect::None;
    };

    match ev.action {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
            let ExitPromptState::Active {
                active_choice,
                switch_from,
                switch_elapsed,
                ..
            } = &mut state.exit_prompt
            else {
                return ThemeEffect::None;
            };
            let prev = *active_choice;
            *active_choice = 1 - prev;
            *switch_from = Some(prev);
            *switch_elapsed = 0.0;
            sfx("assets/sounds/change.ogg")
        }

        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            state.exit_prompt = ExitPromptState::None;
            sfx("assets/sounds/start.ogg")
        }

        VirtualAction::p1_start | VirtualAction::p2_start => {
            state.exit_prompt = ExitPromptState::None;
            if active_choice == 1 {
                sfx_then(
                    "assets/sounds/start.ogg",
                    ThemeEffect::Navigate(Screen::Menu),
                )
            } else {
                sfx("assets/sounds/start.ogg")
            }
        }

        _ => ThemeEffect::None,
    }
}
