use crate::act;
use crate::assets;
use crate::config::{self, SrpgVariant};
use crate::scorebox as scorebox_theme;
use crate::scorebox::{
    SCOREBOX_BORDER, SCOREBOX_H, SCOREBOX_W, ScoreboxCycleState, color_with_alpha, lerp_color,
    logo_alpha, scorebox_cycle_state,
};
use crate::views::ScoreboxSideView;
use deadlib_present::actors::Actor;
use deadlib_present::cache::{TextCache, cached_text};
use deadlib_present::color;
use deadsync_score as score_data;
use std::cell::RefCell;
use std::collections::HashMap;
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
type SelectMusicPaneFilter = score_data::SelectMusicScoreboxFilter;

thread_local! {
    static SCORE_PERCENT_TEXT_CACHE: RefCell<TextCache<u64>> = RefCell::new(HashMap::with_capacity(2048));
    static SCORE_VALUE_TEXT_CACHE: RefCell<TextCache<u64>> = RefCell::new(HashMap::with_capacity(2048));
    static RANK_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(512));
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

#[derive(Clone, Debug)]
pub struct SelectMusicScoreboxView {
    pub mode_text: String,
    pub machine_name: String,
    pub machine_score: Arc<str>,
    pub player_name: String,
    pub player_score: Arc<str>,
    pub rivals: [(String, Arc<str>); 3],
    pub show_rivals: bool,
    pub loading_text: Option<String>,
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
fn select_music_pane_filter() -> SelectMusicPaneFilter {
    let cfg = crate::config::get();
    score_data::SelectMusicScoreboxFilter {
        itg: cfg.select_music_scorebox_cycle_itg,
        ex: cfg.select_music_scorebox_cycle_ex,
        hard_ex: cfg.select_music_scorebox_cycle_hard_ex,
        tournaments: cfg.select_music_scorebox_cycle_tournaments,
    }
}

#[inline(always)]
fn pane_color(kind: PaneKind) -> [f32; 4] {
    match kind {
        PaneKind::Gs | PaneKind::Ex | PaneKind::Other => SCOREBOX_GS_BLUE,
        PaneKind::HardEx => [
            SCOREBOX_GS_BLUE[0]
                + (color::HARD_EX_SCORE_RGBA[0] - SCOREBOX_GS_BLUE[0])
                    * SCOREBOX_HARD_EX_BORDER_TINT,
            SCOREBOX_GS_BLUE[1]
                + (color::HARD_EX_SCORE_RGBA[1] - SCOREBOX_GS_BLUE[1])
                    * SCOREBOX_HARD_EX_BORDER_TINT,
            SCOREBOX_GS_BLUE[2]
                + (color::HARD_EX_SCORE_RGBA[2] - SCOREBOX_GS_BLUE[2])
                    * SCOREBOX_HARD_EX_BORDER_TINT,
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
        view.display_name.as_str(),
        view.groovestats_username.as_str(),
        view.player_initials.as_str(),
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
        view.groovestats_username.as_str(),
        view.display_name.as_str(),
        view.player_initials.as_str(),
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

pub(crate) fn entries_with_local_self_state(
    view: &ScoreboxSideView,
    pane: &score_data::LeaderboardPane,
) -> Vec<score_data::LeaderboardEntry> {
    let kind = score_data::scorebox_pane_kind(pane);
    let mut entries = pane.entries.clone();
    let local_self = local_self_score_10000(view, kind);

    if let Some(entry) = entries.iter_mut().find(|entry| entry.is_self) {
        if let Some((local_score_10000, local_is_fail)) = local_self
            && local_is_fail
            && score_data::same_score_10000(entry.score, local_score_10000)
        {
            entry.is_fail = true;
            if entry.machine_tag.is_none() {
                entry.machine_tag = local_self_machine_tag(view);
            }
        }
        return entries;
    }

    if let Some(entry) = entries
        .iter_mut()
        .find(|entry| leaderboard_entry_matches_local_self(view, entry))
    {
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
        return entries;
    }

    entries
}

#[inline(always)]
pub fn select_music_scorebox_view(
    runtime: &ScoreboxSideView,
    chart_hash: Option<&str>,
    show_rivals: bool,
) -> SelectMusicScoreboxView {
    let chart_matches = runtime.chart_hash.as_deref() == chart_hash;
    let fallback_player = chart_matches
        .then_some(runtime.local_itg)
        .flatten()
        .filter(|score| !score.failed || score.score_10000 > 0.0)
        .map(|score| {
            (
                runtime.player_initials.clone(),
                score_text_with_percent(score.score_10000),
            )
        })
        .unwrap_or_else(|| ("----".to_string(), unknown_score_percent_text()));
    let fallback_machine = chart_matches
        .then_some(runtime.machine_itg.as_ref())
        .flatten()
        .filter(|score| !score.failed || score.score_10000 > 0.0)
        .map(|score| {
            (
                score.name.clone(),
                score_text_with_percent(score.score_10000),
            )
        })
        .unwrap_or_else(|| ("----".to_string(), unknown_score_percent_text()));
    let mut view = SelectMusicScoreboxView {
        mode_text: score_data::default_scorebox_mode_text(runtime.show_ex_score).to_string(),
        machine_name: fallback_machine.0,
        machine_score: fallback_machine.1,
        player_name: fallback_player.0,
        player_score: fallback_player.1,
        rivals: std::array::from_fn(|_| ("----".to_string(), unknown_score_percent_text())),
        show_rivals: false,
        loading_text: None,
    };

    if !show_rivals || !runtime.groovestats_active || !chart_matches {
        return view;
    }
    let filter = select_music_pane_filter();
    if !score_data::select_music_scorebox_filter_has_any(filter) {
        return view;
    }
    view.machine_name = "----".to_string();
    view.machine_score = unknown_score_percent_text();
    view.player_name = "----".to_string();
    view.player_score = unknown_score_percent_text();
    view.show_rivals = true;

    if chart_hash.is_none() {
        return view;
    }
    let Some(snapshot) = runtime.leaderboards.as_ref() else {
        return view;
    };

    if snapshot.loading {
        view.loading_text = Some("Loading ...".to_string());
        return view;
    }
    if let Some(error) = snapshot.error.as_deref() {
        view.loading_text = Some(error_text(error).to_string());
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
        view.loading_text = Some("No Scores".to_string());
        return view;
    };

    let kind = score_data::scorebox_pane_kind(pane);
    let entries = entries_with_local_self_state(runtime, pane);
    view.mode_text = score_data::scorebox_pane_mode_text(kind, pane).to_string();
    if entries.is_empty() {
        view.loading_text = Some("No Scores".to_string());
        return view;
    }

    if let Some(world) = entries
        .iter()
        .find(|entry| entry.rank == 1)
        .or_else(|| entries.first())
    {
        view.machine_name =
            score_data::scorebox_machine_tag(world.machine_tag.as_deref(), &world.name);
        view.machine_score = score_text_with_percent(world.score);
    }
    if let Some(player_entry) = entries.iter().find(|entry| entry.is_self) {
        view.player_name = score_data::scorebox_machine_tag(
            player_entry.machine_tag.as_deref(),
            &player_entry.name,
        );
        view.player_score = score_text_with_percent(player_entry.score);
    } else if let Some((local_score_10000, _)) = local_self_score_10000(runtime, kind) {
        view.player_name = local_self_scorebox_name(runtime);
        view.player_score = score_text_with_percent(local_score_10000);
    }
    for (idx, rival) in entries
        .iter()
        .filter(|entry| entry.is_rival)
        .take(3)
        .enumerate()
    {
        view.rivals[idx] = (
            score_data::scorebox_machine_tag(rival.machine_tag.as_deref(), &rival.name),
            score_text_with_percent(rival.score),
        );
    }
    view
}

pub fn select_music_mode_text(show_ex_score: bool) -> String {
    score_data::default_scorebox_mode_text(show_ex_score).to_string()
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
        color::JUDGMENT_RGBA[0]
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
) -> [GameplayScoreboxRow; SCOREBOX_NUM_ENTRIES] {
    let mut rows = empty_rows();
    if entries.is_empty() {
        rows[0] = gameplay_status_row("No Scores");
        return rows;
    }

    let selected = score_data::prioritized_leaderboard_entries(entries, SCOREBOX_NUM_ENTRIES);
    for (slot, entry) in rows.iter_mut().zip(selected.iter()) {
        *slot = gameplay_row_from_entry(entry, kind);
    }
    rows
}

fn gameplay_pane_from_leaderboard(
    pane: &score_data::LeaderboardPane,
    entries: &[score_data::LeaderboardEntry],
) -> GameplayScoreboxPane {
    let kind = score_data::scorebox_pane_kind(pane);
    GameplayScoreboxPane {
        kind,
        is_arrowcloud: pane.is_arrowcloud(),
        mode_text: owned_text(score_data::scorebox_pane_mode_text(kind, pane)),
        border_color: pane_color(kind),
        rows: scorebox_rows_for_kind(entries, kind),
    }
}

fn gameplay_panes_from_snapshot(
    snapshot: &score_data::CachedPlayerLeaderboardData,
    profile_snapshot: &score_data::GameplayScoreboxProfileSnapshot,
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

    let filter = select_music_pane_filter();
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
    let filter = select_music_pane_filter();
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
        panes.push(gameplay_pane_from_leaderboard(pane, entries.as_slice()));
    }
    panes
}

#[inline(always)]
fn is_gs_logo(pane: &GameplayScoreboxPane) -> bool {
    !pane.is_arrowcloud && matches!(pane.kind, PaneKind::Gs | PaneKind::Ex)
}

#[inline(always)]
fn is_ex_text(pane: &GameplayScoreboxPane) -> bool {
    matches!(pane.kind, PaneKind::Ex)
}

fn is_arrowcloud_logo(pane: &GameplayScoreboxPane) -> bool {
    pane.is_arrowcloud
}

#[inline(always)]
fn is_hard_ex_text(pane: &GameplayScoreboxPane) -> bool {
    matches!(pane.kind, PaneKind::HardEx)
}

#[inline(always)]
fn is_srpg_logo(kind: PaneKind) -> bool {
    matches!(kind, PaneKind::Srpg)
}

#[inline(always)]
fn is_itl_logo(kind: PaneKind) -> bool {
    matches!(kind, PaneKind::Itl)
}

#[inline(always)]
fn is_fallback_text(pane: &GameplayScoreboxPane) -> bool {
    matches!(pane.kind, PaneKind::Other)
        || (pane.is_arrowcloud && matches!(pane.kind, PaneKind::Gs))
}

fn push_mode_text(
    actors: &mut Vec<Actor>,
    text: &str,
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
        settext(text.to_owned()):
        align(0.5, 0.5):
        xy(center_x + 2.0 * zoom, center_y - 5.0 * zoom):
        zoom(0.9 * zoom):
        diffuse(c[0], c[1], c[2], c[3]):
        z(z_base + 2):
        horizalign(center)
    ));
}

fn push_centered_logo(
    actors: &mut Vec<Actor>,
    texture: &str,
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
    actors.push(act!(sprite(texture):
        align(0.5, 0.5):
        xy(center_x, center_y):
        setsize(fit.width, fit.height):
        diffuse(c[0], c[1], c[2], c[3]):
        z(z_base + 2)
    ));
}

fn push_mode_overlay(
    actors: &mut Vec<Actor>,
    text: &str,
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
        settext(text.to_owned()):
        align(0.5, 0.5):
        xy(center_x + 2.0 * zoom, center_y - 5.0 * zoom):
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
            cur.mode_text.as_ref(),
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
            next.mode_text.as_ref(),
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
        srpg_logo_texture_key(),
        center_x,
        center_y,
        zoom,
        0.07,
        z_base,
        alpha,
    );
}

pub(crate) fn srpg_logo_texture_key() -> &'static str {
    let cfg = config::get();
    match cfg.srpg_variant {
        SrpgVariant::Srpg10 if cfg.visual_style.is_srpg() => "srpg10_logo_alt.png",
        _ => "srpg9_logo_alt.png",
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
            xy(center_x + (-SCOREBOX_W * 0.5 + 14.0) * zoom, y):
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

    let rank_x = center_x + (-SCOREBOX_W * 0.5 + 27.0) * zoom;
    let name_x = center_x + (-SCOREBOX_W * 0.5 + 30.0) * zoom;
    let score_x = center_x + (-SCOREBOX_W * 0.5 + 160.0) * zoom;

    for (i, row) in rows.iter().enumerate().take(SCOREBOX_NUM_ENTRIES) {
        let y = center_y + (-SCOREBOX_H * 0.5 + 16.0 * (i as f32 + 1.0) - 8.0) * zoom;
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

pub fn gameplay_scorebox_actors_from_snapshot(
    snapshot: Option<&score_data::CachedPlayerLeaderboardData>,
    profile_snapshot: &score_data::GameplayScoreboxProfileSnapshot,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    elapsed_seconds: f32,
) -> Vec<Actor> {
    if !profile_snapshot.display_scorebox || !profile_snapshot.gs_active {
        return Vec::new();
    }
    let Some(snapshot) = snapshot else {
        return Vec::new();
    };
    gameplay_scorebox_actors_from_cached_snapshot(
        snapshot,
        profile_snapshot,
        center_x,
        center_y,
        zoom,
        elapsed_seconds,
    )
}

pub(crate) fn gameplay_scorebox_actors_from_cached_snapshot(
    snapshot: &score_data::CachedPlayerLeaderboardData,
    profile_snapshot: &score_data::GameplayScoreboxProfileSnapshot,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    elapsed_seconds: f32,
) -> Vec<Actor> {
    let panes = gameplay_panes_from_snapshot(snapshot, profile_snapshot);
    gameplay_scorebox_actors_from_panes(&panes, center_x, center_y, zoom, elapsed_seconds)
}

fn gameplay_scorebox_actors_from_panes(
    panes: &[GameplayScoreboxPane],
    center_x: f32,
    center_y: f32,
    zoom: f32,
    elapsed_seconds: f32,
) -> Vec<Actor> {
    if panes.is_empty() {
        return Vec::new();
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

    let mut actors = Vec::with_capacity(4 + SCOREBOX_NUM_ENTRIES * 6);
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
    push_header_overlays(
        &mut actors,
        cycle,
        cur,
        next,
        center_x,
        center_y,
        zoom,
        z_base,
    );

    push_rows(
        &mut actors,
        cur.rows.as_slice(),
        center_x,
        center_y,
        zoom,
        z_base,
        cycle.cur_alpha,
    );
    if cycle.next_idx != cycle.cur_idx {
        push_rows(
            &mut actors,
            next.rows.as_slice(),
            center_x,
            center_y,
            zoom,
            z_base,
            cycle.next_alpha,
        );
    }

    actors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rank: u32, name: &str, is_self: bool, is_rival: bool) -> score_data::LeaderboardEntry {
        score_data::LeaderboardEntry {
            rank,
            name: name.to_string(),
            machine_tag: None,
            score: 10000.0 - rank as f64,
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

        let rows = scorebox_rows_for_kind(entries.as_slice(), PaneKind::Itl);
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
    fn itl_scorebox_uses_ex_score_color() {
        let entries = vec![
            entry(1, "world", false, false),
            entry(2, "self", true, false),
            entry(3, "rival", false, true),
        ];

        let rows = scorebox_rows_for_kind(entries.as_slice(), PaneKind::Itl);

        for row in rows.iter().take(3) {
            assert_eq!(row.score_color, color::JUDGMENT_RGBA[0]);
        }
    }

    #[test]
    fn entries_with_local_self_state_marks_matching_online_name_as_self() {
        let runtime = ScoreboxSideView {
            display_name: "Self Player".to_string(),
            player_initials: "SELF".to_string(),
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
    }

    #[test]
    fn select_music_view_uses_prepared_local_records() {
        let runtime = ScoreboxSideView {
            chart_hash: Some("chart".to_string()),
            player_initials: "P1".to_string(),
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

        let view = select_music_scorebox_view(&runtime, Some("chart"), false);

        assert_eq!(view.player_name, "P1");
        assert_eq!(view.player_score.as_ref(), "98.76%");
        assert_eq!(view.machine_name, "AAA");
        assert_eq!(view.machine_score.as_ref(), "99.99%");
        assert!(!view.show_rivals);

        let stale = select_music_scorebox_view(&runtime, Some("other"), false);
        assert_eq!(stale.player_name, "----");
        assert_eq!(stale.machine_name, "----");
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
        let prev = crate::config::get();
        crate::config::update_select_music_scorebox_cycle_itg(false);
        crate::config::update_select_music_scorebox_cycle_ex(false);
        crate::config::update_select_music_scorebox_cycle_hard_ex(true);
        crate::config::update_select_music_scorebox_cycle_tournaments(false);

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
        let panes = gameplay_panes_from_snapshot(&snapshot, &profile_snapshot);

        crate::config::update_select_music_scorebox_cycle_itg(prev.select_music_scorebox_cycle_itg);
        crate::config::update_select_music_scorebox_cycle_ex(prev.select_music_scorebox_cycle_ex);
        crate::config::update_select_music_scorebox_cycle_hard_ex(
            prev.select_music_scorebox_cycle_hard_ex,
        );
        crate::config::update_select_music_scorebox_cycle_tournaments(
            prev.select_music_scorebox_cycle_tournaments,
        );

        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].kind, PaneKind::HardEx);
        assert_eq!(panes[0].mode_text.as_ref(), "H.EX");
    }
}
