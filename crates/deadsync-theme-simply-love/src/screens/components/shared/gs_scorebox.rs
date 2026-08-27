use crate::act;
use crate::assets;
use crate::scorebox as scorebox_theme;
use crate::scorebox::{
    SCOREBOX_BORDER, SCOREBOX_H, SCOREBOX_W, ScoreboxCycleState, color_with_alpha, lerp_color,
    logo_alpha, scorebox_cycle_state,
};
use crate::views::ScoreboxSideView;
use deadlib_present::actors::Actor;
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadlib_present::color;
use deadlib_present::color::{JudgmentColorRole as Role, JudgmentPalette};
use deadsync_config::prelude::SrpgVariant;
use deadsync_score as score_data;
use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

pub(crate) const SCOREBOX_NUM_ENTRIES: usize = 5;
const SCOREBOX_GS_BLUE: [f32; 4] = color::rgba_hex("#007b85");
const SCOREBOX_SRPG_YELLOW: [f32; 4] = [1.0, 0.972, 0.792, 1.0];
const SCOREBOX_ITL_PINK: [f32; 4] = [1.0, 0.2, 0.406, 1.0];
const SCOREBOX_SELF: [f32; 4] = color::rgba_hex("#A1FF94");
const SCOREBOX_RIVAL: [f32; 4] = color::rgba_hex("#C29CFF");
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
    name: Arc<str>,
    score: Arc<str>,
    rank_color: [f32; 4],
    name_color: [f32; 4],
    score_color: [f32; 4],
}

#[derive(Clone, Debug)]
struct GameplayScoreboxPane {
    kind: PaneKind,
    is_arrowcloud: bool,
    mode_text: Arc<str>,
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
        if !profile.display_scorebox || !profile.gs_active {
            return Self::default();
        }
        Self {
            panes: snapshot.map_or_else(Vec::new, |snapshot| {
                gameplay_panes_from_snapshot(snapshot, profile, filter, palette)
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
/// Worst-case frame work is one chart comparison, one view branch, and eleven
/// bounded Arc clones per visible side.
#[derive(Clone, Debug)]
pub(crate) struct SelectMusicScoreboxPresentation {
    local: SelectMusicScoreboxView,
    online: SelectMusicScoreboxView,
    stale: SelectMusicScoreboxView,
}

impl SelectMusicScoreboxPresentation {
    pub(crate) fn new(runtime: &ScoreboxSideView) -> Self {
        let chart_present = runtime.chart_hash.is_some();
        Self {
            local: build_select_music_scorebox_view(runtime, true, chart_present, false),
            online: build_select_music_scorebox_view(runtime, true, chart_present, true),
            stale: build_select_music_scorebox_view(runtime, false, false, false),
        }
    }

    #[inline(always)]
    pub(crate) const fn view(
        &self,
        chart_matches: bool,
        show_online: bool,
    ) -> &SelectMusicScoreboxView {
        if !chart_matches {
            &self.stale
        } else if show_online {
            &self.online
        } else {
            &self.local
        }
    }
}

impl Default for SelectMusicScoreboxPresentation {
    fn default() -> Self {
        Self::new(&ScoreboxSideView::default())
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
        PaneKind::Srpg | PaneKind::Other => None,
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

    if !show_rivals || !runtime.groovestats_active || !chart_matches {
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
    let filtered_panes =
        score_data::select_music_scorebox_filtered_panes(data.panes.as_slice(), filter);
    let Some(pane) =
        score_data::preferred_primary_scorebox_pane(filtered_panes.as_slice(), show_ex)
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
        view.machine_score = score_text_with_percent(world.score);
    }
    if let Some(player_entry) = entries.iter().find(|entry| entry.is_self) {
        view.player_name = Arc::from(score_data::scorebox_machine_tag(
            player_entry.machine_tag.as_deref(),
            &player_entry.name,
        ));
        view.player_score = score_text_with_percent(player_entry.score);
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
            score_text_with_percent(rival.score),
        );
    }
    view
}

#[inline(always)]
fn gameplay_empty_row() -> GameplayScoreboxRow {
    GameplayScoreboxRow {
        rank: empty_text(),
        name: empty_text(),
        score: empty_text(),
        rank_color: [1.0; 4],
        name_color: [1.0; 4],
        score_color: [1.0; 4],
    }
}

#[inline(always)]
fn gameplay_status_row(text: &str) -> GameplayScoreboxRow {
    GameplayScoreboxRow {
        rank: empty_text(),
        name: owned_text(text),
        score: empty_text(),
        rank_color: [1.0; 4],
        name_color: [1.0; 4],
        score_color: [1.0; 4],
    }
}

fn empty_rows() -> [GameplayScoreboxRow; SCOREBOX_NUM_ENTRIES] {
    std::array::from_fn(|_| gameplay_empty_row())
}

fn gameplay_status_pane(show_ex_score: bool, text: &str) -> GameplayScoreboxPane {
    let mut rows = empty_rows();
    rows[0] = gameplay_status_row(text);
    let kind = if show_ex_score {
        PaneKind::Ex
    } else {
        PaneKind::Gs
    };
    GameplayScoreboxPane {
        kind,
        is_arrowcloud: false,
        mode_text: owned_text(score_data::default_scorebox_mode_text(show_ex_score)),
        border_color: SCOREBOX_GS_BLUE,
        rows,
    }
}

fn gameplay_row_from_entry(
    entry: &score_data::LeaderboardEntry,
    kind: PaneKind,
    palette: JudgmentPalette,
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
        name: owned_text(name),
        score: score_text_without_percent(entry.score),
        rank_color,
        name_color,
        score_color,
    }
}

fn scorebox_rows_for_kind(
    entries: &[score_data::LeaderboardEntry],
    kind: PaneKind,
    palette: JudgmentPalette,
) -> [GameplayScoreboxRow; SCOREBOX_NUM_ENTRIES] {
    let mut rows = empty_rows();
    if entries.is_empty() {
        rows[0] = gameplay_status_row("No Scores");
        return rows;
    }

    let selected = score_data::neighboring_leaderboard_entry_refs(entries, SCOREBOX_NUM_ENTRIES);
    for (slot, entry) in rows.iter_mut().zip(selected) {
        *slot = gameplay_row_from_entry(entry, kind, palette);
    }
    rows
}

fn gameplay_pane_from_leaderboard(
    pane: &score_data::LeaderboardPane,
    entries: &[score_data::LeaderboardEntry],
    palette: JudgmentPalette,
) -> GameplayScoreboxPane {
    let kind = score_data::scorebox_pane_kind(pane);
    GameplayScoreboxPane {
        kind,
        is_arrowcloud: pane.is_arrowcloud(),
        mode_text: owned_text(score_data::scorebox_pane_mode_text(kind, pane)),
        border_color: pane_color(kind),
        rows: scorebox_rows_for_kind(entries, kind, palette),
    }
}

fn gameplay_panes_from_snapshot(
    snapshot: &score_data::CachedPlayerLeaderboardData,
    profile_snapshot: &score_data::GameplayScoreboxProfileSnapshot,
    filter: score_data::SelectMusicScoreboxFilter,
    palette: JudgmentPalette,
) -> Vec<GameplayScoreboxPane> {
    if snapshot.loading {
        return vec![gameplay_status_pane(
            profile_snapshot.show_ex_score,
            "Loading ...",
        )];
    }
    if let Some(error) = snapshot.error.as_deref() {
        let text = error_text(error);
        return vec![gameplay_status_pane(profile_snapshot.show_ex_score, text)];
    }
    let Some(data) = snapshot.data.as_ref() else {
        return vec![gameplay_status_pane(
            profile_snapshot.show_ex_score,
            "No Scores",
        )];
    };
    if data.panes.is_empty() {
        return vec![gameplay_status_pane(
            profile_snapshot.show_ex_score,
            "No Scores",
        )];
    }

    if !score_data::select_music_scorebox_filter_has_any(filter) {
        return Vec::new();
    }

    let filtered = score_data::select_music_scorebox_filtered_panes(data.panes.as_slice(), filter);
    if filtered.is_empty() {
        return vec![gameplay_status_pane(
            profile_snapshot.show_ex_score,
            "No Scores",
        )];
    }

    let mut panes = Vec::with_capacity(filtered.len());
    for pane in filtered {
        panes.push(gameplay_pane_from_leaderboard(
            pane,
            pane.entries.as_slice(),
            palette,
        ));
    }
    panes
}

fn select_music_panes_from_snapshot(
    snapshot: &score_data::CachedPlayerLeaderboardData,
    runtime: &ScoreboxSideView,
) -> Vec<GameplayScoreboxPane> {
    if snapshot.loading {
        return vec![gameplay_status_pane(runtime.show_ex_score, "Loading ...")];
    }
    if let Some(error) = snapshot.error.as_deref() {
        let text = error_text(error);
        return vec![gameplay_status_pane(runtime.show_ex_score, text)];
    }
    let Some(data) = snapshot.data.as_ref() else {
        return vec![gameplay_status_pane(runtime.show_ex_score, "No Scores")];
    };
    let filter = runtime.pane_filter;
    if !score_data::select_music_scorebox_filter_has_any(filter) {
        return Vec::new();
    }

    let filtered = score_data::select_music_scorebox_filtered_panes(data.panes.as_slice(), filter);
    if filtered.is_empty() {
        return vec![gameplay_status_pane(runtime.show_ex_score, "No Scores")];
    }
    let mut panes = Vec::with_capacity(filtered.len());
    for pane in filtered {
        let entries = entries_with_local_self_state(runtime, pane);
        panes.push(gameplay_pane_from_leaderboard(
            pane,
            entries.as_ref(),
            JudgmentPalette::default(),
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
    text: &Arc<str>,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    z_base: i16,
    alpha: f32,
) {
    if text.is_empty() || alpha <= 0.0 {
        return;
    }
    let c = color_with_alpha([1.0, 1.0, 1.0, SCOREBOX_MODE_ALPHA], alpha);
    actors.push(act!(text:
        font("miso"):
        settext(Arc::clone(text)):
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
    let dims = assets::texture_dims(texture).unwrap_or(assets::TexMeta { w: 1, h: 1 });
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

pub fn select_music_scorebox_actors(
    runtime: &ScoreboxSideView,
    chart_hash: Option<&str>,
    show_scorebox: bool,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    elapsed_seconds: f32,
) -> Vec<Actor> {
    if !show_scorebox || !runtime.groovestats_active || runtime.chart_hash.as_deref() != chart_hash
    {
        return Vec::new();
    }
    if chart_hash.is_none() {
        return Vec::new();
    }
    let Some(snapshot) = runtime.leaderboards.as_ref() else {
        return Vec::new();
    };
    let panes = select_music_panes_from_snapshot(snapshot, runtime);
    gameplay_scorebox_actors_from_panes(&panes, center_x, center_y, zoom, elapsed_seconds)
}

fn gameplay_scorebox_actors_from_panes(
    panes: &[GameplayScoreboxPane],
    center_x: f32,
    center_y: f32,
    zoom: f32,
    elapsed_seconds: f32,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(4 + SCOREBOX_NUM_ENTRIES * 6);
    push_gameplay_scorebox_actors_from_panes(
        &mut actors,
        panes,
        center_x,
        center_y,
        zoom,
        elapsed_seconds,
    );
    actors
}

fn push_gameplay_scorebox_actors_from_panes(
    actors: &mut Vec<Actor>,
    panes: &[GameplayScoreboxPane],
    center_x: f32,
    center_y: f32,
    zoom: f32,
    elapsed_seconds: f32,
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
    push_header_overlays(actors, cycle, cur, next, center_x, center_y, zoom, z_base);

    push_rows(
        actors,
        cur.rows.as_slice(),
        center_x,
        center_y,
        zoom,
        z_base,
        cycle.cur_alpha,
    );
    if cycle.next_idx != cycle.cur_idx {
        push_rows(
            actors,
            next.rows.as_slice(),
            center_x,
            center_y,
            zoom,
            z_base,
            cycle.next_alpha,
        );
    }
}

#[cfg(feature = "bench-support")]
fn gameplay_scorebox_actor_checksum(actors: &[Actor]) -> usize {
    actors.iter().fold(actors.len(), |checksum, actor| {
        let value = match actor {
            Actor::Text { content, .. } => content.len(),
            Actor::Sprite { source, .. } => source.texture_key().map_or(0, str::len),
            _ => 1,
        };
        checksum.rotate_left(5) ^ value
    })
}

/// Transition macrobenchmark for prewarmed leaderboard composition.
#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GameplayScoreboxBenchmark {
    plan: GameplayScoreboxPlan,
    scratch: Vec<Actor>,
}

#[cfg(feature = "bench-support")]
impl GameplayScoreboxBenchmark {
    pub fn new() -> Self {
        let entry = |rank: u32, pane: usize| score_data::LeaderboardEntry {
            rank,
            name: format!("player-{pane}-{rank}"),
            machine_tag: None,
            score: 10_000.0 - f64::from(rank),
            date: String::new(),
            is_rival: rank == 2,
            is_self: rank == 5,
            is_fail: false,
        };
        let panes = (0..4)
            .map(|pane| score_data::LeaderboardPane {
                name: ["GrooveStats", "EX", "SRPG", "ITL"][pane].to_string(),
                entries: (1..=5).map(|rank| entry(rank, pane)).collect(),
                is_ex: pane == 1,
                disabled: false,
                personalized: true,
                arrowcloud_kind: None,
            })
            .collect();
        let snapshot = score_data::CachedPlayerLeaderboardData {
            loading: false,
            error: None,
            data: Some(Arc::new(score_data::PlayerLeaderboardData {
                panes,
                srpg_self_score: None,
                itl_self_score: None,
                itl_self_rank: None,
            })),
        };
        let mut profile = score_data::GameplayScoreboxProfileSnapshot::default();
        profile.display_scorebox = true;
        profile.gs_active = true;
        let filter = score_data::SelectMusicScoreboxFilter {
            itg: true,
            ex: true,
            hard_ex: true,
            tournaments: true,
        };
        let plan = GameplayScoreboxPlan::new_with_palette(
            Some(&snapshot),
            &profile,
            filter,
            JudgmentPalette::default(),
        );
        let mut scratch = Vec::new();
        plan.push_actors(&mut scratch, 320.0, 160.0, 1.0, 4.25);
        scratch.clear();
        Self { plan, scratch }
    }

    pub fn frame(&mut self, elapsed_seconds: f32) -> usize {
        self.scratch.clear();
        self.plan
            .push_actors(&mut self.scratch, 320.0, 160.0, 1.0, elapsed_seconds);
        gameplay_scorebox_actor_checksum(std::hint::black_box(&self.scratch))
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

    fn scorebox_profile(show_ex_score: bool) -> score_data::GameplayScoreboxProfileSnapshot {
        let mut snapshot = score_data::GameplayScoreboxProfileSnapshot::default();
        snapshot.show_ex_score = show_ex_score;
        snapshot
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
            JudgmentPalette::default(),
        );
        let ranks = rows
            .iter()
            .filter_map(|row| row.rank.strip_suffix('.'))
            .map(|rank| rank.parse::<u32>().unwrap())
            .collect::<Vec<_>>();
        let names = rows
            .iter()
            .map(|row| row.name.as_ref().to_string())
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
            JudgmentPalette::default(),
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
            JudgmentPalette::default(),
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

        let entries = entries_with_local_self_state(&ScoreboxSideView::default(), &pane);

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
        let view = presentation.view(true, false);

        assert_eq!(view.player_name.as_ref(), "P1");
        assert_eq!(view.player_score.as_ref(), "98.76%");
        assert_eq!(view.machine_name.as_ref(), "AAA");
        assert_eq!(view.machine_score.as_ref(), "99.99%");
        assert_eq!(view.mode_label.as_ref(), "ITG Score");
        assert!(!view.show_rivals);
        assert!(std::ptr::eq(view, presentation.view(true, false)));

        let stale = presentation.view(false, false);
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
            name: owned_text("DF.LemmingOnTheRun"),
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

        let profile_snapshot = scorebox_profile(false);
        let panes = gameplay_panes_from_snapshot(
            &snapshot,
            &profile_snapshot,
            score_data::SelectMusicScoreboxFilter {
                itg: false,
                ex: false,
                hard_ex: true,
                tournaments: false,
            },
            JudgmentPalette::default(),
        );

        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].kind, PaneKind::HardEx);
        assert_eq!(panes[0].mode_text.as_ref(), "H.EX");
    }
}
