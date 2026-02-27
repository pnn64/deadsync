use crate::act;
use crate::assets;
use crate::game::{profile, scores};
use crate::ui::actors::Actor;
use crate::ui::color;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::thread::LocalKey;

const SCOREBOX_NUM_ENTRIES: usize = 5;
const SCOREBOX_LOOP_SECONDS: f32 = 5.0;
const SCOREBOX_TRANSITION_SECONDS: f32 = 1.0;
const SCOREBOX_W: f32 = 162.0;
const SCOREBOX_H: f32 = 80.0;
const SCOREBOX_BORDER: f32 = 5.0;
const SCOREBOX_GS_BLUE: [f32; 4] = color::rgba_hex("#007b85");
const SCOREBOX_RPG_YELLOW: [f32; 4] = [1.0, 0.972, 0.792, 1.0];
const SCOREBOX_ITL_PINK: [f32; 4] = [1.0, 0.2, 0.406, 1.0];
const SCOREBOX_SELF: [f32; 4] = color::rgba_hex("#A1FF94");
const SCOREBOX_RIVAL: [f32; 4] = color::rgba_hex("#C29CFF");
const SCOREBOX_MODE_ALPHA: f32 = 0.35;
const SCOREBOX_GS_LOGO_ALPHA: f32 = 0.5;
const SCOREBOX_EX_TEXT_ALPHA: f32 = 0.3;
const SCOREBOX_HARD_EX_TEXT_ALPHA: f32 = 0.32;
const SCOREBOX_ARROWCLOUD_LOGO_ALPHA: f32 = 0.5;
const SCOREBOX_ARROWCLOUD_LOGO_ZOOM: f32 = 0.06;
const SCOREBOX_RPG_LOGO_ALPHA: f32 = 0.5;
const SCOREBOX_ITL_LOGO_ALPHA: f32 = 0.2;
const SCOREBOX_LOGO_MAX_W_FRAC: f32 = 0.94;
const SCOREBOX_LOGO_MAX_H_FRAC: f32 = 0.94;
const SCOREBOX_HARD_EX_BORDER_TINT: f32 = 0.35;
const TEXT_CACHE_LIMIT: usize = 8192;

type TextCache<K> = HashMap<K, Arc<str>>;

thread_local! {
    static SCORE_PERCENT_TEXT_CACHE: RefCell<TextCache<u64>> = RefCell::new(HashMap::with_capacity(2048));
    static SCORE_VALUE_TEXT_CACHE: RefCell<TextCache<u64>> = RefCell::new(HashMap::with_capacity(2048));
    static RANK_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(512));
}

#[inline(always)]
fn cached_text<K, F>(cache: &'static LocalKey<RefCell<TextCache<K>>>, key: K, build: F) -> Arc<str>
where
    K: Copy + Eq + std::hash::Hash,
    F: FnOnce() -> String,
{
    cache.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(text) = cache.get(&key) {
            return text.clone();
        }
        let text: Arc<str> = Arc::<str>::from(build());
        if cache.len() < TEXT_CACHE_LIMIT {
            cache.insert(key, text.clone());
        }
        text
    })
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

#[inline(always)]
fn cached_percent_text(percent: f64) -> Arc<str> {
    let percent = if percent.is_finite() {
        percent.clamp(0.0, 100.0)
    } else {
        0.0
    };
    cached_text(&SCORE_PERCENT_TEXT_CACHE, percent.to_bits(), || {
        format!("{percent:.2}%")
    })
}

#[derive(Clone, Debug)]
struct GameplayScoreboxRow {
    rank: Arc<str>,
    name: String,
    score: Arc<str>,
    rank_color: [f32; 4],
    name_color: [f32; 4],
    score_color: [f32; 4],
}

#[derive(Clone, Debug)]
struct GameplayScoreboxPane {
    kind: PaneKind,
    mode_text: String,
    border_color: [f32; 4],
    rows: Vec<GameplayScoreboxRow>,
}

#[derive(Clone, Copy, Debug)]
struct ScoreboxCycleState {
    cur_idx: usize,
    next_idx: usize,
    border_mix: f32,
    cur_alpha: f32,
    next_alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneKind {
    Gs,
    Ex,
    HardEx,
    Rpg,
    Itl,
    Other,
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
fn error_text(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        "Timed Out".to_string()
    } else {
        "Failed to Load 😞".to_string()
    }
}

#[inline(always)]
fn pane_kind(pane: &scores::LeaderboardPane) -> PaneKind {
    if pane.is_arrowcloud() {
        return PaneKind::HardEx;
    }
    if pane.is_groovestats() {
        return if pane.is_ex {
            PaneKind::Ex
        } else {
            PaneKind::Gs
        };
    }
    let lower = pane.name.to_ascii_lowercase();
    if lower.contains("rpg") {
        PaneKind::Rpg
    } else if lower.contains("itl") {
        PaneKind::Itl
    } else if pane.is_ex {
        PaneKind::Ex
    } else {
        PaneKind::Other
    }
}

#[inline(always)]
fn pane_mode_text(kind: PaneKind, pane: &scores::LeaderboardPane) -> String {
    match kind {
        PaneKind::Gs => "ITG".to_string(),
        PaneKind::Ex => "EX".to_string(),
        PaneKind::HardEx => "H.EX".to_string(),
        PaneKind::Rpg => "RPG".to_string(),
        PaneKind::Itl => "ITL".to_string(),
        PaneKind::Other => pane.name.clone(),
    }
}

#[inline(always)]
fn pane_color(kind: PaneKind) -> [f32; 4] {
    match kind {
        PaneKind::Gs | PaneKind::Ex | PaneKind::Other => SCOREBOX_GS_BLUE,
        PaneKind::HardEx => [
            SCOREBOX_GS_BLUE[0]
                + (color::HARD_EX_SCORE_RGBA[0] - SCOREBOX_GS_BLUE[0]) * SCOREBOX_HARD_EX_BORDER_TINT,
            SCOREBOX_GS_BLUE[1]
                + (color::HARD_EX_SCORE_RGBA[1] - SCOREBOX_GS_BLUE[1]) * SCOREBOX_HARD_EX_BORDER_TINT,
            SCOREBOX_GS_BLUE[2]
                + (color::HARD_EX_SCORE_RGBA[2] - SCOREBOX_GS_BLUE[2]) * SCOREBOX_HARD_EX_BORDER_TINT,
            1.0,
        ],
        PaneKind::Rpg => SCOREBOX_RPG_YELLOW,
        PaneKind::Itl => SCOREBOX_ITL_PINK,
    }
}

#[inline(always)]
fn machine_tag(machine_tag: Option<&str>, name: &str) -> String {
    let src = machine_tag.unwrap_or(name).trim();
    if src.is_empty() {
        return "----".to_string();
    }
    let mut out = String::with_capacity(4);
    for ch in src.chars().take(4) {
        out.push(ch.to_ascii_uppercase());
    }
    out
}

#[inline(always)]
fn score_text_with_percent(score_10000: f64) -> Arc<str> {
    cached_percent_text(score_10000 / 100.0)
}

#[inline(always)]
fn score_text_without_percent(score_10000: f64) -> Arc<str> {
    let score = if score_10000.is_finite() {
        (score_10000 / 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    cached_text(&SCORE_VALUE_TEXT_CACHE, score.to_bits(), || {
        format!("{score:.2}")
    })
}

#[inline(always)]
fn rank_text(rank: u32) -> Arc<str> {
    cached_text(&RANK_TEXT_CACHE, rank, || format!("{rank}."))
}

fn preferred_primary_pane(
    panes: &[scores::LeaderboardPane],
    show_ex: bool,
) -> Option<&scores::LeaderboardPane> {
    let want = if show_ex { PaneKind::Ex } else { PaneKind::Gs };
    panes
        .iter()
        .find(|pane| pane_kind(pane) == want)
        .or_else(|| panes.iter().find(|pane| pane_kind(pane) == PaneKind::Gs))
        .or_else(|| panes.iter().find(|pane| pane_kind(pane) == PaneKind::Ex))
        .or_else(|| panes.first())
}

#[inline(always)]
fn default_mode_text_for_side(side: profile::PlayerSide) -> String {
    if profile::get_for_side(side).show_ex_score {
        "EX".to_string()
    } else {
        "ITG".to_string()
    }
}

pub fn select_music_scorebox_view(
    side: profile::PlayerSide,
    chart_hash: Option<&str>,
    fallback_machine: (String, Arc<str>),
    fallback_player: (String, Arc<str>),
) -> SelectMusicScoreboxView {
    let mut view = SelectMusicScoreboxView {
        mode_text: default_mode_text_for_side(side),
        machine_name: fallback_machine.0,
        machine_score: fallback_machine.1,
        player_name: fallback_player.0,
        player_score: fallback_player.1,
        rivals: std::array::from_fn(|_| ("----".to_string(), unknown_score_percent_text())),
        show_rivals: false,
        loading_text: None,
    };

    if !scores::is_gs_active_for_side(side) {
        return view;
    }
    view.machine_name = "----".to_string();
    view.machine_score = unknown_score_percent_text();
    view.player_name = "----".to_string();
    view.player_score = unknown_score_percent_text();
    view.show_rivals = true;

    let Some(hash) = chart_hash else {
        return view;
    };
    let Some(snapshot) =
        scores::get_or_fetch_player_leaderboards_for_side(hash, side, SCOREBOX_NUM_ENTRIES)
    else {
        return view;
    };

    if snapshot.loading {
        view.loading_text = Some("Loading ...".to_string());
        return view;
    }
    if let Some(error) = snapshot.error.as_deref() {
        view.loading_text = Some(error_text(error));
        return view;
    }

    let show_ex = profile::get_for_side(side).show_ex_score;
    let Some(data) = snapshot.data else {
        return view;
    };
    let Some(pane) = preferred_primary_pane(&data.panes, show_ex) else {
        view.loading_text = Some("No Scores".to_string());
        return view;
    };

    let kind = pane_kind(pane);
    view.mode_text = pane_mode_text(kind, pane);

    if let Some(world) = pane
        .entries
        .iter()
        .find(|entry| entry.rank == 1)
        .or_else(|| pane.entries.first())
    {
        view.machine_name = machine_tag(world.machine_tag.as_deref(), &world.name);
        view.machine_score = score_text_with_percent(world.score);
    }
    if let Some(player_entry) = pane.entries.iter().find(|entry| entry.is_self) {
        view.player_name = machine_tag(player_entry.machine_tag.as_deref(), &player_entry.name);
        view.player_score = score_text_with_percent(player_entry.score);
    }
    for (idx, rival) in pane
        .entries
        .iter()
        .filter(|entry| entry.is_rival)
        .take(3)
        .enumerate()
    {
        view.rivals[idx] = (
            machine_tag(rival.machine_tag.as_deref(), &rival.name),
            score_text_with_percent(rival.score),
        );
    }
    view
}

pub fn select_music_mode_text(side: profile::PlayerSide, chart_hash: Option<&str>) -> String {
    select_music_scorebox_view(
        side,
        chart_hash,
        ("----".to_string(), unknown_score_percent_text()),
        ("----".to_string(), unknown_score_percent_text()),
    )
    .mode_text
}

#[inline(always)]
fn gameplay_empty_row() -> GameplayScoreboxRow {
    GameplayScoreboxRow {
        rank: empty_text(),
        name: String::new(),
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
        name: text.to_string(),
        score: empty_text(),
        rank_color: [1.0; 4],
        name_color: [1.0; 4],
        score_color: [1.0; 4],
    }
}

#[inline(always)]
fn padded_rows(mut rows: Vec<GameplayScoreboxRow>) -> Vec<GameplayScoreboxRow> {
    while rows.len() < SCOREBOX_NUM_ENTRIES {
        rows.push(gameplay_empty_row());
    }
    rows
}

fn gameplay_status_pane(side: profile::PlayerSide, text: &str) -> GameplayScoreboxPane {
    let rows = padded_rows(vec![gameplay_status_row(text)]);
    let kind = if profile::get_for_side(side).show_ex_score {
        PaneKind::Ex
    } else {
        PaneKind::Gs
    };
    GameplayScoreboxPane {
        kind,
        mode_text: default_mode_text_for_side(side),
        border_color: SCOREBOX_GS_BLUE,
        rows,
    }
}

fn gameplay_row_from_entry(
    entry: &scores::LeaderboardEntry,
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
    } else if matches!(kind, PaneKind::Ex) {
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
        name: name.to_string(),
        score: score_text_without_percent(entry.score),
        rank_color,
        name_color,
        score_color,
    }
}

fn gameplay_pane_from_leaderboard(pane: &scores::LeaderboardPane) -> GameplayScoreboxPane {
    let kind = pane_kind(pane);
    let mode_text = pane_mode_text(kind, pane);
    let border_color = pane_color(kind);

    fn scorebox_rows_for_kind(
        entries: &[scores::LeaderboardEntry],
        kind: PaneKind,
    ) -> Vec<scores::LeaderboardEntry> {
        if !matches!(kind, PaneKind::HardEx) {
            return entries.iter().take(SCOREBOX_NUM_ENTRIES).cloned().collect();
        }

        let mut sorted = entries.to_vec();
        sorted.sort_by_key(|entry| entry.rank);
        let mut out: Vec<scores::LeaderboardEntry> = Vec::with_capacity(SCOREBOX_NUM_ENTRIES);

        // Always include world record/top row first.
        if let Some(top) = sorted.first().cloned() {
            out.push(top);
        }

        // Always include self if present.
        if let Some(self_entry) = sorted.iter().find(|entry| entry.is_self) {
            let already = out.iter().any(|e| {
                e.rank == self_entry.rank && e.name.eq_ignore_ascii_case(self_entry.name.as_str())
            });
            if !already && out.len() < SCOREBOX_NUM_ENTRIES {
                out.push(self_entry.clone());
            }
        }

        // Always include rivals when space permits.
        for rival in sorted.iter().filter(|entry| entry.is_rival) {
            let already = out.iter().any(|e| {
                e.rank == rival.rank && e.name.eq_ignore_ascii_case(rival.name.as_str())
            });
            if !already && out.len() < SCOREBOX_NUM_ENTRIES {
                out.push(rival.clone());
            }
        }

        // Fill remaining slots with best ranked leftover rows.
        for entry in &sorted {
            let already = out.iter().any(|e| {
                e.rank == entry.rank && e.name.eq_ignore_ascii_case(entry.name.as_str())
            });
            if !already {
                out.push(entry.clone());
            }
            if out.len() >= SCOREBOX_NUM_ENTRIES {
                break;
            }
        }

        out.sort_by_key(|entry| entry.rank);
        out
    }

    let mut rows = Vec::with_capacity(SCOREBOX_NUM_ENTRIES);
    if pane.entries.is_empty() {
        rows.push(gameplay_status_row("No Scores"));
    } else {
        let display_entries = scorebox_rows_for_kind(pane.entries.as_slice(), kind);
        for entry in &display_entries {
            rows.push(gameplay_row_from_entry(entry, kind));
        }
    }

    GameplayScoreboxPane {
        kind,
        mode_text,
        border_color,
        rows: padded_rows(rows),
    }
}

fn gameplay_panes_from_snapshot(
    snapshot: scores::CachedPlayerLeaderboardData,
    side: profile::PlayerSide,
) -> Vec<GameplayScoreboxPane> {
    if snapshot.loading {
        return vec![gameplay_status_pane(side, "Loading ...")];
    }
    if let Some(error) = snapshot.error.as_deref() {
        let text = error_text(error);
        return vec![gameplay_status_pane(side, &text)];
    }
    let Some(data) = snapshot.data else {
        return vec![gameplay_status_pane(side, "No Scores")];
    };
    if data.panes.is_empty() {
        return vec![gameplay_status_pane(side, "No Scores")];
    }

    let mut panes = Vec::with_capacity(data.panes.len());
    for pane in &data.panes {
        panes.push(gameplay_pane_from_leaderboard(pane));
    }
    panes
}

#[inline(always)]
fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    let t = clamp01(t);
    a + (b - a) * t
}

#[inline(always)]
fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
        lerp(a[3], b[3], t),
    ]
}

#[inline(always)]
fn color_with_alpha(mut rgba: [f32; 4], alpha: f32) -> [f32; 4] {
    rgba[3] *= clamp01(alpha);
    rgba
}

#[inline(always)]
fn is_gs_logo(kind: PaneKind) -> bool {
    matches!(kind, PaneKind::Gs | PaneKind::Ex)
}

#[inline(always)]
fn is_ex_text(kind: PaneKind) -> bool {
    matches!(kind, PaneKind::Ex)
}

fn is_arrowcloud_logo(kind: PaneKind) -> bool {
    matches!(kind, PaneKind::HardEx)
}

#[inline(always)]
fn is_hard_ex_text(kind: PaneKind) -> bool {
    matches!(kind, PaneKind::HardEx)
}

#[inline(always)]
fn is_rpg_logo(kind: PaneKind) -> bool {
    matches!(kind, PaneKind::Rpg)
}

#[inline(always)]
fn is_itl_logo(kind: PaneKind) -> bool {
    matches!(kind, PaneKind::Itl)
}

#[inline(always)]
fn is_fallback_text(kind: PaneKind) -> bool {
    matches!(kind, PaneKind::Other)
}

fn logo_alpha(
    cycle: ScoreboxCycleState,
    cur_on: bool,
    next_on: bool,
    target: f32,
    enter_in_second_half: bool,
) -> f32 {
    if cycle.cur_idx == cycle.next_idx {
        return if cur_on { target } else { 0.0 };
    }

    let t = cycle.border_mix;
    let start = if cur_on { target } else { 0.0 };
    if enter_in_second_half {
        if next_on {
            if t < 0.5 {
                start
            } else {
                lerp(start, target, (t - 0.5) * 2.0)
            }
        } else if t < 0.5 {
            lerp(start, 0.0, t * 2.0)
        } else {
            0.0
        }
    } else if next_on {
        if t < 0.5 {
            lerp(start, target, t * 2.0)
        } else {
            target
        }
    } else if t < 0.5 {
        start
    } else {
        lerp(start, 0.0, (t - 0.5) * 2.0)
    }
}

fn scorebox_cycle_state(num_panes: usize, elapsed_seconds: f32) -> ScoreboxCycleState {
    if num_panes <= 1 {
        return ScoreboxCycleState {
            cur_idx: 0,
            next_idx: 0,
            border_mix: 0.0,
            cur_alpha: 1.0,
            next_alpha: 0.0,
        };
    }

    let cycle_len = SCOREBOX_LOOP_SECONDS + SCOREBOX_TRANSITION_SECONDS;
    let elapsed = elapsed_seconds.max(0.0);
    let cycle_num = (elapsed / cycle_len).floor() as usize;
    let cycle_pos = elapsed - (cycle_num as f32) * cycle_len;
    let cur_idx = cycle_num % num_panes;

    if cycle_pos < SCOREBOX_LOOP_SECONDS {
        return ScoreboxCycleState {
            cur_idx,
            next_idx: cur_idx,
            border_mix: 0.0,
            cur_alpha: 1.0,
            next_alpha: 0.0,
        };
    }

    let next_idx = (cur_idx + 1) % num_panes;
    let t = clamp01((cycle_pos - SCOREBOX_LOOP_SECONDS) / SCOREBOX_TRANSITION_SECONDS);
    let (cur_alpha, next_alpha) = if t < 0.5 {
        (1.0 - t * 2.0, 0.0)
    } else {
        (0.0, (t - 0.5) * 2.0)
    };

    ScoreboxCycleState {
        cur_idx,
        next_idx,
        border_mix: t,
        cur_alpha,
        next_alpha,
    }
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
        settext(text):
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
    let mut width = dims.w.max(1) as f32 * sprite_zoom * zoom;
    let mut height = dims.h.max(1) as f32 * sprite_zoom * zoom;
    let max_width = SCOREBOX_W * SCOREBOX_LOGO_MAX_W_FRAC * zoom;
    let max_height = SCOREBOX_H * SCOREBOX_LOGO_MAX_H_FRAC * zoom;
    if width > 0.0 && height > 0.0 {
        let fit = (max_width / width).min(max_height / height).min(1.0);
        width *= fit;
        height *= fit;
    }
    let c = color_with_alpha([1.0; 4], alpha);
    actors.push(act!(sprite(texture):
        align(0.5, 0.5):
        xy(center_x, center_y):
        setsize(width, height):
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
        settext(text):
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
    if is_fallback_text(cur.kind) {
        push_mode_text(
            actors,
            cur.mode_text.as_str(),
            center_x,
            center_y,
            zoom,
            z_base,
            cycle.cur_alpha,
        );
    }
    if cycle.next_idx != cycle.cur_idx && is_fallback_text(next.kind) {
        push_mode_text(
            actors,
            next.mode_text.as_str(),
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
    cur: PaneKind,
    next: PaneKind,
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
    cur: PaneKind,
    next: PaneKind,
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
    cur: PaneKind,
    next: PaneKind,
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
    cur: PaneKind,
    next: PaneKind,
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

fn push_rpg_logo_overlay(
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
        is_rpg_logo(cur),
        is_rpg_logo(next),
        SCOREBOX_RPG_LOGO_ALPHA,
        false,
    );
    push_centered_logo(
        actors,
        "srpg9_logo_alt.png",
        center_x,
        center_y,
        zoom,
        0.07,
        z_base,
        alpha,
    );
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
    push_gs_logo_overlay(
        actors, cycle, cur.kind, next.kind, center_x, center_y, zoom, z_base,
    );
    push_arrowcloud_logo_overlay(
        actors, cycle, cur.kind, next.kind, center_x, center_y, zoom, z_base,
    );
    push_ex_header_overlay(
        actors, cycle, cur.kind, next.kind, center_x, center_y, zoom, z_base,
    );
    push_hard_ex_header_overlay(
        actors, cycle, cur.kind, next.kind, center_x, center_y, zoom, z_base,
    );
    push_rpg_logo_overlay(
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
        zoom(0.87 * zoom):
        maxwidth(30.0):
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
            settext(row.name.as_str()):
            align(0.0, 0.5):
            xy(name_x, y):
            zoom(0.87 * zoom):
            maxwidth(100.0):
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

pub fn gameplay_scorebox_actors(
    side: profile::PlayerSide,
    chart_hash: Option<&str>,
    show_scorebox: bool,
    center_x: f32,
    center_y: f32,
    zoom: f32,
    elapsed_seconds: f32,
) -> Vec<Actor> {
    if !show_scorebox || !scores::is_gs_active_for_side(side) {
        return Vec::new();
    }
    let Some(hash) = chart_hash else {
        return Vec::new();
    };
    let Some(snapshot) =
        scores::get_or_fetch_player_leaderboards_for_side(hash, side, SCOREBOX_NUM_ENTRIES)
    else {
        return Vec::new();
    };
    let panes = gameplay_panes_from_snapshot(snapshot, side);
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
