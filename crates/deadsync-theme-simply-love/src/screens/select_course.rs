use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{self, tr};
use crate::assets::{FontRole, machine_font_key};
use crate::effects::sfx;
use crate::rgba_const;
use crate::screens::components::{
    select_music::{music_wheel, screen_bars, select_pane, step_artist_bar},
    shared::{banner as shared_banner, mode_pads, timers, transitions, visual_style_bg},
};
use crate::screens::input as screen_input;
use crate::screens::{Screen, ThemeEffect};
pub use crate::views::{CourseStagePlan, SelectedCoursePlan};
use crate::views::{
    CourseTypeView, MusicWheelRankSource, MusicWheelRuntimeRequest, MusicWheelRuntimeView,
    SelectCourseContextView, SelectCourseInitView, SelectCourseRuntimeView,
    SelectCourseScoreRequest, SelectCourseScoreView, SelectFlowPlayerView,
};
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadlib_present::color;
use deadlib_present::space::{
    is_wide, screen_center_x, screen_center_y, screen_height, screen_width,
};
use deadsync_chart::song::standard_difficulty_index;
use deadsync_chart::{ChartData, SongData, SongPack};
use deadsync_input::{InputEvent, PadDir, VirtualAction};
use deadsync_profile as profile_data;
use deadsync_score::default_scorebox_mode_text;
use deadsync_simfile::course::{
    self, COURSE_RATING_ORDER, CourseFile, CourseSong, CourseTotals, Difficulty, SongSort,
    add_chart_totals, course_difficulty_from_meters, course_meter, nearest_filled_slot,
    push_song_bpm_range, resolve_course_stage, song_unique_key,
};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
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
static WHEEL_CONTENT_GENERATION: AtomicU64 = AtomicU64::new(1);
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
    static SCORE_PERCENT_CACHE: RefCell<TextCache<u64>> = RefCell::new(text_cache_with_capacity(1024));
}

#[inline(always)]
fn zero_count_text() -> Arc<str> {
    static ZERO: OnceLock<Arc<str>> = OnceLock::new();
    ZERO.get_or_init(|| Arc::<str>::from("0")).clone()
}

#[inline(always)]
fn unknown_text() -> Arc<str> {
    static UNKNOWN: OnceLock<Arc<str>> = OnceLock::new();
    UNKNOWN.get_or_init(|| Arc::<str>::from("?")).clone()
}

#[inline(always)]
fn empty_text() -> Arc<str> {
    static EMPTY: OnceLock<Arc<str>> = OnceLock::new();
    EMPTY.get_or_init(|| Arc::<str>::from("")).clone()
}

#[inline(always)]
fn missing_step_index_text() -> Arc<str> {
    static MISSING: OnceLock<Arc<str>> = OnceLock::new();
    MISSING.get_or_init(|| Arc::<str>::from("#-")).clone()
}

#[inline(always)]
fn zero_time_text() -> Arc<str> {
    static ZERO: OnceLock<Arc<str>> = OnceLock::new();
    ZERO.get_or_init(|| Arc::<str>::from("0:00")).clone()
}

#[inline(always)]
fn placeholder_score_percent() -> Arc<str> {
    static UNKNOWN: OnceLock<Arc<str>> = OnceLock::new();
    UNKNOWN.get_or_init(|| Arc::<str>::from("??.??%")).clone()
}

#[inline(always)]
fn placeholder_name_text() -> Arc<str> {
    static PLACEHOLDER: OnceLock<Arc<str>> = OnceLock::new();
    PLACEHOLDER.get_or_init(|| Arc::<str>::from("----")).clone()
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

/// Immutable actor-ready text compiled with resolved course metadata.
///
/// The game thread builds each entry once during Select Course initialization.
/// Values live for the screen, have no miss/eviction path or background work,
/// and are dropped with the metadata map. Actor frames clone fixed `Arc`s; the
/// worst-case text work is therefore bounded by the visible row count.
#[derive(Clone, Debug)]
struct CourseSongEntry {
    title: Arc<str>,
    difficulty: String,
    meter_text: Arc<str>,
    index_text: Arc<str>,
    step_artist: Arc<str>,
}

impl CourseSongEntry {
    fn new(
        title: String,
        difficulty: String,
        meter: u32,
        index: usize,
        step_artist: String,
    ) -> Self {
        Self {
            title: title.into(),
            difficulty,
            meter_text: meter.to_string().into(),
            index_text: format!("#{}", index + 1).into(),
            step_artist: step_artist.into(),
        }
    }
}

#[derive(Clone, Debug)]
struct CourseMeta {
    source: CourseFile,
    path: PathBuf,
    score_hash: String,
    name: String,
    scripter: String,
    description: Arc<str>,
    banner_path: Option<PathBuf>,
    ratings: Vec<Option<CourseRatingMeta>>,
    default_rating_index: usize,
    min_bpm: Option<f64>,
    max_bpm: Option<f64>,
    total_length_seconds: i32,
    has_random_entries: bool,
    has_most_played_entries: bool,
    course_type: CourseTypeView,
    lives: i32,
}

#[derive(Clone, Debug)]
struct CourseRatingMeta {
    course_difficulty: Difficulty,
    entries: Vec<CourseSongEntry>,
    stats_text: CourseStatsText,
    course_difficulty_name: String,
    course_stepchart_label: String,
    course_meter: Option<u32>,
    course_meter_text: Arc<str>,
    entry_count_text: Arc<str>,
    min_bpm: Option<f64>,
    max_bpm: Option<f64>,
    total_length_seconds: i32,
    runtime_stages: Vec<CourseStagePlan>,
}

/// Immutable actor-ready stat text compiled with each course rating.
///
/// The game thread creates this fixed six-string value during Select Course
/// initialization. It lives with course metadata for the screen and has no
/// misses, growth, eviction, locking, or background work. Actor frames only
/// clone its `Arc`s, teardown runs on the game thread, focused tests provide
/// instrumentation, and worst-case frame work is six reference-count bumps.
#[derive(Clone, Debug)]
struct CourseStatsText {
    steps: Arc<str>,
    jumps: Arc<str>,
    holds: Arc<str>,
    mines: Arc<str>,
    hands: Arc<str>,
    rolls: Arc<str>,
}

impl CourseStatsText {
    fn new(totals: &CourseTotals, has_rated_entries: bool) -> Self {
        if !has_rated_entries {
            return Self::unknown();
        }
        Self {
            steps: totals.steps.to_string().into(),
            jumps: totals.jumps.to_string().into(),
            holds: totals.holds.to_string().into(),
            mines: totals.mines.to_string().into(),
            hands: totals.hands.to_string().into(),
            rolls: totals.rolls.to_string().into(),
        }
    }

    fn unknown() -> Self {
        Self {
            steps: unknown_text(),
            jumps: unknown_text(),
            holds: unknown_text(),
            mines: unknown_text(),
            hands: unknown_text(),
            rolls: unknown_text(),
        }
    }
}

/// Fixed Select Course score-pane presentation owned by the game thread.
///
/// Four shared strings live for the screen and are rebuilt only when the shell
/// supplies changed score data. There is no per-frame miss path, growth,
/// eviction, locking, or background work; replacement and teardown drop values
/// on the game thread. Focused synchronization tests provide instrumentation,
/// and worst-case rebuild work is four short string conversions.
struct CourseScoreText {
    machine_name: Arc<str>,
    machine_score: Arc<str>,
    player_name: Arc<str>,
    player_score: Arc<str>,
}

impl CourseScoreText {
    fn new(view: &SelectCourseScoreView) -> Self {
        let (player_name, player_score) = view.player_score_percent.map_or_else(
            || (placeholder_name_text(), placeholder_score_percent()),
            |score| {
                (
                    Arc::from(view.player_initials.as_str()),
                    cached_score_percent_text(score),
                )
            },
        );
        let (machine_name, machine_score) =
            match (view.machine_initials.as_deref(), view.machine_score_percent) {
                (Some(initials), Some(score)) => {
                    (Arc::from(initials), cached_score_percent_text(score))
                }
                _ => (placeholder_name_text(), placeholder_score_percent()),
            };
        Self {
            machine_name,
            machine_score,
            player_name,
            player_score,
        }
    }
}

/// Actor-ready course summary presentation owned by the game thread.
///
/// A fixed four-string value is populated during screen initialization, lives
/// for the screen, and is rebuilt only when course selection, rating, or music
/// rate changes. There is no per-frame miss path, growth, eviction, locking, or
/// background work; replacement and teardown happen on the game thread. Focused
/// formatting tests provide instrumentation, and worst-case rebuild work is
/// bounded BPM and duration formatting.
struct CourseSummaryText {
    songs: Arc<str>,
    bpm: Arc<str>,
    length: Arc<str>,
    description: Arc<str>,
}

impl CourseSummaryText {
    fn empty() -> Self {
        Self {
            songs: zero_count_text(),
            bpm: unknown_text(),
            length: zero_time_text(),
            description: empty_text(),
        }
    }

    fn selected(
        songs: Arc<str>,
        min_bpm: Option<f64>,
        max_bpm: Option<f64>,
        length_seconds: i32,
        music_rate: f32,
        description: Arc<str>,
    ) -> Self {
        Self {
            songs,
            bpm: format_bpm_range(min_bpm, max_bpm).into(),
            length: format_len(((length_seconds.max(0) as f32) / music_rate).round() as i32).into(),
            description,
        }
    }
}

/// Resolved course selection owned by the game thread.
///
/// This fixed view is populated during screen initialization and refreshed at
/// wheel or rating transitions. It lives for the screen, performs one bounded
/// metadata lookup on a course change, and has no frame-time misses, growth,
/// eviction, locking, or background work. Replacement and teardown happen on
/// the game thread; selection tests provide instrumentation, and actor-frame
/// lookup cost is a borrowed field access.
#[derive(Default)]
struct CourseSelection {
    meta: Option<Arc<CourseMeta>>,
    rating_index: usize,
}

/// Game-thread-owned, fixed-size translated text for Select Course's stable
/// actor labels.
///
/// The 14 entries are loaded during screen initialization and refreshed only
/// after the observable locale revision changes. There is no miss or eviction
/// path, replaced `Arc`s are released on the game thread, and steady-frame work
/// is one atomic revision load. Sync's result is test-visible; the bounded
/// refresh cost is 14 language-map lookups.
struct SelectCourseLabels {
    revision: u64,
    title: Arc<str>,
    songs: Arc<str>,
    bpm: Arc<str>,
    length: Arc<str>,
    step_artist: Arc<str>,
    select_hint: Arc<str>,
    pick_prompt: Arc<str>,
    options_prompt: Arc<str>,
    entering_options: Arc<str>,
    exit_prompt: Arc<str>,
    no: Arc<str>,
    yes: Arc<str>,
    keep_playing: Arc<str>,
    finished: Arc<str>,
}

impl SelectCourseLabels {
    fn load() -> Self {
        let mut labels = Self {
            revision: 0,
            title: tr("ScreenTitles", "SelectCourse"),
            songs: tr("SelectCourse", "SongsLabel"),
            bpm: tr("SelectMusic", "BPMLabel"),
            length: tr("SelectMusic", "LengthLabel"),
            step_artist: tr("SelectCourse", "StepArtistPlaceholder"),
            select_hint: tr("SelectCourse", "SelectCourseHint"),
            pick_prompt: tr("SelectCourse", "PickCoursePrompt"),
            options_prompt: tr("SelectMusic", "PressStartForOptions"),
            entering_options: tr("SelectMusic", "EnteringOptions"),
            exit_prompt: tr("SelectMusic", "ExitGamePrompt"),
            no: tr("Common", "No"),
            yes: tr("Common", "Yes"),
            keep_playing: tr("SelectMusic", "KeepPlayingInfo"),
            finished: tr("SelectMusic", "FinishedInfo"),
        };
        labels.revision = i18n::revision();
        labels
    }

    fn sync(&mut self) -> bool {
        if self.revision == i18n::revision() {
            return false;
        }
        *self = Self::load();
        true
    }
}

struct InitData {
    all_entries: Vec<MusicWheelEntry>,
    course_meta_by_path: HashMap<PathBuf, Arc<CourseMeta>>,
    course_text_color_overrides: HashMap<usize, [f32; 4]>,
    resolver: CourseResolver,
}

struct CourseResolver {
    by_group_song: HashMap<(String, String), Arc<SongData>>,
    by_song: HashMap<String, Arc<SongData>>,
    songs_by_group: HashMap<String, Vec<Arc<SongData>>>,
    all_songs: Vec<Arc<SongData>>,
    song_play_counts: HashMap<String, u32>,
    song_grade_counts: HashMap<String, course::CourseGradeCounts>,
    target_chart_type: String,
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

enum BannerSource {
    Song(Arc<SongData>),
    Shared(Arc<Path>),
}

impl BannerSource {
    #[inline(always)]
    fn path(&self) -> Option<&Path> {
        match self {
            Self::Song(song) => song.banner_path.as_deref(),
            Self::Shared(path) => Some(path),
        }
    }
}

pub struct State {
    pub entries: Vec<MusicWheelEntry>,
    pub selected_index: usize,
    pub active_color_index: i32,
    pub selection_animation_timer: f32,
    pub wheel_offset_from_selection: f32,
    pub current_banner_key: Arc<str>,
    session_elapsed: f32,
    session_timer: timers::TimerText,
    context: SelectCourseContextView,
    players: [SelectFlowPlayerView; 2],
    music_wheel: MusicWheelRuntimeView,
    score_view: SelectCourseScoreView,
    score_text: CourseScoreText,
    summary_text: CourseSummaryText,
    course_selection: CourseSelection,
    labels: SelectCourseLabels,
    wheel_content_generation: u64,

    all_entries: Vec<MusicWheelEntry>,
    course_meta_by_path: HashMap<PathBuf, Arc<CourseMeta>>,
    course_text_color_overrides: HashMap<usize, [f32; 4]>,
    resolver: CourseResolver,
    bg: visual_style_bg::State,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    last_requested_banner_source: Option<BannerSource>,
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
    song_packs: &[SongPack],
    played_chart_counts: &[(String, u32)],
) -> (
    HashMap<(String, String), Arc<SongData>>,
    HashMap<String, Arc<SongData>>,
    HashMap<String, Vec<Arc<SongData>>>,
    Vec<Arc<SongData>>,
    HashMap<String, u32>,
) {
    let total_songs = song_packs.iter().map(|pack| pack.songs.len()).sum();
    let mut by_group_song = HashMap::with_capacity(total_songs);
    let mut by_song = HashMap::with_capacity(total_songs);
    let mut songs_by_group: HashMap<String, Vec<Arc<SongData>>> =
        HashMap::with_capacity(song_packs.len());
    let mut all_songs = Vec::with_capacity(total_songs);

    for pack in song_packs {
        if pack.songs.is_empty() {
            continue;
        }
        let group_name = pack.group_name.trim().to_string();
        let group_key = group_name.to_ascii_lowercase();
        let group_songs = songs_by_group.entry(group_name).or_default();
        group_songs.reserve(pack.songs.len());
        for song in &pack.songs {
            all_songs.push(song.clone());
            group_songs.push(song.clone());
            let Some(song_key) = song_dir_key(song) else {
                continue;
            };
            by_group_song.insert((group_key.clone(), song_key.clone()), song.clone());
            by_song.entry(song_key).or_insert_with(|| song.clone());
        }
    }

    let song_play_counts = build_song_play_counts(song_packs, played_chart_counts, total_songs);

    (
        by_group_song,
        by_song,
        songs_by_group,
        all_songs,
        song_play_counts,
    )
}

fn build_song_play_counts(
    song_packs: &[SongPack],
    played_chart_counts: &[(String, u32)],
    total_songs: usize,
) -> HashMap<String, u32> {
    if played_chart_counts.is_empty() {
        return HashMap::new();
    }

    let mut plays_by_chart: FxHashMap<&str, u32> =
        FxHashMap::with_capacity_and_hasher(played_chart_counts.len(), Default::default());
    for (chart_hash, plays) in played_chart_counts {
        plays_by_chart
            .entry(chart_hash.as_str())
            .and_modify(|count| *count = count.saturating_add(*plays))
            .or_insert(*plays);
    }

    let mut song_play_counts: HashMap<String, u32> =
        HashMap::with_capacity(plays_by_chart.len().min(total_songs));
    for pack in song_packs {
        for song in &pack.songs {
            let mut matched = false;
            let mut song_plays = 0_u32;
            for chart in &song.charts {
                if let Some(plays) = plays_by_chart.remove(chart.short_hash.as_str()) {
                    matched = true;
                    song_plays = song_plays.saturating_add(plays);
                }
            }
            if matched {
                song_play_counts
                    .entry(song_unique_key(song))
                    .and_modify(|count| *count = count.saturating_add(song_plays))
                    .or_insert(song_plays);
            }
            if plays_by_chart.is_empty() {
                return song_play_counts;
            }
        }
    }

    song_play_counts
}

fn build_song_grade_counts(
    song_packs: &[SongPack],
    chart_grades: &[(String, u8)],
    chart_type: &str,
) -> HashMap<String, course::CourseGradeCounts> {
    if chart_grades.is_empty() {
        return HashMap::new();
    }
    let grades_by_chart: FxHashMap<&str, u8> = chart_grades
        .iter()
        .map(|(chart_hash, grade)| (chart_hash.as_str(), *grade))
        .collect();
    let mut song_grades = HashMap::new();
    for pack in song_packs {
        for song in &pack.songs {
            let mut counts = course::CourseGradeCounts::default();
            let mut has_grade = false;
            for chart in &song.charts {
                if !chart.chart_type.eq_ignore_ascii_case(chart_type) {
                    continue;
                }
                let Some(grade) = grades_by_chart.get(chart.short_hash.as_str()).copied() else {
                    continue;
                };
                let Some(count) = counts.get_mut(grade as usize) else {
                    continue;
                };
                *count = count.saturating_add(1);
                has_grade = true;
            }
            if has_grade {
                song_grades.insert(song_unique_key(song), counts);
            }
        }
    }
    song_grades
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
        translit_artist: String::new(),
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
        max_bpm: meta.max_bpm.unwrap_or_else(|| meta.min_bpm.unwrap_or(0.0)),
        normalized_bpms: String::new(),
        music_length_seconds: meta.total_length_seconds.max(0) as f32,
        first_second: 0.0,
        total_length_seconds: meta.total_length_seconds.max(0),
        precise_last_second_seconds: meta.total_length_seconds.max(0) as f32,
        charts: Vec::new(),
    }
}

fn build_init_data(init_view: &SelectCourseInitView) -> InitData {
    let translated_titles = init_view.translated_titles;
    let target_chart_type = init_view.context.play_style.chart_type();
    let (by_group_song, by_song, songs_by_group, all_songs, song_play_counts) =
        build_song_lookup(&init_view.song_packs, &init_view.played_chart_counts);
    let song_grade_counts = build_song_grade_counts(
        &init_view.song_packs,
        &init_view.chart_grades,
        target_chart_type,
    );

    let mut grouped: HashMap<String, Vec<Arc<CourseMeta>>> = HashMap::new();
    let mut course_meta_by_path: HashMap<PathBuf, Arc<CourseMeta>> = HashMap::new();

    for (path, course) in &init_view.courses {
        let course_type = course::course_type(course);
        let mut total_seconds = 0i32;
        let mut min_bpm = None;
        let mut max_bpm = None;
        let mut selected_song_keys = Vec::with_capacity(course.entries.len());
        let mut has_random_entries = false;
        let mut has_most_played_entries = false;
        let random_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0_u64, |d| d.as_nanos() as u64);

        for (entry_idx, entry) in course.entries.iter().enumerate() {
            if matches!(
                &entry.song,
                CourseSong::RandomAny
                    | CourseSong::RandomWithinGroup { .. }
                    | CourseSong::Select(_)
            ) {
                has_random_entries = true;
            }
            if matches!(
                &entry.song,
                CourseSong::SortPick {
                    sort: SongSort::MostPlays,
                    ..
                }
            ) || matches!(
                &entry.song,
                CourseSong::Select(select) if select.sort == Some(SongSort::MostPlays)
            ) {
                has_most_played_entries = true;
            }

            let resolved = resolve_course_stage(
                path,
                entry_idx,
                random_seed,
                entry,
                &by_group_song,
                &by_song,
                &all_songs,
                &songs_by_group,
                &song_play_counts,
                &song_grade_counts,
                course_type,
                &selected_song_keys,
                target_chart_type,
                Difficulty::Medium,
            );

            if let Some(stage) = resolved.as_ref() {
                let song_data = &stage.song;
                selected_song_keys.push(song_unique_key(song_data));
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
            let mut rating_song_keys = Vec::with_capacity(course.entries.len());
            let mut rating_total_seconds = 0i32;
            let mut rating_min_bpm = None;
            let mut rating_max_bpm = None;

            for (entry_idx, entry) in course.entries.iter().enumerate() {
                let Some(stage) = resolve_course_stage(
                    path,
                    entry_idx,
                    random_seed,
                    entry,
                    &by_group_song,
                    &by_song,
                    &all_songs,
                    &songs_by_group,
                    &song_play_counts,
                    &song_grade_counts,
                    course_type,
                    &rating_song_keys,
                    target_chart_type,
                    course_diff,
                ) else {
                    continue;
                };
                let song_data = &stage.song;
                let Some(chart) = song_data.charts.get(stage.chart_index) else {
                    continue;
                };
                rating_song_keys.push(song_unique_key(song_data));
                let len = if song_data.music_length_seconds > 0.0 {
                    song_data.music_length_seconds.round() as i32
                } else {
                    song_data.total_length_seconds.max(0)
                };
                rating_total_seconds = rating_total_seconds.saturating_add(len.max(0));
                push_song_bpm_range(&mut rating_min_bpm, &mut rating_max_bpm, song_data);
                runtime_stages.push(CourseStagePlan {
                    song: song_data.clone(),
                    chart_hash: chart.short_hash.clone(),
                    modifiers: stage.modifiers,
                    gain_seconds: stage.gain_seconds,
                    gain_lives: stage.gain_lives,
                });
                add_chart_totals(&mut totals, chart);
                rated_entry_count = rated_entry_count.saturating_add(1);
                meter_sum = meter_sum.saturating_add(chart.meter);
                meter_count = meter_count.saturating_add(1);
                let entry_index = entries.len();
                entries.push(CourseSongEntry::new(
                    song_data.display_full_title(translated_titles),
                    chart.difficulty.to_ascii_lowercase(),
                    chart.meter,
                    entry_index,
                    chart_step_artist(chart),
                ));
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
            let course_meter_text =
                course_meter.map_or_else(unknown_text, |meter| Arc::<str>::from(meter.to_string()));
            let entry_count_text = Arc::<str>::from(entries.len().to_string());
            let stats_text = CourseStatsText::new(&totals, rated_entry_count > 0);

            ratings[course_diff as usize] = Some(CourseRatingMeta {
                course_difficulty: course_diff,
                entries,
                stats_text,
                course_difficulty_name,
                course_stepchart_label,
                course_meter,
                course_meter_text,
                entry_count_text,
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
            .unwrap_or_else(|| (min_bpm, max_bpm, total_seconds.max(0)));
        let meta = Arc::new(CourseMeta {
            source: course.clone(),
            path: path.clone(),
            score_hash: course_score_hash(path),
            name: course_name(path, course),
            scripter: course.scripter.clone(),
            description: Arc::from(course.description.as_str()),
            banner_path: course::resolve_course_banner_path(path, &course.banner),
            ratings,
            default_rating_index,
            min_bpm: meta_min_bpm,
            max_bpm: meta_max_bpm,
            total_length_seconds: meta_total_length_seconds,
            has_random_entries,
            has_most_played_entries,
            course_type: match course_type {
                course::CourseType::Nonstop => CourseTypeView::Nonstop,
                course::CourseType::Oni => CourseTypeView::Oni,
                course::CourseType::Endless => CourseTypeView::Endless,
                course::CourseType::Survival => CourseTypeView::Survival,
            },
            lives: course.lives,
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
        resolver: CourseResolver {
            by_group_song,
            by_song,
            songs_by_group,
            all_songs,
            song_play_counts,
            song_grade_counts,
            target_chart_type: target_chart_type.to_string(),
        },
    }
}

fn rebuild_displayed_entries(state: &mut State) {
    state.wheel_content_generation = WHEEL_CONTENT_GENERATION.fetch_add(1, Ordering::Relaxed);
    let selected_path = match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(song)) => Some(song.simfile_path.clone()),
        _ => None,
    };
    state.entries.clear();
    state.entries.reserve(state.all_entries.len());
    for entry in &state.all_entries {
        let include = match entry {
            MusicWheelEntry::Song(song) => state
                .course_meta_by_path
                .get(&song.simfile_path)
                .is_none_or(|meta| {
                    (state.context.policy.show_random_courses || !meta.has_random_entries)
                        && (state.context.policy.show_most_played_courses
                            || !meta.has_most_played_entries)
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
        state.last_requested_banner_source = None;
        state.banner_high_quality_requested = false;
        state.last_rating_nav_dir_p1 = None;
        state.last_rating_nav_time_p1 = None;
        state.last_rating_nav_dir_p2 = None;
        state.last_rating_nav_time_p2 = None;
        sync_course_selection(state);
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
    state.last_requested_banner_source = None;
    state.banner_high_quality_requested = false;
    state.last_rating_nav_dir_p1 = None;
    state.last_rating_nav_time_p1 = None;
    state.last_rating_nav_dir_p2 = None;
    state.last_rating_nav_time_p2 = None;
    sync_course_selection(state);
}

fn selected_course_meta(state: &State) -> Option<&CourseMeta> {
    state.course_selection.meta.as_deref()
}

fn course_selection_matches(state: &State) -> bool {
    match (
        state.entries.get(state.selected_index),
        selected_course_meta(state),
    ) {
        (Some(MusicWheelEntry::Song(song)), Some(meta)) => song.simfile_path == meta.path,
        (Some(MusicWheelEntry::PackHeader { .. }) | None, None) => true,
        _ => false,
    }
}

fn stored_rating_index(state: &State, meta: &CourseMeta) -> usize {
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

fn sync_course_selection(state: &mut State) {
    let meta = state
        .entries
        .get(state.selected_index)
        .and_then(|entry| match entry {
            MusicWheelEntry::Song(song) => state.course_meta_by_path.get(&song.simfile_path),
            MusicWheelEntry::PackHeader { .. } => None,
        })
        .cloned();
    let rating_index = meta
        .as_deref()
        .map_or(0, |meta| stored_rating_index(state, meta));
    if let Some(meta) = meta.as_deref() {
        state
            .selected_rating_index_by_path
            .insert(meta.path.clone(), rating_index);
    }
    state.course_selection = CourseSelection { meta, rating_index };
    rebuild_course_summary(state);
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
    sync_course_selection(state);

    if let Some(meta) = selected_course_meta(state) {
        let idx = course_difficulty_name
            .and_then(|diff_name| {
                meta.ratings.iter().position(|slot| {
                    slot.as_ref().is_some_and(|rating| {
                        rating
                            .course_difficulty_name
                            .eq_ignore_ascii_case(diff_name)
                    })
                })
            })
            .unwrap_or_else(|| selected_course_rating_index(state));
        set_selected_course_rating_index(state, idx);
    }

    state.last_rating_nav_dir_p1 = None;
    state.last_rating_nav_time_p1 = None;
    state.last_rating_nav_dir_p2 = None;
    state.last_rating_nav_time_p2 = None;
    true
}

#[inline(always)]
const fn selected_course_rating_index(state: &State) -> usize {
    state.course_selection.rating_index
}

#[inline(always)]
fn selected_course_rating<'a>(state: &State, meta: &'a CourseMeta) -> Option<&'a CourseRatingMeta> {
    meta.ratings
        .get(selected_course_rating_index(state))
        .and_then(Option::as_ref)
}

fn rebuild_course_summary(state: &mut State) {
    let Some(meta) = selected_course_meta(state) else {
        state.summary_text = CourseSummaryText::empty();
        return;
    };
    let rating = selected_course_rating(state, meta);
    let min_bpm = rating.and_then(|rating| rating.min_bpm).or(meta.min_bpm);
    let max_bpm = rating.and_then(|rating| rating.max_bpm).or(meta.max_bpm);
    let length_seconds = rating
        .map(|rating| rating.total_length_seconds.max(0))
        .filter(|seconds| *seconds > 0)
        .unwrap_or_else(|| meta.total_length_seconds.max(0));
    let songs = rating.map_or_else(zero_count_text, |rating| {
        Arc::clone(&rating.entry_count_text)
    });
    let summary = CourseSummaryText::selected(
        songs,
        min_bpm,
        max_bpm,
        length_seconds,
        state.context.music_rate,
        Arc::clone(&meta.description),
    );
    state.summary_text = summary;
}

#[inline(always)]
fn set_selected_course_rating_index(state: &mut State, idx: usize) {
    let Some(meta) = selected_course_meta(state) else {
        rebuild_course_summary(state);
        return;
    };
    if meta.ratings.is_empty() {
        rebuild_course_summary(state);
        return;
    }
    let preferred = idx.min(meta.ratings.len().saturating_sub(1));
    let selected = nearest_filled_slot(&meta.ratings, preferred).unwrap_or(preferred);
    if selected == state.course_selection.rating_index {
        return;
    }
    let path = meta.path.clone();
    state.selected_rating_index_by_path.insert(path, selected);
    state.course_selection.rating_index = selected;
    rebuild_course_summary(state);
}

pub fn selected_course_plan(state: &State) -> Option<SelectedCoursePlan> {
    let meta = selected_course_meta(state)?;
    let rating = selected_course_rating(state, meta)?;
    if rating.runtime_stages.is_empty() {
        return None;
    }
    Some(course_plan(meta, rating, rating.runtime_stages.clone()))
}

fn course_plan(
    meta: &CourseMeta,
    rating: &CourseRatingMeta,
    stages: Vec<CourseStagePlan>,
) -> SelectedCoursePlan {
    SelectedCoursePlan {
        path: meta.path.clone(),
        name: meta.name.clone(),
        banner_path: meta.banner_path.clone(),
        score_hash: meta.score_hash.clone(),
        song_stub: Arc::new(make_course_song(meta)),
        course_difficulty_name: rating.course_difficulty_name.clone(),
        course_meter: rating.course_meter,
        course_stepchart_label: rating.course_stepchart_label.clone(),
        course_type: meta.course_type,
        lives: meta.lives,
        stages,
    }
}

fn fresh_course_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0_u64, |duration| duration.as_nanos() as u64)
}

pub fn reroll_selected_endless_plan(state: &State) -> Option<SelectedCoursePlan> {
    let meta = selected_course_meta(state)?;
    if meta.course_type != CourseTypeView::Endless {
        return None;
    }
    let rating = selected_course_rating(state, meta)?;
    let mut selected_song_keys = Vec::with_capacity(meta.source.entries.len());
    let mut stages = Vec::with_capacity(meta.source.entries.len());
    let seed = fresh_course_seed();
    for (entry_index, entry) in meta.source.entries.iter().enumerate() {
        let Some(stage) = resolve_course_stage(
            &meta.path,
            entry_index,
            seed,
            entry,
            &state.resolver.by_group_song,
            &state.resolver.by_song,
            &state.resolver.all_songs,
            &state.resolver.songs_by_group,
            &state.resolver.song_play_counts,
            &state.resolver.song_grade_counts,
            course::CourseType::Endless,
            &selected_song_keys,
            &state.resolver.target_chart_type,
            rating.course_difficulty,
        ) else {
            continue;
        };
        let Some(chart) = stage.song.charts.get(stage.chart_index) else {
            continue;
        };
        selected_song_keys.push(song_unique_key(&stage.song));
        stages.push(CourseStagePlan {
            song: stage.song.clone(),
            chart_hash: chart.short_hash.clone(),
            modifiers: stage.modifiers,
            gain_seconds: stage.gain_seconds,
            gain_lives: stage.gain_lives,
        });
    }
    (!stages.is_empty()).then(|| course_plan(meta, rating, stages))
}

#[inline(always)]
fn selected_banner_path(state: &State) -> Option<&Path> {
    match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(song)) => song.banner_path.as_deref(),
        Some(MusicWheelEntry::PackHeader { banner_path, .. }) => banner_path.as_deref(),
        None => None,
    }
}

#[inline(always)]
fn selected_banner_source(state: &State) -> Option<BannerSource> {
    match state.entries.get(state.selected_index) {
        Some(MusicWheelEntry::Song(song)) if song.banner_path.is_some() => {
            Some(BannerSource::Song(song.clone()))
        }
        Some(MusicWheelEntry::PackHeader {
            banner_path: Some(path),
            ..
        }) => Some(BannerSource::Shared(path.clone())),
        _ => None,
    }
}

fn restore_last_course(state: &mut State, init_view: &SelectCourseInitView) {
    let Some(path) = init_view.last_course_path.as_deref() else {
        return;
    };
    restore_selection_for_course(state, path, init_view.last_course_difficulty.as_deref());
}

/// Synchronizes the shell-owned session time and its retained presentation.
pub fn sync_session_elapsed(state: &mut State, session_elapsed: f32) {
    state.session_elapsed = session_elapsed;
    state.session_timer.sync(session_elapsed);
}

pub fn init(init_view: SelectCourseInitView) -> State {
    let init = build_init_data(&init_view);
    let score_view = SelectCourseScoreView::default();
    let score_text = CourseScoreText::new(&score_view);
    let mut state = State {
        entries: Vec::new(),
        selected_index: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        selection_animation_timer: 0.0,
        wheel_offset_from_selection: 0.0,
        current_banner_key: Arc::<str>::from("banner1.png"),
        session_elapsed: 0.0,
        session_timer: timers::TimerText::default(),
        context: init_view.context,
        players: Default::default(),
        music_wheel: MusicWheelRuntimeView::default(),
        score_view,
        score_text,
        summary_text: CourseSummaryText::empty(),
        course_selection: CourseSelection::default(),
        labels: SelectCourseLabels::load(),
        wheel_content_generation: 0,
        all_entries: init.all_entries,
        course_meta_by_path: init.course_meta_by_path,
        course_text_color_overrides: init.course_text_color_overrides,
        resolver: init.resolver,
        bg: visual_style_bg::State::new(),
        nav_key_held_direction: None,
        nav_key_held_since: None,
        last_requested_banner_source: None,
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
    restore_last_course(&mut state, &init_view);
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
fn music_wheel_hold_spin_speed(state: &State) -> f32 {
    let configured = state.context.policy.music_wheel_switch_speed;
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
    let previous_index = state.selected_index;
    if dist > 0 {
        state.selected_index = (state.selected_index + 1) % len;
        state.wheel_offset_from_selection += 1.0;
    } else {
        state.selected_index = (state.selected_index + len - 1) % len;
        state.wheel_offset_from_selection -= 1.0;
    }
    state.time_since_selection_change = 0.0;
    if state.selected_index != previous_index {
        if !course_selection_matches(state) {
            sync_course_selection(state);
        }
    }
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
    let hold_spin_speed = music_wheel_hold_spin_speed(state);
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
    let current = selected_course_rating_index(state);
    let next = match dir {
        PadDir::Up => (0..current).rev().find(|&idx| meta.ratings[idx].is_some()),
        PadDir::Down => {
            ((current + 1)..meta.ratings.len()).find(|&idx| meta.ratings[idx].is_some())
        }
        _ => None,
    };
    if let Some(next) = next {
        set_selected_course_rating_index(state, next);
        return sfx(if matches!(dir, PadDir::Up) {
            "assets/sounds/easier.ogg"
        } else {
            "assets/sounds/harder.ogg"
        });
    }
    ThemeEffect::None
}

#[inline(always)]
const fn clear_wheel_hold(state: &mut State) {
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
    let current = selected_course_rating_index(state);
    let next = if delta < 0 {
        (0..current).rev().find(|&idx| meta.ratings[idx].is_some())
    } else {
        ((current + 1)..meta.ratings.len()).find(|&idx| meta.ratings[idx].is_some())
    };
    let Some(next) = next else {
        return None;
    };
    set_selected_course_rating_index(state, next);
    Some(if delta < 0 {
        "assets/sounds/easier.ogg"
    } else {
        "assets/sounds/harder.ogg"
    })
}

fn append_rating_cancel_sounds(effects: &mut Vec<ThemeEffect>, undo_sound: Option<&'static str>) {
    let start_len = effects.len();
    if let Some(path) = undo_sound {
        effects.push(sfx(path));
    }
    effects.push(sfx("assets/sounds/change.ogg"));
    debug_assert!(matches!(effects.len() - start_len, 1 | 2));
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

pub fn handle_input(state: &mut State, ev: &InputEvent, effects: &mut Vec<ThemeEffect>) {
    let start_len = effects.len();
    handle_input_impl(state, ev, effects).append_to(effects);
    debug_assert!(effects.len() - start_len <= 2);
}

fn handle_input_impl(
    state: &mut State,
    ev: &InputEvent,
    effects: &mut Vec<ThemeEffect>,
) -> ThemeEffect {
    let dedicated_three_key_nav = state.context.policy.dedicated_three_key_nav;
    let three_key_action = screen_input::three_key_menu_action_enabled(
        &mut state.menu_lr_chord,
        ev,
        dedicated_three_key_nav,
    );
    if dedicated_three_key_nav && matches!(state.three_key_focus, ThreeKeyFocus::Wheel) {
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
                        effects.extend([
                            sfx("assets/sounds/start.ogg"),
                            ThemeEffect::Navigate(Screen::Menu),
                        ]);
                        ThemeEffect::None
                    } else {
                        sfx("assets/sounds/start.ogg")
                    }
                }
            };
        }
        return handle_exit_prompt_input(state, ev, effects);
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

    if dedicated_three_key_nav && let Some((_, nav)) = three_key_action {
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
                    append_rating_cancel_sounds(effects, undo_sound);
                    ThemeEffect::None
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

    let play_style = state.context.play_style;
    if play_style.is_versus() {
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

    match state.context.player_side {
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

pub fn update(state: &mut State, dt: f32, effects: &mut Vec<ThemeEffect>) {
    let start_len = effects.len();
    update_impl(state, dt, effects);
    debug_assert!(effects.len() - start_len <= 1);
}

fn update_impl(state: &mut State, dt: f32, effects: &mut Vec<ThemeEffect>) {
    state.labels.sync();
    let dt = dt.max(0.0);

    match state.out_prompt {
        OutPromptState::PressStartForOptions { elapsed } => {
            let elapsed = elapsed + dt;
            if elapsed >= SHOW_OPTIONS_MESSAGE_SECONDS {
                state.out_prompt = OutPromptState::None;
                effects.push(ThemeEffect::NavigateNoFade(Screen::Gameplay));
                return;
            }
            state.out_prompt = OutPromptState::PressStartForOptions { elapsed };
            return;
        }
        OutPromptState::EnteringOptions { elapsed } => {
            let elapsed = elapsed + dt;
            if elapsed >= ENTERING_OPTIONS_TOTAL_SECONDS {
                state.out_prompt = OutPromptState::None;
                effects.push(ThemeEffect::NavigateNoFade(Screen::PlayerOptions));
                return;
            }
            state.out_prompt = OutPromptState::EnteringOptions { elapsed };
            return;
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
        if !course_selection_matches(state) {
            sync_course_selection(state);
        }
    }

    if state.time_since_selection_change >= BANNER_UPDATE_DELAY_SECONDS {
        let banner = selected_banner_path(state);
        let last_banner = state
            .last_requested_banner_source
            .as_ref()
            .and_then(BannerSource::path);
        if banner != last_banner {
            let request_path = banner.map(Path::to_path_buf);
            let retained_source = selected_banner_source(state);
            state.last_requested_banner_source = retained_source;
            state.banner_high_quality_requested = false;
            effects.push(ThemeEffect::Runtime(
                crate::SimplyLoveRuntimeRequest::Media(crate::SimplyLoveMediaRequest::Banner(
                    request_path,
                )),
            ));
            return;
        }
        if banner.is_some()
            && !state.banner_high_quality_requested
            && state.nav_key_held_direction.is_none()
            && state.wheel_offset_from_selection.abs() < 0.0001
        {
            let request_path = banner.map(Path::to_path_buf);
            state.banner_high_quality_requested = true;
            effects.push(ThemeEffect::Runtime(
                crate::SimplyLoveRuntimeRequest::Media(crate::SimplyLoveMediaRequest::Banner(
                    request_path,
                )),
            ));
            return;
        }
    }

    if selection_changed {
        effects.push(sfx("assets/sounds/change.ogg"));
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
    state.last_requested_banner_source = None;
    state.banner_high_quality_requested = false;
    state.out_prompt = OutPromptState::None;
    state.exit_prompt = ExitPromptState::None;
}

#[inline(always)]
pub const fn allows_late_join(_state: &State) -> bool {
    true
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
fn course_arrow_bounce01(selection_beat: f32, global_offset_seconds: f32) -> f32 {
    // Match SelectMusic arrow timing: effectperiod(1) + effectoffset(-10*GlobalOffsetSeconds).
    let effect_offset = -10.0 * global_offset_seconds;
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
            state.context.play_style,
            None,
        ),
    }
}

/// Compact identity for every field used by the Select Course wheel and score
/// requests. Content rebuilds receive a process-wide revision so replacing a
/// screen state cannot collide with the previous state's token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectCourseRuntimeToken {
    content_generation: u64,
    selected_index: usize,
}

#[inline(always)]
pub const fn runtime_token(state: &State) -> SelectCourseRuntimeToken {
    SelectCourseRuntimeToken {
        content_generation: state.wheel_content_generation,
        selected_index: state.selected_index,
    }
}

#[inline(always)]
pub fn score_runtime_request(state: &State) -> SelectCourseScoreRequest<'_> {
    let course_hash = selected_course_meta(state).map(|meta| meta.score_hash.as_str());
    SelectCourseScoreRequest { course_hash }
}

#[inline(always)]
pub fn sync_context(state: &mut State, context: SelectCourseContextView) {
    let course_filter_changed = state.context.policy.show_random_courses
        != context.policy.show_random_courses
        || state.context.policy.show_most_played_courses != context.policy.show_most_played_courses;
    let music_rate_changed = state.context.music_rate.to_bits() != context.music_rate.to_bits();
    state.context = context;
    if course_filter_changed {
        rebuild_displayed_entries(state);
    } else if music_rate_changed {
        rebuild_course_summary(state);
    }
}

#[inline(always)]
pub fn sync_runtime_view(state: &mut State, view: SelectCourseRuntimeView) {
    if let Some(players) = view.players {
        state.players = players;
    }
    if let Some(music_wheel) = view.music_wheel {
        state.music_wheel = music_wheel;
    }
    if let Some(score) = view.score
        && score != state.score_view
    {
        state.score_text = CourseScoreText::new(&score);
        state.score_view = score;
    }
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    _asset_manager: &AssetManager,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(256);
    let side = state.context.player_side;
    let play_style = state.context.play_style;
    let is_p2_single = profile_data::is_single_p2_side(play_style, side);
    let selected_entry = state.entries.get(state.selected_index);
    let selected_meta = selected_course_meta(state);
    let selected_rating = selected_meta.and_then(|meta| selected_course_rating(state, meta));
    let selected_rating_index = selected_course_rating_index(state);
    let selection_animation_beat = course_selection_anim_beat(state);
    let selected_diff_col = selected_rating.map(|rating| {
        color::difficulty_rgba_with_scheme(
            rating.course_difficulty_name.as_str(),
            state.active_color_index,
            state.context.policy.difficulty_color_scheme,
        )
    });

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
            visual_policy,
        },
    );
    actors.push(sl_select_music_bg_flash());
    screen_bars::push(
        actors,
        state.labels.title.as_ref(),
        std::array::from_fn(|idx| screen_bars::Player {
            joined: state.players[idx].joined,
            guest: state.players[idx].guest,
            display_name: &state.players[idx].display_name,
            avatar_texture_key: state.players[idx].avatar_texture_key.as_deref(),
        }),
        visual_policy,
    );
    actors.push(timers::build_session(
        &state.session_timer,
        visual_policy.machine_font,
    ));

    actors.push(mode_pads::build_label(
        default_scorebox_mode_text(state.score_view.mode_show_ex_score),
        visual_policy.machine_font,
    ));
    actors.extend(mode_pads::build(
        state.context.play_style,
        state.context.joined,
    ));

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

    let songs_label = Arc::clone(&state.labels.songs);
    let songs_value = Arc::clone(&state.summary_text.songs);
    let bpm_text = Arc::clone(&state.summary_text.bpm);
    let len_text = Arc::clone(&state.summary_text.length);
    let desc_text = Arc::clone(&state.summary_text.description);

    let (stats_text, meter_text) = selected_rating.map_or_else(
        || (CourseStatsText::unknown(), unknown_text()),
        |rating| {
            (
                rating.stats_text.clone(),
                Arc::clone(&rating.course_meter_text),
            )
        },
    );

    let pane_sel_col =
        selected_diff_col.unwrap_or_else(|| color::simply_love_rgba(state.active_color_index));
    let pane_cx = if is_p2_single {
        screen_width() * 0.75 + 5.0
    } else {
        screen_width() * 0.25 - 5.0
    };
    select_pane::push_base(
        actors,
        select_pane::StatsPaneParams {
            machine_font: visual_policy.machine_font,
            pane_cx,
            accent_color: pane_sel_col,
            values: select_pane::StatsValues {
                steps: stats_text.steps,
                mines: stats_text.mines,
                jumps: stats_text.jumps,
                hands: stats_text.hands,
                holds: stats_text.holds,
                rolls: stats_text.rolls,
            },
            meter: Some(meter_text),
        },
    );
    let pane_layout = select_pane::layout();
    let lines = [
        (
            Arc::clone(&state.score_text.machine_name),
            Arc::clone(&state.score_text.machine_score),
        ),
        (
            Arc::clone(&state.score_text.player_name),
            Arc::clone(&state.score_text.player_score),
        ),
    ];
    for (i, (name, pct)) in lines.into_iter().enumerate() {
        actors.push(act!(text: font("miso"): settext(name): align(0.5, 0.5): xy(pane_cx + pane_layout.cols[2] - 50.0 * pane_layout.text_zoom, pane_layout.pane_top + pane_layout.rows[i]): maxwidth(30.0): zoom(pane_layout.text_zoom): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
        actors.push(act!(text: font("miso"): settext(pct): align(1.0, 0.5): xy(pane_cx + pane_layout.cols[2] + 25.0 * pane_layout.text_zoom, pane_layout.pane_top + pane_layout.rows[i]): zoom(pane_layout.text_zoom): z(121): diffuse(0.0, 0.0, 0.0, 1.0)));
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
                    act!(text: font("miso"): settext(Arc::clone(&state.labels.bpm)): align(1.0, 0.0): y(10.0): diffuse(0.5, 0.5, 0.5, 1.0): z(52)),
                    act!(text: font("miso"): settext(bpm_text): align(0.0, 0.0): xy(5.0, 10.0): zoomtoheight(15.0): diffuse(1.0, 1.0, 1.0, 1.0): z(52)),
                    act!(text: font("miso"): settext(Arc::clone(&state.labels.length)): align(1.0, 0.0): xy(box_w - 130.0, 10.0): diffuse(0.5, 0.5, 0.5, 1.0): z(52)),
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
                Arc::clone(&entry.index_text),
                Arc::clone(&entry.step_artist),
                selected_diff_col.unwrap_or([0.5, 0.5, 0.5, 1.0]),
            )
        }
        Some(_) => (
            missing_step_index_text(),
            Arc::clone(&state.labels.step_artist),
            selected_diff_col.unwrap_or([0.5, 0.5, 0.5, 1.0]),
        ),
        _ => (
            missing_step_index_text(),
            Arc::clone(&state.labels.step_artist),
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
            let diff_color = color::difficulty_rgba_with_scheme(
                &entry.difficulty,
                state.active_color_index,
                state.context.policy.difficulty_color_scheme,
            );
            let mut meter_actor = act!(text:
                font("miso"):
                settext(Arc::clone(&entry.meter_text)):
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
                settext(Arc::clone(&entry.title)):
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
            settext(Arc::clone(&state.labels.select_hint)):
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
    let rating_len = selected_meta.map_or(0, |meta| meta.ratings.len());
    let rating_top_index = if rating_len > COURSE_RATING_VISIBLE_SLOTS {
        selected_rating_index
            .saturating_sub(COURSE_RATING_VISIBLE_SLOTS - 1)
            .min(rating_len - COURSE_RATING_VISIBLE_SLOTS)
    } else {
        0
    };
    if let Some(meta) = selected_meta {
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
                let color = color::difficulty_rgba_with_scheme(
                    rating.course_difficulty_name.as_str(),
                    state.active_color_index,
                    state.context.policy.difficulty_color_scheme,
                );
                actors.push(act!(text:
                    font(machine_font_key(visual_policy.machine_font, FontRole::Header)):
                    settext(Arc::clone(&rating.course_meter_text)):
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
        let bounce = course_arrow_bounce01(
            selection_animation_beat,
            state.context.policy.global_offset_seconds,
        );
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
            machine_font: visual_policy.machine_font,
            entries: &state.entries,
            selected_index: state.selected_index,
            position_offset_from_selection: state.wheel_offset_from_selection,
            selection_animation_timer: state.selection_animation_timer,
            selection_animation_beat,
            color_pack_headers: true,
            pack_color_indices: None,
            song_box_color: None,
            song_text_color: Some(COURSE_WHEEL_SONG_TEXT_COLOR),
            song_text_color_overrides: Some(&state.course_text_color_overrides),
            show_pack_sync: false,
            show_music_wheel_grades: true,
            show_music_wheel_lamps: true,
            itl_rank_mode: crate::config::SelectMusicItlRankMode::None,
            itl_wheel_mode: crate::config::SelectMusicItlWheelMode::Off,
            song_select_bg_mode: crate::config::SelectMusicSongSelectBgMode::Off,
            song_select_bg_paths: &[],
            song_select_bg_texture_keys: &[],
            expanded_series_name: None,
            expanded_pack_name: None,
            new_pack_names: None,
            default_sync_offset: crate::config::DefaultSyncOffset::Null,
            runtime: &state.music_wheel,
        },
    );

    if !matches!(selected_entry, Some(MusicWheelEntry::Song(_))) {
        actors.push(act!(text:
            font("miso"):
            settext(Arc::clone(&state.labels.pick_prompt)):
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
                    font(machine_font_key(visual_policy.machine_font, FontRole::Header)):
                    settext(Arc::clone(&state.labels.options_prompt)):
                    align(0.5, 0.5):
                    xy(screen_center_x(), screen_center_y()):
                    zoom(0.75):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(1401)
                ));
            }
            OutPromptState::EnteringOptions { .. } => {
                actors.push(act!(text:
                    font(machine_font_key(visual_policy.machine_font, FontRole::Header)):
                    settext(Arc::clone(&state.labels.options_prompt)):
                    align(0.5, 0.5):
                    xy(screen_center_x(), screen_center_y()):
                    zoom(0.75):
                    diffuse(1.0, 1.0, 1.0, 1.0):
                    z(1401):
                    linear(ENTERING_OPTIONS_FADE_OUT_SECONDS): alpha(0.0)
                ));
                actors.push(act!(text:
                    font(machine_font_key(visual_policy.machine_font, FontRole::Header)):
                    settext(Arc::clone(&state.labels.entering_options)):
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
            settext(Arc::clone(&state.labels.exit_prompt)):
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
        let no_label = Arc::clone(&state.labels.no);
        let yes_label = Arc::clone(&state.labels.yes);
        let no_info = Arc::clone(&state.labels.keep_playing);
        let yes_info = Arc::clone(&state.labels.finished);
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
            visual_policy.machine_font,
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
            visual_policy.machine_font,
        );
    }
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(256);
    push_actors(&mut actors, state, asset_manager, Default::default());
    actors
}

#[inline(always)]
const fn begin_exit_prompt(state: &mut State) {
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
    machine_font: crate::config::MachineFont,
) {
    let mut rgba = [1.0; 4];
    if active {
        rgba = active_rgba;
    }
    rgba[3] *= alpha;

    out.push(act!(text:
        align(0.5, 0.5):
        xy(cx, cy):
        font(machine_font_key(machine_font, FontRole::Header)):
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

fn handle_exit_prompt_input(
    state: &mut State,
    ev: &InputEvent,
    effects: &mut Vec<ThemeEffect>,
) -> ThemeEffect {
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
                effects.extend([
                    sfx("assets/sounds/start.ogg"),
                    ThemeEffect::Navigate(Screen::Menu),
                ]);
                ThemeEffect::None
            } else {
                sfx("assets/sounds/start.ogg")
            }
        }

        _ => ThemeEffect::None,
    }
}

#[cfg(test)]
mod effect_buffer_tests {
    use super::*;
    use deadsync_core::input::InputSource;
    use deadsync_theme::AudioRequest;

    fn press(action: VirtualAction) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed: true,
            source: InputSource::Keyboard,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    fn is_sfx(effect: &ThemeEffect, expected: &str) -> bool {
        matches!(
            effect,
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                AudioRequest::PlaySfx(path)
            )) if *path == expected
        )
    }

    #[test]
    fn rating_cancel_appends_undo_sound_before_change_sound() {
        let mut effects = Vec::with_capacity(8);
        append_rating_cancel_sounds(&mut effects, Some("assets/sounds/easier.ogg"));

        assert_eq!(effects.capacity(), 8);
        assert_eq!(effects.len(), 2);
        assert!(is_sfx(&effects[0], "assets/sounds/easier.ogg"));
        assert!(is_sfx(&effects[1], "assets/sounds/change.ogg"));
    }

    #[test]
    fn exit_confirm_appends_start_sound_before_navigation() {
        let mut state = init(SelectCourseInitView::default());
        begin_exit_prompt(&mut state);
        let ExitPromptState::Active { active_choice, .. } = &mut state.exit_prompt else {
            unreachable!();
        };
        *active_choice = 1;
        let mut effects = Vec::with_capacity(8);

        handle_input(&mut state, &press(VirtualAction::p1_start), &mut effects);

        assert_eq!(effects.capacity(), 8);
        assert_eq!(effects.len(), 2);
        assert!(is_sfx(&effects[0], "assets/sounds/start.ogg"));
        assert!(matches!(effects[1], ThemeEffect::Navigate(Screen::Menu)));
    }

    #[test]
    fn selection_update_separates_sound_from_delayed_banner_request() {
        let mut state = init(SelectCourseInitView::default());
        state.selected_index = 1;
        state.last_requested_banner_source = Some(super::BannerSource::Shared(Arc::from(
            PathBuf::from("old-banner"),
        )));
        state.time_since_selection_change = BANNER_UPDATE_DELAY_SECONDS;
        let mut effects = Vec::with_capacity(8);

        update(&mut state, 0.0, &mut effects);

        assert_eq!(effects.capacity(), 8);
        assert_eq!(effects.len(), 1);
        assert!(is_sfx(&effects[0], "assets/sounds/change.ogg"));

        effects.clear();
        update(&mut state, BANNER_UPDATE_DELAY_SECONDS, &mut effects);

        assert!(matches!(
            effects.as_slice(),
            [ThemeEffect::Runtime(
                crate::SimplyLoveRuntimeRequest::Media(crate::SimplyLoveMediaRequest::Banner(None))
            )]
        ));
    }

    #[test]
    fn settled_banner_retains_wheel_source_without_repeat_request() {
        let mut state = init(SelectCourseInitView::default());
        let banner: Arc<Path> = Arc::from(PathBuf::from("course-banner.png"));
        state.entries = vec![MusicWheelEntry::PackHeader {
            name: Arc::from("Course"),
            original_index: 0,
            banner_path: Some(banner.clone()),
            song_count: 1,
            pack_key: Some(Arc::from("Course")),
            parent_series: None,
        }];
        state.selected_index = 0;
        state.prev_selected_index = 0;
        state.time_since_selection_change = BANNER_UPDATE_DELAY_SECONDS;
        let mut effects = Vec::with_capacity(8);

        update(&mut state, 0.0, &mut effects);

        assert!(matches!(
            effects.as_slice(),
            [ThemeEffect::Runtime(
                crate::SimplyLoveRuntimeRequest::Media(crate::SimplyLoveMediaRequest::Banner(
                    Some(path)
                ))
            )] if path == &PathBuf::from("course-banner.png")
        ));
        let Some(super::BannerSource::Shared(retained)) =
            state.last_requested_banner_source.as_ref()
        else {
            panic!("banner state should retain the selected wheel source");
        };
        assert!(Arc::ptr_eq(retained, &banner));

        state.banner_high_quality_requested = true;
        effects.clear();
        update(&mut state, 0.0, &mut effects);
        assert!(effects.is_empty());
    }
}

#[cfg(test)]
mod song_lookup_tests {
    use super::*;
    use deadlib_present::actors::TextContent;
    use deadsync_chart::SyncPref;

    #[test]
    fn course_mode_label_uses_static_actor_text() {
        let mut state = init(SelectCourseInitView::default());
        for (show_ex, expected) in [(false, "ITG"), (true, "EX")] {
            state.score_view.mode_show_ex_score = show_ex;
            let actors = get_actors(&state, &AssetManager::new());
            assert!(actors.iter().any(|actor| {
                matches!(
                    actor,
                    Actor::Text {
                        content: TextContent::Static(text),
                        ..
                    } if *text == expected
                )
            }));
        }
    }

    #[test]
    fn course_song_entry_compiles_actor_text_once() {
        let entry = CourseSongEntry::new(
            "Song Title".to_owned(),
            "hard".to_owned(),
            12,
            2,
            "Step Artist".to_owned(),
        );

        assert_eq!(entry.title.as_ref(), "Song Title");
        assert_eq!(entry.meter_text.as_ref(), "12");
        assert_eq!(entry.index_text.as_ref(), "#3");
        assert_eq!(entry.step_artist.as_ref(), "Step Artist");

        let title = Arc::clone(&entry.title);
        assert!(Arc::ptr_eq(&title, &entry.title));
    }

    #[test]
    fn course_score_text_rebuilds_only_for_changed_score_data() {
        let mut state = init(SelectCourseInitView::default());
        let score = SelectCourseScoreView {
            player_initials: "AAA".to_owned(),
            player_score_percent: Some(0.95),
            machine_initials: Some("MCH".to_owned()),
            machine_score_percent: Some(0.99),
            ..Default::default()
        };
        sync_runtime_view(
            &mut state,
            SelectCourseRuntimeView {
                score: Some(score.clone()),
                ..Default::default()
            },
        );
        let player_name = Arc::clone(&state.score_text.player_name);
        let player_score = Arc::clone(&state.score_text.player_score);

        assert_eq!(player_name.as_ref(), "AAA");
        assert_eq!(player_score.as_ref(), "95.00%");

        sync_runtime_view(
            &mut state,
            SelectCourseRuntimeView {
                score: Some(score),
                ..Default::default()
            },
        );
        assert!(Arc::ptr_eq(&state.score_text.player_name, &player_name));
        assert!(Arc::ptr_eq(&state.score_text.player_score, &player_score));

        sync_runtime_view(
            &mut state,
            SelectCourseRuntimeView {
                score: Some(SelectCourseScoreView {
                    player_initials: "BBB".to_owned(),
                    player_score_percent: Some(0.96),
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        assert_eq!(state.score_text.player_name.as_ref(), "BBB");
        assert_eq!(state.score_text.player_score.as_ref(), "96.00%");
        assert!(!Arc::ptr_eq(&state.score_text.player_name, &player_name));
    }

    #[test]
    fn course_summary_compiles_actor_text_at_transition_time() {
        let summary = CourseSummaryText::selected(
            Arc::from("3"),
            Some(120.0),
            Some(180.0),
            125,
            2.0,
            Arc::from("A short course"),
        );

        assert_eq!(summary.songs.as_ref(), "3");
        assert_eq!(summary.bpm.as_ref(), "120-180");
        assert_eq!(summary.length.as_ref(), "1:03");
        assert_eq!(summary.description.as_ref(), "A short course");
    }

    #[test]
    fn course_stats_compile_actor_text_once() {
        let stats = CourseStatsText::new(
            &CourseTotals {
                steps: 100,
                jumps: 20,
                holds: 10,
                mines: 5,
                hands: 4,
                rolls: 3,
            },
            true,
        );

        assert_eq!(stats.steps.as_ref(), "100");
        assert_eq!(stats.jumps.as_ref(), "20");
        assert_eq!(stats.holds.as_ref(), "10");
        assert_eq!(stats.mines.as_ref(), "5");
        assert_eq!(stats.hands.as_ref(), "4");
        assert_eq!(stats.rolls.as_ref(), "3");

        let steps = Arc::clone(&stats.steps);
        assert!(Arc::ptr_eq(&steps, &stats.steps));
    }

    #[test]
    fn course_labels_remain_shared_while_locale_is_unchanged() {
        let mut labels = SelectCourseLabels::load();
        let title = Arc::clone(&labels.title);

        assert!(!labels.sync());
        assert!(Arc::ptr_eq(&labels.title, &title));
    }

    #[test]
    fn session_elapsed_sync_retains_text_until_the_visible_second_changes() {
        let mut state = init(SelectCourseInitView::default());
        sync_session_elapsed(&mut state, 125.1);
        let first = state.session_timer.text().to_owned();

        assert_eq!(first, "02:05");
        sync_session_elapsed(&mut state, 125.9);
        assert_eq!(state.session_timer.text(), first);

        sync_session_elapsed(&mut state, 126.0);
        assert_eq!(state.session_timer.text(), "02:06");
        assert_ne!(state.session_timer.text(), first);
    }

    #[test]
    fn runtime_updates_retain_clean_subviews_and_revise_filtered_content() {
        let mut state = init(SelectCourseInitView::default());
        let initial_token = runtime_token(&state);
        let mut context = state.context;
        context.policy.show_random_courses = !context.policy.show_random_courses;
        sync_context(&mut state, context);
        assert_ne!(initial_token, runtime_token(&state));

        sync_runtime_view(
            &mut state,
            SelectCourseRuntimeView {
                players: Some([
                    SelectFlowPlayerView {
                        joined: true,
                        display_name: "Alice".to_owned(),
                        ..Default::default()
                    },
                    Default::default(),
                ]),
                music_wheel: Some(MusicWheelRuntimeView {
                    translated_titles: true,
                    ..Default::default()
                }),
                score: Some(SelectCourseScoreView {
                    player_initials: "AAA".to_owned(),
                    player_score_percent: Some(0.95),
                    ..Default::default()
                }),
            },
        );
        sync_runtime_view(&mut state, SelectCourseRuntimeView::default());

        assert_eq!(state.players[0].display_name, "Alice");
        assert!(state.music_wheel.translated_titles);
        assert_eq!(state.score_view.player_initials, "AAA");
        assert_eq!(state.score_view.player_score_percent, Some(0.95));
    }

    fn chart(hash: &str) -> ChartData {
        ChartData {
            chart_type: "dance-single".to_owned(),
            difficulty: "Hard".to_owned(),
            description: String::new(),
            chart_name: String::new(),
            meter: 10,
            step_artist: String::new(),
            music_path: None,
            short_hash: hash.to_owned(),
            stats: Default::default(),
            tech_counts: Default::default(),
            mines_nonfake: 0,
            stamina_counts: Default::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            matrix_profile: Box::default(),
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

    fn song(path: &str, chart_hashes: &[&str]) -> Arc<SongData> {
        Arc::new(SongData {
            simfile_path: PathBuf::from(path),
            title: String::new(),
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
            charts: chart_hashes.iter().map(|hash| chart(hash)).collect(),
        })
    }

    fn pack(name: &str, songs: Vec<Arc<SongData>>) -> SongPack {
        SongPack {
            group_name: name.to_owned(),
            name: name.to_owned(),
            sort_title: String::new(),
            translit_title: String::new(),
            series: String::new(),
            folder_series: String::new(),
            year: 0,
            sync_pref: SyncPref::Default,
            directory: PathBuf::from("Songs").join(name),
            banner_path: None,
            songs,
        }
    }

    #[test]
    fn song_lookup_handles_sparse_duplicate_play_counts() {
        let alpha = song("Songs/Pack A/Alpha/alpha.ssc", &["duplicate", "alpha"]);
        let beta = song("Songs/Pack A/Beta/beta.ssc", &["beta"]);
        let gamma = song("Songs/Pack B/Gamma/gamma.ssc", &["duplicate"]);
        let packs = [
            pack("Pack A", vec![alpha.clone(), beta.clone()]),
            pack("Pack B", vec![gamma.clone()]),
            pack("Empty Pack", Vec::new()),
        ];
        let played = vec![
            ("duplicate".to_owned(), 5),
            ("duplicate".to_owned(), 7),
            ("alpha".to_owned(), u32::MAX),
            ("beta".to_owned(), 3),
            ("missing".to_owned(), 99),
        ];

        let (_, _, groups, all, counts) = build_song_lookup(&packs, &played);
        assert_eq!(groups.len(), 2, "empty packs must not create lookup groups");
        assert!(groups.contains_key("Pack A"));
        assert!(!groups.contains_key("pack a"));
        assert_eq!(all.len(), 3);
        assert_eq!(counts.get(&song_unique_key(&alpha)), Some(&u32::MAX));
        assert_eq!(counts.get(&song_unique_key(&beta)), Some(&3));
        assert!(!counts.contains_key(&song_unique_key(&gamma)));
    }
}
