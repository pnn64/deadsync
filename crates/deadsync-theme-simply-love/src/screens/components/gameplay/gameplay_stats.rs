use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{LookupKey, lookup_key, tr};
use crate::assets::{FontRole, machine_font_key};
use crate::screens::components::gameplay::score_counter::{
    ScoreCounterParams, prewarm_score_counter_layout, push_score_counter,
};
use crate::screens::components::gameplay::step_stats_gifs;
use crate::screens::components::shared::heart_rate;
use crate::screens::gameplay::{self as gameplay_screen, GameplayCoreState, State};
use crate::step_stats as step_stats_theme;
use crate::step_stats::{
    STEP_STATS_BANNER_H, STEP_STATS_BANNER_W, StepStatsGraphRect, StepStatsPaneLayout,
    StepStatsPaneParams,
};
use deadlib_present::actors::{Actor, InlineText, SizeSpec, TextAlign, TextContent};
#[cfg(feature = "bench-support")]
use deadlib_present::cache::{SharedStrCache, cached_shared_str};
#[cfg(feature = "bench-support")]
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadlib_present::color;
use deadlib_present::compose::{ComposeScratch, TextLayoutCache, prewarm_frame_inline_text_slot};
use deadlib_present::density;
use deadlib_present::font;
use deadlib_present::space::*;
use deadlib_render::BlendMode;
use deadsync_core::input::MAX_PLAYERS;
#[cfg(feature = "bench-support")]
use deadsync_gameplay::{FantasticWindowOptions, blue_fantastic_window_ms};
use deadsync_profile as profile_data;
use deadsync_profile_gameplay::score_display_mode_from_profile;
use deadsync_rules::judgment::{self, JudgeGrade};
use deadsync_rules::timing::LiveTimingSnapshot;
use std::cell::RefCell;
#[cfg(feature = "bench-support")]
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use super::{FRAME_TEXT_LIVE_TIMING_BASE, FRAME_TEXT_VERTEX_BUFFERS};

#[cfg(feature = "bench-support")]
const TEXT_CACHE_LIMIT: usize = 8192;
const COUNT_PREWARM_CAP: u32 = 2048;
const TIME_PREWARM_CAP_S: u32 = 600;
const PEAK_NPS_GRAPH_PAD: f32 = 4.0;
const PEAK_NPS_ALPHA: f32 = 0.75;
const DISABLED_WINDOW_RGBA: [f32; 4] = color::JUDGMENT_FA_PLUS_WHITE_EVAL_DIM_RGBA;

static EMPTY_STATS_TEXT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from(""));

#[cfg(feature = "bench-support")]
thread_local! {
    static BENCH_STATS_STR_CACHE: RefCell<SharedStrCache> =
        RefCell::new(HashMap::with_capacity(16));
}

struct FixedTextCache<K, V, const N: usize> {
    entries: [Option<(K, V)>; N],
    last: Option<(K, V)>,
    next: usize,
}

impl<K: Copy + Eq, V: Copy, const N: usize> FixedTextCache<K, V, N> {
    const fn new() -> Self {
        Self {
            entries: [None; N],
            last: None,
            next: 0,
        }
    }

    #[inline(always)]
    fn get_or_insert(&mut self, key: K, build: impl FnOnce() -> V) -> V {
        if let Some((cached, value)) = self.last
            && cached == key
        {
            return value;
        }
        if let Some(entry) = self
            .entries
            .iter()
            .flatten()
            .find(|(cached, _)| *cached == key)
            .copied()
        {
            self.last = Some(entry);
            return entry.1;
        }
        let value = build();
        self.entries[self.next] = Some((key, value));
        self.last = Some((key, value));
        self.next = (self.next + 1) % N;
        value
    }
}

struct SlotTextCache<K, V, const N: usize> {
    entries: [Option<(K, V)>; N],
}

impl<K: Copy + Eq, V: Copy, const N: usize> SlotTextCache<K, V, N> {
    const fn new() -> Self {
        Self { entries: [None; N] }
    }

    #[inline(always)]
    fn get_or_insert(&mut self, slot: usize, key: K, build: impl FnOnce() -> V) -> V {
        let entry = &mut self.entries[slot];
        if let Some((cached, value)) = *entry
            && cached == key
        {
            return value;
        }
        let value = build();
        *entry = Some((key, value));
        value
    }
}

// Gameplay-thread-only, session-lifetime inline memo tables. They have fixed
// entry caps, are populated during song-load text prewarm and first use, and
// never allocate, scan-prune, or destroy heap resources. A miss performs at
// most N key comparisons plus <=14 bytes of stack formatting, then overwrites
// one slot round-robin. Their compile-time bounds are their instrumentation:
// full-width padded values retain 32 visible HUD keys, game-time text and
// widths retain 8, and live timing has one direct slot for each of the 6
// two-player stat pairs.
// Destruction occurs at thread shutdown. Split judgment counts bypass a cache
// entirely by combining a static zero run with inline decimal text.
thread_local! {
    static PADDED_NUM_INLINE_CACHE: RefCell<FixedTextCache<(u32, u8), InlineText, 32>> = const {
        RefCell::new(FixedTextCache::new())
    };
    static GAME_TIME_INLINE_CACHE: RefCell<FixedTextCache<(u32, u8), InlineText, 8>> = const {
        RefCell::new(FixedTextCache::new())
    };
    static GAME_TIME_WIDTH_INLINE_CACHE: RefCell<FixedTextCache<(u32, u8), f32, 8>> = const {
        RefCell::new(FixedTextCache::new())
    };
    static LIVE_TIMING_INLINE_CACHE: RefCell<SlotTextCache<(i32, i32), InlineText, 6>> = const {
        RefCell::new(SlotTextCache::new())
    };
}

#[cfg(feature = "bench-support")]
thread_local! {
    static BENCH_PADDED_NUM_CACHE: RefCell<TextCache<(u32, u8)>> = RefCell::new(text_cache_with_capacity(2048));
    static BENCH_PADDED_DIM_CACHE: RefCell<TextCache<(u32, u8)>> = RefCell::new(text_cache_with_capacity(2048));
    static BENCH_PADDED_BRIGHT_CACHE: RefCell<TextCache<(u32, u8)>> = RefCell::new(text_cache_with_capacity(2048));
    static BENCH_GAME_TIME_CACHE: RefCell<TextCache<(u32, u8)>> = RefCell::new(text_cache_with_capacity(1024));
    static BENCH_LIVE_TIMING_CACHE: RefCell<TextCache<(i32, i32)>> = RefCell::new(text_cache_with_capacity(4096));
    static BENCH_BLUE_WINDOW_LABEL_CACHE: RefCell<TextCache<i32>> = RefCell::new(text_cache_with_capacity(64));
    static BENCH_PEAK_NPS_CACHE: RefCell<TextCache<u32>> = RefCell::new(text_cache_with_capacity(512));
}

static DIGIT_TEXT: LazyLock<[Arc<str>; 10]> =
    LazyLock::new(|| ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"].map(Arc::<str>::from));
const ZERO_RUNS: [&str; 10] = [
    "",
    "0",
    "00",
    "000",
    "0000",
    "00000",
    "000000",
    "0000000",
    "00000000",
    "000000000",
];
static SLASH_TEXT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::<str>::from("/"));
static LIVE_TIMING_LABELS: LazyLock<[Arc<str>; 3]> = LazyLock::new(|| {
    [
        Arc::<str>::from("Mean (64n/All [ms])"),
        Arc::<str>::from("Mean Abs (64n/All [ms])"),
        Arc::<str>::from("Max (64n/All [ms])"),
    ]
});

#[inline(always)]
fn gameplay_font_key(state: &State, role: FontRole) -> &'static str {
    machine_font_key(state.machine_font(), role)
}

#[derive(Clone, Copy)]
struct LabeledColor {
    label: LookupKey,
    color: [f32; 4],
}

const JUDGMENT_INFO: [LabeledColor; 6] = [
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentFantastic"),
        color: color::JUDGMENT_RGBA[0],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentExcellent"),
        color: color::JUDGMENT_RGBA[1],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentGreat"),
        color: color::JUDGMENT_RGBA[2],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentDecent"),
        color: color::JUDGMENT_RGBA[3],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentWayOff"),
        color: color::JUDGMENT_RGBA[4],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentMiss"),
        color: color::JUDGMENT_RGBA[5],
    },
];

const STEP_INFO_LABELS: [LookupKey; 4] = [
    lookup_key("Gameplay", "SongInfoSong"),
    lookup_key("Gameplay", "SongInfoArtist"),
    lookup_key("Gameplay", "SongInfoPack"),
    lookup_key("Gameplay", "SongInfoDesc"),
];
const STEP_INFO_COURSE_LABELS: [LookupKey; 4] = [
    lookup_key("Gameplay", "SongInfoSong"),
    lookup_key("Gameplay", "SongInfoArtist"),
    lookup_key("Gameplay", "SongInfoCourse"),
    lookup_key("Gameplay", "SongInfoDesc"),
];

const HOLDS_MINES_ROLLS_LABELS: [LookupKey; 3] = [
    lookup_key("Gameplay", "HoldsLabel"),
    lookup_key("Gameplay", "MinesLabel"),
    lookup_key("Gameplay", "RollsLabel"),
];

#[inline(always)]
#[cfg(feature = "bench-support")]
fn step_info_label(index: usize, course: bool) -> Arc<str> {
    let labels = if course {
        &STEP_INFO_COURSE_LABELS
    } else {
        &STEP_INFO_LABELS
    };
    labels
        .get(index)
        .map(LookupKey::get)
        .unwrap_or_else(|| Arc::from(""))
}

#[cfg(feature = "bench-support")]
fn holds_mines_rolls_label(index: usize) -> Arc<str> {
    HOLDS_MINES_ROLLS_LABELS
        .get(index)
        .map(LookupKey::get)
        .unwrap_or_else(|| Arc::from(""))
}

#[cfg(feature = "bench-support")]
fn judgment_label(index: usize) -> Arc<str> {
    JUDGMENT_INFO
        .get(index)
        .map(|info| info.label.get())
        .unwrap_or_else(|| Arc::from(""))
}

fn time_remaining_text(state: &State) -> Arc<str> {
    Arc::clone(&state.gameplay_stats_text.time_remaining)
}

fn time_total_text(state: &State) -> Arc<str> {
    Arc::clone(&state.gameplay_stats_text.time_total)
}

/// Song-lifetime text identities used by Step Statistics composition.
///
/// The gameplay thread owns one song-lifetime plan. Localization and owned
/// chart strings are resolved at screen entry; live frames only clone `Arc`
/// handles and index fixed arrays. Practice refreshes peak NPS only on its
/// explicit rate-change boundary. There is no cache lookup, growth, eviction,
/// or miss path during ordinary frames, and all strings die with the screen.
pub(crate) struct GameplayStatsTextPlan {
    step_info: [Arc<str>; 4],
    holds_mines_rolls: [Arc<str>; 3],
    judgments: [Arc<str>; 6],
    time_remaining: Arc<str>,
    time_total: Arc<str>,
    song_artist: Arc<str>,
    descriptions: [[Arc<str>; 2]; MAX_PLAYERS],
    description_counts: [u8; MAX_PLAYERS],
    peak_nps: [Arc<str>; MAX_PLAYERS],
    blue_window: [Arc<str>; MAX_PLAYERS],
    blue_window_ms: [f32; MAX_PLAYERS],
}

impl GameplayStatsTextPlan {
    pub(crate) fn from_gameplay(gameplay: &GameplayCoreState, course: bool) -> Self {
        let descriptions = std::array::from_fn(|player| {
            let chart = &gameplay.charts()[player];
            (chart.description.as_str(), chart.step_artist.as_str())
        });
        let rate = gameplay.music_rate();
        let peak_nps = std::array::from_fn(|player| {
            peak_nps_text((gameplay.charts()[player].max_nps as f32 * rate).max(0.0))
        });
        let blue_window_ms = std::array::from_fn(|player| gameplay.player_blue_window_ms(player));
        let blue_window = blue_window_ms.map(|ms| blue_window_label(ms.round() as i32));
        Self::from_parts_with_derived(
            gameplay.song().artist.as_str(),
            descriptions,
            course,
            peak_nps,
            blue_window,
            blue_window_ms,
        )
    }

    #[cfg(feature = "bench-support")]
    fn from_parts(
        song_artist: &str,
        chart_text: [(&str, &str); MAX_PLAYERS],
        course: bool,
    ) -> Self {
        Self::from_parts_with_derived(
            song_artist,
            chart_text,
            course,
            std::array::from_fn(|_| peak_nps_text(0.0)),
            std::array::from_fn(|_| blue_window_label(0)),
            [0.0; MAX_PLAYERS],
        )
    }

    fn from_parts_with_derived(
        song_artist: &str,
        chart_text: [(&str, &str); MAX_PLAYERS],
        course: bool,
        peak_nps: [Arc<str>; MAX_PLAYERS],
        blue_window: [Arc<str>; MAX_PLAYERS],
        blue_window_ms: [f32; MAX_PLAYERS],
    ) -> Self {
        let labels = if course {
            &STEP_INFO_COURSE_LABELS
        } else {
            &STEP_INFO_LABELS
        };
        let mut description_counts = [0u8; MAX_PLAYERS];
        let descriptions = std::array::from_fn(|player| {
            let desc = chart_text[player].0.trim();
            let credit = chart_text[player].1.trim();
            let mut values = [Arc::clone(&EMPTY_STATS_TEXT), Arc::clone(&EMPTY_STATS_TEXT)];
            if !desc.is_empty() {
                values[usize::from(description_counts[player])] = Arc::from(desc);
                description_counts[player] += 1;
            }
            if !credit.is_empty() && credit != desc && description_counts[player] < 2 {
                values[usize::from(description_counts[player])] = Arc::from(credit);
                description_counts[player] += 1;
            }
            values
        });
        Self {
            step_info: std::array::from_fn(|index| labels[index].get()),
            holds_mines_rolls: std::array::from_fn(|index| HOLDS_MINES_ROLLS_LABELS[index].get()),
            judgments: std::array::from_fn(|index| JUDGMENT_INFO[index].label.get()),
            time_remaining: tr("Gameplay", "TimeRemaining"),
            time_total: tr("Gameplay", ["TimeSong", "TimeCourse"][usize::from(course)]),
            song_artist: Arc::from(song_artist),
            descriptions,
            description_counts,
            peak_nps,
            blue_window,
            blue_window_ms,
        }
    }

    #[inline(always)]
    fn step_info(&self, index: usize) -> Arc<str> {
        self.step_info
            .get(index)
            .map_or_else(|| Arc::clone(&EMPTY_STATS_TEXT), Arc::clone)
    }

    #[inline(always)]
    fn holds_mines_rolls(&self, index: usize) -> Arc<str> {
        self.holds_mines_rolls
            .get(index)
            .map_or_else(|| Arc::clone(&EMPTY_STATS_TEXT), Arc::clone)
    }

    #[inline(always)]
    fn judgment(&self, index: usize) -> Arc<str> {
        self.judgments
            .get(index)
            .map_or_else(|| Arc::clone(&EMPTY_STATS_TEXT), Arc::clone)
    }

    #[inline(always)]
    fn description(&self, player: usize, elapsed: f32) -> Arc<str> {
        let Some(values) = self.descriptions.get(player) else {
            return Arc::from("");
        };
        let count = usize::from(self.description_counts[player]);
        if count == 0 {
            return Arc::clone(&values[0]);
        }
        let index = ((elapsed / 2.0).floor() as usize) % count;
        Arc::clone(&values[index])
    }

    #[inline(always)]
    fn peak_nps(&self, player: usize) -> Arc<str> {
        self.peak_nps
            .get(player)
            .map_or_else(|| Arc::clone(&EMPTY_STATS_TEXT), Arc::clone)
    }

    #[inline(always)]
    fn blue_window(&self, player: usize) -> Arc<str> {
        self.blue_window
            .get(player)
            .map_or_else(|| Arc::clone(&EMPTY_STATS_TEXT), Arc::clone)
    }

    #[inline(always)]
    fn blue_window_ms(&self, player: usize) -> f32 {
        self.blue_window_ms.get(player).copied().unwrap_or_default()
    }

    pub(crate) fn sync_music_rate(&mut self, max_nps: [f32; MAX_PLAYERS], rate: f32) {
        self.peak_nps = max_nps.map(|peak| peak_nps_text((peak * rate).max(0.0)));
    }
}

#[cfg(feature = "bench-support")]
#[inline(always)]
fn stats_text_checksum(checksum: usize, text: &Arc<str>) -> usize {
    checksum.rotate_left(5) ^ text.len()
}

/// Focused old/new harness for immutable Step Statistics text resolution.
#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GameplayStatsTextBenchmark {
    plan: GameplayStatsTextPlan,
    artist: String,
    chart_text: [(String, String); MAX_PLAYERS],
}

#[cfg(feature = "bench-support")]
impl GameplayStatsTextBenchmark {
    pub fn new() -> Self {
        let artist = "Benchmark Artist".to_string();
        let chart_text = [
            ("Hard 11".to_string(), "P1 Credit".to_string()),
            ("Expert 13".to_string(), "P2 Credit".to_string()),
        ];
        let plan = GameplayStatsTextPlan::from_parts(
            artist.as_str(),
            std::array::from_fn(|player| {
                (chart_text[player].0.as_str(), chart_text[player].1.as_str())
            }),
            false,
        );
        let benchmark = Self {
            plan,
            artist,
            chart_text,
        };
        std::hint::black_box(benchmark.legacy_frame(4.25));
        benchmark
    }

    pub fn legacy_frame(&self, elapsed_seconds: f32) -> usize {
        let mut checksum = 0usize;
        for index in 0..4 {
            checksum = stats_text_checksum(checksum, &step_info_label(index, false));
        }
        for index in 0..3 {
            checksum = stats_text_checksum(checksum, &holds_mines_rolls_label(index));
        }
        for index in 0..6 {
            checksum = stats_text_checksum(checksum, &judgment_label(index));
        }
        checksum = stats_text_checksum(checksum, &tr("Gameplay", "TimeRemaining"));
        checksum = stats_text_checksum(checksum, &tr("Gameplay", "TimeSong"));
        checksum = stats_text_checksum(
            checksum,
            &cached_shared_str(&BENCH_STATS_STR_CACHE, &self.artist, TEXT_CACHE_LIMIT),
        );
        for (description, credit) in &self.chart_text {
            let texts = [description.as_str(), credit.as_str()];
            let index = ((elapsed_seconds / 2.0).floor() as usize) % texts.len();
            checksum = stats_text_checksum(
                checksum,
                &cached_shared_str(&BENCH_STATS_STR_CACHE, texts[index], TEXT_CACHE_LIMIT),
            );
        }
        std::hint::black_box(checksum)
    }

    pub fn prewarmed_frame(&self, elapsed_seconds: f32) -> usize {
        let mut checksum = 0usize;
        for index in 0..4 {
            checksum = stats_text_checksum(checksum, &self.plan.step_info(index));
        }
        for index in 0..3 {
            checksum = stats_text_checksum(checksum, &self.plan.holds_mines_rolls(index));
        }
        for index in 0..6 {
            checksum = stats_text_checksum(checksum, &self.plan.judgment(index));
        }
        checksum = stats_text_checksum(checksum, &Arc::clone(&self.plan.time_remaining));
        checksum = stats_text_checksum(checksum, &Arc::clone(&self.plan.time_total));
        checksum = stats_text_checksum(checksum, &Arc::clone(&self.plan.song_artist));
        for player in 0..MAX_PLAYERS {
            checksum =
                stats_text_checksum(checksum, &self.plan.description(player, elapsed_seconds));
        }
        std::hint::black_box(checksum)
    }

    pub fn behavior_matches(&self) -> bool {
        [0.0, 2.0, 4.25, 7.9]
            .into_iter()
            .all(|elapsed| self.legacy_frame(elapsed) == self.prewarmed_frame(elapsed))
    }
}

/// Focused old/new harness for song-invariant derived Step Statistics data.
#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GameplayStatsDerivedTextBenchmark {
    plan: GameplayStatsTextPlan,
    max_nps: [f32; MAX_PLAYERS],
    rate: f32,
}

#[cfg(feature = "bench-support")]
impl Default for GameplayStatsDerivedTextBenchmark {
    fn default() -> Self {
        let max_nps = [12.75, 18.5];
        let rate = 1.1;
        let peak_nps = max_nps.map(|peak| peak_nps_text(peak * rate));
        let blue_window_ms = std::array::from_fn(benchmark_blue_window_ms);
        let blue_window = blue_window_ms.map(|ms| blue_window_label(ms.round() as i32));
        Self {
            plan: GameplayStatsTextPlan::from_parts_with_derived(
                "Artist",
                [("Hard 11", "P1 Credit"), ("Expert 13", "P2 Credit")],
                false,
                peak_nps,
                blue_window,
                blue_window_ms,
            ),
            max_nps,
            rate,
        }
    }
}

#[cfg(feature = "bench-support")]
fn benchmark_blue_window_ms(player: usize) -> f32 {
    blue_fantastic_window_ms(FantasticWindowOptions {
        base_fa_plus_s: 0.015,
        custom_fantastic_window_s: (player == 0).then_some(0.012),
        fa_plus_10ms_blue_window: player == 1,
    })
}

#[cfg(feature = "bench-support")]
impl GameplayStatsDerivedTextBenchmark {
    const SAMPLES: usize = 256;

    pub fn legacy_peak_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let player = sample % MAX_PLAYERS;
            let peak = (std::hint::black_box(self.max_nps[player]) * self.rate).max(0.0);
            let text = cached_peak_nps_text_legacy(peak);
            checksum.rotate_left(7) ^ text.len() ^ sample
        })
    }

    pub fn planned_peak_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let player = sample % MAX_PLAYERS;
            let text = self.plan.peak_nps(std::hint::black_box(player));
            checksum.rotate_left(7) ^ text.len() ^ sample
        })
    }

    pub fn legacy_blue_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let player = std::hint::black_box(sample % MAX_PLAYERS);
            let ms = benchmark_blue_window_ms(player);
            let text = cached_blue_window_label_legacy(ms.round() as i32);
            checksum.rotate_left(7) ^ text.len() ^ ms.to_bits() as usize ^ sample
        })
    }

    pub fn planned_blue_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let player = std::hint::black_box(sample % MAX_PLAYERS);
            let ms = self.plan.blue_window_ms(player);
            let text = self.plan.blue_window(player);
            checksum.rotate_left(7) ^ text.len() ^ ms.to_bits() as usize ^ sample
        })
    }

    pub fn behavior_matches(&self) -> bool {
        self.legacy_peak_frame(17) == self.planned_peak_frame(17)
            && self.legacy_blue_frame(17) == self.planned_blue_frame(17)
    }
}

#[cfg(test)]
mod gameplay_stats_text_plan_tests {
    use super::*;

    #[test]
    fn prewarmed_text_preserves_labels_and_description_cycle() {
        let plan = GameplayStatsTextPlan::from_parts_with_derived(
            "Artist",
            [("Description", "Credit"), ("Same", "Same")],
            true,
            [peak_nps_text(14.25), peak_nps_text(18.5)],
            [blue_window_label(12), blue_window_label(10)],
            [12.0, 10.0],
        );

        assert_eq!(
            plan.step_info(2).as_ref(),
            tr("Gameplay", "SongInfoCourse").as_ref()
        );
        assert_eq!(
            plan.holds_mines_rolls(1).as_ref(),
            tr("Gameplay", "MinesLabel").as_ref()
        );
        assert_eq!(
            plan.judgment(0).as_ref(),
            JUDGMENT_INFO[0].label.get().as_ref()
        );
        assert_eq!(
            plan.time_total.as_ref(),
            tr("Gameplay", "TimeCourse").as_ref()
        );
        assert_eq!(plan.song_artist.as_ref(), "Artist");
        assert_eq!(plan.description(0, 0.0).as_ref(), "Description");
        assert_eq!(plan.description(0, 2.0).as_ref(), "Credit");
        assert_eq!(plan.description(0, 4.0).as_ref(), "Description");
        assert_eq!(plan.description(1, 2.0).as_ref(), "Same");
        assert_eq!(plan.step_info(usize::MAX).as_ref(), "");
        assert_eq!(plan.peak_nps(0).as_ref(), peak_nps_text(14.25).as_ref());
        assert_eq!(plan.blue_window(0).as_ref(), blue_window_label(12).as_ref());
        assert_eq!(plan.blue_window_ms(0), 12.0);
        let mut plan = plan;
        plan.sync_music_rate([10.0, 20.0], 1.5);
        assert_eq!(plan.peak_nps(0).as_ref(), peak_nps_text(15.0).as_ref());
        assert_eq!(plan.peak_nps(1).as_ref(), peak_nps_text(30.0).as_ref());
    }
}

#[inline(always)]
fn step_stats_player_idx(state: &State, player_side: profile_data::PlayerSide) -> usize {
    match (state.num_players(), player_side) {
        (2, profile_data::PlayerSide::P2) => 1,
        _ => 0,
    }
}

#[inline(always)]
fn step_stats_mask(
    state: &State,
    player_side: profile_data::PlayerSide,
) -> profile_data::StepStatisticsMask {
    let player_idx = step_stats_player_idx(state, player_side);
    state
        .profiles()
        .get(player_idx)
        .map_or(profile_data::StepStatisticsMask::empty(), |p| {
            p.step_statistics
        })
}

#[inline(always)]
fn any_step_stats_enabled(state: &State, bit: profile_data::StepStatisticsMask) -> bool {
    state
        .profiles()
        .iter()
        .take(state.num_players())
        .any(|p| p.step_statistics.contains(bit))
}

#[derive(Clone, Copy)]
struct StepStatsTimeDisplay {
    total_seconds: f32,
    elapsed_seconds: f32,
}

#[inline(always)]
fn step_stats_music_rate(state: &State) -> f32 {
    let music_rate = state.music_rate();
    if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    }
}

fn step_stats_time_display(state: &State, player_idx: usize) -> StepStatsTimeDisplay {
    let rate = step_stats_music_rate(state);
    let (base_elapsed, total) = state.course_display_timing().map_or_else(
        || (0.0, state.song().precise_last_second().max(0.0)),
        |timing| {
            (
                timing.elapsed_seconds.max(0.0),
                timing.total_seconds.max(0.0),
            )
        },
    );
    let current_music_seconds =
        deadsync_core::song_time::song_time_ns_to_seconds(state.current_music_time_ns());
    let stage_elapsed = state
        .players()
        .get(player_idx)
        .and_then(|player| player.fail_time)
        .unwrap_or(current_music_seconds)
        .max(0.0);
    let elapsed = (base_elapsed + stage_elapsed).clamp(0.0, total);
    StepStatsTimeDisplay {
        total_seconds: total / rate,
        elapsed_seconds: elapsed / rate,
    }
}

fn step_stats_hmr_categories(state: &State, player_idx: usize) -> [(usize, u32, u32); 3] {
    if player_idx >= state.num_players() || player_idx >= MAX_PLAYERS {
        return [(0, 0, 0), (1, 0, 0), (2, 0, 0)];
    }
    let p = &state.players()[player_idx];
    let carry = state.display_carry_for_player(player_idx);
    let totals = state.display_totals_for_player(player_idx);
    [
        (
            0usize,
            p.holds_held.saturating_add(carry.holds_held),
            totals.holds_total,
        ),
        (
            1usize,
            p.mines_avoided.saturating_add(carry.mines_avoided),
            totals.mines_total,
        ),
        (
            2usize,
            p.rolls_held.saturating_add(carry.rolls_held),
            totals.rolls_total,
        ),
    ]
}

// A 240-second graph sampled no faster than every 0.25 seconds retains at most
// 961 visible points. Compact before the 63-point stale prefix can overflow the
// 1,024-point gameplay reservation.
const DENSITY_LIFE_COMPACT_THRESHOLD: usize = 63;
const DENSITY_LIFE_LINE_WIDTH: f32 = 2.0;

fn density_life_edge_feather(
    pixel_width: u32,
    pixel_height: u32,
    logical_width: f32,
    logical_height: f32,
) -> f32 {
    if pixel_width == 0
        || pixel_height == 0
        || !logical_width.is_finite()
        || !logical_height.is_finite()
        || logical_width <= 0.0
        || logical_height <= 0.0
    {
        return 0.5;
    }
    let pixel_scale =
        (pixel_width as f32 / logical_width + pixel_height as f32 / logical_height) * 0.5;
    pixel_scale.recip() * 0.5
}

fn clip_density_life_points_impl(
    points: &mut Vec<[f32; 2]>,
    offset: f32,
    compact_threshold: usize,
) -> usize {
    let first_visible = points.partition_point(|p| p[0] < offset);
    if first_visible == 0 {
        return 0;
    }
    if first_visible >= points.len() {
        points.clear();
        return 0;
    }

    let a = points[first_visible - 1];
    let b = points[first_visible];
    let dx = (b[0] - a[0]).max(0.000_001_f32);
    let t = ((offset - a[0]) / dx).clamp(0.0_f32, 1.0_f32);
    points[first_visible - 1] = [offset, a[1] + (b[1] - a[1]) * t];
    let stale = first_visible - 1;
    if stale < compact_threshold.max(1) {
        return 0;
    }

    points.drain(0..stale);
    points.len()
}

fn clip_density_life_points(points: &mut Vec<[f32; 2]>, offset: f32) {
    clip_density_life_points_impl(points, offset, DENSITY_LIFE_COMPACT_THRESHOLD);
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_clip_density_life_points_legacy(points: &mut Vec<[f32; 2]>, offset: f32) -> usize {
    clip_density_life_points_impl(points, offset, 1)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_clip_density_life_points(points: &mut Vec<[f32; 2]>, offset: f32) -> usize {
    clip_density_life_points_impl(points, offset, DENSITY_LIFE_COMPACT_THRESHOLD)
}

#[cfg(test)]
mod density_life_clip_tests {
    use super::{
        DENSITY_LIFE_COMPACT_THRESHOLD, clip_density_life_points_impl, density_life_edge_feather,
    };

    fn visible_points(points: &[[f32; 2]], offset: f32) -> &[[f32; 2]] {
        let start = points.partition_point(|point| point[0] < offset);
        &points[start..]
    }

    #[test]
    fn batched_compaction_preserves_the_interpolated_visible_history() {
        const VISIBLE_POINTS: usize = 961;
        let initial = (0..VISIBLE_POINTS)
            .map(|index| [index as f32, (index % 7) as f32])
            .collect::<Vec<_>>();
        let mut legacy = initial.clone();
        let mut batched = initial;
        batched.reserve_exact(DENSITY_LIFE_COMPACT_THRESHOLD);
        let capacity = batched.capacity();
        let mut legacy_moves = 0usize;
        let mut batched_moves = 0usize;

        for step in 1..=DENSITY_LIFE_COMPACT_THRESHOLD * 3 {
            let x = (VISIBLE_POINTS + step - 1) as f32;
            let point = [x, (step % 11) as f32];
            legacy.push(point);
            batched.push(point);
            let offset = step as f32 + 0.25;

            legacy_moves += clip_density_life_points_impl(&mut legacy, offset, 1);
            batched_moves +=
                clip_density_life_points_impl(&mut batched, offset, DENSITY_LIFE_COMPACT_THRESHOLD);

            assert_eq!(
                visible_points(&batched, offset),
                visible_points(&legacy, offset)
            );
            assert!(batched.len() < legacy.len() + DENSITY_LIFE_COMPACT_THRESHOLD);
            assert_eq!(batched.capacity(), capacity);
        }

        assert!(batched_moves * 20 < legacy_moves);
    }

    #[test]
    fn batched_compaction_clears_a_fully_expired_history() {
        let mut points = vec![[1.0, 0.25], [2.0, 0.75]];

        clip_density_life_points_impl(&mut points, 3.0, DENSITY_LIFE_COMPACT_THRESHOLD);

        assert!(points.is_empty());
    }

    #[test]
    fn edge_feather_tracks_half_a_physical_pixel() {
        let feather = density_life_edge_feather(1600, 900, 854.0, 480.0);

        assert!((feather - 0.266_75).abs() < 0.001);
        assert_eq!(density_life_edge_feather(854, 480, 854.0, 480.0), 0.5);
    }
}

fn refresh_density_graph_meshes_for_player(state: &mut State, player_idx: usize) {
    let graph = state.gameplay.density_graph_view();
    let num_players = state.gameplay.num_players();
    let render = &mut state.density_graph;
    let graph_w = graph.graph_w;
    let graph_h = graph.graph_h;
    let scaled_width = graph.scaled_width;
    if player_idx >= num_players
        || graph_w <= 0.0_f32
        || graph_h <= 0.0_f32
        || scaled_width <= 0.0_f32
    {
        render.mesh[player_idx] = None;
        render.life_mesh[player_idx] = None;
        render.mesh_offset[player_idx] = 0.0;
        render.life_mesh_offset[player_idx] = 0.0;
        state
            .gameplay
            .set_density_graph_life_dirty(player_idx, false);
        return;
    }

    let offset = (graph.u0 * scaled_width).clamp(0.0_f32, scaled_width);
    if offset != render.mesh_offset[player_idx] {
        render.mesh_offset[player_idx] = offset;
        density::update_density_hist_mesh_reusable(
            &mut render.mesh[player_idx],
            render.cache[player_idx].as_ref(),
            offset,
            graph_w,
        );
    }

    let prev_offset = render.life_mesh_offset[player_idx];
    let offset_changed = offset != prev_offset;
    if !offset_changed && !state.gameplay.density_graph_life_dirty(player_idx) {
        return;
    }

    render.life_mesh_offset[player_idx] = offset;
    state
        .gameplay
        .set_density_graph_life_dirty(player_idx, false);
    if offset > prev_offset
        && let Some(points) = state.gameplay.density_graph_life_points_mut(player_idx)
    {
        clip_density_life_points(points, offset);
    }
    let Some(points) = state.gameplay.density_graph_life_points(player_idx) else {
        render.life_mesh[player_idx] = None;
        return;
    };
    if points.len() < 2 {
        render.life_mesh[player_idx] = None;
        return;
    }

    let (pixel_width, pixel_height) = current_window_px();
    let edge_feather =
        density_life_edge_feather(pixel_width, pixel_height, screen_width(), screen_height());
    density::update_density_life_mesh_reusable(
        &mut render.life_mesh[player_idx],
        points,
        offset,
        graph_w,
        DENSITY_LIFE_LINE_WIDTH,
        edge_feather,
        [1.0_f32, 1.0_f32, 1.0_f32, 1.0_f32],
    );
}

pub fn refresh_density_graph_meshes(state: &mut State) {
    for player_idx in 0..state.num_players() {
        refresh_density_graph_meshes_for_player(state, player_idx);
    }
}

fn push_density_graph_at(
    actors: &mut Vec<Actor>,
    state: &State,
    player_idx: usize,
    x0: f32,
    y0: f32,
) {
    if player_idx >= state.num_players() {
        return;
    }

    const BG_RGB: [f32; 3] = [
        30.0 / 255.0, // 0x1E
        40.0 / 255.0, // 0x28
        47.0 / 255.0, // 0x2F
    ];

    let graph = state.gameplay.density_graph_view();
    let graph_w = graph.graph_w;
    let graph_h = graph.graph_h;
    if graph_w <= 0.0_f32 || graph_h <= 0.0_f32 {
        return;
    }

    let bg_alpha = if state.profiles()[player_idx].transparent_density_graph_bg {
        0.5
    } else {
        1.0
    };

    actors.push(act!(quad:
        align(0.0, 0.0): xy(x0, y0):
        zoomto(graph_w, graph_h):
        diffuse(BG_RGB[0], BG_RGB[1], BG_RGB[2], bg_alpha):
        z(59)
    ));

    if let Some(mesh) = &state.density_graph.mesh[player_idx]
        && !mesh.is_empty()
    {
        actors.push(Actor::ReusableMesh {
            align: [0.0, 0.0],
            offset: [x0 + DENSITY_LIFE_LINE_WIDTH * 0.5, y0],
            size: [SizeSpec::Px(graph_w), SizeSpec::Px(graph_h)],
            tint: [1.0; 4],
            vertices: Arc::clone(mesh),
            visible: true,
            blend: BlendMode::Alpha,
            z: 60,
        });
    }

    if let Some(mesh) = &state.density_graph.life_mesh[player_idx]
        && !mesh.is_empty()
    {
        actors.push(Actor::ReusableMesh {
            align: [0.0, 0.0],
            offset: [x0, y0],
            size: [SizeSpec::Px(graph_w), SizeSpec::Px(graph_h)],
            tint: [1.0; 4],
            vertices: Arc::clone(mesh),
            visible: true,
            blend: BlendMode::Alpha,
            z: 61,
        });
    }
}

#[inline(always)]
fn padded_num_text(count: u32, digits: usize) -> TextContent {
    let digits = digits.clamp(1, u8::MAX as usize);
    if digits <= InlineText::CAPACITY {
        let key = (count, digits as u8);
        let text = PADDED_NUM_INLINE_CACHE.with(|cache| {
            cache.borrow_mut().get_or_insert(key, || {
                InlineText::format(format_args!("{count:0digits$}"))
                    .expect("a padded u32 within the inline capacity fits")
            })
        });
        TextContent::Inline(text)
    } else {
        TextContent::Owned(format!("{count:0digits$}"))
    }
}

#[inline(always)]
fn padded_dim_len(full: &str, count: u32, digits: usize) -> usize {
    if count == 0 {
        digits.saturating_sub(1).min(full.len())
    } else {
        full.find(|c: char| c != '0').unwrap_or(full.len())
    }
}

#[inline(always)]
fn padded_runs(count: u32, digits: usize) -> (TextContent, TextContent) {
    let digits = digits.clamp(1, u8::MAX as usize);
    if digits <= ZERO_RUNS.len() {
        let count_digits = if count == 0 {
            1
        } else {
            count.ilog10() as usize + 1
        };
        let dim_len = digits.saturating_sub(count_digits);
        (
            TextContent::Static(ZERO_RUNS[dim_len]),
            TextContent::inline_u32(count),
        )
    } else {
        let full = padded_num_text(count, digits);
        let split = padded_dim_len(full.as_str(), count, digits);
        (
            TextContent::Owned(full.as_str()[..split].to_owned()),
            TextContent::Owned(full.as_str()[split..].to_owned()),
        )
    }
}

fn blue_window_label(ms: i32) -> Arc<str> {
    use crate::assets::i18n::tr_fmt;
    Arc::from(tr_fmt(
        "Gameplay",
        "BlueWindowLabel",
        &[("ms", &ms.to_string())],
    ))
}

#[cfg(feature = "bench-support")]
#[inline(always)]
fn cached_blue_window_label_legacy(ms: i32) -> Arc<str> {
    cached_text(&BENCH_BLUE_WINDOW_LABEL_CACHE, ms, TEXT_CACHE_LIMIT, || {
        blue_window_label(ms).as_ref().to_owned()
    })
}

#[inline(always)]
fn standard_row_disabled(disabled_windows: [bool; 5], row: usize) -> bool {
    row < 5 && disabled_windows[row]
}

#[inline(always)]
fn split_row_disabled(disabled_windows: [bool; 5], row: usize) -> bool {
    match row {
        0 | 1 => disabled_windows[0],
        2 => disabled_windows[1],
        3 => disabled_windows[2],
        4 => disabled_windows[3],
        5 => disabled_windows[4],
        _ => false,
    }
}

#[inline(always)]
fn padded_runs_for_window(count: u32, digits: usize, disabled: bool) -> (TextContent, TextContent) {
    if disabled {
        (padded_num_text(count, digits), TextContent::default())
    } else {
        padded_runs(count, digits)
    }
}

fn peak_nps_text(peak: f32) -> Arc<str> {
    use crate::assets::i18n::tr_fmt;
    Arc::from(tr_fmt(
        "Gameplay",
        "PeakNps",
        &[("peak_nps", &format!("{:.2}", peak.max(0.0)))],
    ))
}

#[cfg(feature = "bench-support")]
#[inline(always)]
fn cached_peak_nps_text_legacy(peak: f32) -> Arc<str> {
    cached_text(
        &BENCH_PEAK_NPS_CACHE,
        peak.to_bits(),
        TEXT_CACHE_LIMIT,
        || peak_nps_text(peak).as_ref().to_owned(),
    )
}

#[inline(always)]
fn game_time_text(seconds: u32, mode: u8) -> TextContent {
    let key = (seconds, mode);
    let text = GAME_TIME_INLINE_CACHE.with(|cache| {
        cache.borrow_mut().get_or_insert(key, || {
            let seconds = seconds as u64;
            let minutes = seconds / 60;
            let secs = seconds % 60;
            match mode {
                0 => {
                    let hours = seconds / 3600;
                    let mins = (seconds % 3600) / 60;
                    InlineText::format(format_args!("{hours}:{mins:02}:{secs:02}"))
                }
                1 => InlineText::format(format_args!("{minutes:02}:{secs:02}")),
                _ => InlineText::format(format_args!("{minutes}:{secs:02}")),
            }
            .expect("a u32 game time always fits the inline text payload")
        })
    });
    TextContent::Inline(text)
}

#[inline(always)]
fn timing_tenths(ms: f32) -> i32 {
    if ms.is_finite() {
        (ms * 10.0).round() as i32
    } else {
        0
    }
}

#[inline(always)]
fn live_timing_pair_text_in_slot(recent_ms: f32, all_ms: f32, slot: usize) -> TextContent {
    let key = (timing_tenths(recent_ms), timing_tenths(all_ms));
    if key.0.unsigned_abs() > 9_999 || key.1.unsigned_abs() > 9_999 {
        return TextContent::Owned(format!(
            "{:.1}/{:.1}",
            key.0 as f32 * 0.1,
            key.1 as f32 * 0.1
        ));
    }
    let text = LIVE_TIMING_INLINE_CACHE.with(|cache| {
        cache.borrow_mut().get_or_insert(slot, key, || {
            InlineText::format(format_args!(
                "{:.1}/{:.1}",
                key.0 as f32 * 0.1,
                key.1 as f32 * 0.1
            ))
            .expect("two rounded f32 timing values always fit the inline text payload")
        })
    });
    TextContent::frame_inline_slot(text, FRAME_TEXT_LIVE_TIMING_BASE + slot.min(5) as u8)
}

#[inline(always)]
fn live_timing_stat_mask(index: usize) -> profile_data::LiveTimingStatsMask {
    match index {
        0 => profile_data::LiveTimingStatsMask::MEAN,
        1 => profile_data::LiveTimingStatsMask::MEAN_ABS,
        _ => profile_data::LiveTimingStatsMask::MAX,
    }
}

#[inline(always)]
fn live_timing_enabled_count(mask: profile_data::LiveTimingStatsMask) -> usize {
    usize::from(mask.contains(profile_data::LiveTimingStatsMask::MEAN))
        + usize::from(mask.contains(profile_data::LiveTimingStatsMask::MEAN_ABS))
        + usize::from(mask.contains(profile_data::LiveTimingStatsMask::MAX))
}

#[inline(always)]
fn live_timing_value(stats: LiveTimingSnapshot, player_idx: usize, index: usize) -> TextContent {
    let slot = player_idx * LIVE_TIMING_LABELS.len() + index;
    match index {
        0 => live_timing_pair_text_in_slot(stats.recent.mean_ms, stats.all.mean_ms, slot),
        1 => live_timing_pair_text_in_slot(stats.recent.mean_abs_ms, stats.all.mean_abs_ms, slot),
        _ => live_timing_pair_text_in_slot(stats.recent.max_abs_ms, stats.all.max_abs_ms, slot),
    }
}

#[inline(always)]
fn game_time_mode(total_seconds: f32) -> u8 {
    if total_seconds >= 3600.0 {
        0
    } else if total_seconds >= 600.0 {
        1
    } else {
        2
    }
}

#[inline(always)]
fn game_time_key(seconds: f32, total_seconds: f32) -> (u32, u8) {
    (seconds.max(0.0) as u32, game_time_mode(total_seconds))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_padded_runs_legacy(count: u32, digits: usize) -> (Arc<str>, Arc<str>) {
    let digits = digits.clamp(1, u8::MAX as usize);
    let build_run = |dim: bool| {
        let full: Arc<str> = Arc::from(format!("{count:0digits$}"));
        let split = padded_dim_len(full.as_ref(), count, digits);
        Arc::from(if dim { &full[..split] } else { &full[split..] })
    };
    (build_run(true), build_run(false))
}

#[cfg(feature = "bench-support")]
fn benchmark_cached_padded_num(count: u32, digits: u8) -> Arc<str> {
    cached_text(
        &BENCH_PADDED_NUM_CACHE,
        (count, digits),
        TEXT_CACHE_LIMIT,
        || format!("{count:0width$}", width = digits as usize),
    )
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_padded_runs_cached(count: u32, digits: usize) -> (Arc<str>, Arc<str>) {
    let digits = digits.clamp(1, u8::MAX as usize) as u8;
    let dim = cached_text(
        &BENCH_PADDED_DIM_CACHE,
        (count, digits),
        TEXT_CACHE_LIMIT,
        || {
            let full = benchmark_cached_padded_num(count, digits);
            let split = padded_dim_len(full.as_ref(), count, digits as usize);
            full[..split].to_owned()
        },
    );
    let bright = cached_text(
        &BENCH_PADDED_BRIGHT_CACHE,
        (count, digits),
        TEXT_CACHE_LIMIT,
        || {
            let full = benchmark_cached_padded_num(count, digits);
            let split = padded_dim_len(full.as_ref(), count, digits as usize);
            full[split..].to_owned()
        },
    );
    (dim, bright)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_padded_runs(count: u32, digits: usize) -> (TextContent, TextContent) {
    padded_runs(count, digits)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_game_time_legacy(seconds: u32, mode: u8) -> Arc<str> {
    let seconds = seconds as u64;
    let minutes = seconds / 60;
    let secs = seconds % 60;
    Arc::from(match mode {
        0 => {
            let hours = seconds / 3600;
            let mins = (seconds % 3600) / 60;
            format!("{hours}:{mins:02}:{secs:02}")
        }
        1 => format!("{minutes:02}:{secs:02}"),
        _ => format!("{minutes}:{secs:02}"),
    })
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_game_time_cached(seconds: u32, mode: u8) -> Arc<str> {
    cached_text(
        &BENCH_GAME_TIME_CACHE,
        (seconds, mode),
        TEXT_CACHE_LIMIT,
        || benchmark_game_time_legacy(seconds, mode).to_string(),
    )
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_game_time(seconds: u32, mode: u8) -> TextContent {
    game_time_text(seconds, mode)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_live_timing_legacy(recent_ms: f32, all_ms: f32) -> Arc<str> {
    let key = (timing_tenths(recent_ms), timing_tenths(all_ms));
    Arc::from(format!(
        "{:.1}/{:.1}",
        key.0 as f32 * 0.1,
        key.1 as f32 * 0.1
    ))
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_live_timing_cached(recent_ms: f32, all_ms: f32) -> Arc<str> {
    let key = (timing_tenths(recent_ms), timing_tenths(all_ms));
    cached_text(&BENCH_LIVE_TIMING_CACHE, key, TEXT_CACHE_LIMIT, || {
        format!("{:.1}/{:.1}", key.0 as f32 * 0.1, key.1 as f32 * 0.1)
    })
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_live_timing(recent_ms: f32, all_ms: f32) -> TextContent {
    live_timing_pair_text_in_slot(recent_ms, all_ms, 0)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_judgment_rows_legacy(labels: &[Arc<str>; 6]) -> usize {
    let labels = labels.iter().cloned().collect::<Vec<_>>();
    labels.iter().fold(0usize, |sum, label| {
        sum.wrapping_mul(31).wrapping_add(label.len())
    })
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_judgment_rows(labels: &[Arc<str>; 6]) -> usize {
    labels.iter().fold(0usize, |sum, label| {
        let label = Arc::clone(label);
        sum.wrapping_mul(31).wrapping_add(label.len())
    })
}

#[cfg(test)]
mod dynamic_hud_text_tests {
    use super::{
        game_time_text, live_timing_pair_text_in_slot, padded_num_text, padded_runs_for_window,
    };
    use deadlib_present::actors::{InlineText, TextContent};

    #[test]
    fn padded_counts_preserve_zero_padding_and_disabled_window_behavior() {
        for (count, digits, expected) in [
            (0, 4, "0000"),
            (7, 4, "0007"),
            (42, 4, "0042"),
            (12_345, 4, "12345"),
            (u32::MAX, 10, "4294967295"),
        ] {
            let text = padded_num_text(count, digits);
            assert_eq!(text.as_str(), expected);
            assert!(matches!(text, TextContent::Inline(_)));
        }

        let (dim, bright) = padded_runs_for_window(42, 4, false);
        assert_eq!((dim.as_str(), bright.as_str()), ("00", "42"));
        let (disabled, empty) = padded_runs_for_window(42, 4, true);
        assert_eq!((disabled.as_str(), empty.as_str()), ("0042", ""));
        assert!(matches!(empty, TextContent::Static("")));

        let oversized = padded_num_text(7, InlineText::CAPACITY + 1);
        assert_eq!(
            oversized.as_str(),
            format!("{:0width$}", 7, width = InlineText::CAPACITY + 1)
        );
        assert!(matches!(oversized, TextContent::Owned(_)));
    }

    #[test]
    fn game_clock_preserves_every_display_mode_without_owned_text() {
        for (seconds, mode, expected) in [
            (0, 2, "0:00"),
            (59, 2, "0:59"),
            (600, 1, "10:00"),
            (3_599, 1, "59:59"),
            (3_600, 0, "1:00:00"),
            (u32::MAX, 0, "1193046:28:15"),
        ] {
            let text = game_time_text(seconds, mode);
            assert_eq!(text.as_str(), expected);
            assert!(matches!(text, TextContent::Inline(_)));
        }
    }

    #[test]
    fn live_timing_preserves_tenth_rounding_and_nonfinite_fallback() {
        for (recent, all, expected) in [
            (0.0, 0.0, "0.0/0.0"),
            (12.34, -5.66, "12.3/-5.7"),
            (f32::NAN, f32::INFINITY, "0.0/0.0"),
        ] {
            let text = live_timing_pair_text_in_slot(recent, all, 0);
            assert_eq!(text.as_str(), expected);
            assert!(matches!(
                text,
                TextContent::FrameInline {
                    slot: super::FRAME_TEXT_LIVE_TIMING_BASE,
                    ..
                }
            ));
        }

        let recent = f32::MAX;
        let all = f32::MIN;
        let recent_tenths = super::timing_tenths(recent);
        let all_tenths = super::timing_tenths(all);
        let expected = format!(
            "{:.1}/{:.1}",
            recent_tenths as f32 * 0.1,
            all_tenths as f32 * 0.1
        );
        let text = live_timing_pair_text_in_slot(recent, all, 0);
        assert_eq!(text.as_str(), expected);
        assert!(matches!(text, TextContent::Owned(_)));
    }
}

#[inline(always)]
fn glyph_width_scaled(
    metrics_font: &font::Font,
    all_fonts: &font::FontMap,
    ch: char,
    zoom: f32,
) -> f32 {
    font::find_glyph(metrics_font, ch, all_fonts).map_or(0, |glyph| glyph.advance_i32) as f32 * zoom
}

#[inline(always)]
fn push_versus_count_texts(
    actors: &mut Vec<Actor>,
    state: &State,
    is_p1: bool,
    anchor_x: f32,
    y: f32,
    digit_w: f32,
    numbers_zoom_x: f32,
    numbers_zoom_y: f32,
    dim_text: TextContent,
    bright_text: TextContent,
    dim: [f32; 4],
    bright: [f32; 4],
    z: i16,
) {
    let dim_len = dim_text.len() as f32;
    let bright_len = bright_text.len() as f32;
    if is_p1 {
        if !dim_text.is_empty() {
            let mut a = act!(text:
                font(gameplay_font_key(state, FontRole::ScreenEval)): settext(dim_text):
                align(0.0, 0.5): xy(anchor_x, y):
                zoom(numbers_zoom_y):
                diffuse(dim[0], dim[1], dim[2], dim[3]):
                z(z):
                horizalign(left)
            );
            if let Actor::Text { scale, .. } = &mut a {
                scale[0] = numbers_zoom_x;
                scale[1] = numbers_zoom_y;
            }
            actors.push(a);
        }
        if !bright_text.is_empty() {
            let mut a = act!(text:
                font(gameplay_font_key(state, FontRole::ScreenEval)): settext(bright_text):
                align(0.0, 0.5): xy(anchor_x + dim_len * digit_w, y):
                zoom(numbers_zoom_y):
                diffuse(bright[0], bright[1], bright[2], bright[3]):
                z(z):
                horizalign(left)
            );
            if let Actor::Text { scale, .. } = &mut a {
                scale[0] = numbers_zoom_x;
                scale[1] = numbers_zoom_y;
            }
            actors.push(a);
        }
    } else {
        if !bright_text.is_empty() {
            let mut a = act!(text:
                font(gameplay_font_key(state, FontRole::ScreenEval)): settext(bright_text):
                align(1.0, 0.5): xy(anchor_x, y):
                zoom(numbers_zoom_y):
                diffuse(bright[0], bright[1], bright[2], bright[3]):
                z(z):
                horizalign(right)
            );
            if let Actor::Text { scale, .. } = &mut a {
                scale[0] = numbers_zoom_x;
                scale[1] = numbers_zoom_y;
            }
            actors.push(a);
        }
        if !dim_text.is_empty() {
            let mut a = act!(text:
                font(gameplay_font_key(state, FontRole::ScreenEval)): settext(dim_text):
                align(1.0, 0.5): xy(anchor_x - bright_len * digit_w, y):
                zoom(numbers_zoom_y):
                diffuse(dim[0], dim[1], dim[2], dim[3]):
                z(z):
                horizalign(right)
            );
            if let Actor::Text { scale, .. } = &mut a {
                scale[0] = numbers_zoom_x;
                scale[1] = numbers_zoom_y;
            }
            actors.push(a);
        }
    }
}

#[inline(always)]
fn cached_game_time_width_for_key(key: (u32, u8), asset_manager: &AssetManager) -> f32 {
    GAME_TIME_WIDTH_INLINE_CACHE.with(|cache| {
        cache.borrow_mut().get_or_insert(key, || {
            let text = game_time_text(key.0, key.1);
            asset_manager
                .with_fonts(|all_fonts| {
                    asset_manager.with_font("miso", |f| {
                        font::measure_line_width_logical(f, text.as_str(), all_fonts) as f32
                    })
                })
                .unwrap_or(0.0)
        })
    })
}

#[inline(always)]
fn digit_text(digit: u8) -> Arc<str> {
    if digit.is_ascii_digit() {
        DIGIT_TEXT[(digit - b'0') as usize].clone()
    } else {
        Arc::<str>::from("")
    }
}

#[inline(always)]
fn step_info_label_text(state: &State, index: usize) -> Arc<str> {
    state.gameplay_stats_text.step_info(index)
}

#[inline(always)]
fn holds_mines_rolls_label_text(state: &State, index: usize) -> Arc<str> {
    state.gameplay_stats_text.holds_mines_rolls(index)
}

pub fn prewarm_text_layout(
    cache: &mut TextLayoutCache,
    fonts: &font::FontMap,
    asset_manager: &AssetManager,
    state: &State,
) {
    prewarm_score_counter_layout(cache, fonts, gameplay_font_key(state, FontRole::Numbers));
    let mut max_count = 0u32;
    for player in 0..state.num_players() {
        let totals = state.display_totals_for_player(player);
        max_count = max_count
            .max(totals.total_steps)
            .max(totals.holds_total)
            .max(totals.rolls_total)
            .max(totals.mines_total);
        for (_, achieved, total) in step_stats_hmr_categories(state, player) {
            max_count = max_count.max(achieved).max(total);
        }
    }
    let digits = if max_count > 0 {
        (max_count.ilog10() as usize + 1).max(4)
    } else {
        4
    };
    for count in 0..=max_count.min(COUNT_PREWARM_CAP) {
        let (dim, bright) = padded_runs(count, digits);
        cache.prewarm_text(
            fonts,
            gameplay_font_key(state, FontRole::ScreenEval),
            dim.as_str(),
            None,
        );
        cache.prewarm_text(
            fonts,
            gameplay_font_key(state, FontRole::ScreenEval),
            bright.as_str(),
            None,
        );
    }
    let (dim, bright) = padded_runs(max_count, digits);
    cache.prewarm_text(
        fonts,
        gameplay_font_key(state, FontRole::ScreenEval),
        dim.as_str(),
        None,
    );
    cache.prewarm_text(
        fonts,
        gameplay_font_key(state, FontRole::ScreenEval),
        bright.as_str(),
        None,
    );
    for player in 0..state.num_players() {
        let totals = state.display_totals_for_player(player);
        for count in [
            totals.total_steps,
            totals.holds_total,
            totals.rolls_total,
            totals.mines_total,
        ] {
            let (dim, bright) = padded_runs(count, digits);
            cache.prewarm_text(
                fonts,
                gameplay_font_key(state, FontRole::ScreenEval),
                dim.as_str(),
                None,
            );
            cache.prewarm_text(
                fonts,
                gameplay_font_key(state, FontRole::ScreenEval),
                bright.as_str(),
                None,
            );
        }
        for (_, achieved, total) in step_stats_hmr_categories(state, player) {
            for count in [achieved, total] {
                let (dim, bright) = padded_runs(count, digits);
                cache.prewarm_text(
                    fonts,
                    gameplay_font_key(state, FontRole::ScreenEval),
                    dim.as_str(),
                    None,
                );
                cache.prewarm_text(
                    fonts,
                    gameplay_font_key(state, FontRole::ScreenEval),
                    bright.as_str(),
                    None,
                );
            }
        }
    }
    // Leave the first gameplay frame's full-width disabled counter resident
    // after the bounded prewarm walk has cycled through large chart totals.
    let _ = padded_num_text(0, digits);
    let end_seconds = deadsync_core::song_time::song_time_ns_to_seconds(
        state.music_end_time_ns().max(state.notes_end_time_ns()),
    )
    .ceil()
    .max(0.0) as u32;
    let display_end_seconds = step_stats_time_display(state, 0)
        .total_seconds
        .ceil()
        .max(0.0) as u32;
    let end_seconds = end_seconds.max(display_end_seconds);
    let mode = game_time_mode(end_seconds as f32);
    for second in 0..=end_seconds.min(TIME_PREWARM_CAP_S) {
        let key = (second, mode);
        let text = game_time_text(second, mode);
        cache.prewarm_text(fonts, "miso", text.as_str(), None);
        let _ = cached_game_time_width_for_key(key, asset_manager);
    }
    let key = (end_seconds, mode);
    let text = game_time_text(end_seconds, mode);
    cache.prewarm_text(fonts, "miso", text.as_str(), None);
    let _ = cached_game_time_width_for_key(key, asset_manager);
    cache.prewarm_text(fonts, "miso", time_total_text(state).as_ref(), None);
    cache.prewarm_text(fonts, "miso", &tr("Gameplay", "TimeSong"), None);
    cache.prewarm_text(fonts, "miso", &tr("Gameplay", "TimeCourse"), None);
    cache.prewarm_text(fonts, "miso", &time_remaining_text(state), None);
    cache.prewarm_text(fonts, "miso", SLASH_TEXT.as_ref(), None);
    for label in LIVE_TIMING_LABELS.iter() {
        cache.prewarm_text(fonts, "miso", label.as_ref(), None);
    }
    for label in &state.gameplay_stats_text.step_info {
        cache.prewarm_text(fonts, "miso", label.as_ref(), None);
    }
    for label in &state.gameplay_stats_text.holds_mines_rolls {
        cache.prewarm_text(fonts, "miso", label.as_ref(), None);
    }
    for label in &state.gameplay_stats_text.judgments {
        cache.prewarm_text(fonts, "miso", label.as_ref(), None);
    }
    for player in 0..state.num_players() {
        cache.prewarm_text(fonts, "miso", state.song_full_title.as_ref(), None);
        cache.prewarm_text(
            fonts,
            "miso",
            state.gameplay_stats_text.song_artist.as_ref(),
            None,
        );
        cache.prewarm_text(fonts, "miso", state.pack_group.as_ref(), None);
        for description in &state.gameplay_stats_text.descriptions[player] {
            cache.prewarm_text(fonts, "miso", description.as_ref(), None);
        }
        let peak = state.gameplay_stats_text.peak_nps(player);
        cache.prewarm_text(fonts, "miso", peak.as_ref(), None);
    }
}

/// Prepares the six direct slots used by two players' live timing values.
pub fn prewarm_frame_text_scratch(
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    fonts: &font::FontMap,
    state: &State,
) {
    let longest =
        InlineText::copy_from("-999.9/-999.9").expect("bounded gameplay timing text fits inline");
    for player in 0..state.num_players() {
        let profile = &state.profiles()[player];
        if !profile.live_timing_stats {
            continue;
        }
        for index in 0..LIVE_TIMING_LABELS.len() {
            if !profile
                .live_timing_stats_mask
                .contains(live_timing_stat_mask(index))
            {
                continue;
            }
            prewarm_frame_inline_text_slot(
                cache,
                scratch,
                fonts,
                "miso",
                longest,
                FRAME_TEXT_LIVE_TIMING_BASE + (player * LIVE_TIMING_LABELS.len() + index) as u8,
                FRAME_TEXT_VERTEX_BUFFERS,
            );
        }
    }
}

pub fn push_step_stats(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    playfield_center_x: f32,
    player_side: profile_data::PlayerSide,
) {
    let wide = is_wide();
    let mask = step_stats_mask(state, player_side);
    if mask.is_empty() {
        return;
    }
    let layout = step_stats_pane_layout(state, playfield_center_x, player_side);
    let player_idx = step_stats_player_idx(state, player_side);
    let extra_actor_count = state.step_stats_extra_resolved[player_idx].actor_count();
    actors.reserve((if wide { 48 } else { 1 }) + extra_actor_count);
    if mask.contains(profile_data::StepStatisticsMask::SONG_BANNER) {
        build_banner(actors, state, layout, wide, player_side);
    }
    let show_pack_info = mask.pack_info_enabled();
    if show_pack_info {
        build_pack_banner(actors, state, layout, wide, player_side);
    }
    build_steps_info(actors, state, layout, wide, player_side, show_pack_info);
    step_stats_gifs::push_step_stats_extra(
        actors,
        state,
        player_idx,
        crate::step_stats_gifs::GifRenderParams {
            player_side,
            wide,
            aspect_ratio: screen_width() / screen_height().max(1.0),
            pane_x: layout.sidepane_center_x,
            pane_y: layout.sidepane_center_y,
            banner_data_zoom: layout.banner_data_zoom,
            note_field_is_centered: layout.note_field_is_centered,
        },
    );
    build_side_pane(
        actors,
        state,
        asset_manager,
        layout,
        wide,
        player_side,
        mask,
    );
    if mask.contains(profile_data::StepStatisticsMask::STEP_COUNTS) {
        let player_idx = step_stats_player_idx(state, player_side);
        if state.profiles()[player_idx].display_scorebox {
            build_scorebox_pane(actors, state, layout, wide, player_side);
        } else {
            build_holds_mines_rolls_pane(actors, state, asset_manager, layout, wide, player_side);
        }
    }
}

fn step_stats_pane_layout(
    state: &State,
    playfield_center_x: f32,
    player_side: profile_data::PlayerSide,
) -> StepStatsPaneLayout {
    step_stats_theme::pane_layout(StepStatsPaneParams {
        screen_w: screen_width(),
        screen_h: screen_height(),
        screen_center_x: screen_center_x(),
        screen_center_y: screen_center_y(),
        playfield_center_x,
        player_side,
        num_players: state.num_players(),
        notefield_width: notefield_width(state),
        wide: is_wide(),
    })
}

pub fn push_heart_rates(actors: &mut Vec<Actor>, state: &State, playfield_center_x: f32) {
    let elapsed = state.gameplay.total_elapsed_in_screen();
    for player_idx in 0..state.num_players() {
        let side = gameplay_screen::runtime_profile_side(state, player_idx);
        let side_idx = profile_data::player_side_index(side);
        let reading = state.heart_rate_view.players[side_idx];
        if !reading.configured {
            continue;
        }
        let layout = step_stats_pane_layout(state, playfield_center_x, side);
        let x_sign = if side == profile_data::PlayerSide::P1 {
            1.0
        } else {
            -1.0
        };
        let x = layout.sidepane_center_x + x_sign * 94.0 * layout.banner_data_zoom;
        let y = layout.sidepane_center_y - 37.0 * layout.banner_data_zoom;
        heart_rate::push(
            actors,
            reading,
            state.heart_rate_text.get(side_idx),
            elapsed,
            x,
            y,
            layout.banner_data_zoom,
            72,
        );
    }
}

fn song_info_text_zoom(layout: StepStatsPaneLayout) -> f32 {
    step_stats_theme::song_info_text_zoom(layout, screen_width() / screen_height().max(1.0))
}

fn step_stats_density_graph_rect(state: &State, layout: StepStatsPaneLayout) -> StepStatsGraphRect {
    step_stats_theme::density_graph_rect(state.gameplay.density_graph_view().graph_w, layout)
}

fn push_peak_nps_on_graph(
    actors: &mut Vec<Actor>,
    state: &State,
    player_idx: usize,
    player_side: profile_data::PlayerSide,
    graph: StepStatsGraphRect,
    zoom: f32,
) {
    if player_idx >= state.num_players() {
        return;
    }

    let peak_nps_text = state.gameplay_stats_text.peak_nps(player_idx);
    let align_left = player_side == profile_data::PlayerSide::P2;
    let x = if align_left {
        graph.x + PEAK_NPS_GRAPH_PAD
    } else {
        graph.x + graph.w - PEAK_NPS_GRAPH_PAD
    };
    let y = graph.y + PEAK_NPS_GRAPH_PAD;
    let max_w = (graph.w - PEAK_NPS_GRAPH_PAD * 2.0).max(1.0);

    if align_left {
        actors.push(act!(text:
            font("miso"):
            settext(peak_nps_text):
            align(0.0, 0.0):
            xy(x, y):
            zoom(zoom):
            maxwidth(max_w):
            diffuse(1.0, 1.0, 1.0, PEAK_NPS_ALPHA):
            horizalign(left):
            z(200)
        ));
    } else {
        actors.push(act!(text:
            font("miso"):
            settext(peak_nps_text):
            align(1.0, 0.0):
            xy(x, y):
            zoom(zoom):
            maxwidth(max_w):
            diffuse(1.0, 1.0, 1.0, PEAK_NPS_ALPHA):
            horizalign(right):
            z(200)
        ));
    }
}

pub fn push_versus_step_stats(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
) {
    if !is_wide() {
        return;
    }
    // Simply Love shows centered step stats in 2P versus on widescreen, but not on ultrawide
    // (ultrawide already has native per-player side panes).
    let is_ultrawide = screen_width() / screen_height().max(1.0) > (21.0 / 9.0);
    if is_ultrawide {
        return;
    }
    if state.num_players() < 2 {
        return;
    }
    let show_judgments_for: [bool; 2] = [
        state.profiles()[0]
            .step_statistics
            .contains(profile_data::StepStatisticsMask::JUDGMENT_COUNTER),
        state.profiles()[1]
            .step_statistics
            .contains(profile_data::StepStatisticsMask::JUDGMENT_COUNTER),
    ];
    let show_score_for: [bool; 2] = [
        state.profiles()[0].score_position == profile_data::ScorePosition::StepStatistics
            && !state.profiles()[0].step_statistics.is_empty(),
        state.profiles()[1].score_position == profile_data::ScorePosition::StepStatistics
            && !state.profiles()[1].step_statistics.is_empty(),
    ];
    let show_song_banner =
        any_step_stats_enabled(state, profile_data::StepStatisticsMask::SONG_BANNER);
    if !show_judgments_for[0]
        && !show_judgments_for[1]
        && !show_score_for[0]
        && !show_score_for[1]
        && !show_song_banner
    {
        return;
    }

    let center_x = screen_center_x();

    let total_tapnotes = (0..state.num_players())
        .map(|player| state.display_totals_for_player(player).total_steps)
        .max()
        .unwrap_or(0) as f32;
    let digits = if total_tapnotes > 0.0 {
        (total_tapnotes.log10().floor() as usize + 1).max(4)
    } else {
        4
    };

    let group_zoom_y = 0.8_f32;
    let group_zoom_x = if digits > 4 {
        (group_zoom_y - 0.12 * (digits.saturating_sub(4) as f32)).max(0.1)
    } else {
        group_zoom_y
    };
    let numbers_zoom_y = group_zoom_y * 0.5;
    let numbers_zoom_x = group_zoom_x * 0.5;
    let y_base = -280.0;

    // Keep the background bar below the top HUD (song title/BPM), but let the
    // digits sit above playfield elements if needed.
    let z_bg = 80i16;
    let z_fg = 110i16;

    actors.reserve(128);
    if show_judgments_for[0] || show_judgments_for[1] {
        // Center black column behind the counters (SL: VersusStepStatistics.lua).
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(screen_center_x(), screen_center_y()):
            zoomto(150.0, screen_height()):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(z_bg)
        ));
    }

    if show_judgments_for[0] || show_judgments_for[1] {
        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font(gameplay_font_key(state, FontRole::ScreenEval), |f| {
                let digit_w = glyph_width_scaled(f, all_fonts, '0', numbers_zoom_x);
                if digit_w <= 0.0 {
                    return;
                }

                // Simply Love (VersusStepStatistics.lua) positions the two TapNoteJudgments actorframes at:
                // P1: x=-64, P2: x=+66 (relative to center). TapNoteJudgments internally uses
                // `PlayerNumber:Reverse()[player]` for halign, which is P1=0 (left), P2=1 (right),
                // so both number blocks extend inward and sit inside the 150px black column.
                let base_anchor_p1 = center_x - 64.0; // left edge for P1 block
                let base_anchor_p2 = center_x + 66.0; // right edge for P2 block
                let block_w = (digits as f32) * digit_w;
                let bar_left = center_x - 75.0;
                let bar_right = center_x + 75.0;
                let margin = 4.0;
                let anchor_p1 =
                    base_anchor_p1.clamp(bar_left + margin, bar_right - margin - block_w);
                let anchor_p2 =
                    base_anchor_p2.clamp(bar_left + margin + block_w, bar_right - margin);

                for (player_idx, show) in show_judgments_for.iter().copied().enumerate() {
                    if !show {
                        continue;
                    }
                    let is_p1 = player_idx == 0;
                    let group_y = 100.0;
                    let anchor_x = if is_p1 { anchor_p1 } else { anchor_p2 };
                    let group_origin_y = screen_center_y() + group_y;

                    let player_profile = &state.profiles()[player_idx];
                    let show_fa_plus_window = player_profile.show_fa_plus_window;
                    let show_fa_split =
                        show_fa_plus_window || player_profile.custom_fantastic_window;
                    let row_height = if show_fa_split { 29.0 } else { 35.0 };
                    let disabled_windows = player_profile.timing_windows.disabled_windows();

                    let (start, end) = state.note_range_for_player(player_idx);
                    if show_fa_split && end > start {
                        let blue_window_ms = state.gameplay_stats_text.blue_window_ms(player_idx);
                        let wc = state.display_window_counts(
                            player_idx,
                            Some(blue_window_ms),
                            blue_window_ms,
                        );
                        let counts = [wc.w0, wc.w1, wc.w2, wc.w3, wc.w4, wc.w5, wc.miss];
                        let bright_colors = [
                            color::JUDGMENT_RGBA[0],
                            color::JUDGMENT_FA_PLUS_WHITE_RGBA,
                            color::JUDGMENT_RGBA[1],
                            color::JUDGMENT_RGBA[2],
                            color::JUDGMENT_RGBA[3],
                            color::JUDGMENT_RGBA[4],
                            color::JUDGMENT_RGBA[5],
                        ];
                        let dim_colors = [
                            color::JUDGMENT_DIM_RGBA[0],
                            color::JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_RGBA,
                            color::JUDGMENT_DIM_RGBA[1],
                            color::JUDGMENT_DIM_RGBA[2],
                            color::JUDGMENT_DIM_RGBA[3],
                            color::JUDGMENT_DIM_RGBA[4],
                            color::JUDGMENT_DIM_RGBA[5],
                        ];
                        for (row_i, count) in counts.iter().copied().enumerate() {
                            let disabled = split_row_disabled(disabled_windows, row_i);
                            let y = group_origin_y
                                + (y_base + row_i as f32 * row_height) * group_zoom_y;
                            let (dim_text, bright_text) =
                                padded_runs_for_window(count, digits, disabled);
                            let dim_color = if disabled {
                                DISABLED_WINDOW_RGBA
                            } else {
                                dim_colors[row_i]
                            };
                            let bright_color = if disabled {
                                DISABLED_WINDOW_RGBA
                            } else {
                                bright_colors[row_i]
                            };
                            push_versus_count_texts(
                                actors,
                                state,
                                is_p1,
                                anchor_x,
                                y,
                                digit_w,
                                numbers_zoom_x,
                                numbers_zoom_y,
                                dim_text,
                                bright_text,
                                dim_color,
                                bright_color,
                                z_fg,
                            );
                        }
                    } else {
                        let counts = [
                            state.display_judgment_count(player_idx, JudgeGrade::Fantastic),
                            state.display_judgment_count(player_idx, JudgeGrade::Excellent),
                            state.display_judgment_count(player_idx, JudgeGrade::Great),
                            state.display_judgment_count(player_idx, JudgeGrade::Decent),
                            state.display_judgment_count(player_idx, JudgeGrade::WayOff),
                            state.display_judgment_count(player_idx, JudgeGrade::Miss),
                        ];
                        for (row_i, count) in counts.iter().copied().enumerate() {
                            let disabled = standard_row_disabled(disabled_windows, row_i);
                            let y = group_origin_y
                                + (y_base + row_i as f32 * row_height) * group_zoom_y;
                            let (dim_text, bright_text) =
                                padded_runs_for_window(count, digits, disabled);
                            let dim_color = if disabled {
                                DISABLED_WINDOW_RGBA
                            } else {
                                color::JUDGMENT_DIM_RGBA[row_i]
                            };
                            let bright_color = if disabled {
                                DISABLED_WINDOW_RGBA
                            } else {
                                color::JUDGMENT_RGBA[row_i]
                            };
                            push_versus_count_texts(
                                actors,
                                state,
                                is_p1,
                                anchor_x,
                                y,
                                digit_w,
                                numbers_zoom_x,
                                numbers_zoom_y,
                                dim_text,
                                bright_text,
                                dim_color,
                                bright_color,
                                z_fg,
                            );
                        }
                    }
                }
            });
        });
    }

    for (player_idx, show) in show_judgments_for.iter().copied().enumerate() {
        let player_profile = &state.profiles()[player_idx];
        if !show && !show_score_for[player_idx] {
            continue;
        }
        if !player_profile.nps_graph_at_top && !show_score_for[player_idx] {
            continue;
        }

        let (score_value, score_color) = if player_profile.show_ex_score {
            let blue_window_ms = state.gameplay_stats_text.blue_window_ms(player_idx);
            (
                state
                    .display_gameplay_ex_score_percent(
                        player_idx,
                        score_display_mode_from_profile(player_profile.score_display_mode),
                        blue_window_ms,
                    )
                    .max(0.0),
                color::JUDGMENT_RGBA[0],
            )
        } else {
            let score_percent = state.display_gameplay_itg_score_percent(
                player_idx,
                score_display_mode_from_profile(player_profile.score_display_mode),
            );
            (score_percent, [1.0, 1.0, 1.0, 1.0])
        };
        let x = center_x + if player_idx == 0 { -7.0 } else { 65.0 };
        push_score_counter(
            actors,
            asset_manager.fonts(),
            ScoreCounterParams {
                value: score_value,
                font: gameplay_font_key(state, FontRole::Numbers),
                position: [x, screen_center_y() - 150.0],
                align: [1.0, 1.0],
                text_align: TextAlign::Right,
                zoom: 0.25,
                color: score_color,
                z: z_fg,
            },
        );

        if player_profile.show_ex_score && player_profile.show_hard_ex_score {
            let blue_window_ms = state.gameplay_stats_text.blue_window_ms(player_idx);
            let hard_ex_percent = state.display_gameplay_hard_ex_score_percent(
                player_idx,
                score_display_mode_from_profile(player_profile.score_display_mode),
                blue_window_ms,
            );
            let hex = color::HARD_EX_SCORE_RGBA;
            let is_p1 = player_idx == 0;
            push_score_counter(
                actors,
                asset_manager.fonts(),
                ScoreCounterParams {
                    value: hard_ex_percent.max(0.0),
                    font: gameplay_font_key(state, FontRole::Numbers),
                    position: [
                        if is_p1 { x + 1.0 } else { x - 52.0 },
                        screen_center_y() - 154.0,
                    ],
                    align: if is_p1 { [0.0, 0.0] } else { [1.0, 0.0] },
                    text_align: if is_p1 {
                        TextAlign::Left
                    } else {
                        TextAlign::Right
                    },
                    zoom: 0.13,
                    color: hex,
                    z: z_fg,
                },
            );
        }
    }

    if show_song_banner && let Some(key) = &state.song_banner_key {
        actors.push(act!(sprite(key):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_center_y() + 70.0):
            setsize(418.0, 164.0):
            zoom(0.3):
            z(z_fg)
        ));
    }
}

pub fn push_double_step_stats(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    playfield_center_x: f32,
) {
    if !is_wide() {
        return;
    }
    let aspect_ratio = screen_width() / screen_height().max(1.0);
    let is_ultrawide = aspect_ratio > (21.0 / 9.0);
    if is_ultrawide {
        return;
    }
    if state.cols_per_player() <= 4 {
        return;
    }
    let mask = state
        .profiles()
        .first()
        .map_or(profile_data::StepStatisticsMask::empty(), |p| {
            p.step_statistics
        });
    if mask.is_empty() {
        return;
    }
    let display_scorebox = state.profiles().first().is_some_and(|p| p.display_scorebox);

    let Some(notefield_width) = notefield_width(state) else {
        return;
    };

    let layout = step_stats_theme::double_pane_layout(
        screen_center_x(),
        screen_center_y(),
        screen_width(),
        screen_height(),
        playfield_center_x,
    );
    let pane_cx = layout.pane_center_x;
    let pane_cy = layout.pane_center_y;
    let note_field_is_centered = layout.note_field_is_centered;
    let banner_data_zoom = layout.banner_data_zoom;

    actors.reserve(256);

    // DarkBackground.lua (double): two 200px-wide panels flanking the notefield.
    let nf_half_w = notefield_width * 0.5;
    let bg_y = screen_center_y();
    let z_bg = -80i16;
    actors.push(act!(quad:
        align(1.0, 0.5):
        xy(pane_cx - nf_half_w, bg_y):
        zoomto(200.0, screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.95):
        z(z_bg)
    ));
    actors.push(act!(quad:
        align(0.0, 0.5):
        xy(pane_cx + nf_half_w, bg_y):
        zoomto(200.0, screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.95):
        z(z_bg)
    ));

    // Banner.lua (double): xy(GetNotefieldWidth() - 140, -200)
    let song_banner = step_stats_theme::double_song_banner_placement(layout, notefield_width);
    if mask.contains(profile_data::StepStatisticsMask::SONG_BANNER)
        && let Some(banner_key) = &state.song_banner_key
    {
        actors.push(act!(sprite(banner_key):
            align(0.5, 0.5): xy(song_banner.x, song_banner.y):
            setsize(STEP_STATS_BANNER_W, STEP_STATS_BANNER_H):
            zoom(song_banner.zoom):
            z(-50)
        ));
    }

    // Banner2.lua (zmod pack banner): static (no animation) at the final position.
    if mask.pack_info_enabled()
        && let Some(pack_key) = state.pack_banner_key.as_ref()
    {
        let pack_banner = step_stats_theme::double_pack_banner_placement(layout, notefield_width);
        actors.push(act!(sprite(pack_key):
            align(0.5, 0.5): xy(pack_banner.x, pack_banner.y):
            setsize(STEP_STATS_BANNER_W, STEP_STATS_BANNER_H):
            zoom(pack_banner.zoom):
            z(-49)
        ));
    }

    step_stats_gifs::push_step_stats_extra(
        actors,
        state,
        0,
        crate::step_stats_gifs::GifRenderParams {
            player_side: gameplay_screen::runtime_profile_side(state, 0),
            wide: true,
            aspect_ratio,
            pane_x: pane_cx,
            pane_y: pane_cy,
            banner_data_zoom,
            note_field_is_centered,
        },
    );

    // TapNoteJudgments.lua (double): x(-GetNotefieldWidth() + 75), y(40), zoom(0.8)
    if mask.contains(profile_data::StepStatisticsMask::JUDGMENT_COUNTER) {
        let origin_x = pane_cx + ((-notefield_width + 75.0) * banner_data_zoom);
        let origin_y = pane_cy + (40.0 * banner_data_zoom);
        let base_zoom = 0.8 * banner_data_zoom;

        let total_tapnotes = state.display_totals_for_player(0).total_steps as f32;
        let digits = if total_tapnotes > 0.0 {
            (total_tapnotes.log10().floor() as usize + 1).max(4)
        } else {
            4
        };
        let show_fa_plus_window = state.profiles()[0].show_fa_plus_window;
        let player_profile = &state.profiles()[0];
        let show_fa_split = show_fa_plus_window || player_profile.custom_fantastic_window;
        let show_blue_ms_label = player_profile.custom_fantastic_window
            || (show_fa_plus_window && player_profile.fa_plus_10ms_blue_window);
        let disabled_windows = player_profile.timing_windows.disabled_windows();
        let blue_window_ms = state.gameplay_stats_text.blue_window_ms(0);
        let blue_window_label = state.gameplay_stats_text.blue_window(0);
        let row_height = if show_fa_split { 29.0 } else { 35.0 };
        let y_base = -280.0;

        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font(gameplay_font_key(state, FontRole::ScreenEval), |f| {
                let numbers_zoom = base_zoom * 0.5;
                let digit_w = glyph_width_scaled(f, all_fonts, '0', numbers_zoom);
                if digit_w <= 0.0 {
                    return;
                }
                let block_w = digit_w * digits as f32;
                let numbers_left_x = origin_x + (1.4 * block_w);
                let label_x =
                    origin_x + ((80.0 + (digits.saturating_sub(4) as f32 * 16.0)) * base_zoom);
                let label_zoom = base_zoom * 0.833;
                let show_standard_judgments = !show_fa_split;

                if show_standard_judgments {
                    let counts = [
                        state.display_judgment_count(0, JudgeGrade::Fantastic),
                        state.display_judgment_count(0, JudgeGrade::Excellent),
                        state.display_judgment_count(0, JudgeGrade::Great),
                        state.display_judgment_count(0, JudgeGrade::Decent),
                        state.display_judgment_count(0, JudgeGrade::WayOff),
                        state.display_judgment_count(0, JudgeGrade::Miss),
                    ];
                    for (row_i, count) in counts.into_iter().enumerate() {
                        let label = state.gameplay_stats_text.judgment(row_i);
                        let disabled = standard_row_disabled(disabled_windows, row_i);
                        let local_y = y_base + (row_i as f32 * row_height);
                        let y_numbers = origin_y + (local_y * base_zoom);
                        let y_label = origin_y + ((local_y + 1.0) * base_zoom);
                        let bright = if disabled {
                            DISABLED_WINDOW_RGBA
                        } else {
                            color::JUDGMENT_RGBA[row_i]
                        };
                        let dim = if disabled {
                            DISABLED_WINDOW_RGBA
                        } else {
                            color::JUDGMENT_DIM_RGBA[row_i]
                        };
                        let (dim_text, bright_text) =
                            padded_runs_for_window(count, digits, disabled);
                        let dim_len = dim_text.len() as f32;

                        if !dim_text.is_empty() {
                            actors.push(act!(text:
                                font(gameplay_font_key(state, FontRole::ScreenEval)): settext(dim_text):
                                align(0.0, 0.5): xy(numbers_left_x, y_numbers):
                                zoom(numbers_zoom):
                                diffuse(dim[0], dim[1], dim[2], dim[3]):
                                z(71):
                                horizalign(left)
                            ));
                        }
                        if !bright_text.is_empty() {
                            actors.push(act!(text:
                                font(gameplay_font_key(state, FontRole::ScreenEval)): settext(bright_text):
                                align(0.0, 0.5): xy(numbers_left_x + dim_len * digit_w, y_numbers):
                                zoom(numbers_zoom):
                                diffuse(bright[0], bright[1], bright[2], bright[3]):
                                z(71):
                                horizalign(left)
                            ));
                        }

                        actors.push(act!(text:
                            font("miso"): settext(label):
                            align(1.0, 0.5): horizalign(right):
                            xy(label_x, y_label):
                            zoom(label_zoom):
                            maxwidth(72.0 * base_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]):
                            z(71)
                        ));

                        if show_blue_ms_label && row_i == 0 {
                            let y = y_label + (12.0 * base_zoom);
                            actors.push(act!(text:
                                font("miso"): settext(blue_window_label.clone()):
                                align(1.0, 0.5): horizalign(right):
                                xy(label_x, y):
                                zoom(0.6 * base_zoom):
                                maxwidth(72.0 * base_zoom):
                                diffuse(bright[0], bright[1], bright[2], bright[3]):
                                z(71)
                            ));
                        }
                    }
                } else {
                    let wc = state.display_window_counts(0, Some(blue_window_ms), blue_window_ms);
                    let counts = [wc.w0, wc.w1, wc.w2, wc.w3, wc.w4, wc.w5, wc.miss];
                    let bright_colors = [
                        color::JUDGMENT_RGBA[0],
                        color::JUDGMENT_FA_PLUS_WHITE_RGBA,
                        color::JUDGMENT_RGBA[1],
                        color::JUDGMENT_RGBA[2],
                        color::JUDGMENT_RGBA[3],
                        color::JUDGMENT_RGBA[4],
                        color::JUDGMENT_RGBA[5],
                    ];
                    let dim_colors = [
                        color::JUDGMENT_DIM_RGBA[0],
                        color::JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_RGBA,
                        color::JUDGMENT_DIM_RGBA[1],
                        color::JUDGMENT_DIM_RGBA[2],
                        color::JUDGMENT_DIM_RGBA[3],
                        color::JUDGMENT_DIM_RGBA[4],
                        color::JUDGMENT_DIM_RGBA[5],
                    ];

                    let fa_label = state.gameplay_stats_text.judgment(0);
                    let labels = [
                        fa_label.clone(),
                        fa_label,
                        state.gameplay_stats_text.judgment(1),
                        state.gameplay_stats_text.judgment(2),
                        state.gameplay_stats_text.judgment(3),
                        state.gameplay_stats_text.judgment(4),
                        state.gameplay_stats_text.judgment(5),
                    ];
                    for row_i in 0..labels.len() {
                        let disabled = split_row_disabled(disabled_windows, row_i);
                        let local_y = y_base + (row_i as f32 * row_height);
                        let y_numbers = origin_y + (local_y * base_zoom);
                        let y_label = origin_y + ((local_y + 1.0) * base_zoom);
                        let bright = if disabled {
                            DISABLED_WINDOW_RGBA
                        } else {
                            bright_colors[row_i]
                        };
                        let dim = if disabled {
                            DISABLED_WINDOW_RGBA
                        } else {
                            dim_colors[row_i]
                        };
                        let count = counts[row_i];
                        let (dim_text, bright_text) =
                            padded_runs_for_window(count, digits, disabled);
                        let dim_len = dim_text.len() as f32;

                        if !dim_text.is_empty() {
                            actors.push(act!(text:
                                font(gameplay_font_key(state, FontRole::ScreenEval)): settext(dim_text):
                                align(0.0, 0.5): xy(numbers_left_x, y_numbers):
                                zoom(numbers_zoom):
                                diffuse(dim[0], dim[1], dim[2], dim[3]):
                                z(71):
                                horizalign(left)
                            ));
                        }
                        if !bright_text.is_empty() {
                            actors.push(act!(text:
                                font(gameplay_font_key(state, FontRole::ScreenEval)): settext(bright_text):
                                align(0.0, 0.5): xy(numbers_left_x + dim_len * digit_w, y_numbers):
                                zoom(numbers_zoom):
                                diffuse(bright[0], bright[1], bright[2], bright[3]):
                                z(71):
                                horizalign(left)
                            ));
                        }

                        actors.push(act!(text:
                            font("miso"): settext(labels[row_i].clone()):
                            align(1.0, 0.5): horizalign(right):
                            xy(label_x, y_label):
                            zoom(label_zoom):
                            maxwidth(72.0 * base_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]):
                            z(71)
                        ));

                        if show_blue_ms_label && row_i == 0 {
                            let y = y_label + (12.0 * base_zoom);
                            actors.push(act!(text:
                                font("miso"): settext(blue_window_label.clone()):
                                align(1.0, 0.5): horizalign(right):
                                xy(label_x, y):
                                zoom(0.6 * base_zoom):
                                maxwidth(72.0 * base_zoom):
                                diffuse(bright[0], bright[1], bright[2], bright[3]):
                                z(71)
                            ));
                        }
                    }
                }
            });
        });
    }

    // HoldsMinesRolls.lua (double): x(-GetNotefieldWidth() + 212), y(-10), zoom(0.8)
    if mask.contains(profile_data::StepStatisticsMask::STEP_COUNTS) && !display_scorebox {
        let frame = step_stats_theme::double_holds_mines_rolls_frame(layout, notefield_width);

        push_holds_mines_rolls_pane_at(
            actors,
            state,
            asset_manager,
            frame.center_x,
            frame.center_y,
            frame.zoom,
        );
    }

    // Scorebox.lua (double): x(GetNotefieldWidth() - 140), y(-115)
    if mask.contains(profile_data::StepStatisticsMask::STEP_COUNTS) && display_scorebox {
        let frame = step_stats_theme::double_scorebox_frame(layout, notefield_width);
        let side = gameplay_screen::runtime_profile_side(state, 0);
        gameplay_screen::push_scorebox_actors_for_side(
            actors,
            state,
            side,
            frame.center_x,
            frame.center_y,
            frame.zoom,
        );
    }

    // Time.lua (double): x(-GetNotefieldWidth() + 150), y(75)
    if mask.contains(profile_data::StepStatisticsMask::SONG_DURATION) {
        let base_x = pane_cx + ((-notefield_width + 150.0) * banner_data_zoom);
        let base_y = pane_cy + (75.0 * banner_data_zoom);

        let time_display = step_stats_time_display(state, 0);
        let total_display_seconds = time_display.total_seconds;
        let elapsed_display_seconds = time_display.elapsed_seconds;

        let total_time_key = game_time_key(total_display_seconds, total_display_seconds);
        let total_time_str = game_time_text(total_time_key.0, total_time_key.1);
        let remaining_display_seconds = (total_display_seconds - elapsed_display_seconds).max(0.0);
        let remaining_time_key = game_time_key(remaining_display_seconds, total_display_seconds);
        let remaining_time_str = game_time_text(remaining_time_key.0, remaining_time_key.1);

        let number_zoom = banner_data_zoom;
        let label_zoom = 0.833 * number_zoom;
        let total_w = cached_game_time_width_for_key(total_time_key, asset_manager);

        // Simply Love (Time.lua):
        // label x = 32 + (total_width - 28) == total_width + 4
        let label_x = base_x + (total_w + 4.0) * number_zoom;

        // Remaining row (y=0)
        actors.push(act!(text:
            font("miso"):
            settext(remaining_time_str):
            align(-1.2, 0.5):
            xy(base_x, base_y):
            zoom(number_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(time_remaining_text(state)):
            align(1.0, 0.5):
            horizalign(right):
            xy(label_x, base_y + 1.0 * number_zoom):
            zoom(label_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));

        // Total row (y=20)
        actors.push(act!(text:
            font("miso"):
            settext(total_time_str):
            align(-1.2, 0.5):
            xy(base_x, base_y + (20.0 * number_zoom)):
            zoom(number_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(time_total_text(state)):
            align(1.0, 0.5):
            horizalign(right):
            xy(label_x, base_y + (20.0 * number_zoom) + 1.0 * number_zoom):
            zoom(label_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(71)
        ));

        let timing_label_x = label_x + (104.0 * number_zoom);
        push_live_timing_stats_at(
            actors,
            state,
            0,
            profile_data::PlayerSide::P1,
            timing_label_x,
            timing_label_x + (156.0 * number_zoom),
            false,
            base_y,
            20.0 * number_zoom,
            label_zoom,
            71,
        );
    }

    // DensityGraph.lua (double): graph ActorFrame xy(260, 40), with width
    // calculated as 95% of the side pane in gameplay init.
    let double_sidepane_width =
        step_stats_theme::double_sidepane_width(screen_width(), notefield_width);
    let double_graph = step_stats_theme::double_density_graph_rect(
        layout,
        screen_width(),
        notefield_width,
        state.gameplay.density_graph_view().graph_w,
    );
    if mask.contains(profile_data::StepStatisticsMask::DENSITY_GRAPH) {
        push_density_graph_at(actors, state, 0, double_graph.x, double_graph.y);
    }

    // Peak NPS text (DensityGraph.lua drives this in SL).
    if mask.contains(profile_data::StepStatisticsMask::PEAK_NPS) {
        push_peak_nps_on_graph(
            actors,
            state,
            0,
            gameplay_screen::runtime_profile_side(state, 0),
            double_graph,
            song_info_text_zoom(StepStatsPaneLayout {
                sidepane_center_x: pane_cx,
                sidepane_center_y: pane_cy,
                sidepane_width: double_sidepane_width,
                note_field_is_centered,
                is_ultrawide,
                banner_data_zoom,
            }),
        );
    }
}

// --- Statics for Judgment Counter Display ---

static JUDGMENT_ORDER: [JudgeGrade; 6] = [
    JudgeGrade::Fantastic,
    JudgeGrade::Excellent,
    JudgeGrade::Great,
    JudgeGrade::Decent,
    JudgeGrade::WayOff,
    JudgeGrade::Miss,
];

fn judgment_info(grade: JudgeGrade) -> &'static LabeledColor {
    &JUDGMENT_INFO[judgment::judge_grade_ix(grade)]
}

fn build_banner(
    actors: &mut Vec<Actor>,
    state: &State,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile_data::PlayerSide,
) {
    if let Some(banner_key) = &state.song_banner_key {
        let placement =
            step_stats_theme::song_banner_placement(layout, wide, player_side, state.num_players());
        actors.push(act!(sprite(banner_key):
            align(0.5, 0.5): xy(placement.x, placement.y):
            setsize(STEP_STATS_BANNER_W, STEP_STATS_BANNER_H): zoom(placement.zoom):
            z(-50)
        ));
    }
}

fn build_pack_banner(
    actors: &mut Vec<Actor>,
    state: &State,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile_data::PlayerSide,
) {
    if !wide {
        return;
    }
    let Some(pack_key) = state.pack_banner_key.as_ref() else {
        return;
    };

    // Arrow Cloud Banner2.lua parity for non-double Step Statistics. The
    // doubles-specific renderer handles its separate left-edge alignment.
    let placement = step_stats_theme::pack_banner_placement(layout, player_side);

    actors.push(act!(sprite(pack_key):
        align(0.5, 0.5):
        xy(placement.x, placement.y):
        setsize(STEP_STATS_BANNER_W, STEP_STATS_BANNER_H):
        zoom(placement.zoom):
        z(-49)
    ));
}

fn build_steps_info(
    actors: &mut Vec<Actor>,
    state: &State,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile_data::PlayerSide,
    show_song_info: bool,
) {
    if !wide {
        return;
    }
    actors.reserve(if layout.note_field_is_centered { 5 } else { 9 });

    // Dark background for the Step Statistics side pane (Simply Love: DarkBackground.lua).
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(layout.sidepane_center_x, screen_center_y()):
        zoomto(layout.sidepane_width, screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.95):
        z(-80)
    ));
    if !show_song_info {
        return;
    }
    let note_field_is_centered = layout.note_field_is_centered;
    let banner_data_zoom = layout.banner_data_zoom;

    let player_idx = match (state.num_players(), player_side) {
        (2, profile_data::PlayerSide::P2) => 1,
        _ => 0,
    };
    let desc_text = state
        .gameplay_stats_text
        .description(player_idx, state.gameplay.total_elapsed_in_screen());

    let ar = screen_width() / screen_height().max(1.0);
    let pnum = profile_data::player_side_number(player_side);
    let pos_sign = if pnum == 1 { -1.0 } else { 1.0 };

    let mut x = -190.0;
    let xoffset = if pnum == 1 { 285.0 } else { 0.0 };
    let mut yoffset = 0.0;
    let mut xvalues = 45.0;
    let mut maxwidth = 320.0;
    if note_field_is_centered {
        xvalues = 0.0;
        yoffset = -5.0;
        if ar > 1.7 {
            x = if pnum == 1 { -220.0 } else { -150.0 };
            maxwidth = 240.0;
        } else {
            x = if pnum == 1 { -240.0 } else { -150.0 };
            maxwidth = 210.0;
        }
    }

    let origin_x = layout.sidepane_center_x + ((x + xoffset) * pos_sign * banner_data_zoom);
    let origin_y = layout.sidepane_center_y + ((-8.0 + yoffset) * banner_data_zoom);
    let group_zoom = song_info_text_zoom(layout);

    let row_h = 16.0;
    let z = 72i16;
    if !note_field_is_centered {
        for i in 0..4 {
            let y = origin_y + (row_h * (i as f32 + 1.0) * group_zoom);
            actors.push(act!(text:
                font("miso"): settext(step_info_label_text(state, i)):
                align(0.0, 0.5): xy(origin_x, y):
                zoom(group_zoom): z(z):
                horizalign(left)
            ));
        }
    }

    let values_x = origin_x + (xvalues * group_zoom);
    let y_song = origin_y + (row_h * 1.0 * group_zoom);
    actors.push(act!(text:
        font("miso"): settext(state.song_full_title.clone()):
        align(0.0, 0.5): xy(values_x, y_song):
        maxwidth(maxwidth):
        zoom(group_zoom): z(z):
        horizalign(left)
    ));
    let y_artist = origin_y + (row_h * 2.0 * group_zoom);
    actors.push(act!(text:
        font("miso"): settext(Arc::clone(&state.gameplay_stats_text.song_artist)):
        align(0.0, 0.5): xy(values_x, y_artist):
        maxwidth(maxwidth):
        zoom(group_zoom): z(z):
        horizalign(left)
    ));
    let y_pack = origin_y + (row_h * 3.0 * group_zoom);
    actors.push(act!(text:
        font("miso"): settext(state.pack_group.clone()):
        align(0.0, 0.5): xy(values_x, y_pack):
        maxwidth(maxwidth):
        zoom(group_zoom): z(z):
        horizalign(left)
    ));
    let y_desc = origin_y + (row_h * 4.0 * group_zoom);
    actors.push(act!(text:
        font("miso"): settext(desc_text):
        align(0.0, 0.5): xy(values_x, y_desc):
        maxwidth(maxwidth):
        zoom(group_zoom): z(z):
        horizalign(left)
    ));
}

fn push_holds_mines_rolls_pane_at(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    frame_cx: f32,
    frame_cy: f32,
    frame_zoom: f32,
) {
    let categories = step_stats_hmr_categories(state, 0);

    let largest_count = categories
        .iter()
        .map(|(_, achieved, total)| (*achieved).max(*total))
        .max()
        .unwrap_or(0);
    let digits_needed = if largest_count == 0 {
        1
    } else {
        (largest_count as f32).log10().floor() as usize + 1
    };
    let digits_to_fmt = digits_needed.clamp(3, 4);
    let row_height = 28.0 * frame_zoom;
    actors.reserve(categories.len() * (digits_to_fmt * 2 + 2));

    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font(gameplay_font_key(state, FontRole::ScreenEval), |metrics_font| {
            let value_zoom = 0.4 * frame_zoom;
            let label_zoom = 0.833 * frame_zoom;
            const GRAY: [f32; 4] = color::rgba_hex("#5A6166");
            let white = [1.0, 1.0, 1.0, 1.0];

            let digit_width = glyph_width_scaled(metrics_font, all_fonts, '0', value_zoom);
            if digit_width <= 0.0 {
                return;
            }
            let slash_width = glyph_width_scaled(metrics_font, all_fonts, '/', value_zoom);

            const LOGICAL_CHAR_WIDTH_FOR_LABEL: f32 = 36.0;
            let fixed_char_width_scaled_for_label = LOGICAL_CHAR_WIDTH_FOR_LABEL * value_zoom;

            for (i, (label_index, achieved, total)) in categories.iter().enumerate() {
                let item_y = frame_cy + (i as f32 - 1.0) * row_height;
                let right_anchor_x = frame_cx;
                let mut cursor_x = right_anchor_x;

                let possible_str = padded_num_text(*total, digits_to_fmt);
                let achieved_str = padded_num_text(*achieved, digits_to_fmt);
                let possible_bytes = possible_str.as_str().as_bytes();
                let achieved_bytes = achieved_str.as_str().as_bytes();
                let possible_split = padded_dim_len(possible_str.as_str(), *total, digits_to_fmt);
                let achieved_split =
                    padded_dim_len(achieved_str.as_str(), *achieved, digits_to_fmt);

                for char_idx in 0..possible_bytes.len() {
                    let original_index = possible_bytes.len() - 1 - char_idx;
                    let color = if original_index < possible_split { GRAY } else { white };
                    let x_pos = cursor_x - (char_idx as f32 * digit_width);
                    actors.push(act!(text:
                        font(gameplay_font_key(state, FontRole::ScreenEval)): settext(digit_text(possible_bytes[original_index])):
                        align(1.0, 0.5): xy(x_pos, item_y):
                        zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3]): z(70)
                    ));
                }
                cursor_x -= possible_bytes.len() as f32 * digit_width;

                actors.push(act!(text:
                    font(gameplay_font_key(state, FontRole::ScreenEval)): settext(SLASH_TEXT.clone()):
                    align(1.0, 0.5): xy(cursor_x, item_y):
                    zoom(value_zoom): diffuse(GRAY[0], GRAY[1], GRAY[2], GRAY[3]): z(70)
                ));
                cursor_x -= slash_width;

                for char_idx in 0..achieved_bytes.len() {
                    let original_index = achieved_bytes.len() - 1 - char_idx;
                    let color = if original_index < achieved_split { GRAY } else { white };
                    let x_pos = cursor_x - (char_idx as f32 * digit_width);
                    actors.push(act!(text:
                        font(gameplay_font_key(state, FontRole::ScreenEval)): settext(digit_text(achieved_bytes[original_index])):
                        align(1.0, 0.5): xy(x_pos, item_y):
                        zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3]): z(70)
                    ));
                }

                let total_value_width_for_label = (achieved_str.len() + 1 + possible_str.len())
                    as f32
                    * fixed_char_width_scaled_for_label;
                let label_x = right_anchor_x - total_value_width_for_label - (10.0 * frame_zoom);

                actors.push(act!(text:
                    font("miso"): settext(holds_mines_rolls_label_text(state, *label_index)):
                    align(1.0, 0.5): xy(label_x, item_y):
                    zoom(label_zoom):
                    horizalign(right):
                    diffuse(white[0], white[1], white[2], white[3]):
                    z(70)
                ));
            }
        });
    });
}

fn notefield_width(state: &State) -> Option<f32> {
    if state.cols_per_player() == 0 {
        return None;
    }
    // Simply Love GetNotefieldWidth() parity: dance single/versus are 256
    // and double is 512. This is independent of Mini, Spacing, and noteskin
    // render scale, so step-stat panes do not drift with visual modifiers.
    Some(state.cols_per_player() as f32 * 64.0)
}

fn build_holds_mines_rolls_pane(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile_data::PlayerSide,
) {
    if !wide {
        return;
    }
    let player_idx = step_stats_player_idx(state, player_side);
    let frame = step_stats_theme::holds_mines_rolls_frame(layout, player_side);

    let categories = step_stats_hmr_categories(state, player_idx);

    let largest_count = categories
        .iter()
        .map(|(_, achieved, total)| (*achieved).max(*total))
        .max()
        .unwrap_or(0);
    let digits_needed = if largest_count == 0 {
        1
    } else {
        (largest_count as f32).log10().floor() as usize + 1
    };
    let digits_to_fmt = digits_needed.clamp(3, 4);
    let row_height = 28.0 * frame.zoom;
    actors.reserve(categories.len() * (digits_to_fmt * 2 + 2));

    asset_manager.with_fonts(|all_fonts| asset_manager.with_font(gameplay_font_key(state, FontRole::ScreenEval), |metrics_font| {
        let value_zoom = 0.4 * frame.zoom;
        let label_zoom = 0.833 * frame.zoom;
        let gray = color::rgba_hex("#5A6166");
        let white = [1.0, 1.0, 1.0, 1.0];

        // --- HYBRID LAYOUT LOGIC ---
        // 1. Measure real character widths for number layout.
        let digit_width = glyph_width_scaled(metrics_font, all_fonts, '0', value_zoom);
        if digit_width <= 0.0 { return; }
        let slash_width = glyph_width_scaled(metrics_font, all_fonts, '/', value_zoom);

        // 2. Use a hardcoded width for calculating the label's position (for theme parity).
        const LOGICAL_CHAR_WIDTH_FOR_LABEL: f32 = 36.0;
        let fixed_char_width_scaled_for_label = LOGICAL_CHAR_WIDTH_FOR_LABEL * value_zoom;

        for (i, (label_index, achieved, total)) in categories.iter().enumerate() {
            let item_y = frame.center_y + (i as f32 - 1.0) * row_height;
            let right_anchor_x = match player_side {
                profile_data::PlayerSide::P1 => frame.center_x,
                profile_data::PlayerSide::P2 => frame.center_x + 100.0 * frame.zoom,
            };
            let mut cursor_x = right_anchor_x;

            let possible_str = padded_num_text(*total, digits_to_fmt);
            let achieved_str = padded_num_text(*achieved, digits_to_fmt);
            let possible_bytes = possible_str.as_str().as_bytes();
            let achieved_bytes = achieved_str.as_str().as_bytes();
            let possible_split = padded_dim_len(possible_str.as_str(), *total, digits_to_fmt);
            let achieved_split = padded_dim_len(achieved_str.as_str(), *achieved, digits_to_fmt);

            // --- Layout Numbers using MEASURED widths ---
            // 1. Draw "possible" number (right-most part)
            for char_idx in 0..possible_bytes.len() {
                let original_index = possible_bytes.len() - 1 - char_idx;
                let color = if original_index < possible_split {
                    gray
                } else {
                    white
                };
                let x_pos = cursor_x - (char_idx as f32 * digit_width);
                actors.push(act!(text:
                    font(gameplay_font_key(state, FontRole::ScreenEval)): settext(digit_text(possible_bytes[original_index])):
                    align(1.0, 0.5): xy(x_pos, item_y):
                    zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3]): z(70)
                ));
            }
            cursor_x -= possible_bytes.len() as f32 * digit_width;

            // 2. Draw slash
            actors.push(act!(text: font(gameplay_font_key(state, FontRole::ScreenEval)): settext(SLASH_TEXT.clone()): align(1.0, 0.5): xy(cursor_x, item_y): zoom(value_zoom): diffuse(gray[0], gray[1], gray[2], gray[3]): z(70)));
            cursor_x -= slash_width;

            // 3. Draw "achieved" number
            for char_idx in 0..achieved_bytes.len() {
                let original_index = achieved_bytes.len() - 1 - char_idx;
                let color = if original_index < achieved_split {
                    gray
                } else {
                    white
                };
                let x_pos = cursor_x - (char_idx as f32 * digit_width);
                actors.push(act!(text:
                    font(gameplay_font_key(state, FontRole::ScreenEval)): settext(digit_text(achieved_bytes[original_index])):
                    align(1.0, 0.5): xy(x_pos, item_y):
                    zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3]): z(70)
                ));
            }

            // --- Position Label using HARDCODED width assumption ---
            let total_value_width_for_label = (achieved_str.len() + 1 + possible_str.len()) as f32 * fixed_char_width_scaled_for_label;
            let label_x = right_anchor_x - total_value_width_for_label - (10.0 * frame.zoom);

            actors.push(act!(text:
                font("miso"): settext(holds_mines_rolls_label_text(state, *label_index)): align(1.0, 0.5): xy(label_x, item_y):
                zoom(label_zoom): horizalign(right): diffuse(white[0], white[1], white[2], white[3]): z(70)
            ));
        }
    }));
}

fn build_scorebox_pane(
    actors: &mut Vec<Actor>,
    state: &State,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile_data::PlayerSide,
) {
    if !wide {
        return;
    }

    let frame = step_stats_theme::scorebox_frame(layout, wide, player_side, state.num_players());

    gameplay_screen::push_scorebox_actors_for_side(
        actors,
        state,
        player_side,
        frame.center_x,
        frame.center_y,
        frame.zoom,
    );
}

#[allow(clippy::too_many_arguments)]
fn push_live_timing_stats_at(
    actors: &mut Vec<Actor>,
    state: &State,
    player_idx: usize,
    player_side: profile_data::PlayerSide,
    label_x: f32,
    value_x: f32,
    value_align_right: bool,
    first_y: f32,
    row_h: f32,
    zoom: f32,
    z: i16,
) {
    if player_idx >= state.num_players() {
        return;
    }

    let profile = &state.profiles()[player_idx];
    if !profile.live_timing_stats {
        return;
    }

    let mask = profile.live_timing_stats_mask;
    let enabled_count = live_timing_enabled_count(mask);
    if enabled_count == 0 {
        return;
    }

    let stats = state.display_live_timing_stats(player_idx);
    let compact = enabled_count >= 3;
    let row_h = if compact { row_h * 0.68 } else { row_h };
    let zoom = if compact { zoom * 0.82 } else { zoom };
    let first_y = if compact {
        first_y - row_h * 0.12
    } else {
        first_y
    };
    let label_max_w = if compact { 150.0 } else { 170.0 } * zoom;
    let mut row = 0usize;

    for index in 0..LIVE_TIMING_LABELS.len() {
        if !mask.contains(live_timing_stat_mask(index)) {
            continue;
        }

        let y = first_y + row_h * row as f32;
        let label = LIVE_TIMING_LABELS[index].clone();
        let value = live_timing_value(stats, player_idx, index);
        row += 1;

        if player_side == profile_data::PlayerSide::P1 {
            actors.push(act!(text: font("miso"): settext(label):
                align(0.0, 0.5): xy(label_x, y):
                zoom(zoom): maxwidth(label_max_w): horizalign(left):
                diffuse(1.0, 1.0, 1.0, 1.0): z(z)
            ));
            if value_align_right {
                actors.push(act!(text: font("miso"): settext(value):
                    align(1.0, 0.5): xy(value_x, y):
                    zoom(zoom): horizalign(right):
                    diffuse(1.0, 1.0, 1.0, 1.0): z(z)
                ));
            } else {
                actors.push(act!(text: font("miso"): settext(value):
                    align(0.0, 0.5): xy(value_x, y):
                    zoom(zoom): horizalign(left):
                    diffuse(1.0, 1.0, 1.0, 1.0): z(z)
                ));
            }
        } else {
            actors.push(act!(text: font("miso"): settext(label):
                align(1.0, 0.5): xy(label_x, y):
                zoom(zoom): maxwidth(label_max_w): horizalign(right):
                diffuse(1.0, 1.0, 1.0, 1.0): z(z)
            ));
            actors.push(act!(text: font("miso"): settext(value):
                align(1.0, 0.5): xy(value_x, y):
                zoom(zoom): horizalign(right):
                diffuse(1.0, 1.0, 1.0, 1.0): z(z)
            ));
        }
    }
}

fn build_side_pane(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: profile_data::PlayerSide,
    mask: profile_data::StepStatisticsMask,
) {
    if !wide {
        return;
    }

    let x_sign = match player_side {
        profile_data::PlayerSide::P1 => 1.0,
        profile_data::PlayerSide::P2 => -1.0,
    };
    let player_idx = step_stats_player_idx(state, player_side);
    let judgments_local_x = if layout.is_ultrawide && state.num_players() > 1 {
        154.0 * x_sign
    } else if layout.note_field_is_centered && wide {
        -156.0 * x_sign
    } else {
        -widescale(152.0, 204.0) * x_sign
    };
    let final_judgments_center_x =
        layout.sidepane_center_x + (judgments_local_x * layout.banner_data_zoom);
    let final_judgments_center_y = layout.sidepane_center_y;
    let parent_local_zoom = 0.8;
    let final_text_base_zoom = layout.banner_data_zoom * parent_local_zoom;

    let total_tapnotes = state.display_totals_for_player(player_idx).total_steps as f32;
    let digits = if total_tapnotes > 0.0 {
        (total_tapnotes.log10().floor() as usize + 1).max(4)
    } else {
        4
    };
    let extra_digits = digits.saturating_sub(4) as f32;
    let base_label_local_x_offset = 80.0;
    const LABEL_DIGIT_STEP: f32 = 16.0;
    const NUMBER_TO_LABEL_GAP: f32 = 8.0;
    let base_numbers_local_x_offset = base_label_local_x_offset - NUMBER_TO_LABEL_GAP;
    let show_fa_plus_window = state.profiles()[player_idx].show_fa_plus_window;
    let player_profile = &state.profiles()[player_idx];
    let show_fa_split = show_fa_plus_window || player_profile.custom_fantastic_window;
    let show_blue_ms_label = player_profile.custom_fantastic_window
        || (show_fa_plus_window && player_profile.fa_plus_10ms_blue_window);
    let disabled_windows = player_profile.timing_windows.disabled_windows();
    let blue_window_ms = state.gameplay_stats_text.blue_window_ms(player_idx);
    let blue_window_label = state.gameplay_stats_text.blue_window(player_idx);
    actors.reserve(if show_fa_split {
        22 + usize::from(show_blue_ms_label)
    } else {
        16
    });
    let row_height = if show_fa_split { 29.0 } else { 35.0 };
    let y_base = -280.0;
    let show_judgments = mask.contains(profile_data::StepStatisticsMask::JUDGMENT_COUNTER);
    let show_duration = mask.contains(profile_data::StepStatisticsMask::SONG_DURATION);

    if show_judgments || show_duration {
        asset_manager.with_fonts(|all_fonts| {
            asset_manager.with_font(gameplay_font_key(state, FontRole::ScreenEval), |f| {
        let numbers_zoom = final_text_base_zoom * 0.5;
        let max_digit_w = glyph_width_scaled(f, all_fonts, '0', numbers_zoom);
        if max_digit_w <= 0.0 { return; }

        let digit_local_width = max_digit_w / final_text_base_zoom;
        let label_local_x_offset = base_label_local_x_offset + (extra_digits * LABEL_DIGIT_STEP);
        let label_world_x =
            final_judgments_center_x + (x_sign * label_local_x_offset * final_text_base_zoom);
        let numbers_local_x_offset = base_numbers_local_x_offset + (extra_digits * digit_local_width);
        let numbers_cx =
            final_judgments_center_x + (x_sign * numbers_local_x_offset * final_text_base_zoom);
        let show_standard_judgments = !show_fa_split;

        if show_judgments && show_standard_judgments {
            // Standard ITG-style rows: Fantastic..Miss using aggregate grade counts.
            for (index, grade) in JUDGMENT_ORDER.iter().enumerate() {
                let info = judgment_info(*grade);
                let count = state.display_judgment_count(player_idx, *grade);
                let disabled = standard_row_disabled(disabled_windows, index);

                let local_y = y_base + (index as f32 * row_height);
                let world_y = final_judgments_center_y + (local_y * final_text_base_zoom);

                let bright = if disabled {
                    DISABLED_WINDOW_RGBA
                } else {
                    info.color
                };
                let dim = if disabled {
                    DISABLED_WINDOW_RGBA
                } else {
                    color::JUDGMENT_DIM_RGBA[index]
                };
                let (dim_text, bright_text) = padded_runs_for_window(count, digits, disabled);
                let dim_len = dim_text.len() as f32;
                let bright_len = bright_text.len() as f32;

                if player_side == profile_data::PlayerSide::P1 {
                    if !bright_text.is_empty() {
                        actors.push(act!(text:
                            font(gameplay_font_key(state, FontRole::ScreenEval)): settext(bright_text):
                            align(1.0, 0.5): xy(numbers_cx, world_y): zoom(numbers_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]): z(71)
                        ));
                    }
                    if !dim_text.is_empty() {
                        actors.push(act!(text:
                            font(gameplay_font_key(state, FontRole::ScreenEval)): settext(dim_text):
                            align(1.0, 0.5): xy(numbers_cx - bright_len * max_digit_w, world_y):
                            zoom(numbers_zoom):
                            diffuse(dim[0], dim[1], dim[2], dim[3]): z(71)
                        ));
                    }
                } else {
                    if !dim_text.is_empty() {
                        actors.push(act!(text:
                            font(gameplay_font_key(state, FontRole::ScreenEval)): settext(dim_text):
                            align(0.0, 0.5): xy(numbers_cx, world_y): zoom(numbers_zoom):
                            diffuse(dim[0], dim[1], dim[2], dim[3]): z(71):
                            horizalign(left)
                        ));
                    }
                    if !bright_text.is_empty() {
                        actors.push(act!(text:
                            font(gameplay_font_key(state, FontRole::ScreenEval)): settext(bright_text):
                            align(0.0, 0.5): xy(numbers_cx + dim_len * max_digit_w, world_y):
                            zoom(numbers_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]): z(71):
                            horizalign(left)
                        ));
                    }
                }

                let label_world_y = world_y + (1.0 * final_text_base_zoom);
                let label_zoom = final_text_base_zoom * 0.833;
                let label = info.label.get();

                if player_side == profile_data::PlayerSide::P1 {
                    actors.push(act!(text:
                        font("miso"): settext(label): align(0.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(left):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                } else {
                    actors.push(act!(text:
                        font("miso"): settext(label): align(1.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(right):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                }
            }
        } else if show_judgments {
            // FA+ mode: split Fantastic into W0 (blue) and W1 (white) using per-note windows,
            // matching Simply Love's FA+ Step Statistics semantics.
            let wc = state.display_window_counts(player_idx, Some(blue_window_ms), blue_window_ms);
            let fantastic_color = judgment_info(JudgeGrade::Fantastic).color;
            let excellent_color = judgment_info(JudgeGrade::Excellent).color;
            let great_color = judgment_info(JudgeGrade::Great).color;
            let decent_color = judgment_info(JudgeGrade::Decent).color;
            let wayoff_color = judgment_info(JudgeGrade::WayOff).color;
            let miss_color = judgment_info(JudgeGrade::Miss).color;

            // Dim palette for FA+ side pane: reuse gameplay dim colors for Fantastic..Miss,
            // and a dedicated dim color for the white FA+ row.
            let dim_fantastic = color::JUDGMENT_DIM_RGBA[0];
            let dim_excellent = color::JUDGMENT_DIM_RGBA[1];
            let dim_great = color::JUDGMENT_DIM_RGBA[2];
            let dim_decent = color::JUDGMENT_DIM_RGBA[3];
            let dim_wayoff = color::JUDGMENT_DIM_RGBA[4];
            let dim_miss = color::JUDGMENT_DIM_RGBA[5];
            let dim_white_fa = color::JUDGMENT_FA_PLUS_WHITE_GAMEPLAY_DIM_RGBA;

            let white_fa_color = color::JUDGMENT_FA_PLUS_WHITE_RGBA;

            let rows: [(usize, [f32; 4], [f32; 4], u32); 7] = [
                (0, fantastic_color, dim_fantastic, wc.w0),
                (0, white_fa_color, dim_white_fa, wc.w1),
                (1, excellent_color, dim_excellent, wc.w2),
                (2, great_color, dim_great, wc.w3),
                (3, decent_color, dim_decent, wc.w4),
                (4, wayoff_color, dim_wayoff, wc.w5),
                (5, miss_color, dim_miss, wc.miss),
            ];

            for (index, (label_index, bright, dim, count)) in rows.iter().enumerate() {
                let disabled = split_row_disabled(disabled_windows, index);
                let local_y = y_base + (index as f32 * row_height);
                let world_y = final_judgments_center_y + (local_y * final_text_base_zoom);

                let bright = if disabled {
                    DISABLED_WINDOW_RGBA
                } else {
                    *bright
                };
                let dim = if disabled {
                    DISABLED_WINDOW_RGBA
                } else {
                    *dim
                };
                let (dim_text, bright_text) =
                    padded_runs_for_window(*count, digits, disabled);
                let dim_len = dim_text.len() as f32;
                let bright_len = bright_text.len() as f32;

                if player_side == profile_data::PlayerSide::P1 {
                    if !bright_text.is_empty() {
                        actors.push(act!(text:
                            font(gameplay_font_key(state, FontRole::ScreenEval)): settext(bright_text):
                            align(1.0, 0.5): xy(numbers_cx, world_y): zoom(numbers_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]): z(71)
                        ));
                    }
                    if !dim_text.is_empty() {
                        actors.push(act!(text:
                            font(gameplay_font_key(state, FontRole::ScreenEval)): settext(dim_text):
                            align(1.0, 0.5): xy(numbers_cx - bright_len * max_digit_w, world_y):
                            zoom(numbers_zoom):
                            diffuse(dim[0], dim[1], dim[2], dim[3]): z(71)
                        ));
                    }
                } else {
                    if !dim_text.is_empty() {
                        actors.push(act!(text:
                            font(gameplay_font_key(state, FontRole::ScreenEval)): settext(dim_text):
                            align(0.0, 0.5): xy(numbers_cx, world_y): zoom(numbers_zoom):
                            diffuse(dim[0], dim[1], dim[2], dim[3]): z(71):
                            horizalign(left)
                        ));
                    }
                    if !bright_text.is_empty() {
                        actors.push(act!(text:
                            font(gameplay_font_key(state, FontRole::ScreenEval)): settext(bright_text):
                            align(0.0, 0.5): xy(numbers_cx + dim_len * max_digit_w, world_y):
                            zoom(numbers_zoom):
                            diffuse(bright[0], bright[1], bright[2], bright[3]): z(71):
                            horizalign(left)
                        ));
                    }
                }

                let label_world_y = world_y + (1.0 * final_text_base_zoom);
                let label_zoom = final_text_base_zoom * 0.833;
                let sublabel_y = label_world_y + (12.0 * final_text_base_zoom);
                let sublabel_zoom = final_text_base_zoom * 0.6;
                let label = state.gameplay_stats_text.judgment(*label_index);

                if player_side == profile_data::PlayerSide::P1 {
                    actors.push(act!(text:
                        font("miso"): settext(label): align(0.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(left):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                    if show_blue_ms_label && index == 0 {
                        actors.push(act!(text:
                            font("miso"): settext(blue_window_label.clone()): align(0.0, 0.5):
                            xy(label_world_x, sublabel_y): zoom(sublabel_zoom):
                            maxwidth(72.0 * final_text_base_zoom): horizalign(left):
                            diffuse(bright[0], bright[1], bright[2], bright[3]):
                            z(71)
                        ));
                    }
                } else {
                    actors.push(act!(text:
                        font("miso"): settext(label): align(1.0, 0.5):
                        xy(label_world_x, label_world_y): zoom(label_zoom):
                        maxwidth(72.0 * final_text_base_zoom): horizalign(right):
                        diffuse(bright[0], bright[1], bright[2], bright[3]):
                        z(71)
                    ));
                    if show_blue_ms_label && index == 0 {
                        actors.push(act!(text:
                            font("miso"): settext(blue_window_label.clone()): align(1.0, 0.5):
                            xy(label_world_x, sublabel_y): zoom(sublabel_zoom):
                            maxwidth(72.0 * final_text_base_zoom): horizalign(right):
                            diffuse(bright[0], bright[1], bright[2], bright[3]):
                            z(71)
                        ));
                    }
                }
            }
        }

        // --- Time Display (Remaining / Total) ---
        if show_duration {
            let local_y = -40.0 * layout.banner_data_zoom;

            let time_display = step_stats_time_display(state, player_idx);
            let total_display_seconds = time_display.total_seconds;
            let elapsed_display_seconds = time_display.elapsed_seconds;

            let total_time_key = game_time_key(total_display_seconds, total_display_seconds);
            let total_time_str = game_time_text(total_time_key.0, total_time_key.1);

            let remaining_display_seconds =
                (total_display_seconds - elapsed_display_seconds).max(0.0);
            let remaining_time_key = game_time_key(remaining_display_seconds, total_display_seconds);
            let remaining_time_str =
                game_time_text(remaining_time_key.0, remaining_time_key.1);

            let font_name = "miso";
            let text_zoom = layout.banner_data_zoom * 0.833;
            // Time values currently render without explicit zoom, so treat as 1.0
            let time_value_zoom = 1.0_f32;

            let numbers_block_width = (digits as f32) * max_digit_w;
            let numbers_left_x = numbers_cx - numbers_block_width + 2.0;

            let total_width_px =
                cached_game_time_width_for_key(total_time_key, asset_manager) * time_value_zoom;
            let remaining_width_px =
                cached_game_time_width_for_key(remaining_time_key, asset_manager) * time_value_zoom;
            // Use "9:59" as the baseline look the layout was tuned for.
            let baseline_width_px =
                cached_game_time_width_for_key((9 * 60 + 59, 2), asset_manager) * time_value_zoom;

            let red_color = color::rgba_hex("#ff3030");
            let white_color = [1.0, 1.0, 1.0, 1.0];
            let remaining_color = if state.players()[player_idx].is_failing {
                red_color
            } else {
                white_color
            };

            // --- Total Time Row ---
            let y_pos_total = layout.sidepane_center_y + local_y + 13.0;
            let label_offset: f32 = 29.0;
            // Keep original spacing for <= 9:59, otherwise push label after the time width
            let desired_gap_px = (label_offset - baseline_width_px).max(4.0_f32);
            let label_offset_total = if total_width_px > baseline_width_px {
                total_width_px + desired_gap_px
            } else {
                label_offset
            };

            let (time_x, label_dir) = if player_side == profile_data::PlayerSide::P1 {
                (numbers_left_x, 1.0_f32)
            } else {
                let numbers_right_x = numbers_cx + numbers_block_width - 2.0;
                (numbers_right_x, -1.0_f32)
            };

            if player_side == profile_data::PlayerSide::P1 {
                actors.push(act!(text: font(font_name): settext(total_time_str):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x, y_pos_total):
                    z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(time_total_text(state)):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x + label_dir * label_offset_total, y_pos_total + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
            } else {
                actors.push(act!(text: font(font_name): settext(total_time_str):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x, y_pos_total):
                    z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(time_total_text(state)):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x + label_dir * label_offset_total, y_pos_total + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
                ));
            }

            // --- Remaining Time Row ---
            let y_pos_remaining = layout.sidepane_center_y + local_y - 7.0;

            // Keep original spacing for <= 9:59, otherwise push label after the time width
            let label_offset_remaining = if remaining_width_px > baseline_width_px {
                remaining_width_px + desired_gap_px
            } else {
                label_offset
            };

            if player_side == profile_data::PlayerSide::P1 {
                actors.push(act!(text: font(font_name): settext(remaining_time_str):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x, y_pos_remaining):
                    z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(time_remaining_text(state)):
                    align(0.0, 0.5): horizalign(left):
                    xy(time_x + label_dir * label_offset_remaining, y_pos_remaining + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
            } else {
                actors.push(act!(text: font(font_name): settext(remaining_time_str):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x, y_pos_remaining):
                    z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
                actors.push(act!(text: font(font_name): settext(time_remaining_text(state)):
                    align(1.0, 0.5): horizalign(right):
                    xy(time_x + label_dir * label_offset_remaining, y_pos_remaining + 1.0):
                    zoom(text_zoom): z(71):
                    diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
                ));
            }

            let max_time_label_offset = label_offset_total.max(label_offset_remaining);
            let timing_gap = 104.0 * layout.banner_data_zoom;
            let timing_value_gap = 156.0 * layout.banner_data_zoom;
            let timing_label_anchor = if player_side == profile_data::PlayerSide::P1 {
                time_x + max_time_label_offset + timing_gap
            } else if layout.note_field_is_centered {
                time_x - max_time_label_offset - timing_gap
            } else {
                // SL OffsetCalc.lua P2 anchors live timing to the left of Time.lua.
                // Keep the non-centered P2 pane from drifting back into duration text.
                time_x - max_time_label_offset - 160.0 * layout.banner_data_zoom
            };
            let right_align_timing_values = layout.note_field_is_centered
                && !layout.is_ultrawide
                && player_side == profile_data::PlayerSide::P1;
            let timing_value_anchor = if right_align_timing_values {
                layout.sidepane_center_x + layout.sidepane_width * 0.5
                    - 18.0 * layout.banner_data_zoom
            } else if player_side == profile_data::PlayerSide::P2 && !layout.note_field_is_centered
            {
                time_x - max_time_label_offset - 95.0 * layout.banner_data_zoom
            } else {
                timing_label_anchor + timing_value_gap
            };
            push_live_timing_stats_at(
                actors,
                state,
                player_idx,
                player_side,
                timing_label_anchor,
                timing_value_anchor,
                right_align_timing_values,
                y_pos_remaining,
                20.0,
                text_zoom,
                71,
            );
        }
            });
        });
    }

    // Density graph (Simply Love StepStatistics/DensityGraph.lua).
    let graph = step_stats_density_graph_rect(state, layout);
    if wide && mask.contains(profile_data::StepStatisticsMask::DENSITY_GRAPH) {
        let graph_view = state.gameplay.density_graph_view();
        if graph_view.graph_w > 0.0_f32 && graph_view.graph_h > 0.0_f32 {
            push_density_graph_at(actors, state, player_idx, graph.x, graph.y);
        }
    }

    // Peak NPS sits on the graph corner and uses the same scale as song info text.
    if wide && mask.contains(profile_data::StepStatisticsMask::PEAK_NPS) {
        push_peak_nps_on_graph(
            actors,
            state,
            player_idx,
            player_side,
            graph,
            song_info_text_zoom(layout),
        );
    }
}
