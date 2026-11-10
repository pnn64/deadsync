use crate::act;
use crate::color;
use crate::scorebox as scorebox_theme;
use crate::scorebox::{
    SCOREBOX_BORDER, SCOREBOX_H, SCOREBOX_W, ScoreboxCycleState, color_with_alpha, lerp_color,
    logo_alpha, scorebox_cycle_state,
};
use crate::views::ScoreboxSideView;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadsync_config::theme::SrpgVariant;
use deadsync_score as score_data;
use deadsync_theme::color::{JudgmentColorRole as Role, JudgmentPalette};
use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

pub(crate) const SCOREBOX_NUM_ENTRIES: usize = 5;
const SCOREBOX_GS_BLUE: [f32; 4] = deadlib_present::color::rgba_hex("#007b85");
const SCOREBOX_SRPG_YELLOW: [f32; 4] = [1.0, 0.972, 0.792, 1.0];
const SCOREBOX_ITL_PINK: [f32; 4] = [1.0, 0.2, 0.406, 1.0];
const SCOREBOX_SELF: [f32; 4] = deadlib_present::color::rgba_hex("#A1FF94");
/// Border for a pane that is not a board yet: loading, an error or no scores.
/// Neutral on purpose, so a board's colour arrives with the board instead of
/// a GrooveStats blue frame recolouring itself to golf or ITL.
const SCOREBOX_STATUS_GREY: [f32; 4] = deadlib_present::color::rgba_hex("#5c5c5c");
const SCOREBOX_RIVAL: [f32; 4] = deadlib_present::color::rgba_hex("#C29CFF");
const SCOREBOX_MODE_ALPHA: f32 = 0.35;
const SCOREBOX_GS_LOGO_ALPHA: f32 = 0.5;
const SCOREBOX_EX_TEXT_ALPHA: f32 = 0.3;
const SCOREBOX_HARD_EX_TEXT_ALPHA: f32 = 0.32;
const SCOREBOX_ARROWCLOUD_LOGO_ALPHA: f32 = 0.5;
const SCOREBOX_ARROWCLOUD_LOGO_ZOOM: f32 = 0.06;
const SCOREBOX_SRPG_LOGO_ALPHA: f32 = 0.5;
const SCOREBOX_ITL_LOGO_ALPHA: f32 = 0.2;
const SCOREBOX_HARD_EX_BORDER_TINT: f32 = 0.35;
const TEXT_CACHE_LIMIT: usize = 8192;

type PaneKind = score_data::ScoreboxPaneKind;

thread_local! {
    static SCORE_PERCENT_TEXT_CACHE: RefCell<TextCache<u64>> = RefCell::new(text_cache_with_capacity(2048));
    static SCORE_VALUE_TEXT_CACHE: RefCell<TextCache<u64>> = RefCell::new(text_cache_with_capacity(2048));
    static RANK_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(text_cache_with_capacity(512));
}

#[inline(always)]
fn empty_text() -> Arc<str> {
    static EMPTY: OnceLock<Arc<str>> = OnceLock::new();
    EMPTY.get_or_init(|| Arc::<str>::from("")).clone()
}

#[inline(always)]
pub(crate) fn unknown_score_percent_text() -> Arc<str> {
    static UNKNOWN: OnceLock<Arc<str>> = OnceLock::new();
    UNKNOWN.get_or_init(|| Arc::<str>::from("??.??%")).clone()
}

#[derive(Clone, Debug)]
struct GameplayScoreboxRow {
    rank: Arc<str>,
    name: TextContent,
    score: Arc<str>,
    rank_color: [f32; 4],
    name_color: [f32; 4],
    score_color: [f32; 4],
}

/// Fairway green for the golf pane's border: deep enough to frame the pattern
/// behind it without glowing next to the blue and pink boards.
pub(crate) const SCOREBOX_GOLF_GREEN: [f32; 4] = [0.16, 0.48, 0.22, 1.0];
/// Pattern tint and alpha over the pane's black fill, in the spirit of the
/// event's result cards: visible texture, never competing with the rows.
const GOLF_PANE_PATTERN_RGB: [f32; 3] = [0.85, 1.0, 0.85];
const GOLF_PANE_PATTERN_ALPHA: f32 = 0.22;
/// Drift in tiles per second.
const GOLF_PANE_PATTERN_UV_VEL: [f32; 2] = [0.02, 0.012];
/// Scorebox tiles are this many pane heights tall so the flags stay legible at
/// scorebox size.
const SCOREBOX_GOLF_PATTERN_TILE_HEIGHTS: f32 = 3.0;

/// Where the course pattern is drawn and how large one tile is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GolfPanePattern {
    pub(crate) center: [f32; 2],
    /// The pane's fill size; the texture repeats to cover it.
    pub(crate) size: [f32; 2],
    /// One pattern tile's height in pixels. Its width follows the texture.
    pub(crate) tile_h: f32,
    pub(crate) alpha: f32,
    pub(crate) z: i16,
}

/// Tile the course pattern across a pane, drifting with `elapsed_seconds`.
/// Pushed right after the pane's fill at the same z.
pub(crate) fn push_golf_pane_pattern(
    actors: &mut Vec<Actor>,
    pattern: GolfPanePattern,
    elapsed_seconds: f32,
) {
    let GolfPanePattern {
        center: [center_x, center_y],
        size: [w, h],
        tile_h,
        alpha,
        z,
    } = pattern;
    if alpha <= 0.0 || tile_h <= 0.0 {
        return;
    }
    let [tex_w, tex_h] = crate::resources::GOLF_PANE_PATTERN_TEXTURE_SIZE;
    let tile_w = tile_h * (tex_w as f32 / tex_h as f32);
    let u0 = (GOLF_PANE_PATTERN_UV_VEL[0] * elapsed_seconds).rem_euclid(1.0);
    let v0 = (GOLF_PANE_PATTERN_UV_VEL[1] * elapsed_seconds).rem_euclid(1.0);
    actors.push(act!(sprite_static(crate::resources::GOLF_PANE_PATTERN_TEXTURE):
        align(0.5, 0.5):
        xy(center_x, center_y):
        setsize(w, h):
        customtexturerect(u0, v0, u0 + w / tile_w, v0 + h / tile_h):
        diffuse(GOLF_PANE_PATTERN_RGB[0], GOLF_PANE_PATTERN_RGB[1], GOLF_PANE_PATTERN_RGB[2], alpha):
        z(z)
    ));
}

#[derive(Clone, Debug)]
struct GameplayScoreboxPane {
    kind: PaneKind,
    is_arrowcloud: bool,
    mode_text: TextContent,
    border_color: [f32; 4],
    rows: [GameplayScoreboxRow; SCOREBOX_NUM_ENTRIES],
}

/// Screen-owned leaderboard pane data prepared outside gameplay composition.
///
/// The gameplay thread owns one plan per player side. Snapshot integration
/// rebuilds it only when the shell supplies an updated leaderboard. Live frames
/// borrow the bounded pane vector and append directly into the reusable screen
/// actor buffer, with no filtering, text creation, temporary actor vector, or
/// eviction. Storage is released with the gameplay screen; steady-frame work is
/// bounded by the two currently blended panes and five rows per pane.
#[derive(Default)]
pub(crate) struct GameplayScoreboxPlan {
    panes: Vec<GameplayScoreboxPane>,
}

impl GameplayScoreboxPlan {
    pub(crate) fn new_with_palette(
        snapshot: Option<&score_data::CachedPlayerLeaderboardData>,
        profile: &score_data::GameplayScoreboxProfileSnapshot,
        filter: score_data::SelectMusicScoreboxFilter,
        palette: JudgmentPalette,
    ) -> Self {
        if !profile.display_scorebox || !profile.online_active() {
            return Self::default();
        }
        Self {
            panes: snapshot.map_or_else(Vec::new, |snapshot| {
                gameplay_panes_from_snapshot(snapshot, filter, palette)
            }),
        }
    }

    pub(crate) fn push_actors(
        &self,
        actors: &mut Vec<Actor>,
        center_x: f32,
        center_y: f32,
        zoom: f32,
        elapsed_seconds: f32,
    ) {
        push_gameplay_scorebox_actors_from_panes(
            actors,
            &self.panes,
            center_x,
            center_y,
            zoom,
            elapsed_seconds,
            1.0,
        );
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SelectMusicScoreboxView {
    pub mode_label: Arc<str>,
    pub machine_name: Arc<str>,
    pub machine_score: Arc<str>,
    pub player_name: Arc<str>,
    pub player_score: Arc<str>,
    pub rivals: [(Arc<str>, Arc<str>); 3],
    pub show_rivals: bool,
}

/// Screen-owned actor-ready Select Music scorebox text.
///
/// The main thread owns three fixed views per side and rebuilds them only when
/// the shell emits a changed chart/profile/score snapshot. Actor frames borrow
/// one view and clone its Arcs. A chart mismatch selects the stale placeholder
/// view, so a newly moved wheel row never displays the previous chart's
/// records. There is no miss insertion, growth, eviction, or pruning.
/// Replacement and final cache release happen on the main thread; frame-local
/// actor Arc decrements remain bounded. Focused scorebox tests cover matched and
/// stale selection, and performance pass 56 records allocation counts.
/// Restarts the Select Music scorebox cycle on its first pane whenever a side's
/// boards change, so a golf hole opens on golf and every other chart on the
/// player's primary board, instead of landing wherever the screen clock points.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ScoreboxCycleClock {
    chart_hash: Option<String>,
    loaded: bool,
    origin_seconds: f32,
}

impl ScoreboxCycleClock {
    /// Note the chart and whether its boards have arrived; a change restarts the cycle at `now_seconds`.
    pub(crate) fn sync(&mut self, runtime: &ScoreboxSideView, now_seconds: f32) {
        let loaded = runtime
            .leaderboards
            .as_ref()
            .is_some_and(|leaderboards| !leaderboards.loading);
        if self.chart_hash.as_deref() != runtime.chart_hash.as_deref() || self.loaded != loaded {
            self.chart_hash.clone_from(&runtime.chart_hash);
            self.loaded = loaded;
            self.origin_seconds = now_seconds;
        }
    }

    #[must_use]
    pub(crate) fn elapsed_seconds(&self, now_seconds: f32) -> f32 {
        (now_seconds - self.origin_seconds).max(0.0)
    }
}

/// Worst-case frame work is one chart comparison, one view branch, and eleven
/// bounded Arc clones per visible side.
#[derive(Clone, Debug)]
pub(crate) struct SelectMusicScoreboxPresentation {
    local: SelectMusicScoreboxView,
    online: SelectMusicScoreboxView,
    /// The online view with its loading label suppressed, shown while a fresh
    /// fetch is still inside the loading grace window.
    pending: SelectMusicScoreboxView,
    stale: SelectMusicScoreboxView,
}

impl SelectMusicScoreboxPresentation {
    pub(crate) fn new(runtime: &ScoreboxSideView) -> Self {
        let chart_present = runtime.chart_hash.is_some();
        let online = build_select_music_scorebox_view(runtime, true, chart_present, true);
        let mut pending = online.clone();
        if runtime
            .leaderboards
            .as_ref()
            .is_some_and(|snapshot| snapshot.loading)
        {
            pending.mode_label = score_mode_label(score_data::default_scorebox_mode_text(
                runtime.show_ex_score,
            ));
        }
        Self {
            local: build_select_music_scorebox_view(runtime, true, chart_present, false),
            online,
            pending,
            stale: build_select_music_scorebox_view(runtime, false, false, false),
        }
    }

    #[inline(always)]
    pub(crate) const fn view(
        &self,
        chart_matches: bool,
        show_online: bool,
        within_loading_grace: bool,
    ) -> &SelectMusicScoreboxView {
        if !chart_matches {
            &self.stale
        } else if show_online && within_loading_grace {
            &self.pending
        } else if show_online {
            &self.online
        } else {
            &self.local
        }
    }
}

impl Default for SelectMusicScoreboxPresentation {
    fn default() -> Self {
        Self::new(&crate::views::ScoreboxSideView::default())
    }
}

#[inline(always)]
fn error_text(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        "Timed Out"
    } else {
        "Failed to Load 😞"
    }
}

#[inline(always)]
fn pane_color(kind: PaneKind) -> [f32; 4] {
    match kind {
        PaneKind::Gs | PaneKind::Ex | PaneKind::Other => SCOREBOX_GS_BLUE,
        PaneKind::HardEx => [
            (color::HARD_EX_SCORE_RGBA[0] - SCOREBOX_GS_BLUE[0])
                .mul_add(SCOREBOX_HARD_EX_BORDER_TINT, SCOREBOX_GS_BLUE[0]),
            (color::HARD_EX_SCORE_RGBA[1] - SCOREBOX_GS_BLUE[1])
                .mul_add(SCOREBOX_HARD_EX_BORDER_TINT, SCOREBOX_GS_BLUE[1]),
            (color::HARD_EX_SCORE_RGBA[2] - SCOREBOX_GS_BLUE[2])
                .mul_add(SCOREBOX_HARD_EX_BORDER_TINT, SCOREBOX_GS_BLUE[2]),
            1.0,
        ],
        PaneKind::Golf => SCOREBOX_GOLF_GREEN,
        PaneKind::Srpg => SCOREBOX_SRPG_YELLOW,
        PaneKind::Itl => SCOREBOX_ITL_PINK,
    }
}

#[inline(always)]
fn score_text_with_percent(score_10000: f64) -> Arc<str> {
    let percent = score_data::scorebox_score_percent(score_10000);
    cached_text(
        &SCORE_PERCENT_TEXT_CACHE,
        percent.to_bits(),
        TEXT_CACHE_LIMIT,
        || score_data::format_scorebox_score_percent(score_10000),
    )
}

#[inline(always)]
fn score_text_without_percent(score_10000: f64) -> Arc<str> {
    let score = score_data::scorebox_score_percent(score_10000);
    cached_text(
        &SCORE_VALUE_TEXT_CACHE,
        score.to_bits(),
        TEXT_CACHE_LIMIT,
        || score_data::format_scorebox_score_value(score_10000),
    )
}

#[inline(always)]
fn rank_text(rank: u32) -> Arc<str> {
    cached_text(&RANK_TEXT_CACHE, rank, TEXT_CACHE_LIMIT, || {
        score_data::format_scorebox_rank(rank)
    })
}

#[inline(always)]
fn owned_text(text: &str) -> Arc<str> {
    Arc::<str>::from(text)
}

/// Pane text. Retained gameplay plans keep one shared pointer for the text
/// layout cache; Select Music rebuilds its panes every frame, so its text is
/// looked up by content instead of by a new pointer each frame.
#[inline(always)]
fn pane_text(text: &str, retained: bool) -> TextContent {
    if retained {
        TextContent::Shared(owned_text(text))
    } else {
        TextContent::inline_str(text).unwrap_or_else(|| TextContent::Owned(text.to_owned()))
    }
}

#[inline(always)]
fn placeholder_text() -> Arc<str> {
    static PLACEHOLDER: OnceLock<Arc<str>> = OnceLock::new();
    PLACEHOLDER.get_or_init(|| Arc::<str>::from("----")).clone()
}

fn score_mode_label(mode: &str) -> Arc<str> {
    Arc::<str>::from(format!("{mode} Score"))
}

fn local_self_machine_tag(view: &ScoreboxSideView) -> Option<String> {
    let initials = view.player_initials.trim();
    if initials.is_empty() {
        None
    } else {
        Some(initials.to_string())
    }
}

fn local_self_scorebox_name(view: &ScoreboxSideView) -> String {
    let fallback = [
        view.display_name.as_ref(),
        view.groovestats_username.as_ref(),
        view.player_initials.as_ref(),
    ]
    .into_iter()
    .map(str::trim)
    .find(|value| !value.is_empty())
    .unwrap_or("----");
    let tag = local_self_machine_tag(view);
    score_data::scorebox_machine_tag(tag.as_deref(), fallback)
}

fn leaderboard_entry_matches_local_self(
    view: &ScoreboxSideView,
    entry: &score_data::LeaderboardEntry,
) -> bool {
    let name = entry.name.trim();
    if name.is_empty() {
        return false;
    }
    [
        view.groovestats_username.as_ref(),
        view.display_name.as_ref(),
        view.player_initials.as_ref(),
    ]
    .into_iter()
    .map(str::trim)
    .any(|candidate| !candidate.is_empty() && candidate.eq_ignore_ascii_case(name))
}

fn local_self_score_10000(view: &ScoreboxSideView, kind: PaneKind) -> Option<(f64, bool)> {
    let score = match kind {
        PaneKind::Gs => view.local_itg,
        PaneKind::Ex => view.local_ex,
        PaneKind::HardEx => view.local_hard_ex,
        PaneKind::Itl => view.local_itl,
        PaneKind::Golf | PaneKind::Srpg | PaneKind::Other => None,
    }?;
    Some((score.score_10000, score.failed))
}

pub(crate) fn entries_with_local_self_state<'a>(
    view: &ScoreboxSideView,
    pane: &'a score_data::LeaderboardPane,
) -> Cow<'a, [score_data::LeaderboardEntry]> {
    let kind = score_data::scorebox_pane_kind(pane);
    let local_self = local_self_score_10000(view, kind);

    if let Some(index) = pane.entries.iter().position(|entry| entry.is_self) {
        let entry = &pane.entries[index];
        if let Some((local_score_10000, local_is_fail)) = local_self
            && local_is_fail
            && score_data::same_score_10000(entry.score, local_score_10000)
        {
            if entry.is_fail
                && (entry.machine_tag.is_some() || view.player_initials.trim().is_empty())
            {
                return Cow::Borrowed(pane.entries.as_slice());
            }
            let mut entries = pane.entries.clone();
            let entry = &mut entries[index];
            entry.is_fail = true;
            if entry.machine_tag.is_none() {
                entry.machine_tag = local_self_machine_tag(view);
            }
            return Cow::Owned(entries);
        }
        return Cow::Borrowed(pane.entries.as_slice());
    }

    if let Some(index) = pane
        .entries
        .iter()
        .position(|entry| leaderboard_entry_matches_local_self(view, entry))
    {
        let mut entries = pane.entries.clone();
        let entry = &mut entries[index];
        entry.is_self = true;
        if entry.machine_tag.is_none() {
            entry.machine_tag = local_self_machine_tag(view);
        }
        if let Some((local_score_10000, local_is_fail)) = local_self
            && local_is_fail
            && score_data::same_score_10000(entry.score, local_score_10000)
        {
            entry.is_fail = true;
        }
        return Cow::Owned(entries);
    }

    Cow::Borrowed(pane.entries.as_slice())
}

fn build_select_music_scorebox_view(
    runtime: &ScoreboxSideView,
    chart_matches: bool,
    chart_present: bool,
    show_rivals: bool,
) -> SelectMusicScoreboxView {
    let fallback_player = chart_matches
        .then_some(runtime.local_itg)
        .flatten()
        .filter(|score| !score.failed || score.score_10000 > 0.0)
        .map(|score| {
            (
                Arc::clone(&runtime.player_initials),
                score_text_with_percent(score.score_10000),
            )
        })
        .unwrap_or_else(|| (placeholder_text(), unknown_score_percent_text()));
    let fallback_machine = chart_matches
        .then_some(runtime.machine_itg.as_ref())
        .flatten()
        .filter(|score| !score.failed || score.score_10000 > 0.0)
        .map(|score| {
            (
                Arc::<str>::from(score.name.as_str()),
                score_text_with_percent(score.score_10000),
            )
        })
        .unwrap_or_else(|| (placeholder_text(), unknown_score_percent_text()));
    let mut view = SelectMusicScoreboxView {
        mode_label: score_mode_label(score_data::default_scorebox_mode_text(
            runtime.show_ex_score,
        )),
        machine_name: fallback_machine.0,
        machine_score: fallback_machine.1,
        player_name: fallback_player.0,
        player_score: fallback_player.1,
        rivals: std::array::from_fn(|_| (placeholder_text(), unknown_score_percent_text())),
        show_rivals: false,
    };

    if !show_rivals || !runtime.online_active || !chart_matches {
        return view;
    }
    let filter = runtime.pane_filter;
    if !score_data::select_music_scorebox_filter_has_any(filter) {
        return view;
    }
    view.machine_name = placeholder_text();
    view.machine_score = unknown_score_percent_text();
    view.player_name = placeholder_text();
    view.player_score = unknown_score_percent_text();
    view.show_rivals = true;

    if !chart_present {
        return view;
    }
    let Some(snapshot) = runtime.leaderboards.as_ref() else {
        return view;
    };

    if snapshot.loading {
        view.mode_label = owned_text("Loading ...");
        return view;
    }
    if let Some(error) = snapshot.error.as_deref() {
        view.mode_label = owned_text(error_text(error));
        return view;
    }

    let show_ex = runtime.show_ex_score;
    let Some(data) = snapshot.data.as_ref() else {
        return view;
    };
    let filtered_panes = score_data::select_music_scorebox_pane_refs(data.panes.as_slice(), filter);
    let Some(pane) =
        score_data::preferred_primary_scorebox_pane(
            filtered_panes.as_slice(),
            show_ex,
            filter.events_first,
        )
    else {
        view.mode_label = owned_text("No Scores");
        return view;
    };

    let kind = score_data::scorebox_pane_kind(pane);
    let entries = entries_with_local_self_state(runtime, pane);
    view.mode_label = score_mode_label(score_data::scorebox_pane_mode_text(kind, pane));
    if entries.is_empty() {
        view.mode_label = owned_text("No Scores");
        return view;
    }

    if let Some(world) = entries
        .iter()
        .find(|entry| entry.rank == 1)
        .or_else(|| entries.first())
    {
        view.machine_name = Arc::from(score_data::scorebox_machine_tag(
            world.machine_tag.as_deref(),
            &world.name,
        ));
        view.machine_score = if kind == PaneKind::Golf {
            Arc::from(format!("{:.2}", world.score / 1000.0))
        } else {
            score_text_with_percent(world.score)
        };
    }
    if let Some(player_entry) = entries.iter().find(|entry| entry.is_self) {
        view.player_name = Arc::from(score_data::scorebox_machine_tag(
            player_entry.machine_tag.as_deref(),
            &player_entry.name,
        ));
        view.player_score = if kind == PaneKind::Golf {
            Arc::from(format!("{:.2}", player_entry.score / 1000.0))
        } else {
            score_text_with_percent(player_entry.score)
        };
    } else if let Some((local_score_10000, _)) = local_self_score_10000(runtime, kind) {
        view.player_name = Arc::from(local_self_scorebox_name(runtime));
        view.player_score = score_text_with_percent(local_score_10000);
    }
    for (idx, rival) in entries
        .iter()
        .filter(|entry| entry.is_rival)
        .take(3)
        .enumerate()
    {
        view.rivals[idx] = (
            Arc::from(score_data::scorebox_machine_tag(
                rival.machine_tag.as_deref(),
                &rival.name,
            )),
            if kind == PaneKind::Golf {
                Arc::from(format!("{:.2}", rival.score / 1000.0))
            } else {
                score_text_with_percent(rival.score)
            },
        );
    }
    view
}

#[inline(always)]
fn gameplay_empty_row() -> GameplayScoreboxRow {
    GameplayScoreboxRow {
        rank: empty_text(),
        name: TextContent::Shared(empty_text()),
        score: empty_text(),
        rank_color: [1.0; 4],
        name_color: [1.0; 4],
        score_color: [1.0; 4],
    }
}

#[inline(always)]
fn gameplay_status_row(text: &str, retained: bool) -> GameplayScoreboxRow {
    GameplayScoreboxRow {
        rank: empty_text(),
        name: pane_text(text, retained),
        score: empty_text(),
        rank_color: [1.0; 4],
        name_color: [1.0; 4],
        score_color: [1.0; 4],
    }
}

fn empty_rows() -> [GameplayScoreboxRow; SCOREBOX_NUM_ENTRIES] {
    std::array::from_fn(|_| gameplay_empty_row())
}

/// A status pane stands in for a board that is loading, failed or empty. It
/// belongs to no service, so it carries no logo, mode text or service colour:
/// `Other` with empty mode text draws nothing in the header.
fn gameplay_status_pane(text: &str, retained: bool) -> GameplayScoreboxPane {
    let mut rows = empty_rows();
    rows[0] = gameplay_status_row(text, retained);
    GameplayScoreboxPane {
        kind: PaneKind::Other,
        is_arrowcloud: false,
        mode_text: pane_text("", retained),
        border_color: SCOREBOX_STATUS_GREY,
        rows,
    }
}

fn gameplay_row_from_entry(
    entry: &score_data::LeaderboardEntry,
    kind: PaneKind,
    palette: JudgmentPalette,
    retained: bool,
) -> GameplayScoreboxRow {
    let mut rank_color = [1.0; 4];
    let mut name_color = [1.0; 4];
    if entry.is_self {
        rank_color = SCOREBOX_SELF;
        name_color = SCOREBOX_SELF;
    } else if entry.is_rival {
        rank_color = SCOREBOX_RIVAL;
        name_color = SCOREBOX_RIVAL;
    }

    let score_color = if entry.is_fail {
        [1.0, 0.0, 0.0, 1.0]
    } else if matches!(kind, PaneKind::Ex | PaneKind::Itl) {
        palette.color(Role::FantasticBlue)
    } else if matches!(kind, PaneKind::HardEx) {
        color::HARD_EX_SCORE_RGBA
    } else if entry.is_self {
        SCOREBOX_SELF
    } else if entry.is_rival {
        SCOREBOX_RIVAL
    } else {
        [1.0; 4]
    };

    let name = {
        let trimmed = entry.name.trim();
        if trimmed.is_empty() { "----" } else { trimmed }
    };

    GameplayScoreboxRow {
        rank: rank_text(entry.rank),
        name: pane_text(name, retained),
        score: if kind == PaneKind::Golf {
            Arc::from(format!("{:.2}", entry.score / 1000.0))
        } else {
            score_text_without_percent(entry.score)
        },
        rank_color,
        name_color,
        score_color,
    }
}

fn scorebox_rows_for_kind(
    entries: &[score_data::LeaderboardEntry],
    kind: PaneKind,
    palette: JudgmentPalette,
    retained: bool,
) -> [GameplayScoreboxRow; SCOREBOX_NUM_ENTRIES] {
    let mut rows = empty_rows();
    if entries.is_empty() {
        rows[0] = gameplay_status_row("No Scores", retained);
        return rows;
    }

    let selected = score_data::neighboring_leaderboard_entry_refs(entries, SCOREBOX_NUM_ENTRIES);
    for (slot, entry) in rows.iter_mut().zip(selected) {
        *slot = gameplay_row_from_entry(entry, kind, palette, retained);
    }
    rows
}

fn gameplay_pane_from_leaderboard(
    pane: &score_data::LeaderboardPane,
    entries: &[score_data::LeaderboardEntry],
    palette: JudgmentPalette,
    retained: bool,
) -> GameplayScoreboxPane {
    let kind = score_data::scorebox_pane_kind(pane);
    GameplayScoreboxPane {
        kind,
        is_arrowcloud: pane.is_arrowcloud(),
        mode_text: pane_text(score_data::scorebox_pane_mode_text(kind, pane), retained),
        border_color: pane_color(kind),
        rows: scorebox_rows_for_kind(entries, kind, palette, retained),
    }
}

fn gameplay_panes_from_snapshot(
    snapshot: &score_data::CachedPlayerLeaderboardData,
    filter: score_data::SelectMusicScoreboxFilter,
    palette: JudgmentPalette,
) -> Vec<GameplayScoreboxPane> {
    if snapshot.loading {
        return vec![gameplay_status_pane("Loading ...", true)];
    }
    if let Some(error) = snapshot.error.as_deref() {
        let text = error_text(error);
        return vec![gameplay_status_pane(text, true)];
    }
    let Some(data) = snapshot.data.as_ref() else {
        return vec![gameplay_status_pane("No Scores", true)];
    };
    if data.panes.is_empty() {
        return vec![gameplay_status_pane("No Scores", true)];
    }

    if !score_data::select_music_scorebox_filter_has_any(filter) {
        return Vec::new();
    }

    let filtered = score_data::select_music_scorebox_pane_refs(data.panes.as_slice(), filter);
    if filtered.is_empty() {
        return vec![gameplay_status_pane("No Scores", true)];
    }

    let mut panes = Vec::with_capacity(filtered.len());
    for pane in filtered {
        panes.push(gameplay_pane_from_leaderboard(
            pane,
            pane.entries.as_slice(),
            palette,
            true,
        ));
    }
    panes
}

/// Panes for the wheel-side scorebox. Inside the loading grace window a fresh
/// fetch shows the status frame with blank rows instead of a loading label.
fn select_music_panes_from_snapshot(
    snapshot: &score_data::CachedPlayerLeaderboardData,
    runtime: &ScoreboxSideView,
    within_loading_grace: bool,
) -> Vec<GameplayScoreboxPane> {
    if snapshot.loading {
        let text = if within_loading_grace { "" } else { "Loading ..." };
        return vec![gameplay_status_pane(text, false)];
    }
    if let Some(error) = snapshot.error.as_deref() {
        let text = error_text(error);
        return vec![gameplay_status_pane(text, false)];
    }
    let Some(data) = snapshot.data.as_ref() else {
        return vec![gameplay_status_pane("No Scores", false)];
    };
    let filter = runtime.pane_filter;
    if !score_data::select_music_scorebox_filter_has_any(filter) {
        return Vec::new();
    }

    let filtered = score_data::select_music_scorebox_pane_refs(data.panes.as_slice(), filter);
    if filtered.is_empty() {
        return vec![gameplay_status_pane("No Scores", false)];
    }
    let mut panes = Vec::with_capacity(filtered.len());
    for pane in filtered {
        let entries = entries_with_local_self_state(runtime, pane);
        panes.push(gameplay_pane_from_leaderboard(
            pane,
            entries.as_ref(),
            crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE,
            false,
        ));
    }
    panes
}

#[inline(always)]
const fn is_gs_logo(pane: &GameplayScoreboxPane) -> bool {
    !pane.is_arrowcloud && matches!(pane.kind, PaneKind::Gs | PaneKind::Ex)
}

#[inline(always)]
const fn is_ex_text(pane: &GameplayScoreboxPane) -> bool {
    matches!(pane.kind, PaneKind::Ex)
}

const fn is_arrowcloud_logo(pane: &GameplayScoreboxPane) -> bool {
    pane.is_arrowcloud
}

#[inline(always)]
const fn is_hard_ex_text(pane: &GameplayScoreboxPane) -> bool {
    matches!(pane.kind, PaneKind::HardEx)
}

#[inline(always)]
const fn is_srpg_logo(kind: PaneKind) -> bool {
    matches!(kind, PaneKind::Srpg)
}

#[inline(always)]
const fn is_itl_logo(kind: PaneKind) -> bool {
    matches!(kind, PaneKind::Itl)
}

#[inline(always)]
const fn is_fallback_text(pane: &GameplayScoreboxPane) -> bool {
    matches!(pane.kind, PaneKind::Other)
        || (pane.is_arrowcloud && matches!(pane.kind, PaneKind::Gs))
}

fn push_mode_text(
    actors: &mut Vec<Actor>,
    text: &TextContent,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
    alpha: f32,
) {
    if text.as_str().is_empty() || alpha <= 0.0 {
        return;
    }
    let c = color_with_alpha([1.0, 1.0, 1.0, SCOREBOX_MODE_ALPHA], alpha);
    actors.push(act!(text:
        font("miso"):
        settext(text.clone()):
        align(0.5, 0.5):
        xy(2.0f32.mul_add(zoom, center_x), 5.0f32.mul_add(-zoom, center_y)):
        zoom(0.9 * zoom):
        diffuse(c[0], c[1], c[2], c[3]):
        z(z_base + 2):
        horizalign(center)
    ));
}

fn push_centered_logo(
    actors: &mut Vec<Actor>,
    texture: &'static str,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    sprite_zoom: f32,
    z_base: i16,
    alpha: f32,
) {
    if alpha <= 0.0 {
        return;
    }
    let dims =
        deadlib_assets::texture_dims(texture).unwrap_or(deadlib_assets::TexMeta { w: 1, h: 1 });
    let fit = scorebox_theme::fit_scorebox_logo(dims.w, dims.h, sprite_zoom, zoom);
    let c = color_with_alpha([1.0; 4], alpha);
    actors.push(act!(sprite_static(texture):
        align(0.5, 0.5):
        xy(center_x, center_y):
        setsize(fit.width, fit.height):
        diffuse(c[0], c[1], c[2], c[3]):
        z(z_base + 2)
    ));
}

fn push_mode_overlay(
    actors: &mut Vec<Actor>,
    text: &'static str,
    rgba: [f32; 4],
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
    alpha: f32,
) {
    if alpha <= 0.0 || text.is_empty() {
        return;
    }
    let c = color_with_alpha(rgba, alpha);
    actors.push(act!(text:
        font("miso"):
        settext(text):
        align(0.5, 0.5):
        xy(2.0f32.mul_add(zoom, center_x), 5.0f32.mul_add(-zoom, center_y)):
        zoom(0.9 * zoom):
        diffuse(c[0], c[1], c[2], c[3]):
        z(z_base + 2):
        horizalign(center)
    ));
}

fn push_fallback_mode_text(
    actors: &mut Vec<Actor>,
    cycle: ScoreboxCycleState,
    cur: &GameplayScoreboxPane,
    next: &GameplayScoreboxPane,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
) {
    if is_fallback_text(cur) {
        push_mode_text(
            actors,
            &cur.mode_text,
            center_x,
            center_y,
            zoom,
            z_base,
            cycle.cur_alpha,
        );
    }
    if cycle.next_idx != cycle.cur_idx && is_fallback_text(next) {
        push_mode_text(
            actors,
            &next.mode_text,
            center_x,
            center_y,
            zoom,
            z_base,
            cycle.next_alpha,
        );
    }
}

fn push_gs_logo_overlay(
    actors: &mut Vec<Actor>,
    cycle: ScoreboxCycleState,
    cur: &GameplayScoreboxPane,
    next: &GameplayScoreboxPane,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
) {
    let alpha = logo_alpha(
        cycle,
        is_gs_logo(cur),
        is_gs_logo(next),
        SCOREBOX_GS_LOGO_ALPHA,
        true,
    );
    push_centered_logo(
        actors,
        "GrooveStats.png",
        center_x,
        center_y,
        zoom,
        0.8,
        z_base,
        alpha,
    );
}

fn push_arrowcloud_logo_overlay(
    actors: &mut Vec<Actor>,
    cycle: ScoreboxCycleState,
    cur: &GameplayScoreboxPane,
    next: &GameplayScoreboxPane,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
) {
    let alpha = logo_alpha(
        cycle,
        is_arrowcloud_logo(cur),
        is_arrowcloud_logo(next),
        SCOREBOX_ARROWCLOUD_LOGO_ALPHA,
        true,
    );
    push_centered_logo(
        actors,
        "arrowcloud.png",
        center_x,
        center_y,
        zoom,
        SCOREBOX_ARROWCLOUD_LOGO_ZOOM,
        z_base,
        alpha,
    );
}

fn push_ex_header_overlay(
    actors: &mut Vec<Actor>,
    cycle: ScoreboxCycleState,
    cur: &GameplayScoreboxPane,
    next: &GameplayScoreboxPane,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
) {
    let alpha = logo_alpha(
        cycle,
        is_ex_text(cur),
        is_ex_text(next),
        SCOREBOX_EX_TEXT_ALPHA,
        true,
    );
    push_mode_overlay(
        actors, "EX", [1.0; 4], center_x, center_y, zoom, z_base, alpha,
    );
}

fn push_hard_ex_header_overlay(
    actors: &mut Vec<Actor>,
    cycle: ScoreboxCycleState,
    cur: &GameplayScoreboxPane,
    next: &GameplayScoreboxPane,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
) {
    let alpha = logo_alpha(
        cycle,
        is_hard_ex_text(cur),
        is_hard_ex_text(next),
        SCOREBOX_HARD_EX_TEXT_ALPHA,
        true,
    );
    push_mode_overlay(
        actors,
        "H.EX",
        color::HARD_EX_SCORE_RGBA,
        center_x,
        center_y,
        zoom,
        z_base,
        alpha,
    );
}

fn push_srpg_logo_overlay(
    actors: &mut Vec<Actor>,
    cycle: ScoreboxCycleState,
    cur: PaneKind,
    next: PaneKind,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
) {
    let alpha = logo_alpha(
        cycle,
        is_srpg_logo(cur),
        is_srpg_logo(next),
        SCOREBOX_SRPG_LOGO_ALPHA,
        false,
    );
    push_centered_logo(
        actors,
        srpg_logo_texture_key(SrpgVariant::CURRENT),
        center_x,
        center_y,
        zoom,
        0.07,
        z_base,
        alpha,
    );
}

pub(crate) const fn srpg_logo_texture_key(variant: SrpgVariant) -> &'static str {
    match variant {
        SrpgVariant::Srpg9 => "srpg9_logo_alt.png",
        SrpgVariant::Srpg10 => "srpg10_logo_alt.png",
        SrpgVariant::Ecs15 => crate::visual_styles::ECS15_TITLE_LOGO,
    }
}

fn push_itl_logo_overlay(
    actors: &mut Vec<Actor>,
    cycle: ScoreboxCycleState,
    cur: PaneKind,
    next: PaneKind,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
) {
    let alpha = logo_alpha(
        cycle,
        is_itl_logo(cur),
        is_itl_logo(next),
        SCOREBOX_ITL_LOGO_ALPHA,
        false,
    );
    push_centered_logo(
        actors, "ITL.png", center_x, center_y, zoom, 0.45, z_base, alpha,
    );
}

fn push_header_overlays(
    actors: &mut Vec<Actor>,
    cycle: ScoreboxCycleState,
    cur: &GameplayScoreboxPane,
    next: &GameplayScoreboxPane,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
) {
    push_gs_logo_overlay(actors, cycle, cur, next, center_x, center_y, zoom, z_base);
    push_arrowcloud_logo_overlay(actors, cycle, cur, next, center_x, center_y, zoom, z_base);
    push_ex_header_overlay(actors, cycle, cur, next, center_x, center_y, zoom, z_base);
    push_hard_ex_header_overlay(actors, cycle, cur, next, center_x, center_y, zoom, z_base);
    push_srpg_logo_overlay(
        actors, cycle, cur.kind, next.kind, center_x, center_y, zoom, z_base,
    );
    push_itl_logo_overlay(
        actors, cycle, cur.kind, next.kind, center_x, center_y, zoom, z_base,
    );
    push_fallback_mode_text(actors, cycle, cur, next, center_x, center_y, zoom, z_base);
}

fn push_rank_marker(
    actors: &mut Vec<Actor>,
    row: &GameplayScoreboxRow,
    index: usize,
    center_x: f32,
    y: f32,
    zoom: f32,
    z_base: i16,
    rank_x: f32,
    rank_color: [f32; 4],
) {
    if index == 0 {
        if row.rank.is_empty() {
            return;
        }
        let crown_col = color_with_alpha([1.0; 4], rank_color[3]);
        actors.push(act!(sprite("crown.png"):
            align(0.5, 0.5):
            xy((-SCOREBOX_W).mul_add(0.5, 14.0).mul_add(zoom, center_x), y):
            zoom(0.09 * zoom):
            diffuse(crown_col[0], crown_col[1], crown_col[2], crown_col[3]):
            z(z_base + 3)
        ));
        return;
    }
    actors.push(act!(text:
        font("miso"):
        settext(row.rank.clone()):
        align(1.0, 0.5):
        xy(rank_x, y):
        maxwidth(30.0):
        zoom(0.87 * zoom):
        diffuse(rank_color[0], rank_color[1], rank_color[2], rank_color[3]):
        z(z_base + 3):
        horizalign(right)
    ));
}

fn push_rows(
    actors: &mut Vec<Actor>,
    rows: &[GameplayScoreboxRow],
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
    alpha: f32,
) {
    if alpha <= 0.0 {
        return;
    }

    let rank_x = (-SCOREBOX_W).mul_add(0.5, 27.0).mul_add(zoom, center_x);
    let name_x = (-SCOREBOX_W).mul_add(0.5, 30.0).mul_add(zoom, center_x);
    let score_x = (-SCOREBOX_W).mul_add(0.5, 160.0).mul_add(zoom, center_x);

    for (i, row) in rows.iter().enumerate().take(SCOREBOX_NUM_ENTRIES) {
        let y = (16.0f32.mul_add(i as f32 + 1.0, -SCOREBOX_H * 0.5) - 8.0).mul_add(zoom, center_y);
        let rank_col = color_with_alpha(row.rank_color, alpha);
        let name_col = color_with_alpha(row.name_color, alpha);
        let score_col = color_with_alpha(row.score_color, alpha);
        push_rank_marker(actors, row, i, center_x, y, zoom, z_base, rank_x, rank_col);
        actors.push(act!(text:
            font("miso"):
            settext(row.name.clone()):
            align(0.0, 0.5):
            xy(name_x, y):
            maxwidth(100.0):
            zoom(0.87 * zoom):
            diffuse(name_col[0], name_col[1], name_col[2], name_col[3]):
            z(z_base + 3):
            horizalign(left)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(row.score.clone()):
            align(1.0, 0.5):
            xy(score_x, y):
            zoom(0.87 * zoom):
            diffuse(score_col[0], score_col[1], score_col[2], score_col[3]):
            z(z_base + 3):
            horizalign(right)
        ));
    }
}

pub fn push_select_music_scorebox_actors(
    actors: &mut Vec<Actor>,
    runtime: &ScoreboxSideView,
    chart_hash: Option<&str>,
    show_scorebox: bool,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    elapsed_seconds: f32,
    fetch: SelectMusicScoreboxFetch,
) {
    if !show_scorebox || !runtime.online_active || runtime.chart_hash.as_deref() != chart_hash {
        return;
    }
    if chart_hash.is_none() {
        return;
    }
    let Some(snapshot) = runtime.leaderboards.as_ref() else {
        return;
    };
    let panes = select_music_panes_from_snapshot(snapshot, runtime, fetch.within_loading_grace);
    push_gameplay_scorebox_actors_from_panes(
        actors,
        &panes,
        center_x,
        center_y,
        zoom,
        elapsed_seconds,
        fetch.rows_alpha,
    );
}

/// Where a Select Music side's online fetch is, evaluated on the screen clock
/// rather than the scorebox cycle clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectMusicScoreboxFetch {
    pub within_loading_grace: bool,
    pub rows_alpha: f32,
}

impl SelectMusicScoreboxFetch {
    #[must_use]
    pub fn at(phase: scorebox_theme::ScoreboxFetchPhase, now_seconds: f32) -> Self {
        Self {
            within_loading_grace: phase.within_loading_grace(now_seconds),
            rows_alpha: phase.reveal_alpha(now_seconds),
        }
    }
}

/// `rows_alpha` scales only the rows: the frame, header and course pattern
/// stay solid, so a fetch that just resolved fades its entries in under an
/// unchanged frame.
fn push_gameplay_scorebox_actors_from_panes(
    actors: &mut Vec<Actor>,
    panes: &[GameplayScoreboxPane],
    center_x: f32,
    center_y: f32,
    zoom: f32,
    elapsed_seconds: f32,
    rows_alpha: f32,
) {
    if panes.is_empty() {
        return;
    }

    let cycle = scorebox_cycle_state(panes.len(), elapsed_seconds);
    let cur = &panes[cycle.cur_idx];
    let next = &panes[cycle.next_idx];
    let border_color = if cycle.cur_idx == cycle.next_idx {
        cur.border_color
    } else {
        lerp_color(cur.border_color, next.border_color, cycle.border_mix)
    };

    let z_base = 71_i16;
    let w = SCOREBOX_W * zoom;
    let h = SCOREBOX_H * zoom;
    let border = SCOREBOX_BORDER * zoom;

    actors.reserve(4 + SCOREBOX_NUM_ENTRIES * 6);
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(center_x, center_y):
        setsize(w + border, h + border):
        diffuse(border_color[0], border_color[1], border_color[2], border_color[3]):
        z(z_base)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(center_x, center_y):
        setsize(w, h):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(z_base + 1)
    ));
    // The golf pane carries the course pattern; it fades with the pane as the
    // cycle crosses into or out of it.
    let mut golf_mix = 0.0;
    if cur.kind == PaneKind::Golf {
        golf_mix += cycle.cur_alpha;
    }
    if cycle.next_idx != cycle.cur_idx && next.kind == PaneKind::Golf {
        golf_mix += cycle.next_alpha;
    }
    push_golf_pane_pattern(
        actors,
        GolfPanePattern {
            center: [center_x, center_y],
            size: [w, h],
            tile_h: h * SCOREBOX_GOLF_PATTERN_TILE_HEIGHTS,
            alpha: GOLF_PANE_PATTERN_ALPHA * golf_mix.min(1.0),
            z: z_base + 1,
        },
        elapsed_seconds,
    );
    push_header_overlays(actors, cycle, cur, next, center_x, center_y, zoom, z_base);

    push_rows(
        actors,
        cur.rows.as_slice(),
        center_x,
        center_y,
        zoom,
        z_base,
        cycle.cur_alpha * rows_alpha,
    );
    if cycle.next_idx != cycle.cur_idx {
        push_rows(
            actors,
            next.rows.as_slice(),
            center_x,
            center_y,
            zoom,
            z_base,
            cycle.next_alpha * rows_alpha,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_srpg_panes_use_current_event_logo() {
        assert_eq!(SrpgVariant::CURRENT, SrpgVariant::Srpg10);
        assert_eq!(
            srpg_logo_texture_key(SrpgVariant::CURRENT),
            "srpg10_logo_alt.png"
        );
        assert_eq!(
            srpg_logo_texture_key(SrpgVariant::Srpg9),
            "srpg9_logo_alt.png"
        );
    }

    fn entry(rank: u32, name: &str, is_self: bool, is_rival: bool) -> score_data::LeaderboardEntry {
        score_data::LeaderboardEntry {
            rank,
            name: name.to_string(),
            machine_tag: None,
            score: 10000.0 - f64::from(rank),
            date: String::new(),
            is_rival,
            is_self,
            is_fail: false,
        }
    }

    fn pane(name: &str, entries: Vec<score_data::LeaderboardEntry>) -> score_data::LeaderboardPane {
        score_data::LeaderboardPane {
            name: name.to_string(),
            entries,
            is_ex: false,
            disabled: false,
            personalized: true,
            arrowcloud_kind: None,
        }
    }

    #[test]
    fn non_hard_ex_scorebox_keeps_self_row() {
        let entries = vec![
            entry(1, "world", false, false),
            entry(2, "rival-a", false, true),
            entry(3, "rival-b", false, true),
            entry(4, "rival-c", false, true),
            entry(5, "rival-d", false, true),
            entry(473, "self", true, false),
        ];

        let rows = scorebox_rows_for_kind(
            entries.as_slice(),
            PaneKind::Itl,
            crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE,
            true,
        );
        let ranks = rows
            .iter()
            .filter_map(|row| row.rank.strip_suffix('.'))
            .map(|rank| rank.parse::<u32>().unwrap())
            .collect::<Vec<_>>();
        let names = rows
            .iter()
            .map(|row| row.name.as_str().to_string())
            .collect::<Vec<_>>();

        assert_eq!(ranks, vec![1, 2, 3, 4, 473]);
        assert!(names.iter().any(|name| name == "self"));
    }

    #[test]
    fn scorebox_keeps_rows_nearest_self() {
        let entries = vec![
            entry(1, "world", false, false),
            entry(66, "far-6", false, false),
            entry(67, "far-5", false, false),
            entry(68, "far-4", false, false),
            entry(69, "near-3", false, false),
            entry(70, "near-2", false, false),
            entry(71, "near-1", false, false),
            entry(72, "self", true, false),
        ];

        let rows = scorebox_rows_for_kind(
            entries.as_slice(),
            PaneKind::Itl,
            crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE,
            true,
        );
        let ranks = rows
            .iter()
            .filter_map(|row| row.rank.strip_suffix('.'))
            .map(|rank| rank.parse::<u32>().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(ranks, vec![1, 69, 70, 71, 72]);
    }

    #[test]
    fn itl_scorebox_uses_ex_score_color() {
        let entries = vec![
            entry(1, "world", false, false),
            entry(2, "self", true, false),
            entry(3, "rival", false, true),
        ];

        let rows = scorebox_rows_for_kind(
            entries.as_slice(),
            PaneKind::Itl,
            crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE,
            true,
        );

        for row in rows.iter().take(3) {
            assert_eq!(row.score_color, color::JUDGMENT_RGBA[0]);
        }
    }

    #[test]
    fn entries_with_local_self_state_marks_matching_online_name_as_self() {
        let runtime = ScoreboxSideView {
            display_name: "Self Player".into(),
            player_initials: "SELF".into(),
            ..Default::default()
        };
        let pane = pane("GrooveStats", vec![entry(7, "Self Player", false, false)]);

        let entries = entries_with_local_self_state(&runtime, &pane);

        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_self);
        assert_eq!(entries[0].machine_tag, local_self_machine_tag(&runtime));
    }

    #[test]
    fn entries_with_local_self_state_does_not_add_missing_self_row() {
        let pane = pane(
            "GrooveStats",
            vec![
                entry(1, "world", false, false),
                entry(2, "rival", false, true),
            ],
        );

        let entries =
            entries_with_local_self_state(&crate::views::ScoreboxSideView::default(), &pane);

        assert_eq!(entries.len(), 2);
        assert!(!entries.iter().any(|entry| entry.is_self));
        assert!(matches!(&entries, Cow::Borrowed(_)));
    }

    #[test]
    fn select_music_view_uses_prepared_local_records() {
        let runtime = ScoreboxSideView {
            chart_hash: Some("chart".to_string()),
            player_initials: "P1".into(),
            local_itg: Some(crate::views::ScoreboxLocalView {
                score_10000: 9876.0,
                failed: false,
            }),
            machine_itg: Some(crate::views::ScoreboxMachineView {
                name: "AAA".to_string(),
                score_10000: 9999.0,
                failed: false,
            }),
            ..Default::default()
        };

        let presentation = SelectMusicScoreboxPresentation::new(&runtime);
        let view = presentation.view(true, false, false);

        assert_eq!(view.player_name.as_ref(), "P1");
        assert_eq!(view.player_score.as_ref(), "98.76%");
        assert_eq!(view.machine_name.as_ref(), "AAA");
        assert_eq!(view.machine_score.as_ref(), "99.99%");
        assert_eq!(view.mode_label.as_ref(), "ITG Score");
        assert!(!view.show_rivals);
        assert!(std::ptr::eq(view, presentation.view(true, false, false)));

        let stale = presentation.view(false, false, false);
        assert_eq!(stale.player_name.as_ref(), "----");
        assert_eq!(stale.machine_name.as_ref(), "----");
        assert_eq!(stale.player_score.as_ref(), "??.??%");
        assert_eq!(stale.machine_score.as_ref(), "??.??%");
    }

    #[test]
    fn scorebox_text_width_caps_precede_zoom() {
        let mut rows = empty_rows();
        rows[1] = GameplayScoreboxRow {
            rank: owned_text("123456789."),
            name: TextContent::Shared(owned_text("DF.LemmingOnTheRun")),
            score: owned_text("100.00"),
            rank_color: [1.0; 4],
            name_color: [1.0; 4],
            score_color: [1.0; 4],
        };
        let mut actors = Vec::new();
        push_rows(&mut actors, &rows, 0.0, 0.0, 0.5, 0, 1.0);

        for (text, width) in [("123456789.", 30.0), ("DF.LemmingOnTheRun", 100.0)] {
            let Some(Actor::Text {
                scale,
                max_width,
                max_w_pre_zoom,
                ..
            }) = actors.iter().find(
                |actor| matches!(actor, Actor::Text { content, .. } if content.as_str() == text),
            )
            else {
                panic!("expected scorebox text actor for {text}");
            };
            assert_eq!(*scale, [0.435; 2]);
            assert_eq!(*max_width, Some(width));
            assert!(*max_w_pre_zoom);
        }
    }

    #[test]
    fn gameplay_panes_respect_select_music_leaderboard_filter() {
        let snapshot = score_data::CachedPlayerLeaderboardData {
            loading: false,
            error: None,
            data: Some(std::sync::Arc::new(score_data::PlayerLeaderboardData {
                panes: vec![
                    pane("GrooveStats", vec![entry(1, "itg", false, false)]),
                    score_data::LeaderboardPane {
                        name: "ArrowCloud".to_string(),
                        entries: vec![entry(1, "hard-ex", false, false)],
                        is_ex: false,
                        disabled: false,
                        personalized: true,
                        arrowcloud_kind: Some(score_data::ArrowCloudPaneKind::HardEx),
                    },
                ],
                srpg_self_score: None,
                itl_self_score: None,
                itl_self_rank: None,
            })),
        };

        let panes = gameplay_panes_from_snapshot(
            &snapshot,
            score_data::SelectMusicScoreboxFilter {
                itg: false,
                ex: false,
                hard_ex: true,
                tournaments: false,
                golf: false,
                events_first: true,
            },
            crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE,
        );

        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].kind, PaneKind::HardEx);
        assert_eq!(panes[0].mode_text.as_str(), "H.EX");
    }

    #[test]
    fn status_panes_carry_no_service_branding() {
        let runtime = ScoreboxSideView {
            show_ex_score: true,
            ..ScoreboxSideView::default()
        };
        let loading = score_data::CachedPlayerLeaderboardData::loading();
        let panes = select_music_panes_from_snapshot(&loading, &runtime, false);
        assert_eq!(panes.len(), 1);
        let pane = &panes[0];
        assert_eq!(pane.kind, PaneKind::Other);
        assert!(!pane.is_arrowcloud);
        assert!(pane.mode_text.as_str().is_empty());
        assert_eq!(pane.border_color, SCOREBOX_STATUS_GREY);
        assert_eq!(pane.rows[0].name.as_str(), "Loading ...");
        assert!(!is_gs_logo(pane) && !is_ex_text(pane) && !is_arrowcloud_logo(pane));

        let mut actors = Vec::new();
        push_gameplay_scorebox_actors_from_panes(&mut actors, &panes, 100.0, 100.0, 1.0, 0.0, 1.0);
        assert!(
            actors.iter().all(|actor| !matches!(
                actor,
                Actor::Sprite { source, .. } if source.texture_key().is_some()
            )),
            "a status pane draws no service logo"
        );

        let errored = score_data::CachedPlayerLeaderboardData {
            loading: false,
            error: Some(Arc::from("timed out")),
            data: None,
        };
        let gameplay = gameplay_panes_from_snapshot(
            &errored,
            score_data::SelectMusicScoreboxFilter::default(),
            crate::color::SIMPLY_LOVE_JUDGMENT_PALETTE,
        );
        assert_eq!(gameplay.len(), 1);
        assert_eq!(gameplay[0].kind, PaneKind::Other);
        assert_eq!(gameplay[0].border_color, SCOREBOX_STATUS_GREY);
        assert!(gameplay[0].mode_text.as_str().is_empty());
    }

    fn online_runtime(leaderboards: score_data::CachedPlayerLeaderboardData) -> ScoreboxSideView {
        ScoreboxSideView {
            joined: true,
            chart_hash: Some("chart".to_string()),
            online_active: true,
            leaderboards: Some(leaderboards),
            ..ScoreboxSideView::default()
        }
    }

    #[test]
    fn select_music_loading_pane_waits_out_the_grace_window() {
        let runtime = online_runtime(score_data::CachedPlayerLeaderboardData::loading());
        let snapshot = runtime.leaderboards.as_ref().unwrap();

        let pending = select_music_panes_from_snapshot(snapshot, &runtime, true);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].kind, PaneKind::Other);
        assert_eq!(pending[0].border_color, SCOREBOX_STATUS_GREY);
        assert!(
            pending[0].rows.iter().all(|row| row.name.as_str().is_empty()),
            "inside the grace window the box shows no loading label"
        );

        let loading = select_music_panes_from_snapshot(snapshot, &runtime, false);
        assert_eq!(loading[0].rows[0].name.as_str(), "Loading ...");
    }

    #[test]
    fn pending_presentation_view_hides_the_loading_label() {
        let runtime = online_runtime(score_data::CachedPlayerLeaderboardData::loading());
        let presentation = SelectMusicScoreboxPresentation::new(&runtime);
        assert_eq!(
            presentation.view(true, true, false).mode_label.as_ref(),
            "Loading ..."
        );
        assert_eq!(
            presentation.view(true, true, true).mode_label.as_ref(),
            "ITG Score"
        );
        assert!(std::ptr::eq(
            presentation.view(true, false, true),
            presentation.view(true, false, false)
        ));
        assert!(std::ptr::eq(
            presentation.view(false, true, true),
            presentation.view(false, true, false)
        ));
    }

    #[test]
    fn revealing_scorebox_fades_rows_but_keeps_the_frame() {
        let runtime = online_runtime(score_data::CachedPlayerLeaderboardData::ready(
            score_data::PlayerLeaderboardData {
                panes: vec![pane("GrooveStats", vec![entry(1, "AAA", false, false)])],
                srpg_self_score: None,
                itl_self_score: None,
                itl_self_rank: None,
            },
        ));
        let just_ready = scorebox_theme::ScoreboxFetchPhase {
            loading_since: Some(0.0),
            ready_at: Some(1.0),
        };
        let push_at = |now: f32| {
            let mut actors = Vec::new();
            push_select_music_scorebox_actors(
                &mut actors,
                &runtime,
                Some("chart"),
                true,
                0.0,
                0.0,
                1.0,
                0.0,
                SelectMusicScoreboxFetch::at(just_ready, now),
            );
            actors
        };
        let name_rows = |actors: &[Actor]| {
            actors
                .iter()
                .filter(|actor| matches!(actor, Actor::Text { content, .. } if content.as_str() == "AAA"))
                .count()
        };

        let hidden = push_at(1.0);
        assert!(hidden.len() >= 2, "frame quads are drawn at reveal start");
        assert_eq!(name_rows(&hidden), 0);

        let shown = push_at(2.0);
        assert_eq!(name_rows(&shown), 1);
        assert_eq!(
            SelectMusicScoreboxFetch::at(scorebox_theme::ScoreboxFetchPhase::default(), 0.0),
            SelectMusicScoreboxFetch {
                within_loading_grace: false,
                rows_alpha: 1.0,
            }
        );
    }
}


#[cfg(test)]
mod cycle_clock_tests {
    use super::*;

    fn runtime(chart_hash: Option<&str>, leaderboards: Option<score_data::CachedPlayerLeaderboardData>) -> ScoreboxSideView {
        ScoreboxSideView {
            chart_hash: chart_hash.map(str::to_owned),
            leaderboards,
            ..ScoreboxSideView::default()
        }
    }

    #[test]
    fn cycle_restarts_when_a_charts_boards_arrive_and_holds_while_they_stay() {
        let mut clock = ScoreboxCycleClock::default();
        clock.sync(&runtime(Some("abc"), Some(score_data::CachedPlayerLeaderboardData::loading())), 10.0);
        assert_eq!(clock.elapsed_seconds(12.0), 2.0);

        let ready = score_data::CachedPlayerLeaderboardData { loading: false, data: None, error: None };
        clock.sync(&runtime(Some("abc"), Some(ready.clone())), 13.0);
        assert_eq!(clock.elapsed_seconds(13.5), 0.5, "boards arriving restarts the cycle on the first pane");

        clock.sync(&runtime(Some("abc"), Some(ready.clone())), 40.0);
        assert_eq!(clock.elapsed_seconds(40.0), 27.0, "the same loaded chart keeps cycling");

        clock.sync(&runtime(Some("def"), Some(ready)), 41.0);
        assert_eq!(clock.elapsed_seconds(41.0), 0.0, "a new chart restarts the cycle");
    }

    #[test]
    fn cycle_never_reports_negative_elapsed_after_a_clock_reset() {
        let mut clock = ScoreboxCycleClock::default();
        clock.sync(&runtime(Some("abc"), None), 30.0);
        assert_eq!(clock.elapsed_seconds(1.0), 0.0);
    }
}

#[cfg(test)]
mod golf_tests {
    use super::*;

    #[test]
    fn golf_pane_pattern_tiles_and_drifts_inside_the_pane() {
        let pattern = GolfPanePattern {
            center: [100.0, 50.0],
            size: [162.0, 80.0],
            tile_h: 240.0,
            alpha: 0.22,
            z: 72,
        };
        let mut actors = Vec::new();
        push_golf_pane_pattern(&mut actors, pattern, 10.0);
        let Some(Actor::Sprite { source, uv_rect, tint, .. }) = actors.first() else {
            panic!("the pattern is one sprite");
        };
        assert_eq!(source.texture_key(), Some(crate::resources::GOLF_PANE_PATTERN_TEXTURE));
        let uv = uv_rect.expect("the pattern repeats through a custom texture rect");
        assert!((uv[2] - uv[0] - 162.0 / 300.0).abs() < 1e-4, "u span is pane width over tile width");
        assert!((uv[3] - uv[1] - 80.0 / 240.0).abs() < 1e-4, "v span is pane height over tile height");
        assert!((tint[3] - 0.22).abs() < 1e-6);

        let mut later = Vec::new();
        push_golf_pane_pattern(&mut later, pattern, 20.0);
        let Some(Actor::Sprite { uv_rect: later_uv, .. }) = later.first() else { panic!() };
        assert_ne!(later_uv, uv_rect, "the pattern drifts");

        let mut none = Vec::new();
        push_golf_pane_pattern(&mut none, GolfPanePattern { alpha: 0.0, ..pattern }, 0.0);
        assert!(none.is_empty(), "a fully faded pattern pushes nothing");
    }

    #[test]
    fn golf_pane_border_is_a_muted_green() {
        let [r, g, b, a] = pane_color(PaneKind::Golf);
        assert!(g > r && g > b, "still reads as green");
        assert!(g < 0.6, "but no longer glows");
        assert_eq!(a, 1.0);
    }

    #[test]
    fn golf_rows_format_raw_strokes_without_percent_clamping() {
        let entry = score_data::LeaderboardEntry { rank: 1, name: "Golf Player".into(), machine_tag: None, score: 123456.0, date: String::new(), is_rival: false, is_self: false, is_fail: false };
        let row = gameplay_row_from_entry(&entry, PaneKind::Golf, color::SIMPLY_LOVE_JUDGMENT_PALETTE, false);
        assert_eq!(row.score.as_ref(), "123.46");
        let ace = score_data::LeaderboardEntry { score: 0.0, ..entry };
        let row = gameplay_row_from_entry(&ace, PaneKind::Golf, color::SIMPLY_LOVE_JUDGMENT_PALETTE, false);
        assert_eq!(row.score.as_ref(), "0.00");
    }
}
