use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::sprite_sheet_dims;
use crate::engine::gfx::{
    BlendMode, INVALID_TMESH_CACHE_KEY, MeshMode, MeshVertex, TexturedMeshVertex,
};
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::present::actors::{Actor, SizeSpec, TextAlign, TextContent};
use crate::engine::present::anim::EffectState;
use crate::engine::present::cache::{TextCache, cached_text};
use crate::engine::present::color;
use crate::engine::present::compose::TextLayoutCache;
use crate::engine::present::density::{self, DensityHistCache};
use crate::engine::present::font;
use crate::engine::space::widescale;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::chart::{ChartData, GameplayChartData};
use crate::game::parsing::song_lua::{
    SongLuaCapturedActor, SongLuaOverlayActor, SongLuaOverlayBlendMode, SongLuaOverlayCommandBlock,
    SongLuaOverlayKind, SongLuaOverlayMessageCommand, SongLuaOverlayState,
    SongLuaOverlayStateDelta, SongLuaProxyTarget,
};
use crate::game::{profile, scroll::ScrollSpeedSetting, song::SongData};
use crate::screens::components::gameplay::{gameplay_stats, notefield};
use crate::screens::components::shared::banner as shared_banner;
use crate::screens::components::shared::lobby_hud;
use crate::screens::components::shared::screen_bar::{self, AvatarParams, ScreenBarParams};
use crate::screens::{Screen, ScreenAction};
use glam::{Mat4 as Matrix4, Vec3 as Vector3, Vec4 as Vector4};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

const TEXT_CACHE_LIMIT: usize = 8192;
const INTRO_TEXT_SETTLE_SECONDS: f32 = 1.49; // 0.5 + 0.66 + 0.33 (SL OnCommand chain)

use crate::game::gameplay::{
    self as gameplay_core, CourseDisplayCarry, CourseDisplayTotals, GameplayAction, GameplayExit,
    LeadInTiming, MAX_PLAYERS, ReplayInputEdge, ReplayOffsetSnapshot, TRANSITION_IN_DURATION,
    TRANSITION_OUT_DELAY, TRANSITION_OUT_DURATION, TRANSITION_OUT_FADE_DURATION,
    effective_visibility_effects_for_player, handle_input as gameplay_handle_input,
    timing_tick_status_line, toggle_flash_text, update as gameplay_update,
};

pub struct DensityGraphRenderState {
    pub cache: [Option<DensityHistCache>; MAX_PLAYERS],
    pub mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub mesh_offset_px: [i32; MAX_PLAYERS],
    pub life_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub life_mesh_offset_px: [i32; MAX_PLAYERS],
    pub top_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
}

impl DensityGraphRenderState {
    fn from_gameplay(state: &gameplay_core::State) -> Self {
        let top_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] = std::array::from_fn(|player| {
            let graph_w = state.density_graph_top_w[player];
            let graph_h =
                state.density_graph_top_h * state.density_graph_top_scale_y[player].clamp(0.0, 1.0);
            if player >= state.num_players || graph_w <= 0.0 || graph_h <= 0.0 {
                return None;
            }

            let chart = state.charts[player].as_ref();
            let verts = density::build_density_histogram_mesh(
                &chart.measure_nps_vec,
                chart.max_nps,
                &chart.measure_seconds_vec,
                state.density_graph_first_second,
                state.density_graph_last_second,
                graph_w,
                graph_h,
                0.0,
                graph_w,
                None,
                1.0,
            );
            if verts.is_empty() {
                None
            } else {
                Some(Arc::from(verts.into_boxed_slice()))
            }
        });

        let cache: [Option<DensityHistCache>; MAX_PLAYERS] = std::array::from_fn(|player| {
            if player >= state.num_players
                || state.density_graph_graph_w <= 0.0
                || state.density_graph_graph_h <= 0.0
            {
                return None;
            }

            let chart = state.charts[player].as_ref();
            density::build_density_histogram_cache(
                &chart.measure_nps_vec,
                chart.max_nps,
                &chart.measure_seconds_vec,
                state.density_graph_first_second,
                state.density_graph_last_second,
                state.density_graph_scaled_width,
                state.density_graph_graph_h,
                None,
                1.0,
            )
        });

        let mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] = std::array::from_fn(|player| {
            if player >= state.num_players || cache[player].is_none() {
                return None;
            }
            let mut mesh = None;
            density::update_density_hist_mesh(
                &mut mesh,
                cache[player].as_ref(),
                0.0,
                state.density_graph_graph_w,
            );
            mesh
        });

        Self {
            cache,
            mesh,
            mesh_offset_px: [0; MAX_PLAYERS],
            life_mesh: std::array::from_fn(|_| None),
            life_mesh_offset_px: [0; MAX_PLAYERS],
            top_mesh,
        }
    }
}

pub struct State {
    pub(crate) gameplay: gameplay_core::State,
    pub density_graph: DensityGraphRenderState,
}

impl State {
    pub fn from_gameplay(gameplay: gameplay_core::State) -> Self {
        let density_graph = DensityGraphRenderState::from_gameplay(&gameplay);
        Self {
            gameplay,
            density_graph,
        }
    }
}

impl Deref for State {
    type Target = gameplay_core::State;

    fn deref(&self) -> &Self::Target {
        &self.gameplay
    }
}

impl DerefMut for State {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.gameplay
    }
}

pub fn init(
    song: Arc<SongData>,
    charts: [Arc<ChartData>; MAX_PLAYERS],
    gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS],
    active_color_index: i32,
    music_rate: f32,
    scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
    player_profiles: [profile::Profile; MAX_PLAYERS],
    replay_edges: Option<Vec<ReplayInputEdge>>,
    replay_offsets: Option<ReplayOffsetSnapshot>,
    replay_status_text: Option<Arc<str>>,
    stage_intro_text: Arc<str>,
    lead_in_timing: Option<LeadInTiming>,
    course_display_carry: Option<[CourseDisplayCarry; MAX_PLAYERS]>,
    course_display_totals: Option<[CourseDisplayTotals; MAX_PLAYERS]>,
    combo_carry: [u32; MAX_PLAYERS],
) -> State {
    State::from_gameplay(gameplay_core::init(
        song,
        charts,
        gameplay_charts,
        active_color_index,
        music_rate,
        scroll_speed,
        player_profiles,
        replay_edges,
        replay_offsets,
        replay_status_text,
        stage_intro_text,
        lead_in_timing,
        course_display_carry,
        course_display_totals,
        combo_carry,
    ))
}

#[inline(always)]
const fn screen_for_exit(exit: GameplayExit) -> Screen {
    match exit {
        GameplayExit::Complete => Screen::Evaluation,
        GameplayExit::Cancel => Screen::SelectMusic,
    }
}

#[inline(always)]
const fn map_gameplay_action(action: GameplayAction) -> ScreenAction {
    match action {
        GameplayAction::None => ScreenAction::None,
        GameplayAction::Navigate(exit) => ScreenAction::Navigate(screen_for_exit(exit)),
        GameplayAction::NavigateNoFade(exit) => ScreenAction::NavigateNoFade(screen_for_exit(exit)),
    }
}

fn local_lobby_side_is_active(side: profile::PlayerSide) -> bool {
    let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    if !(p1_joined || p2_joined) {
        return profile::get_session_player_side() == side;
    }
    match side {
        profile::PlayerSide::P1 => p1_joined,
        profile::PlayerSide::P2 => p2_joined,
    }
}

fn gameplay_player_index_for_side(state: &State, side: profile::PlayerSide) -> Option<usize> {
    if state.num_players >= 2 {
        return Some(match side {
            profile::PlayerSide::P1 => 0,
            profile::PlayerSide::P2 => 1,
        });
    }
    if state.num_players == 0 || profile::get_session_player_side() != side {
        return None;
    }
    Some(0)
}

fn gameplay_lobby_player_stats(
    state: &State,
    side: profile::PlayerSide,
) -> Option<crate::game::online::lobbies::MachinePlayerStats> {
    let player_idx = gameplay_player_index_for_side(state, side)?;
    let ex_data = crate::game::gameplay::display_ex_score_data(state, player_idx);
    let judgments = crate::game::online::lobbies::LobbyJudgments {
        fantastic_plus: ex_data.counts.w0,
        fantastics: ex_data.counts.w1,
        excellents: ex_data.counts.w2,
        greats: ex_data.counts.w3,
        decents: ex_data.counts.w4,
        way_offs: ex_data.counts.w5,
        misses: ex_data.counts.miss,
        total_steps: ex_data.total_steps,
        mines_hit: ex_data.mines_hit,
        total_mines: ex_data.mines_total,
        holds_held: ex_data.holds_held,
        total_holds: ex_data.holds_total,
        rolls_held: ex_data.rolls_held,
        total_rolls: ex_data.rolls_total,
    };
    Some(crate::game::online::lobbies::MachinePlayerStats {
        judgments: Some(judgments),
        score: Some(
            (crate::game::gameplay::display_itg_score_percent(state, player_idx) * 100.0) as f32,
        ),
        ex_score: Some(crate::game::gameplay::display_ex_score_percent(state, player_idx) as f32),
    })
}

fn local_lobby_ready_tuple(state: &State) -> (bool, bool) {
    (
        local_lobby_side_is_active(profile::PlayerSide::P1) && state.lobby_ready_p1,
        local_lobby_side_is_active(profile::PlayerSide::P2) && state.lobby_ready_p2,
    )
}

fn local_lobby_players_ready(state: &State) -> bool {
    let (p1_ready, p2_ready) = local_lobby_ready_tuple(state);
    let mut any_active = false;
    let mut all_ready = true;
    if local_lobby_side_is_active(profile::PlayerSide::P1) {
        any_active = true;
        all_ready &= p1_ready;
    }
    if local_lobby_side_is_active(profile::PlayerSide::P2) {
        any_active = true;
        all_ready &= p2_ready;
    }
    any_active && all_ready
}

fn set_all_local_lobby_players_ready(state: &mut State, ready: bool) {
    state.lobby_ready_p1 = local_lobby_side_is_active(profile::PlayerSide::P1) && ready;
    state.lobby_ready_p2 = local_lobby_side_is_active(profile::PlayerSide::P2) && ready;
}

fn set_local_lobby_player_ready(state: &mut State, side: profile::PlayerSide) {
    match side {
        profile::PlayerSide::P1 if local_lobby_side_is_active(profile::PlayerSide::P1) => {
            state.lobby_ready_p1 = true;
        }
        profile::PlayerSide::P2 if local_lobby_side_is_active(profile::PlayerSide::P2) => {
            state.lobby_ready_p2 = true;
        }
        _ => {}
    }
}

fn clear_lobby_disconnect_holds(state: &mut State) {
    state.lobby_disconnect_hold_p1 = None;
    state.lobby_disconnect_hold_p2 = None;
}

fn set_lobby_disconnect_hold(
    state: &mut State,
    side: profile::PlayerSide,
    started_at: Option<Instant>,
) {
    match side {
        profile::PlayerSide::P1 if local_lobby_side_is_active(profile::PlayerSide::P1) => {
            state.lobby_disconnect_hold_p1 = started_at;
        }
        profile::PlayerSide::P2 if local_lobby_side_is_active(profile::PlayerSide::P2) => {
            state.lobby_disconnect_hold_p2 = started_at;
        }
        _ => {}
    }
}

fn lobby_disconnect_hold_elapsed(state: &State) -> Option<f32> {
    [
        state.lobby_disconnect_hold_p1,
        state.lobby_disconnect_hold_p2,
    ]
    .into_iter()
    .flatten()
    .map(|started_at| started_at.elapsed().as_secs_f32())
    .max_by(f32::total_cmp)
}

fn lobby_player_on_screen(
    player: &crate::game::online::lobbies::LobbyPlayer,
    screen_name: &str,
) -> bool {
    player.screen_name.eq_ignore_ascii_case(screen_name)
}

fn gameplay_requires_lobby_wait_for(
    joined: Option<&crate::game::online::lobbies::JoinedLobby>,
) -> bool {
    joined.is_some()
}

fn gameplay_requires_lobby_wait() -> bool {
    let snapshot = crate::game::online::lobbies::snapshot();
    gameplay_requires_lobby_wait_for(snapshot.joined_lobby.as_ref())
}

fn gameplay_lobby_wait_text_for(
    joined: &crate::game::online::lobbies::JoinedLobby,
    local_players_ready: bool,
    reconnect_status_text: Option<&str>,
) -> Option<String> {
    if let Some(text) = reconnect_status_text {
        return Some(text.to_string());
    }

    let all_in_gameplay = !joined.players.is_empty()
        && joined
            .players
            .iter()
            .all(|player| lobby_player_on_screen(player, "ScreenGameplay"));
    let all_ready = !joined.players.is_empty() && joined.players.iter().all(|player| player.ready);
    if all_in_gameplay && all_ready {
        return None;
    }

    let mut message = if all_in_gameplay {
        tr("Lobby", "WaitingForReadyUp").to_string()
    } else {
        tr("Lobby", "WaitingForSync").to_string()
    };
    if !local_players_ready {
        message.push('\n');
        message.push_str(&tr("Gameplay", "PressStartToReadyUp"));
    }
    Some(message)
}

fn gameplay_lobby_wait_text(state: &State) -> Option<String> {
    if state.lobby_music_started {
        return None;
    }

    let snapshot = crate::game::online::lobbies::snapshot();
    let joined = snapshot.joined_lobby.as_ref()?;
    let reconnect_status_text = crate::game::online::lobbies::reconnect_status_text();
    gameplay_lobby_wait_text_for(
        joined,
        local_lobby_players_ready(state),
        reconnect_status_text.as_deref(),
    )
}

fn gameplay_lobby_disconnect_prompt(state: &State) -> Option<String> {
    gameplay_lobby_wait_text(state)?;
    let Some(elapsed) = lobby_disconnect_hold_elapsed(state) else {
        return Some(tr("Lobby", "DisconnectBasicPrompt").to_string());
    };
    let remaining =
        (crate::game::online::lobbies::LOBBY_DISCONNECT_HOLD_SECONDS - elapsed).ceil() as i32;
    let remaining = remaining.max(0);
    Some(
        tr_fmt(
            "Lobby",
            "DisconnectHoldingFormat",
            &[
                ("remaining", &remaining.to_string()),
                ("s", if remaining == 1 { "" } else { "s" }),
            ],
        )
        .to_string(),
    )
}

fn gameplay_lobby_hud_status_text(state: &State) -> Option<String> {
    let mut text = gameplay_lobby_wait_text(state)?;
    if let Some(prompt) = gameplay_lobby_disconnect_prompt(state) {
        text.push('\n');
        text.push_str(prompt.as_str());
    }
    Some(text)
}

pub fn on_enter(state: &mut State) {
    state.lobby_music_started = false;
    set_all_local_lobby_players_ready(state, false);
    clear_lobby_disconnect_holds(state);
    if gameplay_requires_lobby_wait() {
        return;
    }

    set_all_local_lobby_players_ready(state, true);
    crate::game::gameplay::start_stage_music(state);
    state.lobby_music_started = true;
}

pub fn update(state: &mut State, delta_time: f32) -> ScreenAction {
    crate::game::online::lobbies::poll_reconnect();

    if !state.lobby_music_started {
        if lobby_disconnect_hold_elapsed(state).is_some_and(|elapsed| {
            elapsed >= crate::game::online::lobbies::LOBBY_DISCONNECT_HOLD_SECONDS
        }) {
            clear_lobby_disconnect_holds(state);
            crate::game::online::lobbies::disconnect();
        }

        let (p1_ready, p2_ready) = local_lobby_ready_tuple(state);
        crate::game::online::lobbies::update_machine_state_sides_with_stats(
            "ScreenGameplay",
            p1_ready,
            p2_ready,
            gameplay_lobby_player_stats(state, profile::PlayerSide::P1),
            gameplay_lobby_player_stats(state, profile::PlayerSide::P2),
        );

        if gameplay_lobby_wait_text(state).is_some() {
            return ScreenAction::None;
        }

        clear_lobby_disconnect_holds(state);
        set_all_local_lobby_players_ready(state, true);
        crate::game::gameplay::start_stage_music(state);
        state.lobby_music_started = true;
    }
    let (p1_ready, p2_ready) = local_lobby_ready_tuple(state);
    crate::game::online::lobbies::update_machine_state_sides_with_stats(
        "ScreenGameplay",
        p1_ready,
        p2_ready,
        gameplay_lobby_player_stats(state, profile::PlayerSide::P1),
        gameplay_lobby_player_stats(state, profile::PlayerSide::P2),
    );
    map_gameplay_action(gameplay_update(state, delta_time))
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if gameplay_lobby_wait_text(state).is_some() {
        match ev.action {
            VirtualAction::p1_start => {
                if ev.pressed {
                    set_local_lobby_player_ready(state, profile::PlayerSide::P1);
                    set_lobby_disconnect_hold(state, profile::PlayerSide::P1, Some(ev.timestamp));
                } else {
                    set_lobby_disconnect_hold(state, profile::PlayerSide::P1, None);
                }
            }
            VirtualAction::p2_start => {
                if ev.pressed {
                    set_local_lobby_player_ready(state, profile::PlayerSide::P2);
                    set_lobby_disconnect_hold(state, profile::PlayerSide::P2, Some(ev.timestamp));
                } else {
                    set_lobby_disconnect_hold(state, profile::PlayerSide::P2, None);
                }
            }
            _ => {}
        }
        return ScreenAction::None;
    }
    map_gameplay_action(gameplay_handle_input(state, ev))
}

thread_local! {
    static SCORE_2DP_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(1024));
    static RATE_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(128));
    static BPM_TEXT_CACHE: RefCell<TextCache<(u64, bool)>> = RefCell::new(HashMap::with_capacity(512));
    static LIFE_PERCENT_TEXT_CACHE: RefCell<TextCache<u32>> =
        RefCell::new(HashMap::with_capacity(1024));
    static METER_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(64));
    static AUTOSYNC_TEXT_CACHE: RefCell<TextCache<AutosyncTextKey>> =
        RefCell::new(HashMap::with_capacity(256));
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct AutosyncTextKey {
    mode: u8,
    old_offset_bits: u32,
    new_offset_bits: u32,
    stddev_bits: u32,
    sample_count: u16,
}

#[inline(always)]
fn empty_text() -> Arc<str> {
    static EMPTY: OnceLock<Arc<str>> = OnceLock::new();
    EMPTY.get_or_init(|| Arc::<str>::from("")).clone()
}

#[inline(always)]
fn quantize_centi_u32(value: f64) -> u32 {
    let value = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    ((value * 100.0).round()).clamp(0.0, u32::MAX as f64) as u32
}

#[inline(always)]
fn quantize_tenths_u32(value: f32) -> u32 {
    let value = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    ((value * 10.0).round()).clamp(0.0, u32::MAX as f32) as u32
}

#[inline(always)]
fn cached_score_2dp(value: f64) -> Arc<str> {
    let key = quantize_centi_u32(value);
    cached_text(&SCORE_2DP_CACHE, key, TEXT_CACHE_LIMIT, || {
        format!("{:.2}", key as f64 / 100.0)
    })
}

#[inline(always)]
fn cached_rate_text(rate: f32) -> Arc<str> {
    if (rate - 1.0).abs() <= 0.001 {
        return empty_text();
    }
    cached_text(&RATE_TEXT_CACHE, rate.to_bits(), TEXT_CACHE_LIMIT, || {
        tr_fmt(
            "Gameplay",
            "RateDisplay",
            &[("rate", &format!("{rate:.2}"))],
        )
        .to_string()
    })
}

#[inline(always)]
fn cached_bpm_text(bpm: f64, show_decimal: bool) -> Arc<str> {
    if !bpm.is_finite() {
        return Arc::<str>::from("0");
    }
    if !show_decimal {
        let rounded = bpm.round().max(0.0);
        let key = (rounded.to_bits(), false);
        return cached_text(&BPM_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
            format!("{rounded:.0}")
        });
    }
    let rounded_tenth = (bpm * 10.0).round() / 10.0;
    let rounded_tenth = rounded_tenth.max(0.0);
    let key = (rounded_tenth.to_bits(), true);
    cached_text(&BPM_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        let nearest_int = rounded_tenth.round();
        if (rounded_tenth - nearest_int).abs() <= 0.001 {
            format!("{nearest_int:.0}")
        } else {
            format!("{rounded_tenth:.1}")
        }
    })
}

#[inline(always)]
fn cached_life_percent_text(life_percent: f32) -> Arc<str> {
    let key = quantize_tenths_u32(life_percent);
    cached_text(&LIFE_PERCENT_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        format!("{:.1}%", key as f32 / 10.0)
    })
}

#[inline(always)]
fn cached_meter_text(meter: u32) -> Arc<str> {
    cached_text(&METER_TEXT_CACHE, meter, TEXT_CACHE_LIMIT, || {
        meter.to_string()
    })
}

fn sync_overlay_text(state: &State) -> Option<(Arc<str>, usize)> {
    let mut lines = [""; 4];
    let mut line_count = 0usize;
    let mut total_len = 0usize;
    if state.autoplay_enabled {
        let line = state.replay_status_text.as_deref().unwrap_or("AutoPlay");
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if let Some(line) = timing_tick_status_line(state) {
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if let Some(line) = crate::game::gameplay::autosync_mode_status_line(state.autosync_mode) {
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if let Some(line) = state.sync_overlay_message.as_deref() {
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if line_count == 0 {
        return None;
    }
    // Do not cache this string by pointer identity. `sync_overlay_message` is rebuilt
    // during live offset tweaks, and allocator address reuse can otherwise return a
    // stale overlay line with the wrong numbers.
    let mut out = String::with_capacity(total_len + line_count.saturating_sub(1));
    out.push_str(lines[0]);
    for line in &lines[1..line_count] {
        out.push('\n');
        out.push_str(line);
    }
    Some((Arc::<str>::from(out), line_count))
}

#[inline(always)]
fn cached_autosync_text(state: &State, old_offset: f32, new_offset: f32) -> Arc<str> {
    let key = AutosyncTextKey {
        mode: state.autosync_mode as u8,
        old_offset_bits: old_offset.to_bits(),
        new_offset_bits: new_offset.to_bits(),
        stddev_bits: state.autosync_standard_deviation.to_bits(),
        sample_count: state.autosync_offset_sample_count.min(u16::MAX as usize) as u16,
    };
    cached_text(&AUTOSYNC_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        let collecting_sample = state
            .autosync_offset_sample_count
            .saturating_add(1)
            .min(crate::game::gameplay::AUTOSYNC_OFFSET_SAMPLE_COUNT);
        format!(
            "Old offset: {old_offset:0.3}\nNew offset: {new_offset:0.3}\nStandard deviation: {stddev:0.3}\nCollecting sample: {collecting_sample} / {max_samples}",
            stddev = state.autosync_standard_deviation,
            max_samples = crate::game::gameplay::AUTOSYNC_OFFSET_SAMPLE_COUNT,
        )
    })
}

pub fn prewarm_text_layout(
    cache: &mut TextLayoutCache,
    fonts: &HashMap<&'static str, font::Font>,
    state: &State,
) {
    let cfg = crate::config::get();
    for centi in 0..=10_000 {
        let text = cached_score_2dp(centi as f64 / 100.0);
        cache.prewarm_text(fonts, "wendy_monospace_numbers", text.as_ref(), None);
    }
    for tenths in 0..=1_000 {
        let text = cached_life_percent_text(tenths as f32 / 10.0);
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
    for player in 0..state.num_players {
        let chart = &state.charts[player];
        let meter_text = cached_meter_text(chart.meter);
        cache.prewarm_text(fonts, "wendy", meter_text.as_ref(), None);
        let detail = color::difficulty_display_name_for_song(
            &chart.difficulty,
            &state.song.title,
            cfg.zmod_rating_box_text,
        );
        cache.prewarm_text(fonts, "miso", detail, None);
        for &(_, bpm) in &state.gameplay_charts[player].timing_segments.bpms {
            let text = cached_bpm_text(
                f64::from(bpm.max(0.0)) * f64::from(state.music_rate),
                cfg.show_bpm_decimal,
            );
            cache.prewarm_text(fonts, "miso", text.as_ref(), None);
        }
    }
    cache.prewarm_text(fonts, "miso", "Assist Tick", None);
    cache.prewarm_text(fonts, "miso", "Hit Tick", None);
    cache.prewarm_text(fonts, "miso", "AutoSync Song", None);
    cache.prewarm_text(fonts, "miso", "AutoSync Machine", None);
    cache.prewarm_text(
        fonts,
        "miso",
        &tr("Gameplay", "ContinueHoldingStartGiveUp"),
        None,
    );
    cache.prewarm_text(
        fonts,
        "miso",
        &tr("Gameplay", "ContinueHoldingBackGiveUp"),
        None,
    );
    cache.prewarm_text(fonts, "miso", &tr("Lobby", "DisconnectBasicPrompt"), None);
    cache.prewarm_text(fonts, "miso", &tr("Gameplay", "DontGoBack"), None);
    if let Some(text) = state.replay_status_text.as_ref() {
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
    if let Some(text) = state.sync_overlay_message.as_ref() {
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
    if state.autosync_mode != crate::game::gameplay::AutosyncMode::Off {
        let (old_offset, new_offset) =
            if state.autosync_mode == crate::game::gameplay::AutosyncMode::Machine {
                (
                    state.initial_global_offset_seconds,
                    state.global_offset_seconds,
                )
            } else {
                (state.initial_song_offset_seconds, state.song_offset_seconds)
            };
        let text = cached_autosync_text(state, old_offset, new_offset);
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
}

// --- TRANSITIONS ---
pub fn in_transition(state: Option<&State>) -> (Vec<Actor>, f32) {
    let text = state
        .map(|gs| gs.stage_intro_text.clone())
        .unwrap_or_else(|| Arc::from("EVENT"));
    let intro_color = state.map_or(color::decorative_rgba(0), |gs| gs.player_color);
    let mut mirrored_splode = act!(sprite("gameplayin_splode.png"):
        align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
        diffuse(intro_color[0], intro_color[1], intro_color[2], 0.8):
        rotationz(-10.0): zoom(0.0):
        z(1101):
        sleep(0.4):
        decelerate(0.6): rotationz(0.0): zoom(1.3): alpha(0.0)
    );
    if let Actor::Sprite { flip_x, .. } = &mut mirrored_splode {
        // Simply Love uses rotationy(180) here; in deadsync 2D parity this is horizontal mirroring.
        *flip_x = true;
    }

    let actors = vec![
        act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(1100):
            sleep(1.4):
            accelerate(0.6): alpha(0.0):
            linear(0.0): visible(false)
        ),
        act!(sprite("gameplayin_splode.png"):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            diffuse(intro_color[0], intro_color[1], intro_color[2], 0.9):
            rotationz(10.0): zoom(0.0):
            z(1101):
            sleep(0.4):
            linear(0.6): rotationz(0.0): zoom(1.1): alpha(0.0)
        ),
        mirrored_splode,
        act!(sprite("gameplayin_minisplode.png"):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            diffuse(intro_color[0], intro_color[1], intro_color[2], 1.0):
            rotationz(10.0): zoom(0.0):
            z(1101):
            sleep(0.4):
            decelerate(0.8): rotationz(0.0): zoom(0.9): alpha(0.0)
        ),
        act!(text:
            font("wendy"): settext(text):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, 0.0):
            z(1102):
            accelerate(0.5): alpha(1.0):
            sleep(0.66):
            accelerate(0.33): zoom(0.4): y(screen_height() - 30.0):
            sleep((TRANSITION_IN_DURATION - INTRO_TEXT_SETTLE_SECONDS).max(0.0))
        ),
    ];
    (actors, TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        sleep(TRANSITION_OUT_DELAY):
        linear(TRANSITION_OUT_FADE_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

// --- DRAWING ---

fn build_background(state: &State, bg_brightness: f32) -> Actor {
    let sw = screen_width();
    let sh = screen_height();
    let bg_brightness = bg_brightness.clamp(0.0, 1.0);
    let mut actor = shared_banner::cover_sprite(
        state.background_texture_key.clone(),
        screen_center_x(),
        screen_center_y(),
        sw,
        sh,
        1.0,
        -100,
    );
    if let Actor::Sprite { tint, .. } = &mut actor {
        *tint = [bg_brightness, bg_brightness, bg_brightness, 1.0];
    }
    actor
}

fn song_lua_has_visible_tex(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    path: &Path,
) -> bool {
    overlays.iter().zip(overlay_states).any(|(overlay, state)| {
        matches!(
            &overlay.kind,
            SongLuaOverlayKind::Sprite { texture_path } if texture_path.as_path() == path
        ) && state.visible
            && state.diffuse[3] > f32::EPSILON
    })
}

fn song_lua_owns_fg_media(
    state: &State,
    overlay_states: &[SongLuaOverlayState],
    path: &Path,
) -> bool {
    if song_lua_has_visible_tex(&state.song_lua_overlays, overlay_states, path) {
        return true;
    }
    for layer in &state.song_lua_background_visual_layers {
        if state.current_music_time_display < layer.start_second {
            continue;
        }
        let layer_states = song_lua_overlay_states_from(
            state.current_music_time_display,
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
        );
        if song_lua_has_visible_tex(&layer.overlays, &layer_states, path) {
            return true;
        }
    }
    for layer in &state.song_lua_foreground_visual_layers {
        if state.current_music_time_display < layer.start_second {
            continue;
        }
        let layer_states = song_lua_overlay_states_from(
            state.current_music_time_display,
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
        );
        if song_lua_has_visible_tex(&layer.overlays, &layer_states, path) {
            return true;
        }
    }
    false
}

fn build_foreground_media(state: &State, overlay_states: &[SongLuaOverlayState]) -> Option<Actor> {
    let path = state.song.active_foreground_path(state.current_beat)?;
    if song_lua_owns_fg_media(state, overlay_states, path) {
        return None;
    }
    Some(shared_banner::cover_sprite(
        path.to_string_lossy().into_owned(),
        screen_center_x(),
        screen_center_y(),
        screen_width(),
        screen_height(),
        1.0,
        1000,
    ))
}

#[inline(always)]
fn song_lua_overlay_space_width(state: &State) -> f32 {
    state.song_lua_screen_width.max(1.0)
}

#[inline(always)]
fn song_lua_overlay_space_height(state: &State) -> f32 {
    state.song_lua_screen_height.max(1.0)
}

fn apply_song_lua_overlay_delta(state: &mut SongLuaOverlayState, delta: &SongLuaOverlayStateDelta) {
    if let Some(value) = delta.x {
        state.x = value;
    }
    if let Some(value) = delta.y {
        state.y = value;
    }
    if let Some(value) = delta.z {
        state.z = value;
    }
    if let Some(value) = delta.fov {
        state.fov = Some(value);
    }
    if let Some(value) = delta.vanishpoint {
        state.vanishpoint = Some(value);
    }
    if let Some(value) = delta.diffuse {
        state.diffuse = value;
    }
    if let Some(value) = delta.visible {
        state.visible = value;
    }
    if let Some(value) = delta.cropleft {
        state.cropleft = value;
    }
    if let Some(value) = delta.cropright {
        state.cropright = value;
    }
    if let Some(value) = delta.croptop {
        state.croptop = value;
    }
    if let Some(value) = delta.cropbottom {
        state.cropbottom = value;
    }
    if let Some(value) = delta.fadeleft {
        state.fadeleft = value;
    }
    if let Some(value) = delta.faderight {
        state.faderight = value;
    }
    if let Some(value) = delta.fadetop {
        state.fadetop = value;
    }
    if let Some(value) = delta.fadebottom {
        state.fadebottom = value;
    }
    if let Some(value) = delta.zoom {
        state.zoom = value;
    }
    if let Some(value) = delta.zoom_x {
        state.zoom_x = value;
    }
    if let Some(value) = delta.zoom_y {
        state.zoom_y = value;
    }
    if let Some(value) = delta.zoom_z {
        state.zoom_z = value;
    }
    if let Some(value) = delta.basezoom {
        state.basezoom = value;
    }
    if let Some(value) = delta.basezoom_x {
        state.basezoom_x = value;
    }
    if let Some(value) = delta.basezoom_y {
        state.basezoom_y = value;
    }
    if let Some(value) = delta.rot_x_deg {
        state.rot_x_deg = value;
    }
    if let Some(value) = delta.rot_y_deg {
        state.rot_y_deg = value;
    }
    if let Some(value) = delta.rot_z_deg {
        state.rot_z_deg = value;
    }
    if let Some(value) = delta.blend {
        state.blend = value;
    }
    if let Some(value) = delta.vibrate {
        state.vibrate = value;
    }
    if let Some(value) = delta.effect_magnitude {
        state.effect_magnitude = value;
    }
    if let Some(value) = delta.effect_clock {
        state.effect_clock = value;
    }
    if let Some(value) = delta.effect_mode {
        state.effect_mode = value;
    }
    if let Some(value) = delta.effect_color1 {
        state.effect_color1 = value;
    }
    if let Some(value) = delta.effect_color2 {
        state.effect_color2 = value;
    }
    if let Some(value) = delta.effect_period {
        state.effect_period = value;
    }
    if let Some(value) = delta.effect_offset {
        state.effect_offset = value;
    }
    if let Some(value) = delta.sprite_animate {
        state.sprite_animate = value;
    }
    if let Some(value) = delta.sprite_loop {
        state.sprite_loop = value;
    }
    if let Some(value) = delta.sprite_playback_rate {
        state.sprite_playback_rate = value;
    }
    if let Some(value) = delta.sprite_state_delay {
        state.sprite_state_delay = value;
    }
    if let Some(value) = delta.sprite_state_index {
        state.sprite_state_index = Some(value);
    }
    if let Some(value) = delta.texture_wrapping {
        state.texture_wrapping = value;
    }
    if let Some(value) = delta.texcoord_offset {
        state.texcoord_offset = Some(value);
    }
    if let Some(value) = delta.custom_texture_rect {
        state.custom_texture_rect = Some(value);
    }
    if let Some(value) = delta.texcoord_velocity {
        state.texcoord_velocity = Some(value);
    }
    if let Some(value) = delta.size {
        state.size = Some(value);
    }
    if let Some(value) = delta.stretch_rect {
        state.stretch_rect = Some(value);
    }
}

fn song_lua_overlay_state_with_delta(
    mut state: SongLuaOverlayState,
    delta: &SongLuaOverlayStateDelta,
) -> SongLuaOverlayState {
    apply_song_lua_overlay_delta(&mut state, delta);
    state
}

fn song_lua_overlay_state_lerp(
    mut from: SongLuaOverlayState,
    to: SongLuaOverlayState,
    t: f32,
    delta: &SongLuaOverlayStateDelta,
) -> SongLuaOverlayState {
    if delta.x.is_some() {
        from.x = (to.x - from.x).mul_add(t, from.x);
    }
    if delta.y.is_some() {
        from.y = (to.y - from.y).mul_add(t, from.y);
    }
    if delta.z.is_some() {
        from.z = (to.z - from.z).mul_add(t, from.z);
    }
    if delta.fov.is_some()
        && let (Some(from_fov), Some(to_fov)) = (from.fov, to.fov)
    {
        from.fov = Some((to_fov - from_fov).mul_add(t, from_fov));
    }
    if delta.vanishpoint.is_some()
        && let (Some(from_vanish), Some(to_vanish)) = (from.vanishpoint, to.vanishpoint)
    {
        from.vanishpoint = Some([
            (to_vanish[0] - from_vanish[0]).mul_add(t, from_vanish[0]),
            (to_vanish[1] - from_vanish[1]).mul_add(t, from_vanish[1]),
        ]);
    }
    if delta.diffuse.is_some() {
        for i in 0..4 {
            from.diffuse[i] = (to.diffuse[i] - from.diffuse[i]).mul_add(t, from.diffuse[i]);
        }
    }
    if delta.cropleft.is_some() {
        from.cropleft = (to.cropleft - from.cropleft).mul_add(t, from.cropleft);
    }
    if delta.cropright.is_some() {
        from.cropright = (to.cropright - from.cropright).mul_add(t, from.cropright);
    }
    if delta.croptop.is_some() {
        from.croptop = (to.croptop - from.croptop).mul_add(t, from.croptop);
    }
    if delta.cropbottom.is_some() {
        from.cropbottom = (to.cropbottom - from.cropbottom).mul_add(t, from.cropbottom);
    }
    if delta.fadeleft.is_some() {
        from.fadeleft = (to.fadeleft - from.fadeleft).mul_add(t, from.fadeleft);
    }
    if delta.faderight.is_some() {
        from.faderight = (to.faderight - from.faderight).mul_add(t, from.faderight);
    }
    if delta.fadetop.is_some() {
        from.fadetop = (to.fadetop - from.fadetop).mul_add(t, from.fadetop);
    }
    if delta.fadebottom.is_some() {
        from.fadebottom = (to.fadebottom - from.fadebottom).mul_add(t, from.fadebottom);
    }
    if delta.zoom.is_some() {
        from.zoom = (to.zoom - from.zoom).mul_add(t, from.zoom);
    }
    if delta.zoom_x.is_some() {
        from.zoom_x = (to.zoom_x - from.zoom_x).mul_add(t, from.zoom_x);
    }
    if delta.zoom_y.is_some() {
        from.zoom_y = (to.zoom_y - from.zoom_y).mul_add(t, from.zoom_y);
    }
    if delta.zoom_z.is_some() {
        from.zoom_z = (to.zoom_z - from.zoom_z).mul_add(t, from.zoom_z);
    }
    if delta.basezoom.is_some() {
        from.basezoom = (to.basezoom - from.basezoom).mul_add(t, from.basezoom);
    }
    if delta.basezoom_x.is_some() {
        from.basezoom_x = (to.basezoom_x - from.basezoom_x).mul_add(t, from.basezoom_x);
    }
    if delta.basezoom_y.is_some() {
        from.basezoom_y = (to.basezoom_y - from.basezoom_y).mul_add(t, from.basezoom_y);
    }
    if delta.rot_x_deg.is_some() {
        from.rot_x_deg = (to.rot_x_deg - from.rot_x_deg).mul_add(t, from.rot_x_deg);
    }
    if delta.rot_y_deg.is_some() {
        from.rot_y_deg = (to.rot_y_deg - from.rot_y_deg).mul_add(t, from.rot_y_deg);
    }
    if delta.rot_z_deg.is_some() {
        from.rot_z_deg = (to.rot_z_deg - from.rot_z_deg).mul_add(t, from.rot_z_deg);
    }
    if delta.effect_magnitude.is_some() {
        for i in 0..3 {
            from.effect_magnitude[i] = (to.effect_magnitude[i] - from.effect_magnitude[i])
                .mul_add(t, from.effect_magnitude[i]);
        }
    }
    if delta.effect_color1.is_some() {
        for i in 0..4 {
            from.effect_color1[i] =
                (to.effect_color1[i] - from.effect_color1[i]).mul_add(t, from.effect_color1[i]);
        }
    }
    if delta.effect_color2.is_some() {
        for i in 0..4 {
            from.effect_color2[i] =
                (to.effect_color2[i] - from.effect_color2[i]).mul_add(t, from.effect_color2[i]);
        }
    }
    if delta.effect_period.is_some() {
        from.effect_period = (to.effect_period - from.effect_period).mul_add(t, from.effect_period);
    }
    if delta.effect_offset.is_some() {
        from.effect_offset = (to.effect_offset - from.effect_offset).mul_add(t, from.effect_offset);
    }
    if delta.sprite_playback_rate.is_some() {
        from.sprite_playback_rate = (to.sprite_playback_rate - from.sprite_playback_rate)
            .mul_add(t, from.sprite_playback_rate);
    }
    if delta.sprite_state_delay.is_some() {
        from.sprite_state_delay =
            (to.sprite_state_delay - from.sprite_state_delay).mul_add(t, from.sprite_state_delay);
    }
    if delta.sprite_state_index.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_state_index = to.sprite_state_index;
    }
    if delta.texcoord_offset.is_some()
        && let (Some(from_offset), Some(to_offset)) = (from.texcoord_offset, to.texcoord_offset)
    {
        from.texcoord_offset = Some([
            (to_offset[0] - from_offset[0]).mul_add(t, from_offset[0]),
            (to_offset[1] - from_offset[1]).mul_add(t, from_offset[1]),
        ]);
    }
    if delta.custom_texture_rect.is_some()
        && let (Some(from_rect), Some(to_rect)) = (from.custom_texture_rect, to.custom_texture_rect)
    {
        from.custom_texture_rect = Some([
            (to_rect[0] - from_rect[0]).mul_add(t, from_rect[0]),
            (to_rect[1] - from_rect[1]).mul_add(t, from_rect[1]),
            (to_rect[2] - from_rect[2]).mul_add(t, from_rect[2]),
            (to_rect[3] - from_rect[3]).mul_add(t, from_rect[3]),
        ]);
    }
    if delta.texcoord_velocity.is_some()
        && let (Some(from_vel), Some(to_vel)) = (from.texcoord_velocity, to.texcoord_velocity)
    {
        from.texcoord_velocity = Some([
            (to_vel[0] - from_vel[0]).mul_add(t, from_vel[0]),
            (to_vel[1] - from_vel[1]).mul_add(t, from_vel[1]),
        ]);
    }
    if delta.size.is_some()
        && let (Some(from_size), Some(to_size)) = (from.size, to.size)
    {
        from.size = Some([
            (to_size[0] - from_size[0]).mul_add(t, from_size[0]),
            (to_size[1] - from_size[1]).mul_add(t, from_size[1]),
        ]);
    }
    if delta.stretch_rect.is_some()
        && let (Some(from_rect), Some(to_rect)) = (from.stretch_rect, to.stretch_rect)
    {
        from.stretch_rect = Some([
            (to_rect[0] - from_rect[0]).mul_add(t, from_rect[0]),
            (to_rect[1] - from_rect[1]).mul_add(t, from_rect[1]),
            (to_rect[2] - from_rect[2]).mul_add(t, from_rect[2]),
            (to_rect[3] - from_rect[3]).mul_add(t, from_rect[3]),
        ]);
    }
    if delta.visible.is_some() && t >= 1.0 - f32::EPSILON {
        from.visible = to.visible;
    }
    if delta.blend.is_some() && t >= 1.0 - f32::EPSILON {
        from.blend = to.blend;
    }
    if delta.vibrate.is_some() && t >= 1.0 - f32::EPSILON {
        from.vibrate = to.vibrate;
    }
    if delta.effect_clock.is_some() && t >= 1.0 - f32::EPSILON {
        from.effect_clock = to.effect_clock;
    }
    if delta.effect_mode.is_some() && t >= 1.0 - f32::EPSILON {
        from.effect_mode = to.effect_mode;
    }
    if delta.sprite_animate.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_animate = to.sprite_animate;
    }
    if delta.sprite_loop.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_loop = to.sprite_loop;
    }
    if delta.texture_wrapping.is_some() && t >= 1.0 - f32::EPSILON {
        from.texture_wrapping = to.texture_wrapping;
    }
    from
}

#[inline(always)]
fn song_lua_valid_sprite_state_index(state: SongLuaOverlayState) -> Option<u32> {
    state.sprite_state_index.filter(|&value| value != u32::MAX)
}

#[inline(always)]
fn song_lua_sprite_sheet_index(
    state: SongLuaOverlayState,
    texture_key: &str,
    total_elapsed: f32,
) -> Option<u32> {
    let start = song_lua_valid_sprite_state_index(state).unwrap_or(0);
    let (cols, rows) = sprite_sheet_dims(texture_key);
    let total = cols.saturating_mul(rows).max(1);
    if state.sprite_animate && state.sprite_state_delay > 0.0 && total > 1 {
        let steps =
            (total_elapsed * state.sprite_playback_rate / state.sprite_state_delay).floor() as i64;
        let frame = i64::from(start) + steps;
        let total = i64::from(total);
        return Some(if state.sprite_loop {
            frame.rem_euclid(total) as u32
        } else {
            frame.clamp(0, total - 1) as u32
        });
    }
    (state.sprite_animate || song_lua_valid_sprite_state_index(state).is_some()).then_some(start)
}

#[inline(always)]
fn song_lua_sprite_sheet_rect(index: u32, cols: u32, rows: u32) -> [f32; 4] {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let col = index % cols;
    let row = (index / cols).min(rows.saturating_sub(1));
    let width = 1.0 / cols as f32;
    let height = 1.0 / rows as f32;
    let left = col as f32 * width;
    let top = row as f32 * height;
    [left, top, left + width, top + height]
}

fn song_lua_overlay_sprite_size(state: SongLuaOverlayState, texture_key: &str) -> Option<[f32; 2]> {
    if let Some(size) = state.size {
        return Some(size);
    }
    let tex = crate::assets::texture_dims(texture_key)?;
    let (mut width, mut height) = (tex.w as f32, tex.h as f32);
    if state.sprite_animate || song_lua_valid_sprite_state_index(state).is_some() {
        let (cols, rows) = sprite_sheet_dims(texture_key);
        width /= cols.max(1) as f32;
        height /= rows.max(1) as f32;
    }
    Some([width, height])
}

fn song_lua_overlay_uv_rect(
    state: SongLuaOverlayState,
    texture_key: Option<&str>,
    total_elapsed: f32,
) -> Option<[f32; 4]> {
    let mut rect = state.custom_texture_rect.or_else(|| {
        let texture_key = texture_key?;
        let state_index = song_lua_sprite_sheet_index(state, texture_key, total_elapsed)?;
        let (cols, rows) = sprite_sheet_dims(texture_key);
        Some(song_lua_sprite_sheet_rect(state_index, cols, rows))
    });
    if rect.is_none() && state.texcoord_offset.is_some() {
        rect = Some([0.0, 0.0, 1.0, 1.0]);
    }
    if let (Some([u0, v0, u1, v1]), Some([dx, dy])) = (rect, state.texcoord_offset) {
        rect = Some([u0 + dx, v0 + dy, u1 + dx, v1 + dy]);
    }
    rect
}

#[inline(always)]
fn song_lua_overlay_axis_scale(state: SongLuaOverlayState) -> [f32; 2] {
    [
        state.basezoom_x * state.zoom_x,
        state.basezoom_y * state.zoom_y,
    ]
}

#[inline(always)]
fn song_lua_overlay_parent_uses_center_origin(
    parent_kind: &SongLuaOverlayKind,
    parent_axis: f32,
    overlay_space_axis: f32,
) -> bool {
    matches!(
        parent_kind,
        SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture
    ) && (parent_axis - 0.5 * overlay_space_axis).abs() <= 0.01
}

fn song_lua_overlay_compose_state(
    parent_kind: &SongLuaOverlayKind,
    parent: SongLuaOverlayState,
    mut child: SongLuaOverlayState,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> SongLuaOverlayState {
    let [parent_scale_x, parent_scale_y] = song_lua_overlay_axis_scale(parent);
    let (sin_z, cos_z) = parent.rot_z_deg.to_radians().sin_cos();
    let epsilon = 0.01;
    let local_x = if matches!(
        parent_kind,
        SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture
    ) && song_lua_overlay_parent_uses_center_origin(
        parent_kind,
        parent.x,
        overlay_space_width,
    ) && (child.x - 0.5 * overlay_space_width).abs() <= epsilon
    {
        0.0
    } else {
        child.x
    } * parent_scale_x;
    let local_y = if matches!(
        parent_kind,
        SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture
    ) && song_lua_overlay_parent_uses_center_origin(
        parent_kind,
        parent.y,
        overlay_space_height,
    ) && (child.y - 0.5 * overlay_space_height).abs() <= epsilon
    {
        0.0
    } else {
        child.y
    } * parent_scale_y;
    child.x = parent.x + local_x * cos_z - local_y * sin_z;
    child.y = parent.y + local_x * sin_z + local_y * cos_z;
    for i in 0..4 {
        child.diffuse[i] *= parent.diffuse[i];
    }
    child.visible = parent.visible && child.visible;
    child.basezoom *= parent.basezoom * parent.zoom;
    child.basezoom_x *= parent.basezoom_x * parent.zoom_x;
    child.basezoom_y *= parent.basezoom_y * parent.zoom_y;
    child.rot_x_deg += parent.rot_x_deg;
    child.rot_y_deg += parent.rot_y_deg;
    child.rot_z_deg += parent.rot_z_deg;
    if let Some([left, top, right, bottom]) = child.stretch_rect
        && parent.rot_x_deg.abs() <= f32::EPSILON
        && parent.rot_y_deg.abs() <= f32::EPSILON
        && parent.rot_z_deg.abs() <= f32::EPSILON
    {
        child.stretch_rect = Some([
            parent.x + left * parent_scale_x,
            parent.y + top * parent_scale_y,
            parent.x + right * parent_scale_x,
            parent.y + bottom * parent_scale_y,
        ]);
    }
    child
}

fn song_lua_overlay_states_from(
    now: f32,
    overlays: &[SongLuaOverlayActor],
    overlay_events: &[Vec<crate::game::gameplay::SongLuaOverlayMessageRuntime>],
    overlay_eases: &[crate::game::gameplay::SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    screen_width: f32,
    screen_height: f32,
) -> Vec<SongLuaOverlayState> {
    let mut out = Vec::with_capacity(overlays.len());
    for (idx, overlay) in overlays.iter().enumerate() {
        let local = song_lua_overlay_render_state_from(
            now,
            idx,
            overlay,
            overlay_events,
            overlay_eases,
            overlay_ease_ranges,
        );
        let composed = overlay
            .parent_index
            .and_then(|parent_index| {
                out.get(parent_index)
                    .copied()
                    .zip(overlays.get(parent_index))
            })
            .map(|(parent, parent_overlay)| {
                song_lua_overlay_compose_state(
                    &parent_overlay.kind,
                    parent,
                    local,
                    screen_width,
                    screen_height,
                )
            })
            .unwrap_or(local);
        out.push(composed);
    }
    out
}

fn song_lua_overlay_states(state: &State) -> Vec<SongLuaOverlayState> {
    song_lua_overlay_states_from(
        state.current_music_time_display,
        &state.song_lua_overlays,
        &state.song_lua_overlay_events,
        &state.song_lua_overlay_eases,
        &state.song_lua_overlay_ease_ranges,
        state.song_lua_screen_width,
        state.song_lua_screen_height,
    )
}

fn song_lua_proxy_active_players(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
) -> [bool; 2] {
    let mut out = [false; 2];
    for (overlay_index, overlay) in overlays.iter().enumerate() {
        let SongLuaOverlayKind::ActorProxy { target } = &overlay.kind else {
            continue;
        };
        let player_index = match target {
            SongLuaProxyTarget::Player { player_index }
            | SongLuaProxyTarget::NoteField { player_index } => *player_index,
            _ => continue,
        };
        if player_index >= out.len() {
            continue;
        }
        let has_source = match target {
            SongLuaProxyTarget::Player { .. } => proxy_sources[player_index]
                .player
                .as_ref()
                .is_some_and(|actors| !actors.is_empty()),
            SongLuaProxyTarget::NoteField { .. } => proxy_sources[player_index]
                .note_field
                .as_ref()
                .is_some_and(|actors| !actors.is_empty()),
            _ => false,
        };
        if !has_source {
            continue;
        }
        if overlay_states
            .get(overlay_index)
            .copied()
            .is_some_and(song_lua_overlay_is_visible)
        {
            out[player_index] = true;
        }
    }
    out
}

fn song_lua_proxy_target_has_source(
    target: &SongLuaProxyTarget,
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
) -> bool {
    match target {
        SongLuaProxyTarget::Player { player_index } => proxy_sources
            .get(*player_index)
            .and_then(|sources| sources.player.as_ref())
            .is_some_and(|actors| !actors.is_empty()),
        SongLuaProxyTarget::NoteField { player_index } => proxy_sources
            .get(*player_index)
            .and_then(|sources| sources.note_field.as_ref())
            .is_some_and(|actors| !actors.is_empty()),
        SongLuaProxyTarget::Judgment { player_index } => proxy_sources
            .get(*player_index)
            .and_then(|sources| sources.judgment.as_ref())
            .is_some_and(|actors| !actors.is_empty()),
        SongLuaProxyTarget::Combo { player_index } => proxy_sources
            .get(*player_index)
            .and_then(|sources| sources.combo.as_ref())
            .is_some_and(|actors| !actors.is_empty()),
        SongLuaProxyTarget::Underlay | SongLuaProxyTarget::Overlay => false,
    }
}

fn song_lua_capture_replaces_player(
    overlays: &[SongLuaOverlayActor],
    capture_index: usize,
    player_index: usize,
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
) -> bool {
    overlays.iter().enumerate().any(|(idx, overlay)| {
        if song_lua_overlay_aft_ancestor(overlays, idx) != Some(capture_index) {
            return false;
        }
        match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                matches!(
                    target,
                    SongLuaProxyTarget::Player { player_index: target_player }
                        | SongLuaProxyTarget::NoteField { player_index: target_player }
                        if *target_player == player_index
                ) && song_lua_proxy_target_has_source(target, proxy_sources)
            }
            SongLuaOverlayKind::AftSprite { capture_name } => {
                song_lua_overlay_capture_index_by_name(overlays, capture_name).is_some_and(
                    |nested_capture| {
                        song_lua_capture_replaces_player(
                            overlays,
                            nested_capture,
                            player_index,
                            proxy_sources,
                        )
                    },
                )
            }
            _ => false,
        }
    })
}

fn song_lua_replacement_active_players(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
) -> [bool; 2] {
    let mut out = song_lua_proxy_active_players(overlays, overlay_states, proxy_sources);
    for (overlay_index, overlay) in overlays.iter().enumerate() {
        let Some(overlay_state) = overlay_states.get(overlay_index) else {
            continue;
        };
        if !overlay_state.visible || overlay_state.diffuse[3] <= f32::EPSILON {
            continue;
        }
        let SongLuaOverlayKind::AftSprite { capture_name } = &overlay.kind else {
            continue;
        };
        let Some(capture_index) = song_lua_overlay_capture_index_by_name(overlays, capture_name)
        else {
            continue;
        };
        for player_index in 0..out.len() {
            if song_lua_capture_replaces_player(
                overlays,
                capture_index,
                player_index,
                proxy_sources,
            ) {
                out[player_index] = true;
            }
        }
    }
    out
}

fn song_lua_overlay_aft_ancestor(
    overlays: &[SongLuaOverlayActor],
    mut index: usize,
) -> Option<usize> {
    while let Some(parent_index) = overlays.get(index).and_then(|overlay| overlay.parent_index) {
        match overlays.get(parent_index).map(|overlay| &overlay.kind) {
            Some(SongLuaOverlayKind::ActorFrameTexture) => return Some(parent_index),
            Some(_) => index = parent_index,
            None => return None,
        }
    }
    None
}

fn song_lua_overlay_capture_index_by_name(
    overlays: &[SongLuaOverlayActor],
    capture_name: &str,
) -> Option<usize> {
    overlays.iter().position(|overlay| {
        matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture)
            && overlay
                .name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(capture_name))
    })
}

#[derive(Clone, Copy, Default)]
struct SongLuaPlayerProxySources<'a> {
    player: Option<&'a [Actor]>,
    note_field: Option<&'a [Actor]>,
    judgment: Option<&'a [Actor]>,
    combo: Option<&'a [Actor]>,
}

#[derive(Clone, Copy, Default)]
struct SongLuaPlayerProxyRequests {
    player: bool,
    note_field: bool,
    judgment: bool,
    combo: bool,
}

#[derive(Clone, Copy, Default)]
struct SongLuaScreenProxySources<'a> {
    players: [SongLuaPlayerProxySources<'a>; 2],
    underlay: Option<&'a [Actor]>,
    overlay: Option<&'a [Actor]>,
}

#[derive(Clone, Copy, Default)]
struct SongLuaScreenProxyRequests {
    players: [SongLuaPlayerProxyRequests; 2],
    underlay: bool,
    overlay: bool,
}

fn song_lua_screen_proxy_sources<'a>(
    actors: &'a [Actor],
    p1_actor_range: Option<(usize, usize)>,
    p2_actor_range: Option<(usize, usize)>,
    p1_sources: [Option<&'a [Actor]>; 3],
    p2_sources: [Option<&'a [Actor]>; 3],
    underlay: Option<&'a [Actor]>,
    overlay: Option<&'a [Actor]>,
) -> SongLuaScreenProxySources<'a> {
    SongLuaScreenProxySources {
        players: [
            SongLuaPlayerProxySources {
                player: p1_actor_range.map(|(start, end)| &actors[start..end]),
                note_field: p1_sources[0],
                judgment: p1_sources[1],
                combo: p1_sources[2],
            },
            SongLuaPlayerProxySources {
                player: p2_actor_range.map(|(start, end)| &actors[start..end]),
                note_field: p2_sources[0],
                judgment: p2_sources[1],
                combo: p2_sources[2],
            },
        ],
        underlay,
        overlay,
    }
}

#[inline(always)]
fn song_lua_overlay_is_visible(state: SongLuaOverlayState) -> bool {
    state.visible && state.diffuse[3] > f32::EPSILON
}

#[inline(always)]
fn song_lua_capture_new_actors(dest: &mut Option<Vec<Actor>>, actors: &[Actor], start: usize) {
    let Some(dest) = dest.as_mut() else {
        return;
    };
    if start >= actors.len() {
        return;
    }
    dest.extend(actors[start..].iter().cloned());
}

#[inline(always)]
fn song_lua_proxy_source<'a>(
    target: &SongLuaProxyTarget,
    proxy_sources: &SongLuaScreenProxySources<'a>,
) -> Option<&'a [Actor]> {
    match target {
        SongLuaProxyTarget::Player { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.player.filter(|actors| !actors.is_empty())),
        SongLuaProxyTarget::NoteField { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.note_field.filter(|actors| !actors.is_empty())),
        SongLuaProxyTarget::Judgment { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.judgment.filter(|actors| !actors.is_empty())),
        SongLuaProxyTarget::Combo { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.combo.filter(|actors| !actors.is_empty())),
        SongLuaProxyTarget::Underlay => proxy_sources.underlay.filter(|actors| !actors.is_empty()),
        SongLuaProxyTarget::Overlay => proxy_sources.overlay.filter(|actors| !actors.is_empty()),
    }
}

fn song_lua_mark_proxy_target(
    requests: &mut SongLuaScreenProxyRequests,
    target: &SongLuaProxyTarget,
) {
    match target {
        SongLuaProxyTarget::Player { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.player = true;
            }
        }
        SongLuaProxyTarget::NoteField { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.note_field = true;
            }
        }
        SongLuaProxyTarget::Judgment { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.judgment = true;
            }
        }
        SongLuaProxyTarget::Combo { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.combo = true;
            }
        }
        SongLuaProxyTarget::Underlay => requests.underlay = true,
        SongLuaProxyTarget::Overlay => requests.overlay = true,
    }
}

fn song_lua_collect_capture_requests(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    capture_index: usize,
    requests: &mut SongLuaScreenProxyRequests,
    capture_stack: &mut Vec<usize>,
) {
    if capture_stack.contains(&capture_index) {
        return;
    }
    capture_stack.push(capture_index);
    for (idx, overlay) in overlays.iter().enumerate() {
        if song_lua_overlay_aft_ancestor(overlays, idx) != Some(capture_index) {
            continue;
        }
        let Some(overlay_state) = overlay_states.get(idx).copied() else {
            continue;
        };
        if !song_lua_overlay_is_visible(overlay_state) {
            continue;
        }
        match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_mark_proxy_target(requests, target);
            }
            SongLuaOverlayKind::AftSprite { capture_name } => {
                if let Some(nested_capture) =
                    song_lua_overlay_capture_index_by_name(overlays, capture_name)
                {
                    song_lua_collect_capture_requests(
                        overlays,
                        overlay_states,
                        nested_capture,
                        requests,
                        capture_stack,
                    );
                }
            }
            _ => {}
        }
    }
    capture_stack.pop();
}

fn song_lua_proxy_requests(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
) -> SongLuaScreenProxyRequests {
    let mut requests = SongLuaScreenProxyRequests::default();
    let mut capture_stack = Vec::with_capacity(4);
    for (idx, overlay) in overlays.iter().enumerate() {
        if song_lua_overlay_aft_ancestor(overlays, idx).is_some() {
            continue;
        }
        let Some(overlay_state) = overlay_states.get(idx).copied() else {
            continue;
        };
        if !song_lua_overlay_is_visible(overlay_state) {
            continue;
        }
        match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_mark_proxy_target(&mut requests, target);
            }
            SongLuaOverlayKind::AftSprite { capture_name } => {
                if let Some(capture_index) =
                    song_lua_overlay_capture_index_by_name(overlays, capture_name)
                {
                    song_lua_collect_capture_requests(
                        overlays,
                        overlay_states,
                        capture_index,
                        &mut requests,
                        &mut capture_stack,
                    );
                }
            }
            _ => {}
        }
    }
    requests
}

fn song_lua_build_proxy_actor(
    state: SongLuaOverlayState,
    z: i16,
    source: &[Actor],
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Actor> {
    if !state.visible || state.diffuse[3] <= f32::EPSILON || source.is_empty() {
        return None;
    }
    Some(Actor::Frame {
        align: [0.0, 0.0],
        offset: [
            state.x * screen_width() / overlay_space_width.max(1.0),
            state.y * screen_height() / overlay_space_height.max(1.0),
        ],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: source
            .iter()
            .cloned()
            .map(|actor| {
                song_lua_style_capture_actor(
                    actor,
                    state.diffuse,
                    Some(song_lua_overlay_blend(state.blend)),
                    0,
                )
            })
            .collect(),
        background: None,
        z,
    })
}

fn song_lua_capture_children(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    asset_manager: &AssetManager,
    capture_index: usize,
    proxy_sources: &SongLuaScreenProxySources<'_>,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Vec<Actor> {
    let mut out = Vec::new();
    for (idx, overlay) in overlays.iter().enumerate() {
        if song_lua_overlay_aft_ancestor(overlays, idx) != Some(capture_index) {
            continue;
        }
        if matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture
        ) {
            continue;
        }
        let overlay_state = overlay_states.get(idx).copied().unwrap_or_default();
        let actor = match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_proxy_source(target, proxy_sources).and_then(|source| {
                    song_lua_build_proxy_actor(
                        overlay_state,
                        0,
                        source,
                        overlay_space_width,
                        overlay_space_height,
                    )
                })
            }
            _ => build_song_lua_overlay_actor(
                overlay,
                overlay_state,
                song_lua_overlay_camera_state(overlays, overlay_states, overlay.parent_index),
                asset_manager,
                0,
                overlay_space_width,
                overlay_space_height,
                0.0,
                0.0,
                0.0,
            ),
        };
        if let Some(actor) = actor {
            out.push(actor);
        }
    }
    out
}

fn song_lua_overlay_apply_blocks(
    state: SongLuaOverlayState,
    blocks: &[SongLuaOverlayCommandBlock],
    elapsed: f32,
) -> SongLuaOverlayState {
    if !elapsed.is_finite() {
        return state;
    }
    let mut current = state;
    for block in blocks {
        if elapsed < block.start {
            break;
        }
        if block.duration <= f32::EPSILON || elapsed >= block.start + block.duration {
            apply_song_lua_overlay_delta(&mut current, &block.delta);
            continue;
        }
        let target = song_lua_overlay_state_with_delta(current, &block.delta);
        let t = crate::game::gameplay::song_lua_ease_factor(
            block.easing.as_deref(),
            ((elapsed - block.start) / block.duration).clamp(0.0, 1.0),
            block.opt1,
            block.opt2,
        );
        return song_lua_overlay_state_lerp(current, target, t, &block.delta);
    }
    current
}

fn apply_song_lua_overlay_runtime_eases_for(
    now: f32,
    overlay_index: usize,
    overlay_eases: &[crate::game::gameplay::SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    mut current: SongLuaOverlayState,
) -> SongLuaOverlayState {
    let Some(ease_range) = overlay_ease_ranges.get(overlay_index) else {
        return current;
    };
    for ease in &overlay_eases[ease_range.clone()] {
        if ease.overlay_index != overlay_index || now < ease.start_second {
            continue;
        }
        if let Some(cutoff_second) = ease.cutoff_second
            && now >= cutoff_second
        {
            continue;
        }
        if now >= ease.sustain_end_second {
            apply_song_lua_overlay_delta(&mut current, &ease.to);
            continue;
        }
        if ease.end_second <= ease.start_second || now >= ease.end_second {
            apply_song_lua_overlay_delta(&mut current, &ease.to);
            continue;
        }
        let t = crate::game::gameplay::song_lua_ease_factor(
            ease.easing.as_deref(),
            ((now - ease.start_second) / (ease.end_second - ease.start_second)).clamp(0.0, 1.0),
            ease.opt1,
            ease.opt2,
        );
        let from_state = song_lua_overlay_state_with_delta(current, &ease.from);
        let to_state = song_lua_overlay_state_with_delta(current, &ease.to);
        current = song_lua_overlay_state_lerp(from_state, to_state, t, &ease.to);
    }
    current
}

fn song_lua_overlay_render_state_from(
    now: f32,
    overlay_index: usize,
    overlay: &SongLuaOverlayActor,
    overlay_events: &[Vec<crate::game::gameplay::SongLuaOverlayMessageRuntime>],
    overlay_eases: &[crate::game::gameplay::SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
) -> SongLuaOverlayState {
    let current = song_lua_message_state(
        now,
        overlay.initial_state,
        &overlay.message_commands,
        overlay_events.get(overlay_index).map(Vec::as_slice),
    );
    apply_song_lua_overlay_runtime_eases_for(
        now,
        overlay_index,
        overlay_eases,
        overlay_ease_ranges,
        current,
    )
}

fn song_lua_message_state(
    now: f32,
    initial_state: SongLuaOverlayState,
    message_commands: &[SongLuaOverlayMessageCommand],
    events: Option<&[crate::game::gameplay::SongLuaOverlayMessageRuntime]>,
) -> SongLuaOverlayState {
    let Some(events) = events else {
        return initial_state;
    };
    let mut current = initial_state;
    let mut active: Option<(&[SongLuaOverlayCommandBlock], SongLuaOverlayState, f32)> = None;
    for event in events {
        let event_second = event.event_second;
        if event_second > now {
            break;
        }
        let Some(command) = message_commands.get(event.command_index) else {
            continue;
        };
        if let Some((blocks, base, start_second)) = active.take() {
            current = song_lua_overlay_apply_blocks(base, blocks, event_second - start_second);
        }
        let base = current;
        current = song_lua_overlay_apply_blocks(base, &command.blocks, 0.0);
        active = Some((&command.blocks, base, event_second));
    }
    if let Some((blocks, base, start_second)) = active {
        current = song_lua_overlay_apply_blocks(base, blocks, now - start_second);
    }
    current
}

fn song_lua_player_render_state(state: &State, player_index: usize) -> SongLuaOverlayState {
    let Some(actor) = state.song_lua_player_actors.get(player_index) else {
        return SongLuaOverlayState::default();
    };
    song_lua_message_state(
        state.current_music_time_display,
        actor.initial_state,
        &actor.message_commands,
        state
            .song_lua_player_events
            .get(player_index)
            .map(Vec::as_slice),
    )
}

fn song_lua_song_foreground_state_from(
    now: f32,
    song_foreground: &SongLuaCapturedActor,
    events: &[crate::game::gameplay::SongLuaOverlayMessageRuntime],
) -> SongLuaOverlayState {
    song_lua_message_state(
        now,
        song_foreground.initial_state,
        &song_foreground.message_commands,
        Some(events),
    )
}

fn song_lua_song_foreground_state(state: &State) -> SongLuaOverlayState {
    song_lua_song_foreground_state_from(
        state.current_music_time_display,
        &state.song_lua_song_foreground,
        state.song_lua_song_foreground_events.as_slice(),
    )
}

fn song_lua_capture_tint(color: [f32; 4], tint: [f32; 4]) -> [f32; 4] {
    [
        color[0] * tint[0],
        color[1] * tint[1],
        color[2] * tint[2],
        color[3] * tint[3],
    ]
}

fn song_lua_add_z(z: i16, delta: i16) -> i16 {
    (i32::from(z) + i32::from(delta)).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

const SONG_LUA_LAYER_Z_BASE: i16 = 1100;

fn song_lua_rounded_z(value: f32) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    value
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

fn song_lua_player_layer_z(
    song_lua_active: bool,
    actor: &SongLuaCapturedActor,
    current: SongLuaOverlayState,
    runtime_z: f32,
) -> i16 {
    if !song_lua_active {
        return 0;
    }
    let _ = actor;
    song_lua_add_z(
        SONG_LUA_LAYER_Z_BASE,
        song_lua_rounded_z(current.z + runtime_z),
    )
}

fn song_lua_style_capture_actor(
    actor: Actor,
    tint: [f32; 4],
    blend: Option<BlendMode>,
    z_shift: i16,
) -> Actor {
    match actor {
        Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            tint: actor_tint,
            glow,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend: actor_blend,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            effect,
        } => Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            tint: song_lua_capture_tint(actor_tint, tint),
            glow,
            z: song_lua_add_z(z, z_shift),
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend: blend.unwrap_or(actor_blend),
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            effect,
        },
        Actor::Text {
            align,
            offset,
            color,
            stroke_color,
            glow,
            font,
            content,
            attributes,
            align_text,
            z,
            scale,
            fit_width,
            fit_height,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            clip,
            blend: actor_blend,
            effect,
        } => Actor::Text {
            align,
            offset,
            color: song_lua_capture_tint(color, tint),
            stroke_color: stroke_color.map(|color| song_lua_capture_tint(color, tint)),
            glow,
            font,
            content,
            attributes,
            align_text,
            z: song_lua_add_z(z, z_shift),
            scale,
            fit_width,
            fit_height,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            clip,
            blend: blend.unwrap_or(actor_blend),
            effect,
        },
        Actor::Mesh {
            align,
            offset,
            size,
            vertices,
            mode,
            visible,
            blend: actor_blend,
            z,
        } => Actor::Mesh {
            align,
            offset,
            size,
            vertices,
            mode,
            visible,
            blend: blend.unwrap_or(actor_blend),
            z: song_lua_add_z(z, z_shift),
        },
        Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            vertices,
            geom_cache_key,
            mode,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend: actor_blend,
            z,
        } => Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            vertices,
            geom_cache_key,
            mode,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend: blend.unwrap_or(actor_blend),
            z: song_lua_add_z(z, z_shift),
        },
        Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        } => Actor::Frame {
            align,
            offset,
            size,
            children: children
                .into_iter()
                .map(|child| song_lua_style_capture_actor(child, tint, blend, z_shift))
                .collect(),
            background,
            z: song_lua_add_z(z, z_shift),
        },
        Actor::Camera {
            view_proj,
            children,
        } => Actor::Camera {
            view_proj,
            children: children
                .into_iter()
                .map(|child| song_lua_style_capture_actor(child, tint, blend, z_shift))
                .collect(),
        },
        Actor::Shadow { len, color, child } => Actor::Shadow {
            len,
            color: song_lua_capture_tint(color, tint),
            child: Box::new(song_lua_style_capture_actor(*child, tint, blend, z_shift)),
        },
    }
}

fn song_lua_capture_transform_matrix(
    state: SongLuaOverlayState,
    extra_offset: [f32; 2],
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Matrix4> {
    let x_scale = screen_width() / overlay_space_width.max(1.0);
    let y_scale = screen_height() / overlay_space_height.max(1.0);
    let translate_x = (state.x - 0.5 * overlay_space_width) * x_scale + extra_offset[0];
    let translate_y = (state.y - 0.5 * overlay_space_height) * y_scale + extra_offset[1];
    let scale_x = state.basezoom_x * state.zoom_x;
    let scale_y = state.basezoom_y * state.zoom_y;
    if translate_x.abs() <= f32::EPSILON
        && translate_y.abs() <= f32::EPSILON
        && state.rot_z_deg.abs() <= f32::EPSILON
        && (scale_x - 1.0).abs() <= f32::EPSILON
        && (scale_y - 1.0).abs() <= f32::EPSILON
    {
        return None;
    }
    Some(
        Matrix4::from_translation(Vector3::new(translate_x, -translate_y, 0.0))
            * Matrix4::from_rotation_z(state.rot_z_deg.to_radians())
            * Matrix4::from_scale(Vector3::new(scale_x, scale_y, 1.0)),
    )
}

fn song_lua_capture_channel_offset(
    name: Option<&str>,
    state: SongLuaOverlayState,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> [f32; 2] {
    if !state.vibrate {
        return [0.0, 0.0];
    }
    let x = state.effect_magnitude[0].abs() * (screen_width() / overlay_space_width.max(1.0));
    let y =
        state.effect_magnitude[1].abs() * (screen_height() / overlay_space_height.max(1.0)) * 0.25;
    match name {
        Some(name) if name.ends_with('R') => [-x, -y],
        Some(name) if name.ends_with('B') => [x, y],
        _ => [0.0, 0.0],
    }
}

fn song_lua_build_capture_actor(
    overlay: &SongLuaOverlayActor,
    state: SongLuaOverlayState,
    z: i16,
    source: Vec<Actor>,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Actor> {
    if !state.visible || state.diffuse[3] <= f32::EPSILON || source.is_empty() {
        return None;
    }
    let blend = match state.blend {
        SongLuaOverlayBlendMode::Alpha => None,
        SongLuaOverlayBlendMode::Add => Some(BlendMode::Add),
    };
    let children = source
        .into_iter()
        .map(|actor| song_lua_style_capture_actor(actor, state.diffuse, blend, z))
        .collect::<Vec<_>>();
    let extra_offset = song_lua_capture_channel_offset(
        overlay.name.as_deref(),
        state,
        overlay_space_width,
        overlay_space_height,
    );
    if let Some(transform) = song_lua_capture_transform_matrix(
        state,
        extra_offset,
        overlay_space_width,
        overlay_space_height,
    ) {
        return Some(Actor::Camera {
            view_proj: Matrix4::orthographic_rh_gl(
                -0.5 * screen_width(),
                0.5 * screen_width(),
                -0.5 * screen_height(),
                0.5 * screen_height(),
                -1.0,
                1.0,
            ) * transform,
            children,
        });
    }
    Some(Actor::Frame {
        align: [0.0, 0.0],
        offset: extra_offset,
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children,
        background: None,
        z: 0,
    })
}

#[inline(always)]
fn song_lua_overlay_blend(blend: SongLuaOverlayBlendMode) -> BlendMode {
    match blend {
        SongLuaOverlayBlendMode::Alpha => BlendMode::Alpha,
        SongLuaOverlayBlendMode::Add => BlendMode::Add,
    }
}

#[inline(always)]
fn song_lua_overlay_effect_state(state: SongLuaOverlayState) -> EffectState {
    let period = state.effect_period.max(f32::EPSILON);
    EffectState {
        clock: state.effect_clock,
        mode: state.effect_mode,
        color1: state.effect_color1,
        color2: state.effect_color2,
        period,
        offset: state.effect_offset,
        timing: [period * 0.5, 0.0, period * 0.5, 0.0, 0.0],
        magnitude: state.effect_magnitude,
        ..EffectState::default()
    }
}

#[inline(always)]
fn song_lua_effect_lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

fn song_lua_apply_overlay_effect(
    effect: EffectState,
    effect_time: f32,
    effect_beat: f32,
    tint: &mut [f32; 4],
    offset: &mut [f32; 3],
    scale: &mut [f32; 3],
    rot_deg: &mut [f32; 3],
) {
    if matches!(effect.mode, crate::engine::present::anim::EffectMode::Spin) {
        let units =
            crate::engine::present::anim::effect_clock_units(effect, effect_time, effect_beat);
        rot_deg[0] = (rot_deg[0] + effect.magnitude[0] * units).rem_euclid(360.0);
        rot_deg[1] = (rot_deg[1] + effect.magnitude[1] * units).rem_euclid(360.0);
        rot_deg[2] = (rot_deg[2] + effect.magnitude[2] * units).rem_euclid(360.0);
    }
    if let Some(percent) =
        crate::engine::present::anim::effect_mix(effect, effect_time, effect_beat)
    {
        match effect.mode {
            crate::engine::present::anim::EffectMode::DiffuseRamp => {
                for (idx, out) in tint.iter_mut().enumerate() {
                    let color =
                        song_lua_effect_lerp(effect.color2[idx], effect.color1[idx], percent)
                            .clamp(0.0, 1.0);
                    *out = (*out * color).clamp(0.0, 1.0);
                }
            }
            crate::engine::present::anim::EffectMode::DiffuseShift => {
                let between = crate::engine::present::anim::glowshift_mix(percent);
                for (idx, out) in tint.iter_mut().enumerate() {
                    let color =
                        song_lua_effect_lerp(effect.color2[idx], effect.color1[idx], between)
                            .clamp(0.0, 1.0);
                    *out = (*out * color).clamp(0.0, 1.0);
                }
            }
            crate::engine::present::anim::EffectMode::Pulse => {
                let pulse = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom =
                    song_lua_effect_lerp(effect.magnitude[0], effect.magnitude[1], pulse).max(0.0);
                scale[0] *= zoom * song_lua_effect_lerp(effect.color1[0], effect.color2[0], pulse);
                scale[1] *= zoom * song_lua_effect_lerp(effect.color1[1], effect.color2[1], pulse);
                scale[2] *= zoom * song_lua_effect_lerp(effect.color1[2], effect.color2[2], pulse);
            }
            crate::engine::present::anim::EffectMode::Bob => {
                let bob = (percent * 2.0 * std::f32::consts::PI).sin();
                for i in 0..3 {
                    offset[i] += effect.magnitude[i] * bob;
                }
            }
            crate::engine::present::anim::EffectMode::Bounce => {
                let bounce = (percent * std::f32::consts::PI).sin();
                for i in 0..3 {
                    offset[i] += effect.magnitude[i] * bounce;
                }
            }
            crate::engine::present::anim::EffectMode::Wag => {
                let wag = (percent * 2.0 * std::f32::consts::PI).sin();
                for i in 0..3 {
                    rot_deg[i] += effect.magnitude[i] * wag;
                }
            }
            crate::engine::present::anim::EffectMode::GlowShift
            | crate::engine::present::anim::EffectMode::Spin
            | crate::engine::present::anim::EffectMode::None => {}
        }
    }
    offset[0] = offset[0].max(-1_000_000.0).min(1_000_000.0);
    offset[1] = offset[1].max(-1_000_000.0).min(1_000_000.0);
    offset[2] = offset[2].max(-1_000_000.0).min(1_000_000.0);
    tint[0] = tint[0].clamp(0.0, 1.0);
    tint[1] = tint[1].clamp(0.0, 1.0);
    tint[2] = tint[2].clamp(0.0, 1.0);
    tint[3] = tint[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
    scale[2] = scale[2].max(0.0);
}

fn song_lua_overlay_camera_state(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    mut index: Option<usize>,
) -> Option<SongLuaOverlayState> {
    while let Some(current) = index {
        let overlay = overlays.get(current)?;
        let state = overlay_states.get(current).copied()?;
        if matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture
        ) && state.fov.is_some()
        {
            return Some(state);
        }
        index = overlay.parent_index;
    }
    None
}

fn song_lua_overlay_view_proj(
    camera_state: SongLuaOverlayState,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Matrix4> {
    let mut fov_deg = camera_state.fov?;
    if !fov_deg.is_finite() || fov_deg <= f32::EPSILON {
        return None;
    }
    fov_deg = fov_deg.clamp(0.1, 179.9);
    let width = screen_width().max(1.0);
    let height = screen_height().max(1.0);
    let x_scale = width / overlay_space_width.max(1.0);
    let y_scale = height / overlay_space_height.max(1.0);
    let vanish = camera_state
        .vanishpoint
        .unwrap_or([0.5 * overlay_space_width, 0.5 * overlay_space_height]);
    let mut vanish_x = width - vanish[0] * x_scale;
    let mut vanish_y = height - vanish[1] * y_scale;
    vanish_x -= 0.5 * width;
    vanish_y -= 0.5 * height;

    let theta = 0.5 * fov_deg.to_radians();
    let dist = (0.5 * width / theta.tan()).max(1.0);
    let proj = Matrix4::frustum_rh_gl(
        (vanish_x - 0.5 * width) / dist,
        (vanish_x + 0.5 * width) / dist,
        (vanish_y + 0.5 * height) / dist,
        (vanish_y - 0.5 * height) / dist,
        1.0,
        dist + 1000.0,
    );
    let eye_x = -vanish_x + 0.5 * width;
    let eye_y = -vanish_y + 0.5 * height;
    let view = Matrix4::look_at_rh(
        Vector3::new(eye_x, eye_y, dist),
        Vector3::new(eye_x, eye_y, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
    );
    Some(proj * view)
}

fn song_lua_project_overlay_point(view_proj: Matrix4, point: [f32; 3]) -> Option<[f32; 2]> {
    let clip = view_proj * Vector4::new(point[0], point[1], point[2], 1.0);
    if !clip.w.is_finite() || clip.w <= f32::EPSILON {
        return None;
    }
    let inv_w = clip.w.recip();
    let ndc_x = clip.x * inv_w;
    let ndc_y = clip.y * inv_w;
    if !(ndc_x.is_finite() && ndc_y.is_finite()) {
        return None;
    }
    Some([
        (0.5 * ndc_x + 0.5) * screen_width(),
        (0.5 - 0.5 * ndc_y) * screen_height(),
    ])
}

fn song_lua_overlay_rect(
    state: SongLuaOverlayState,
    default_size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    size_scale_x: f32,
    size_scale_y: f32,
) -> Option<([f32; 2], [f32; 2])> {
    let (base_center, base_size) = if let Some([left, top, right, bottom]) = state.stretch_rect {
        (
            [
                0.5 * (left + right) * x_scale,
                0.5 * (top + bottom) * y_scale,
            ],
            [
                (right - left).abs() * x_scale * size_scale_x,
                (bottom - top).abs() * y_scale * size_scale_y,
            ],
        )
    } else {
        (
            [state.x * x_scale, state.y * y_scale],
            [
                default_size[0] * x_scale * size_scale_x,
                default_size[1] * y_scale * size_scale_y,
            ],
        )
    };
    if base_size[0] <= f32::EPSILON || base_size[1] <= f32::EPSILON {
        return None;
    }
    let cl = state.cropleft.clamp(0.0, 1.0);
    let cr = state.cropright.clamp(0.0, 1.0);
    let ct = state.croptop.clamp(0.0, 1.0);
    let cb = state.cropbottom.clamp(0.0, 1.0);
    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= f32::EPSILON || sy_crop <= f32::EPSILON {
        return None;
    }
    Some((
        [
            ((cl - cr) * base_size[0]).mul_add(0.5, base_center[0]),
            ((cb - ct) * base_size[1]).mul_add(0.5, base_center[1]),
        ],
        [base_size[0] * sx_crop, base_size[1] * sy_crop],
    ))
}

fn song_lua_overlay_uvs(
    state: SongLuaOverlayState,
    texture_key: Option<&str>,
    flip_x: bool,
    flip_y: bool,
    total_elapsed: f32,
) -> [[f32; 2]; 4] {
    let cl = state.cropleft.clamp(0.0, 1.0);
    let cr = state.cropright.clamp(0.0, 1.0);
    let ct = state.croptop.clamp(0.0, 1.0);
    let cb = state.cropbottom.clamp(0.0, 1.0);
    let [
        mut uv_scale_x,
        mut uv_scale_y,
        mut uv_offset_x,
        mut uv_offset_y,
    ] = if let Some([u0, v0, u1, v1]) = song_lua_overlay_uv_rect(state, texture_key, total_elapsed)
    {
        [
            (u1 - u0).abs().max(1e-6),
            (v1 - v0).abs().max(1e-6),
            u0.min(u1),
            v0.min(v1),
        ]
    } else {
        [1.0, 1.0, 0.0, 0.0]
    };
    uv_offset_x += uv_scale_x * cl;
    uv_offset_y += uv_scale_y * ct;
    uv_scale_x *= (1.0 - cl - cr).max(0.0);
    uv_scale_y *= (1.0 - ct - cb).max(0.0);
    if flip_x {
        uv_offset_x += uv_scale_x;
        uv_scale_x = -uv_scale_x;
    }
    if flip_y {
        uv_offset_y += uv_scale_y;
        uv_scale_y = -uv_scale_y;
    }
    if let Some(velocity) = state.texcoord_velocity {
        uv_offset_x += velocity[0] * total_elapsed;
        uv_offset_y += velocity[1] * total_elapsed;
    }
    [
        [uv_offset_x, uv_offset_y],
        [uv_offset_x + uv_scale_x, uv_offset_y],
        [uv_offset_x + uv_scale_x, uv_offset_y + uv_scale_y],
        [uv_offset_x, uv_offset_y + uv_scale_y],
    ]
}

fn song_lua_projected_overlay_actor(
    texture: Arc<str>,
    tint: [f32; 4],
    blend: BlendMode,
    z: i16,
    center: [f32; 3],
    size: [f32; 2],
    rot_deg: [f32; 3],
    uv: [[f32; 2]; 4],
    view_proj: Matrix4,
) -> Option<Actor> {
    let half_w = 0.5 * size[0];
    let half_h = 0.5 * size[1];
    if half_w <= f32::EPSILON || half_h <= f32::EPSILON {
        return None;
    }
    let model = Matrix4::from_translation(Vector3::new(center[0], center[1], center[2]))
        * Matrix4::from_rotation_x(rot_deg[0].to_radians())
        * Matrix4::from_rotation_y(rot_deg[1].to_radians())
        * Matrix4::from_rotation_z(rot_deg[2].to_radians());
    let local = [
        ([-half_w, -half_h, 0.0], uv[0]),
        ([half_w, -half_h, 0.0], uv[1]),
        ([half_w, half_h, 0.0], uv[2]),
        ([-half_w, half_h, 0.0], uv[3]),
    ];
    let mut corners = [TexturedMeshVertex::default(); 4];
    for (idx, (pos, uv)) in local.into_iter().enumerate() {
        let world = model * Vector4::new(pos[0], pos[1], pos[2], 1.0);
        let screen = song_lua_project_overlay_point(view_proj, [world.x, world.y, world.z])?;
        corners[idx] = TexturedMeshVertex {
            pos: [screen[0], screen[1], 0.0],
            uv,
            tex_matrix_scale: [1.0, 1.0],
            color: tint,
        };
    }
    Some(Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z: 0.0,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: Matrix4::IDENTITY,
        texture,
        tint: [1.0, 1.0, 1.0, 1.0],
        vertices: Arc::from(
            vec![
                corners[0], corners[1], corners[2], corners[0], corners[2], corners[3],
            ]
            .into_boxed_slice(),
        ),
        geom_cache_key: INVALID_TMESH_CACHE_KEY,
        mode: MeshMode::Triangles,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test: false,
        visible: true,
        blend,
        z,
    })
}

fn build_song_lua_overlay_actor(
    overlay: &SongLuaOverlayActor,
    state: SongLuaOverlayState,
    camera_state: Option<SongLuaOverlayState>,
    asset_manager: &AssetManager,
    z: i16,
    overlay_space_width: f32,
    overlay_space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
) -> Option<Actor> {
    if !state.visible || state.diffuse[3] <= f32::EPSILON {
        return None;
    }
    let x_scale = screen_width() / overlay_space_width.max(1.0);
    let y_scale = screen_height() / overlay_space_height.max(1.0);
    let overlay_scale = song_lua_overlay_axis_scale(state);
    let (size_scale_x, flip_x) = if overlay_scale[0] < 0.0 {
        (-overlay_scale[0], true)
    } else {
        (overlay_scale[0], false)
    };
    let (size_scale_y, flip_y) = if overlay_scale[1] < 0.0 {
        (-overlay_scale[1], true)
    } else {
        (overlay_scale[1], false)
    };
    let effect = song_lua_overlay_effect_state(state);
    let overlay_blend = song_lua_overlay_blend(state.blend);
    let perspective_view_proj = camera_state.and_then(|camera| {
        song_lua_overlay_view_proj(camera, overlay_space_width, overlay_space_height)
    });
    match &overlay.kind {
        SongLuaOverlayKind::ActorFrame => None,
        SongLuaOverlayKind::ActorFrameTexture => None,
        SongLuaOverlayKind::ActorProxy { .. } => None,
        SongLuaOverlayKind::AftSprite { .. } => None,
        SongLuaOverlayKind::Sprite { texture_path } => {
            let key = Arc::<str>::from(texture_path.to_string_lossy().into_owned());
            if !asset_manager.has_texture_key(key.as_ref()) {
                return None;
            }
            if let Some(view_proj) = perspective_view_proj {
                let size = song_lua_overlay_sprite_size(state, key.as_ref())?;
                let (center, size) = song_lua_overlay_rect(
                    state,
                    size,
                    x_scale,
                    y_scale,
                    size_scale_x,
                    size_scale_y,
                )?;
                let mut tint = state.diffuse;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut rot_deg = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    effect_time,
                    effect_beat,
                    &mut tint,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut rot_deg,
                );
                return song_lua_projected_overlay_actor(
                    key.clone(),
                    tint,
                    overlay_blend,
                    z,
                    [
                        center[0] + effect_offset[0] * x_scale,
                        center[1] + effect_offset[1] * y_scale,
                        effect_offset[2],
                    ],
                    [size[0] * effect_scale[0], size[1] * effect_scale[1]],
                    rot_deg,
                    song_lua_overlay_uvs(state, Some(key.as_ref()), flip_x, flip_y, total_elapsed),
                    view_proj,
                );
            }
            let mut actor = if let Some([left, top, right, bottom]) = state.stretch_rect {
                act!(sprite(key.clone()):
                    align(0.0, 0.0):
                    xy(left * x_scale, top * y_scale):
                    setsize(
                        (right - left).abs() * x_scale * size_scale_x,
                        (bottom - top).abs() * y_scale * size_scale_y
                    ):
                    z(z)
                )
            } else {
                let size = song_lua_overlay_sprite_size(state, key.as_ref())?;
                act!(sprite(key.clone()):
                    align(0.5, 0.5):
                    xy(state.x * x_scale, state.y * y_scale):
                    setsize(
                        size[0] * x_scale * size_scale_x,
                        size[1] * y_scale * size_scale_y
                    ):
                    z(z)
                )
            };
            if let Actor::Sprite {
                tint,
                cropleft,
                cropright,
                croptop,
                cropbottom,
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                blend,
                rot_x_deg,
                rot_y_deg,
                rot_z_deg,
                offset,
                world_z,
                scale,
                uv_rect,
                texcoordvelocity,
                effect: actor_effect,
                flip_x: actor_flip_x,
                flip_y: actor_flip_y,
                visible,
                ..
            } = &mut actor
            {
                let mut effect_tint = state.diffuse;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    effect_time,
                    effect_beat,
                    &mut effect_tint,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut effect_rot,
                );
                *tint = effect_tint;
                *cropleft = state.cropleft.clamp(0.0, 1.0);
                *cropright = state.cropright.clamp(0.0, 1.0);
                *croptop = state.croptop.clamp(0.0, 1.0);
                *cropbottom = state.cropbottom.clamp(0.0, 1.0);
                *fadeleft = state.fadeleft.clamp(0.0, 1.0);
                *faderight = state.faderight.clamp(0.0, 1.0);
                *fadetop = state.fadetop.clamp(0.0, 1.0);
                *fadebottom = state.fadebottom.clamp(0.0, 1.0);
                *blend = overlay_blend;
                *rot_x_deg = effect_rot[0];
                *rot_y_deg = effect_rot[1];
                *rot_z_deg = effect_rot[2];
                offset[0] += effect_offset[0] * x_scale;
                offset[1] += effect_offset[1] * y_scale;
                *world_z += effect_offset[2];
                scale[0] *= effect_scale[0];
                scale[1] *= effect_scale[1];
                *uv_rect = song_lua_overlay_uv_rect(state, Some(key.as_ref()), total_elapsed);
                *texcoordvelocity = state.texcoord_velocity;
                *actor_effect = EffectState::default();
                *actor_flip_x ^= flip_x;
                *actor_flip_y ^= flip_y;
                *visible = state.visible;
            }
            Some(actor)
        }
        SongLuaOverlayKind::BitmapText {
            font_name,
            text,
            stroke_color,
            ..
        } => {
            let font = if asset_manager.with_font(*font_name, |_| ()).is_some() {
                *font_name
            } else {
                "miso"
            };
            let mut color = state.diffuse;
            let mut effect_offset = [0.0, 0.0, 0.0];
            let mut effect_scale = [1.0, 1.0, 1.0];
            let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
            song_lua_apply_overlay_effect(
                effect,
                effect_time,
                effect_beat,
                &mut color,
                &mut effect_offset,
                &mut effect_scale,
                &mut effect_rot,
            );
            let _ = effect_rot;
            Some(Actor::Text {
                align: [0.5, 0.5],
                offset: [
                    state.x * x_scale + effect_offset[0] * x_scale,
                    state.y * y_scale + effect_offset[1] * y_scale,
                ],
                color,
                stroke_color: *stroke_color,
                glow: [0.0, 0.0, 0.0, 0.0],
                font,
                content: TextContent::from(text),
                attributes: Vec::new(),
                align_text: TextAlign::Center,
                z,
                scale: [
                    size_scale_x * x_scale * effect_scale[0],
                    size_scale_y * y_scale * effect_scale[1],
                ],
                fit_width: None,
                fit_height: None,
                wrap_width_pixels: None,
                max_width: None,
                max_height: None,
                max_w_pre_zoom: false,
                max_h_pre_zoom: false,
                clip: None,
                blend: overlay_blend,
                effect: EffectState::default(),
            })
        }
        SongLuaOverlayKind::Quad => {
            if let Some(view_proj) = perspective_view_proj {
                let (center, size) = song_lua_overlay_rect(
                    state,
                    state.size.unwrap_or([1.0, 1.0]),
                    x_scale,
                    y_scale,
                    size_scale_x,
                    size_scale_y,
                )?;
                let mut tint = state.diffuse;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut rot_deg = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    effect_time,
                    effect_beat,
                    &mut tint,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut rot_deg,
                );
                return song_lua_projected_overlay_actor(
                    Arc::from("__white"),
                    tint,
                    overlay_blend,
                    z,
                    [
                        center[0] + effect_offset[0] * x_scale,
                        center[1] + effect_offset[1] * y_scale,
                        effect_offset[2],
                    ],
                    [size[0] * effect_scale[0], size[1] * effect_scale[1]],
                    rot_deg,
                    song_lua_overlay_uvs(state, None, flip_x, flip_y, total_elapsed),
                    view_proj,
                );
            }
            let mut actor = if let Some([left, top, right, bottom]) = state.stretch_rect {
                act!(quad:
                    align(0.0, 0.0):
                    xy(left * x_scale, top * y_scale):
                    zoomto(
                        (right - left).abs() * x_scale * size_scale_x,
                        (bottom - top).abs() * y_scale * size_scale_y
                    ):
                    diffuse(state.diffuse[0], state.diffuse[1], state.diffuse[2], state.diffuse[3]):
                    z(z)
                )
            } else {
                let size = state.size.unwrap_or([1.0, 1.0]);
                act!(quad:
                    align(0.5, 0.5):
                    xy(state.x * x_scale, state.y * y_scale):
                    zoomto(
                        size[0] * x_scale * size_scale_x,
                        size[1] * y_scale * size_scale_y
                    ):
                    diffuse(state.diffuse[0], state.diffuse[1], state.diffuse[2], state.diffuse[3]):
                    z(z)
                )
            };
            if let Actor::Sprite {
                visible,
                tint,
                cropleft,
                cropright,
                croptop,
                cropbottom,
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                blend,
                rot_x_deg,
                rot_y_deg,
                rot_z_deg,
                offset,
                world_z,
                scale,
                effect: actor_effect,
                flip_x: actor_flip_x,
                flip_y: actor_flip_y,
                ..
            } = &mut actor
            {
                let mut effect_tint = state.diffuse;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    effect_time,
                    effect_beat,
                    &mut effect_tint,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut effect_rot,
                );
                *tint = effect_tint;
                *cropleft = state.cropleft.clamp(0.0, 1.0);
                *cropright = state.cropright.clamp(0.0, 1.0);
                *croptop = state.croptop.clamp(0.0, 1.0);
                *cropbottom = state.cropbottom.clamp(0.0, 1.0);
                *fadeleft = state.fadeleft.clamp(0.0, 1.0);
                *faderight = state.faderight.clamp(0.0, 1.0);
                *fadetop = state.fadetop.clamp(0.0, 1.0);
                *fadebottom = state.fadebottom.clamp(0.0, 1.0);
                *blend = overlay_blend;
                *rot_x_deg = effect_rot[0];
                *rot_y_deg = effect_rot[1];
                *rot_z_deg = effect_rot[2];
                offset[0] += effect_offset[0] * x_scale;
                offset[1] += effect_offset[1] * y_scale;
                *world_z += effect_offset[2];
                scale[0] *= effect_scale[0];
                scale[1] *= effect_scale[1];
                *actor_effect = EffectState::default();
                *actor_flip_x ^= flip_x;
                *actor_flip_y ^= flip_y;
                *visible = state.visible;
            }
            Some(actor)
        }
    }
}

fn song_lua_player_skew_x_matrix(amount: f32) -> Matrix4 {
    Matrix4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, //
        amount, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ])
}

fn song_lua_player_skew_y_matrix(amount: f32) -> Matrix4 {
    Matrix4::from_cols_array(&[
        1.0, amount, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ])
}

#[inline(always)]
fn song_lua_fold_x_around_pivot(x: f32, pivot_x: f32, cos_y: f32) -> f32 {
    pivot_x + (x - pivot_x) * cos_y
}

fn song_lua_player_y_fold_actor(actor: Actor, pivot_x: f32, rotation_y_deg: f32) -> Actor {
    if !pivot_x.is_finite() || !rotation_y_deg.is_finite() || rotation_y_deg.abs() <= f32::EPSILON {
        return actor;
    }
    let cos_y = rotation_y_deg.to_radians().cos();
    match actor {
        Actor::Sprite {
            align,
            mut offset,
            world_z,
            size,
            source,
            tint,
            glow,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            effect,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Sprite {
                align,
                offset,
                world_z,
                size,
                source,
                tint,
                glow,
                z,
                cell,
                grid,
                uv_rect,
                visible,
                flip_x,
                flip_y,
                cropleft,
                cropright,
                croptop,
                cropbottom,
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                blend,
                mask_source,
                mask_dest,
                rot_x_deg,
                rot_y_deg,
                rot_z_deg,
                local_offset,
                local_offset_rot_sin_cos,
                texcoordvelocity,
                animate,
                state_delay,
                scale,
                effect,
            }
        }
        Actor::Text {
            align,
            mut offset,
            color,
            stroke_color,
            glow,
            font,
            content,
            attributes,
            align_text,
            z,
            mut scale,
            fit_width,
            fit_height,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            clip,
            blend,
            effect,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            scale[0] *= cos_y;
            Actor::Text {
                align,
                offset,
                color,
                stroke_color,
                glow,
                font,
                content,
                attributes,
                align_text,
                z,
                scale,
                fit_width,
                fit_height,
                wrap_width_pixels,
                max_width,
                max_height,
                max_w_pre_zoom,
                max_h_pre_zoom,
                clip,
                blend,
                effect,
            }
        }
        Actor::Mesh {
            align,
            mut offset,
            size,
            vertices,
            mode,
            visible,
            blend,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Mesh {
                align,
                offset,
                size,
                vertices,
                mode,
                visible,
                blend,
                z,
            }
        }
        Actor::TexturedMesh {
            align,
            mut offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            vertices,
            geom_cache_key,
            mode,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::TexturedMesh {
                align,
                offset,
                world_z,
                size,
                local_transform,
                texture,
                tint,
                vertices,
                geom_cache_key,
                mode,
                uv_scale,
                uv_offset,
                uv_tex_shift,
                depth_test,
                visible,
                blend,
                z,
            }
        }
        Actor::Frame {
            mut offset,
            children,
            align,
            size,
            background,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Frame {
                align,
                offset,
                size,
                children: children
                    .into_iter()
                    .map(|child| song_lua_player_y_fold_actor(child, pivot_x, rotation_y_deg))
                    .collect(),
                background,
                z,
            }
        }
        Actor::Camera {
            view_proj,
            children,
        } => Actor::Camera {
            view_proj,
            children: children
                .into_iter()
                .map(|child| song_lua_player_y_fold_actor(child, pivot_x, rotation_y_deg))
                .collect(),
        },
        Actor::Shadow { len, color, child } => Actor::Shadow {
            len,
            color,
            child: Box::new(song_lua_player_y_fold_actor(
                *child,
                pivot_x,
                rotation_y_deg,
            )),
        },
    }
}

fn song_lua_player_transform_matrix(
    playfield_center_x: f32,
    target_x: f32,
    target_y: f32,
    rotation_x_deg: f32,
    rotation_z_deg: f32,
    skew_x: f32,
    skew_y: f32,
    zoom_x: f32,
    zoom_y: f32,
    zoom_z: f32,
) -> Option<Matrix4> {
    if !playfield_center_x.is_finite()
        || !target_x.is_finite()
        || !target_y.is_finite()
        || !rotation_x_deg.is_finite()
        || !rotation_z_deg.is_finite()
        || !skew_x.is_finite()
        || !skew_y.is_finite()
        || !zoom_x.is_finite()
        || !zoom_y.is_finite()
        || !zoom_z.is_finite()
    {
        return None;
    }
    let rotation_x_deg = if rotation_x_deg.abs() <= f32::EPSILON {
        0.0
    } else {
        rotation_x_deg
    };
    let rotation_z_deg = if rotation_z_deg.abs() <= f32::EPSILON {
        0.0
    } else {
        rotation_z_deg
    };
    let skew_x = if skew_x.abs() <= f32::EPSILON {
        0.0
    } else {
        skew_x
    };
    let skew_y = if skew_y.abs() <= f32::EPSILON {
        0.0
    } else {
        skew_y
    };
    let zoom_x = if (zoom_x - 1.0).abs() <= f32::EPSILON {
        1.0
    } else {
        zoom_x
    };
    let zoom_y = if (zoom_y - 1.0).abs() <= f32::EPSILON {
        1.0
    } else {
        zoom_y
    };
    let zoom_z = if (zoom_z - 1.0).abs() <= f32::EPSILON {
        1.0
    } else {
        zoom_z
    };
    let translate_x = target_x - playfield_center_x;
    let translate_y = screen_center_y() - target_y;
    if rotation_x_deg.abs() <= f32::EPSILON
        && rotation_z_deg.abs() <= f32::EPSILON
        && skew_x.abs() <= f32::EPSILON
        && skew_y.abs() <= f32::EPSILON
        && (zoom_x - 1.0).abs() <= f32::EPSILON
        && (zoom_y - 1.0).abs() <= f32::EPSILON
        && (zoom_z - 1.0).abs() <= f32::EPSILON
        && translate_x.abs() <= f32::EPSILON
        && translate_y.abs() <= f32::EPSILON
    {
        return None;
    }

    let pivot_x = playfield_center_x - 0.5 * screen_width();
    let pivot_y = 0.5 * screen_height() - screen_center_y();
    Some(
        Matrix4::from_translation(Vector3::new(translate_x, translate_y, 0.0))
            * Matrix4::from_translation(Vector3::new(pivot_x, pivot_y, 0.0))
            * Matrix4::from_rotation_x(rotation_x_deg.to_radians())
            * Matrix4::from_rotation_z(rotation_z_deg.to_radians())
            * song_lua_player_skew_x_matrix(skew_x)
            * song_lua_player_skew_y_matrix(skew_y)
            * Matrix4::from_scale(Vector3::new(zoom_x, zoom_y, zoom_z))
            * Matrix4::from_translation(Vector3::new(-pivot_x, -pivot_y, 0.0)),
    )
}

fn apply_song_lua_player_transform(
    actors: Vec<Actor>,
    z_shift: i16,
    tint: [f32; 4],
    blend: Option<BlendMode>,
    playfield_center_x: f32,
    target_x: f32,
    target_y: f32,
    rotation_x_deg: f32,
    rotation_z_deg: f32,
    rotation_y_deg: f32,
    skew_x: f32,
    skew_y: f32,
    zoom_x: f32,
    zoom_y: f32,
    zoom_z: f32,
) -> Vec<Actor> {
    let actors = if rotation_y_deg.is_finite() && rotation_y_deg.abs() > f32::EPSILON {
        actors
            .into_iter()
            .map(|actor| song_lua_player_y_fold_actor(actor, playfield_center_x, rotation_y_deg))
            .collect()
    } else {
        actors
    };
    let Some(player_transform) = song_lua_player_transform_matrix(
        playfield_center_x,
        target_x,
        target_y,
        rotation_x_deg,
        rotation_z_deg,
        skew_x,
        skew_y,
        zoom_x,
        zoom_y,
        zoom_z,
    ) else {
        return if z_shift == 0 {
            actors
        } else {
            actors
                .into_iter()
                .map(|actor| song_lua_style_capture_actor(actor, [1.0; 4], None, z_shift))
                .collect()
        };
    };
    // notefield::build may already wrap the lane render in a perspective camera.
    // Multiply those cameras in place, and only wrap plain HUD actors here, so
    // the Lua transform affects the whole bundle without being shadowed.
    let root_camera = Matrix4::orthographic_rh_gl(
        -0.5 * screen_width(),
        0.5 * screen_width(),
        -0.5 * screen_height(),
        0.5 * screen_height(),
        -4096.0,
        4096.0,
    ) * player_transform;
    let mut out = Vec::with_capacity(actors.len().saturating_add(1));
    let mut plain_children = Vec::new();
    for actor in actors {
        match actor {
            Actor::Camera {
                view_proj,
                children,
            } => {
                if !plain_children.is_empty() {
                    out.push(Actor::Camera {
                        view_proj: root_camera,
                        children: std::mem::take(&mut plain_children),
                    });
                }
                out.push(Actor::Camera {
                    view_proj: view_proj * player_transform,
                    children,
                });
            }
            other => plain_children.push(other),
        }
    }
    if !plain_children.is_empty() {
        out.push(Actor::Camera {
            view_proj: root_camera,
            children: plain_children,
        });
    }
    if z_shift == 0 {
        if tint == [1.0; 4] && blend.is_none() {
            out
        } else {
            out.into_iter()
                .map(|actor| song_lua_style_capture_actor(actor, tint, blend, 0))
                .collect()
        }
    } else {
        out.into_iter()
            .map(|actor| song_lua_style_capture_actor(actor, tint, blend, z_shift))
            .collect()
    }
}

fn build_song_lua_layer_actors(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    song_foreground_state: SongLuaOverlayState,
    proxy_sources: &SongLuaScreenProxySources<'_>,
    asset_manager: &AssetManager,
    space_width: f32,
    space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
) -> Vec<Actor> {
    let song_lua_overlay_base_z = song_lua_add_z(
        SONG_LUA_LAYER_Z_BASE,
        song_lua_rounded_z(song_foreground_state.z),
    );
    let mut out = Vec::with_capacity(overlays.len());
    for (idx, overlay) in overlays.iter().enumerate() {
        if song_lua_overlay_aft_ancestor(overlays, idx).is_some() {
            continue;
        }
        let overlay_state = overlay_states
            .get(idx)
            .copied()
            .unwrap_or_else(|| SongLuaOverlayState::default());
        let actor = match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_proxy_source(target, proxy_sources).and_then(|source| {
                    song_lua_build_proxy_actor(
                        overlay_state,
                        song_lua_add_z(song_lua_overlay_base_z, idx as i16),
                        source,
                        space_width,
                        space_height,
                    )
                })
            }
            SongLuaOverlayKind::AftSprite { capture_name } => {
                song_lua_overlay_capture_index_by_name(overlays, capture_name).and_then(
                    |capture_index| {
                        let source = song_lua_capture_children(
                            overlays,
                            overlay_states,
                            asset_manager,
                            capture_index,
                            proxy_sources,
                            space_width,
                            space_height,
                        );
                        song_lua_build_capture_actor(
                            overlay,
                            overlay_state,
                            song_lua_add_z(song_lua_overlay_base_z, idx as i16),
                            source,
                            space_width,
                            space_height,
                        )
                    },
                )
            }
            _ => build_song_lua_overlay_actor(
                overlay,
                overlay_state,
                song_lua_overlay_camera_state(overlays, overlay_states, overlay.parent_index),
                asset_manager,
                song_lua_add_z(song_lua_overlay_base_z, idx as i16),
                space_width,
                space_height,
                effect_time,
                effect_beat,
                total_elapsed,
            ),
        };
        if let Some(actor) = actor {
            out.push(actor);
        }
    }
    out
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let cfg = crate::config::get();
    let hud_snapshot = profile::gameplay_hud_snapshot();
    let mut actors = Vec::with_capacity(96);
    let play_style = hud_snapshot.play_style;
    let player_side = hud_snapshot.player_side;
    let is_p2_single =
        play_style == profile::PlayStyle::Single && player_side == profile::PlayerSide::P2;
    let centered_single_notefield = play_style == profile::PlayStyle::Single
        && state.num_players == 1
        && cfg.center_1player_notefield;
    let song_lua_space_width = song_lua_overlay_space_width(state);
    let song_lua_space_height = song_lua_overlay_space_height(state);
    let player_color = if is_p2_single {
        color::decorative_rgba(state.active_color_index - 2)
    } else {
        state.player_color
    };
    let overlay_states = song_lua_overlay_states(state);
    let proxy_requests = song_lua_proxy_requests(&state.song_lua_overlays, &overlay_states);
    let mut underlay_proxy_source = proxy_requests.underlay.then_some(Vec::new());
    let mut overlay_proxy_source = proxy_requests.overlay.then_some(Vec::new());
    // --- Background and Filter ---
    let underlay_start = actors.len();
    actors.push(build_background(state, cfg.bg_brightness));
    for layer in &state.song_lua_background_visual_layers {
        if state.current_music_time_display < layer.start_second {
            continue;
        }
        let overlay_states = song_lua_overlay_states_from(
            state.current_music_time_display,
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
        );
        let song_foreground_state = song_lua_song_foreground_state_from(
            state.current_music_time_display,
            &layer.song_foreground,
            layer.song_foreground_events.as_slice(),
        );
        actors.extend(build_song_lua_layer_actors(
            &layer.overlays,
            &overlay_states,
            song_foreground_state,
            &SongLuaScreenProxySources::default(),
            asset_manager,
            layer.screen_width.max(1.0),
            layer.screen_height.max(1.0),
            state.current_music_time_display,
            state.current_beat,
            state.total_elapsed_in_screen,
        ));
    }
    song_lua_capture_new_actors(&mut underlay_proxy_source, &actors, underlay_start);
    let cover_alpha = |player_idx: usize| -> f32 {
        if player_idx >= state.num_players {
            return 0.0;
        }
        let profile_cover = f32::from(state.player_profiles[player_idx].hide_song_bg);
        profile_cover
            .max(effective_visibility_effects_for_player(state, player_idx).cover)
            .clamp(0.0, 1.0)
    };
    let left_cover = cover_alpha(0);
    let right_cover = if state.num_players > 1 {
        cover_alpha(1)
    } else {
        left_cover
    };
    let sw = screen_width();
    let sh = screen_height();
    let cx = screen_center_x();
    if left_cover > 0.0 || right_cover > 0.0 {
        if (left_cover - right_cover).abs() <= 0.001 {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(0.0, 0.0):
                zoomto(sw, sh):
                diffuse(0.0, 0.0, 0.0, left_cover.max(right_cover)):
                z(-99)
            ));
        } else {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(0.0, 0.0):
                zoomto(cx, sh):
                faderight(0.1):
                diffuse(0.0, 0.0, 0.0, left_cover):
                z(-99)
            ));
            actors.push(act!(quad:
                align(0.0, 0.0): xy(cx, 0.0):
                zoomto(sw - cx, sh):
                fadeleft(0.1):
                diffuse(0.0, 0.0, 0.0, right_cover):
                z(-99)
            ));
        }
    }

    // ITGmania/Simply Love parity: ScreenSyncOverlay status text.
    {
        let overlay_start = actors.len();
        let status_line_count = if let Some((status_text, line_count)) = sync_overlay_text(state) {
            actors.push(act!(text:
                font("miso"):
                settext(status_text):
                align(0.5, 0.5):
                xy(screen_center_x(), screen_center_y() + 150.0):
                shadowlength(2.0):
                strokecolor(0.0, 0.0, 0.0, 1.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(2101)
            ));
            line_count
        } else {
            0
        };

        if let Some((flash, alpha)) = toggle_flash_text(state) {
            let y = if status_line_count == 0 {
                screen_center_y() + 150.0
            } else {
                screen_center_y() + 150.0 + 20.0 * status_line_count as f32
            };
            actors.push(act!(text:
                font("miso"):
                settext(flash):
                align(0.5, 0.5):
                xy(screen_center_x(), y):
                shadowlength(2.0):
                strokecolor(0.0, 0.0, 0.0, alpha):
                diffuse(1.0, 1.0, 1.0, alpha):
                z(2101)
            ));
        }

        if state.autosync_mode != crate::game::gameplay::AutosyncMode::Off {
            let (old_offset, new_offset) =
                if state.autosync_mode == crate::game::gameplay::AutosyncMode::Machine {
                    (
                        state.initial_global_offset_seconds,
                        state.global_offset_seconds,
                    )
                } else {
                    (state.initial_song_offset_seconds, state.song_offset_seconds)
                };
            let adjustments = cached_autosync_text(state, old_offset, new_offset);
            actors.push(act!(text:
                font("miso"):
                settext(adjustments):
                align(0.5, 0.5):
                xy(screen_center_x() + 160.0, screen_center_y()):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(2101)
            ));
        }
        song_lua_capture_new_actors(&mut overlay_proxy_source, &actors, overlay_start);
    }

    // Hold START/BACK prompt (Simply Love parity: ScreenGameplay debug text).
    {
        let overlay_start = actors.len();
        const HOLD_FADE_IN_S: f32 = 1.0 / 8.0;
        const ABORT_FADE_OUT_S: f32 = 0.5;

        let y = screen_height() - 116.0;
        let msg: Option<(String, f32)> = if gameplay_lobby_wait_text(state).is_some() {
            None
        } else if let (Some(key), Some(start)) = (state.hold_to_exit_key, state.hold_to_exit_start)
        {
            let s = match key {
                crate::game::gameplay::HoldToExitKey::Start => {
                    Some(tr("Gameplay", "ContinueHoldingStartGiveUp"))
                }
                crate::game::gameplay::HoldToExitKey::Back => {
                    Some(tr("Gameplay", "ContinueHoldingBackGiveUp"))
                }
            };
            let alpha = (start.elapsed().as_secs_f32() / HOLD_FADE_IN_S).clamp(0.0, 1.0);
            s.map(|text| (text.to_string(), alpha))
        } else if let Some(exit) = &state.exit_transition {
            let t = exit.started_at.elapsed().as_secs_f32();
            match exit.kind {
                crate::game::gameplay::ExitTransitionKind::Out => {
                    let alpha = (1.0 - t / ABORT_FADE_OUT_S).clamp(0.0, 1.0);
                    Some((
                        tr("Gameplay", "ContinueHoldingStartGiveUp").to_string(),
                        alpha,
                    ))
                }
                crate::game::gameplay::ExitTransitionKind::Cancel => {
                    Some((tr("Gameplay", "ContinueHoldingBackGiveUp").to_string(), 1.0))
                }
            }
        } else if let Some(at) = state.hold_to_exit_aborted_at {
            let alpha = (1.0 - at.elapsed().as_secs_f32() / ABORT_FADE_OUT_S).clamp(0.0, 1.0);
            Some((tr("Gameplay", "DontGoBack").to_string(), alpha))
        } else {
            None
        };

        if let Some((text, alpha)) = msg
            && alpha > 0.0
        {
            actors.push(act!(text:
                font("miso"):
                settext(text):
                align(0.5, 0.5):
                xy(screen_center_x(), y):
                zoom(0.75):
                shadowlength(2.0):
                diffuse(1.0, 1.0, 1.0, alpha):
                z(1000)
            ));
        }
        song_lua_capture_new_actors(&mut overlay_proxy_source, &actors, overlay_start);
    }

    let overlay_start = actors.len();
    let lobby_snapshot = crate::game::online::lobbies::snapshot();
    if let Some(joined) = lobby_snapshot.joined_lobby.as_ref() {
        actors.extend(lobby_hud::build_panel(lobby_hud::RenderParams {
            screen_name: "ScreenGameplay",
            joined,
            z: 995,
            show_song_info: false,
            status_text: gameplay_lobby_hud_status_text(state),
        }));
    }
    song_lua_capture_new_actors(&mut overlay_proxy_source, &actors, overlay_start);

    // Fade-to-black when giving up / backing out (Simply Love parity).
    let overlay_start = actors.len();
    if let Some(exit) = &state.exit_transition {
        let alpha = crate::game::gameplay::exit_transition_alpha(exit);
        if alpha > 0.0 {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(0.0, 0.0):
                zoomto(screen_width(), screen_height()):
                diffuse(0.0, 0.0, 0.0, alpha):
                z(1500)
            ));
        }
    }
    song_lua_capture_new_actors(&mut overlay_proxy_source, &actors, overlay_start);

    let notefield_width = |player_idx: usize| -> f32 {
        let Some(ns) = state.noteskin[player_idx].as_ref() else {
            return 256.0;
        };
        let receptor_ns = state.receptor_noteskin[player_idx].as_deref().unwrap_or(ns);
        let cols = state
            .cols_per_player
            .min(ns.column_xs.len())
            .min(receptor_ns.receptor_off.len());
        if cols == 0 {
            return 256.0;
        }
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        for x in ns.column_xs.iter().take(cols) {
            let xf = *x as f32;
            min_x = min_x.min(xf);
            max_x = max_x.max(xf);
        }

        // SL parity (GetNotefieldWidth): layout width is style/lane based and must
        // not shrink/grow with Mini (field zoom).
        let target_arrow_px = 64.0;
        let size = receptor_ns.receptor_off[0].size();
        let w = size[0].max(0) as f32;
        let h = size[1].max(0) as f32;
        let arrow_w = if h > 0.0 && target_arrow_px > 0.0 {
            w * (target_arrow_px / h)
        } else {
            w
        };
        (max_x - min_x) + arrow_w
    };

    let build_player_bundle = |player_idx: usize,
                               profile: &profile::Profile,
                               placement: notefield::FieldPlacement,
                               requests: SongLuaPlayerProxyRequests| {
        let notefield::BuiltNotefield {
            actors,
            layout_center_x,
            field_actors,
            judgment_actors,
            combo_actors,
        } = notefield::build_bundles(
            state,
            profile,
            placement,
            play_style,
            cfg.center_1player_notefield,
            notefield::ProxyCaptureRequests {
                note_field: requests.note_field,
                judgment: requests.judgment,
                combo: requests.combo,
            },
        );
        let player_actor = &state.song_lua_player_actors[player_idx];
        let player_state = song_lua_player_render_state(state, player_idx);
        let song_lua_active = !state.song.foreground_lua_changes.is_empty();
        let rotation_x = player_state.rot_x_deg + state.song_lua_player_rotation_x[player_idx];
        let rotation_z = player_state.rot_z_deg + state.song_lua_player_rotation_z[player_idx];
        let rotation_y = player_state.rot_y_deg + state.song_lua_player_rotation_y[player_idx];
        let skew_x = state.song_lua_player_skew_x[player_idx];
        let skew_y = state.song_lua_player_skew_y[player_idx];
        let zoom_x = player_state.zoom_x * state.song_lua_player_zoom_x[player_idx];
        let zoom_y = player_state.zoom_y * state.song_lua_player_zoom_y[player_idx];
        let zoom_z = player_state.zoom_z * state.song_lua_player_zoom_z[player_idx];
        let target_x = state.song_lua_player_x[player_idx].unwrap_or(player_state.x);
        let target_y = state.song_lua_player_y[player_idx].unwrap_or(player_state.y);
        let z_shift = song_lua_player_layer_z(
            song_lua_active,
            player_actor,
            player_state,
            state.song_lua_player_z[player_idx],
        );
        let player_blend = match player_state.blend {
            SongLuaOverlayBlendMode::Alpha => None,
            SongLuaOverlayBlendMode::Add => Some(BlendMode::Add),
        };
        let render_bundle = |bundle| {
            if !player_state.visible {
                Vec::new()
            } else {
                apply_song_lua_player_transform(
                    bundle,
                    z_shift,
                    player_state.diffuse,
                    player_blend,
                    layout_center_x,
                    target_x,
                    target_y,
                    rotation_x,
                    rotation_z,
                    rotation_y,
                    skew_x,
                    skew_y,
                    zoom_x,
                    zoom_y,
                    zoom_z,
                )
            }
        };
        let player = render_bundle(actors);
        let proxy_sources = [
            requests.note_field.then(|| render_bundle(field_actors)),
            requests.judgment.then(|| render_bundle(judgment_actors)),
            requests.combo.then(|| render_bundle(combo_actors)),
        ];
        (player, layout_center_x, proxy_sources)
    };

    let (
        p1_actors,
        p2_actors,
        p1_proxy_sources,
        p2_proxy_sources,
        playfield_center_x,
        per_player_fields,
    ): (
        Vec<Actor>,
        Option<Vec<Actor>>,
        [Option<Vec<Actor>>; 3],
        [Option<Vec<Actor>>; 3],
        f32,
        [(usize, f32); 2],
    ) = match play_style {
        profile::PlayStyle::Versus => {
            let (p1, p1_x, p1_sources) = build_player_bundle(
                0,
                &state.player_profiles[0],
                notefield::FieldPlacement::P1,
                proxy_requests.players[0],
            );
            let (p2, p2_x, p2_sources) = build_player_bundle(
                1,
                &state.player_profiles[1],
                notefield::FieldPlacement::P2,
                proxy_requests.players[1],
            );
            (
                p1,
                Some(p2),
                p1_sources,
                p2_sources,
                p1_x,
                [(0, p1_x), (1, p2_x)],
            )
        }
        _ => {
            let placement = if is_p2_single {
                notefield::FieldPlacement::P2
            } else {
                notefield::FieldPlacement::P1
            };
            let (nf, nf_x, nf_sources) = build_player_bundle(
                0,
                &state.player_profiles[0],
                placement,
                proxy_requests.players[0],
            );
            (
                nf,
                None,
                nf_sources,
                [None, None, None],
                nf_x,
                [(0, nf_x), (usize::MAX, 0.0)],
            )
        }
    };
    let replacement_proxy_sources = [
        SongLuaPlayerProxySources {
            player: proxy_requests.players[0]
                .player
                .then_some(p1_actors.as_slice()),
            note_field: p1_proxy_sources[0].as_deref(),
            judgment: p1_proxy_sources[1].as_deref(),
            combo: p1_proxy_sources[2].as_deref(),
        },
        SongLuaPlayerProxySources {
            player: proxy_requests.players[1]
                .player
                .then(|| p2_actors.as_deref())
                .flatten(),
            note_field: p2_proxy_sources[0].as_deref(),
            judgment: p2_proxy_sources[1].as_deref(),
            combo: p2_proxy_sources[2].as_deref(),
        },
    ];
    let replacement_active_players = song_lua_replacement_active_players(
        &state.song_lua_overlays,
        &overlay_states,
        &replacement_proxy_sources,
    );

    // Danger overlay (Simply Love parity): red flashing in danger + green recovery, optional HideDanger.
    {
        let underlay_start = actors.len();
        let sw = screen_width();
        let sh = screen_height();
        let cx = screen_center_x();

        for player_idx in 0..state.num_players {
            let Some(rgba) = crate::game::gameplay::danger_overlay_rgba(state, player_idx) else {
                continue;
            };
            let (x, w, fl, fr) = match play_style {
                profile::PlayStyle::Double => (0.0, sw, 0.0, 0.0),
                profile::PlayStyle::Versus => {
                    if player_idx == 0 {
                        (0.0, cx, 0.0, 0.1)
                    } else {
                        (cx, sw - cx, 0.1, 0.0)
                    }
                }
                profile::PlayStyle::Single => {
                    if centered_single_notefield {
                        (0.0, sw, 0.0, 0.0)
                    } else if is_p2_single {
                        (cx, sw - cx, 0.1, 0.0)
                    } else {
                        (0.0, cx, 0.0, 0.1)
                    }
                }
            };

            actors.push(act!(quad:
                align(0.0, 0.0): xy(x, 0.0):
                zoomto(w, sh):
                fadeleft(fl): faderight(fr):
                diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
                z(-99)
            ));
        }
        song_lua_capture_new_actors(&mut underlay_proxy_source, &actors, underlay_start);
    }

    // Background filter per-player (Simply Love parity): draw behind each notefield, not full-screen.
    let underlay_start = actors.len();
    for &(player_idx, field_x) in &per_player_fields {
        if player_idx == usize::MAX || player_idx >= state.num_players {
            continue;
        }
        let filter_alpha = state.player_profiles[player_idx].background_filter.alpha();
        if filter_alpha <= 0.0 {
            continue;
        }
        actors.push(act!(quad:
            align(0.5, 0.5): xy(field_x, screen_center_y()):
            zoomto(notefield_width(player_idx), screen_height()):
            diffuse(0.0, 0.0, 0.0, filter_alpha):
            z(-99)
        ));
    }
    song_lua_capture_new_actors(&mut underlay_proxy_source, &actors, underlay_start);

    // Simply Love parity: BGAnimations/ScreenGameplay underlay/Shared/Header.lua.
    // This translucent top strip sits underneath the UpperNPSGraph and other HUD actors.
    let underlay_start = actors.len();
    actors.push(act!(quad:
        align(0.5, 0.0): xy(screen_center_x(), 0.0):
        zoomto(screen_width(), 80.0):
        diffuse(0.0, 0.0, 0.0, 0.85):
        z(83)
    ));
    song_lua_capture_new_actors(&mut underlay_proxy_source, &actors, underlay_start);

    actors.reserve(p1_actors.len() + p2_actors.as_ref().map_or(0, Vec::len) + 48);
    let mut p1_actor_range = None;
    let mut p2_actor_range = None;
    if let Some(p2_actors) = p2_actors {
        if !replacement_active_players[1] {
            let start = actors.len();
            actors.extend(p2_actors);
            p2_actor_range = Some((start, actors.len()));
        }
    }
    if !replacement_active_players[0] {
        let start = actors.len();
        actors.extend(p1_actors);
        p1_actor_range = Some((start, actors.len()));
    }
    let underlay_tail_start = actors.len();
    let clamped_width = screen_width().clamp(640.0, 854.0);
    let score_x_p1 = screen_center_x() - clamped_width / 4.3;
    let score_x_p2 = screen_center_x() + clamped_width / 2.75;
    let diff_x_p1 = screen_center_x() - widescale(292.5, 342.5);
    let diff_x_p2 = screen_center_x() + widescale(292.5, 342.5);

    let mut players = [(0usize, profile::PlayerSide::P1, 0.0, 0.0, 0.0, 0.0); 2];
    let player_count = match play_style {
        profile::PlayStyle::Versus => {
            players[0] = (
                0,
                profile::PlayerSide::P1,
                per_player_fields[0].1,
                diff_x_p1,
                score_x_p1,
                score_x_p2,
            );
            players[1] = (
                1,
                profile::PlayerSide::P2,
                per_player_fields[1].1,
                diff_x_p2,
                score_x_p2,
                score_x_p1,
            );
            2
        }
        _ if is_p2_single => {
            players[0] = (
                0,
                profile::PlayerSide::P2,
                per_player_fields[0].1,
                diff_x_p2,
                score_x_p2,
                score_x_p1,
            );
            1
        }
        _ => {
            players[0] = (
                0,
                profile::PlayerSide::P1,
                per_player_fields[0].1,
                diff_x_p1,
                score_x_p1,
                score_x_p2,
            );
            1
        }
    };

    let is_ultrawide = screen_width() / screen_height().max(1.0) > (21.0 / 9.0);
    let graph_center_shift = widescale(45.0, 95.0);

    for &(player_idx, player_side, field_x, _, _, _) in &players[..player_count] {
        if !state.player_profiles[player_idx].nps_graph_at_top {
            continue;
        }
        let graph_w = state.density_graph_top_w[player_idx];
        let graph_h = state.density_graph_top_h;
        let graph_mesh_h = graph_h * state.density_graph_top_scale_y[player_idx].clamp(0.0, 1.0);
        if graph_w <= 0.0 || graph_h <= 0.0 || graph_mesh_h <= 0.0 {
            continue;
        }
        let note_field_is_centered = (field_x - screen_center_x()).abs() < 1.0;
        let x = if note_field_is_centered {
            screen_center_x() - graph_w * 0.5
        } else if player_side == profile::PlayerSide::P1 {
            screen_center_x() - graph_w - graph_center_shift
        } else {
            screen_center_x() + graph_center_shift
        };
        let y_bottom = 71.0;
        let y_top = y_bottom - graph_h;
        let y_mesh_top = y_bottom - graph_mesh_h;
        let graph_bg_alpha = if state.player_profiles[player_idx].transparent_density_graph_bg {
            0.5
        } else {
            1.0
        };

        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, y_top):
            zoomto(graph_w, graph_h):
            diffuse(30.0 / 255.0, 40.0 / 255.0, 47.0 / 255.0, graph_bg_alpha):
            z(84)
        ));

        if let Some(mesh) = &state.density_graph.top_mesh[player_idx]
            && !mesh.is_empty()
        {
            actors.push(Actor::Mesh {
                align: [0.0, 0.0],
                offset: [x, y_mesh_top],
                size: [SizeSpec::Px(graph_w), SizeSpec::Px(graph_mesh_h)],
                vertices: mesh.clone(),
                mode: MeshMode::Triangles,
                visible: true,
                blend: BlendMode::Alpha,
                z: 85,
            });
        }

        let duration =
            (state.density_graph_last_second - state.density_graph_first_second).max(0.001_f32);
        let progress_w = (((state.current_music_time_display - state.density_graph_first_second)
            / duration)
            * graph_w)
            .clamp(0.0, graph_w);
        if progress_w > 0.0 {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(x, y_top):
                zoomto(progress_w, graph_h):
                diffuse(0.0, 0.0, 0.0, 0.85):
                z(86)
            ));
        }
    }

    for &(player_idx, player_side, field_x, diff_x, score_x_normal, score_x_other) in
        &players[..player_count]
    {
        let chart = &state.charts[player_idx];
        let difficulty_color = color::difficulty_rgba(&chart.difficulty, state.active_color_index);
        let meter_text = cached_meter_text(chart.meter);
        let meter_detail_text =
            color::difficulty_display_name_for_song(&chart.difficulty, &state.song.title, true);

        // Difficulty Box
        let y = 56.0;
        let mut diff_children = Vec::with_capacity(if cfg.zmod_rating_box_text { 3 } else { 2 });
        diff_children.push(act!(quad:
            align(0.5, 0.5): xy(0.0, 0.0): zoomto(30.0, 30.0):
            diffuse(difficulty_color[0], difficulty_color[1], difficulty_color[2], 1.0)
        ));
        let meter_y = if cfg.zmod_rating_box_text { -4.0 } else { 0.0 };
        diff_children.push(act!(text:
            font("wendy"): settext(meter_text): align(0.5, 0.5): xy(0.0, meter_y):
            zoom(0.4): diffuse(0.0, 0.0, 0.0, 1.0)
        ));
        if cfg.zmod_rating_box_text {
            diff_children.push(act!(text:
                font("miso"):
                settext(meter_detail_text):
                align(0.5, 0.5): xy(0.0, 9.5):
                zoom(0.5):
                diffuse(0.0, 0.0, 0.0, 1.0)
            ));
        }
        actors.push(Actor::Frame {
            align: [0.5, 0.5],
            offset: [diff_x, y],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            children: diff_children,
            background: None,
            z: 90,
        });

        // Score Display
        let note_field_is_centered = (field_x - screen_center_x()).abs() < 1.0;
        let nps_graph_at_top = state.player_profiles[player_idx].nps_graph_at_top;
        let single_score_swapped = state.num_players == 1
            && play_style != profile::PlayStyle::Double
            && nps_graph_at_top
            && !note_field_is_centered;
        let score_x = if single_score_swapped {
            score_x_other
        } else {
            score_x_normal
        };
        let hide_score_for_top_graph = state.num_players > 1 && nps_graph_at_top && !is_ultrawide;

        if !state.player_profiles[player_idx].hide_score && !hide_score_for_top_graph {
            let score_y = 56.0;
            let show_ex_score = state.player_profiles[player_idx].show_ex_score;
            let show_hard_ex_score =
                show_ex_score && state.player_profiles[player_idx].show_hard_ex_score;
            let (score_text, score_color) = if show_ex_score {
                let ex_percent = crate::game::gameplay::display_ex_score_percent(state, player_idx);
                (
                    cached_score_2dp(ex_percent.max(0.0)),
                    color::JUDGMENT_RGBA[0],
                )
            } else {
                let score_percent =
                    (crate::game::gameplay::display_itg_score_percent(state, player_idx) * 100.0)
                        as f32;
                (cached_score_2dp(score_percent as f64), [1.0, 1.0, 1.0, 1.0])
            };

            let is_p2_side = player_side == profile::PlayerSide::P2;
            // Arrow Cloud parity: EX remains the "normal" score position/anchor.
            // H.EX is placed at a different x on P2 so it appears to the left of EX.
            actors.push(act!(text:
                font("wendy_monospace_numbers"): settext(score_text):
                align(1.0, 1.0): xy(score_x, score_y):
                zoom(0.5): horizalign(right):
                diffuse(score_color[0], score_color[1], score_color[2], score_color[3]):
                z(90)
            ));

            if show_hard_ex_score {
                let hard_ex_percent =
                    crate::game::gameplay::display_hard_ex_score_percent(state, player_idx);
                let hex = color::HARD_EX_SCORE_RGBA;
                let hard_ex_x = if single_score_swapped {
                    let swapped_base = if is_p2_side {
                        screen_center_x() - clamped_width / 4.3
                    } else {
                        screen_center_x() + clamped_width / 4.3
                    };
                    swapped_base + 115.0
                } else if is_p2_side {
                    // Arrow Cloud: HardEX uses /4.3 on P2 (while EX uses /2.75).
                    screen_center_x() + clamped_width / 4.3
                } else {
                    score_x
                };

                if is_p2_side {
                    actors.push(act!(text:
                        font("wendy_monospace_numbers"):
                        settext(cached_score_2dp(hard_ex_percent.max(0.0))):
                        align(1.0, 0.0): xy(hard_ex_x, score_y):
                        zoom(0.25): horizalign(right):
                        diffuse(hex[0], hex[1], hex[2], hex[3]):
                        z(90)
                    ));
                } else {
                    actors.push(act!(text:
                        font("wendy_monospace_numbers"):
                        settext(cached_score_2dp(hard_ex_percent.max(0.0))):
                        align(0.0, 0.0): xy(hard_ex_x, score_y):
                        zoom(0.25): horizalign(left):
                        diffuse(hex[0], hex[1], hex[2], hex[3]):
                        z(90)
                    ));
                }
            }
        }
    }
    // Current BPM Display (1:1 with Simply Love)
    {
        let base_bpm = state.timing.get_bpm_for_beat(state.current_beat_display);
        let rate = if state.music_rate.is_finite() {
            state.music_rate as f64
        } else {
            1.0
        };
        let display_bpm = if base_bpm.is_finite() {
            f64::from(base_bpm) * rate
        } else {
            0.0
        };
        let bpm_text = cached_bpm_text(display_bpm, cfg.show_bpm_decimal);
        // Final world-space positions derived from analyzing the SM Lua transforms.
        // The parent frame is bottom-aligned to y=52, and its children are positioned
        // relative to that y-coordinate, with a zoom of 1.33 applied to the whole group.
        let frame_origin_y = 51.0;
        let frame_zoom = 1.33;
        // The BPM text is at y=0 relative to the frame's origin. Its final position is just the origin.
        let bpm_center_y = frame_origin_y;
        // The Rate text is at y=12 relative to the frame's origin. Its offset is scaled by the frame's zoom.
        let rate_center_y = 12.0f64.mul_add(frame_zoom, frame_origin_y);
        let bpm_final_zoom = 1.0 * frame_zoom;
        let rate_final_zoom = 0.5 * frame_zoom;
        let mut bpm_x = screen_center_x();
        let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;
        if state.num_players == 1
            && note_field_is_centered
            && state.player_profiles[0].nps_graph_at_top
        {
            let side_shift = if player_side == profile::PlayerSide::P1 {
                0.3
            } else {
                -0.3
            };
            bpm_x = screen_center_x() + screen_width() * side_shift;
        }
        actors.push(act!(text:
            font("miso"): settext(bpm_text):
            align(0.5, 0.5): xy(bpm_x, bpm_center_y):
            zoom(bpm_final_zoom): horizalign(center): z(90)
        ));
        let rate = if state.music_rate.is_finite() {
            state.music_rate
        } else {
            1.0
        };
        let rate_text = cached_rate_text(rate);
        actors.push(act!(text:
            font("miso"): settext(rate_text):
            align(0.5, 0.5): xy(bpm_x, rate_center_y):
            zoom(rate_final_zoom): horizalign(center): z(90)
        ));
    }
    // Song Title Box (SongMeter)
    {
        let w = widescale(310.0, 417.0);
        let h = 22.0;
        let box_cx = screen_center_x();
        let box_cy = 20.0;
        let mut frame_children = Vec::with_capacity(4);
        frame_children.push(act!(quad: align(0.5, 0.5): xy(w / 2.0, h / 2.0): zoomto(w, h): diffuse(1.0, 1.0, 1.0, 1.0): z(0) ));
        frame_children.push(act!(quad: align(0.5, 0.5): xy(w / 2.0, h / 2.0): zoomto(w - 4.0, h - 4.0): diffuse(0.0, 0.0, 0.0, 1.0): z(1) ));
        if state.song.total_length_seconds > 0 && state.current_music_time_display >= 0.0 {
            let progress = (state.current_music_time_display
                / state.song.total_length_seconds as f32)
                .clamp(0.0, 1.0);
            frame_children.push(act!(quad:
                align(0.0, 0.5): xy(2.0, h / 2.0): zoomto((w - 4.0) * progress, h - 4.0):
                diffuse(player_color[0], player_color[1], player_color[2], 1.0): z(2)
            ));
        }
        let full_title = state.song_full_title.clone();
        frame_children.push(act!(text:
            font("miso"): settext(full_title): align(0.5, 0.5): xy(w / 2.0, h / 2.0):
            zoom(0.8): maxwidth(screen_width() / 2.5 - 10.0): horizalign(center): z(3)
        ));
        actors.push(Actor::Frame {
            align: [0.5, 0.5],
            offset: [box_cx, box_cy],
            size: [SizeSpec::Px(w), SizeSpec::Px(h)],
            background: None,
            z: 90,
            children: frame_children,
        });
    }
    // --- Life Meter ---
    {
        let player_life_color = |player_idx: usize| -> [f32; 4] {
            match play_style {
                profile::PlayStyle::Versus => {
                    if player_idx == 0 {
                        color::decorative_rgba(state.active_color_index)
                    } else {
                        color::decorative_rgba(state.active_color_index - 2)
                    }
                }
                _ => {
                    if is_p2_single {
                        color::decorative_rgba(state.active_color_index - 2)
                    } else {
                        color::decorative_rgba(state.active_color_index)
                    }
                }
            }
        };
        let rainbow_life_color = |elapsed: f32| -> [f32; 4] {
            let phase = elapsed * 2.0;
            let r = (phase + 0.0).sin() * 0.5 + 0.5;
            let g = (phase + std::f32::consts::TAU / 3.0).sin() * 0.5 + 0.5;
            let b = (phase + (2.0 * std::f32::consts::TAU) / 3.0).sin() * 0.5 + 0.5;
            [r, g, b, 1.0]
        };
        let responsive_life_color = |life: f32| -> [f32; 4] {
            let life = life.clamp(0.0, 1.0);
            if life >= 0.9 {
                [0.0, 1.0, ((life - 0.9) * 10.0).clamp(0.0, 1.0), 1.0]
            } else if life >= 0.5 {
                [((0.9 - life) * 2.5).clamp(0.0, 1.0), 1.0, 0.0, 1.0]
            } else {
                [1.0, ((life - 0.2) * (10.0 / 3.0)).clamp(0.0, 1.0), 0.0, 1.0]
            }
        };
        let fill_life_color = |player_idx: usize, life: f32, dead: bool| -> [f32; 4] {
            let profile = &state.player_profiles[player_idx];
            let is_hot = !dead && life >= 1.0;
            if is_hot {
                if profile.rainbow_max {
                    rainbow_life_color(state.total_elapsed_in_screen)
                } else {
                    [1.0, 1.0, 1.0, 1.0]
                }
            } else if profile.responsive_colors {
                responsive_life_color(life)
            } else {
                player_life_color(player_idx)
            }
        };
        let show_standard_life_percent = screen_width() / screen_height().max(1.0) >= (16.0 / 9.0);

        let mut life_players = [(0usize, profile::PlayerSide::P1); 2];
        let life_player_count = match play_style {
            profile::PlayStyle::Versus => {
                life_players[0] = (0, profile::PlayerSide::P1);
                life_players[1] = (1, profile::PlayerSide::P2);
                2
            }
            _ if is_p2_single => {
                life_players[0] = (0, profile::PlayerSide::P2);
                1
            }
            _ => {
                life_players[0] = (0, profile::PlayerSide::P1);
                1
            }
        };

        for &(player_idx, side) in &life_players[..life_player_count] {
            if state.player_profiles[player_idx].hide_lifebar {
                continue;
            }

            // Latch-to-zero for rendering the very frame we die.
            let dead =
                state.players[player_idx].is_failing || state.players[player_idx].life <= 0.0;
            let life_for_render = if dead {
                0.0
            } else {
                state.players[player_idx].life.clamp(0.0, 1.0)
            };
            let is_hot = !dead && life_for_render >= 1.0;
            let life_color = fill_life_color(player_idx, life_for_render, dead);
            let life_percent = life_for_render * 100.0;
            let life_percent_text = cached_life_percent_text(life_percent);

            match state.player_profiles[player_idx].lifemeter_type {
                profile::LifeMeterType::Standard => {
                    let w = 136.0;
                    let h = 18.0;
                    let meter_cy = 20.0;
                    let meter_cx = screen_center_x()
                        + match play_style {
                            profile::PlayStyle::Versus => match side {
                                profile::PlayerSide::P1 => -widescale(238.0, 288.0),
                                profile::PlayerSide::P2 => widescale(238.0, 288.0),
                            },
                            _ => match side {
                                profile::PlayerSide::P1 => -widescale(238.0, 288.0),
                                profile::PlayerSide::P2 => widescale(238.0, 288.0),
                            },
                        };

                    // Frames/border
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w + 4.0, h + 4.0):
                        diffuse(1.0, 1.0, 1.0, 1.0): z(90)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w, h):
                        diffuse(0.0, 0.0, 0.0, 1.0): z(91)
                    ));

                    let filled_width = w * life_for_render;
                    // Never draw swoosh if dead OR nothing to fill.
                    if filled_width > 0.0 && !dead {
                        // Logic Parity:
                        // velocity = -(songposition:GetCurBPS() * 0.5)
                        // if songposition:GetFreeze() or songposition:GetDelay() then velocity = 0 end
                        let bps = state.timing.get_bpm_for_beat(state.current_beat_display) / 60.0;
                        let velocity_x = if state.is_in_freeze || state.is_in_delay {
                            0.0
                        } else {
                            -(bps * 0.5)
                        };

                        let swoosh_alpha = if is_hot { 1.0 } else { 0.2 };

                        // MeterSwoosh
                        actors.push(act!(sprite("swoosh.png"):
                            align(0.0, 0.5):
                            xy(meter_cx - w / 2.0, meter_cy):
                            zoomto(filled_width, h):
                            diffusealpha(swoosh_alpha):
                            texcoordvelocity(velocity_x, 0.0):
                            z(93)
                        ));

                        // MeterFill
                        actors.push(act!(quad:
                            align(0.0, 0.5):
                            xy(meter_cx - w / 2.0, meter_cy):
                            zoomto(filled_width, h):
                            diffuse(life_color[0], life_color[1], life_color[2], 1.0):
                            z(92)
                        ));
                    }

                    if state.player_profiles[player_idx].show_life_percent
                        && show_standard_life_percent
                        && !is_hot
                    {
                        let life_text_color = player_life_color(player_idx);
                        let (outer_x, inner_x, text_x, align_x) = if side == profile::PlayerSide::P1
                        {
                            (meter_cx - 76.0, meter_cx - 77.0, meter_cx - 77.0, 1.0)
                        } else {
                            (meter_cx + 76.0, meter_cx + 77.0, meter_cx + 78.0, 0.0)
                        };
                        actors.push(act!(quad:
                            align(align_x, 0.5): xy(outer_x, meter_cy):
                            zoomto(44.0, 18.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(94)
                        ));
                        actors.push(act!(quad:
                            align(align_x, 0.5): xy(inner_x, meter_cy):
                            zoomto(42.0, 16.0):
                            diffuse(0.0, 0.0, 0.0, 1.0):
                            z(95)
                        ));
                        actors.push(act!(text:
                            font("miso"): settext(life_percent_text.clone()):
                            align(align_x, 0.5): xy(text_x, meter_cy):
                            zoom(1.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(96)
                        ));
                    }
                }
                profile::LifeMeterType::Surround => {
                    let sw = screen_width();
                    let sh = screen_height();
                    let w = sw * 0.5;
                    let h = sh - 80.0;
                    let y = 80.0;
                    let croptop = 1.0 - life_for_render;

                    if play_style == profile::PlayStyle::Double {
                        // Double: two quads flanking left/right, moving in unison.
                        actors.push(act!(quad:
                            align(0.0, 0.0): xy(0.0, y):
                            zoomto(w, h):
                            diffuse(0.2, 0.2, 0.2, 1.0):
                            faderight(0.8):
                            croptop(croptop):
                            z(-98)
                        ));
                        actors.push(act!(quad:
                            align(1.0, 0.0): xy(sw, y):
                            zoomto(w, h):
                            diffuse(0.2, 0.2, 0.2, 1.0):
                            fadeleft(0.8):
                            croptop(croptop):
                            z(-98)
                        ));
                        // Only one player in Double style.
                        break;
                    }

                    let mut surround_color = if state.player_profiles[player_idx].responsive_colors
                    {
                        let mut c = responsive_life_color(life_for_render);
                        c[3] = 0.2;
                        c
                    } else {
                        [0.2, 0.2, 0.2, 1.0]
                    };
                    if life_for_render >= 1.0 && state.player_profiles[player_idx].rainbow_max {
                        let mut c = rainbow_life_color(state.total_elapsed_in_screen);
                        c[3] = if state.player_profiles[player_idx].responsive_colors {
                            0.2
                        } else {
                            1.0
                        };
                        surround_color = c;
                    }

                    match side {
                        profile::PlayerSide::P1 => {
                            actors.push(act!(quad:
                                align(0.0, 0.0): xy(0.0, y):
                                zoomto(w, h):
                                diffuse(surround_color[0], surround_color[1], surround_color[2], surround_color[3]):
                                faderight(0.8):
                                croptop(croptop):
                                z(-98)
                            ));
                        }
                        profile::PlayerSide::P2 => {
                            actors.push(act!(quad:
                                align(1.0, 0.0): xy(sw, y):
                                zoomto(w, h):
                                diffuse(surround_color[0], surround_color[1], surround_color[2], surround_color[3]):
                                fadeleft(0.8):
                                croptop(croptop):
                                z(-98)
                            ));
                        }
                    }
                }
                profile::LifeMeterType::Vertical => {
                    let bar_w = 16.0;
                    let bar_h = 250.0;

                    let x = {
                        // SL: default to _screen.cx +/- SL_WideScale(302, 400).
                        let mut x = screen_center_x()
                            + match side {
                                profile::PlayerSide::P1 => -widescale(302.0, 400.0),
                                profile::PlayerSide::P2 => widescale(302.0, 400.0),
                            };

                        // SL: if double style, position next to notefield.
                        if play_style == profile::PlayStyle::Double {
                            let half_nf = notefield_width(player_idx) * 0.5;
                            x = screen_center_x()
                                + match side {
                                    profile::PlayerSide::P1 => -(half_nf + 10.0),
                                    profile::PlayerSide::P2 => half_nf + 10.0,
                                };
                        }

                        x
                    };

                    let cy = bar_h + 10.0;
                    // Frames/border
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(x, cy): zoomto(bar_w + 2.0, bar_h + 2.0):
                        diffuse(1.0, 1.0, 1.0, 1.0): z(90)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(x, cy): zoomto(bar_w, bar_h):
                        diffuse(0.0, 0.0, 0.0, 1.0): z(91)
                    ));

                    let filled_h = bar_h * life_for_render;

                    // MeterFill
                    if filled_h > 0.0 {
                        actors.push(act!(quad:
                            align(0.0, 1.0):
                            xy(x - bar_w * 0.5, cy + bar_h * 0.5):
                            zoomto(bar_w, filled_h):
                            diffuse(life_color[0], life_color[1], life_color[2], 1.0):
                            z(92)
                        ));
                    }

                    // MeterSwoosh
                    if filled_h > 0.0 && !dead {
                        let bps = state.timing.get_bpm_for_beat(state.current_beat_display) / 60.0;
                        let velocity_x = if state.is_in_freeze || state.is_in_delay {
                            0.0
                        } else {
                            -(bps * 0.5)
                        };
                        let swoosh_alpha = if is_hot { 1.0 } else { 0.2 };

                        actors.push(act!(sprite("swoosh.png"):
                            align(0.5, 0.5):
                            xy(x, (cy + bar_h * 0.5) - filled_h * 0.5):
                            zoomto(filled_h, bar_w):
                            diffusealpha(swoosh_alpha):
                            rotationz(90.0):
                            texcoordvelocity(velocity_x, 0.0):
                            z(93)
                        ));
                    }

                    if state.player_profiles[player_idx].show_life_percent && !is_hot {
                        let life_text_color = player_life_color(player_idx);
                        let text_y = cy + bar_h * 0.5 - (bar_h * life_for_render);
                        let (outer_x, inner_x, text_x, align_x) = if side == profile::PlayerSide::P1
                        {
                            (x + 10.0, x + 11.0, x + 12.0, 0.0)
                        } else {
                            (x - 11.0, x - 12.0, x - 13.0, 1.0)
                        };
                        actors.push(act!(quad:
                            align(align_x, 0.5): xy(outer_x, text_y):
                            zoomto(44.0, 18.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(94)
                        ));
                        actors.push(act!(quad:
                            align(align_x, 0.5): xy(inner_x, text_y):
                            zoomto(42.0, 16.0):
                            diffuse(0.0, 0.0, 0.0, 1.0):
                            z(95)
                        ));
                        actors.push(act!(text:
                            font("miso"): settext(life_percent_text.clone()):
                            align(align_x, 0.5): xy(text_x, text_y):
                            zoom(1.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(96)
                        ));
                    }
                }
            }
        }
    }
    // Simply Love parity: keep Stage/Event text visible at the footer after intro animation ends.
    if !state.stage_intro_text.is_empty()
        && state.total_elapsed_in_screen >= INTRO_TEXT_SETTLE_SECONDS
    {
        actors.push(act!(text:
            font("wendy"): settext(state.stage_intro_text.clone()):
            align(0.5, 0.5): xy(screen_center_x(), screen_height() - 30.0):
            zoom(0.4):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(110)
        ));
    }
    let p1_avatar = hud_snapshot
        .p1
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });
    let p2_avatar = hud_snapshot
        .p2
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });

    let p1_joined = hud_snapshot.p1.joined;
    let p2_joined = hud_snapshot.p2.joined;
    let p1_guest = hud_snapshot.p1.guest;
    let p2_guest = hud_snapshot.p2.guest;

    let insert_card_text = tr("Common", "InsertCard");
    let (p1_footer_text, p1_footer_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                &*insert_card_text
            } else {
                hud_snapshot.p1.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (None, None)
    };
    let (p2_footer_text, p2_footer_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                &*insert_card_text
            } else {
                hud_snapshot.p2.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (None, None)
    };

    let (footer_left, footer_right, left_avatar, right_avatar) =
        if play_style == profile::PlayStyle::Versus {
            (
                p1_footer_text,
                p2_footer_text,
                p1_footer_avatar,
                p2_footer_avatar,
            )
        } else {
            match player_side {
                profile::PlayerSide::P1 => (p1_footer_text, None, p1_footer_avatar, None),
                profile::PlayerSide::P2 => (None, p2_footer_text, None, p2_footer_avatar),
            }
        };
    actors.push(screen_bar::build(ScreenBarParams {
        title: "",
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        fg_color: [1.0; 4],
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
    }));
    let show_step_stats = match play_style {
        profile::PlayStyle::Single | profile::PlayStyle::Double => state
            .player_profiles
            .first()
            .is_some_and(|p| p.data_visualizations == profile::DataVisualizations::StepStatistics),
        profile::PlayStyle::Versus => {
            state.player_profiles.first().is_some_and(|p| {
                p.data_visualizations == profile::DataVisualizations::StepStatistics
            }) || state.player_profiles.get(1).is_some_and(|p| {
                p.data_visualizations == profile::DataVisualizations::StepStatistics
            })
        }
    };
    if show_step_stats {
        if state.num_cols <= 4 && play_style != profile::PlayStyle::Versus {
            actors.extend(gameplay_stats::build(
                state,
                asset_manager,
                playfield_center_x,
                player_side,
            ));
        } else if play_style == profile::PlayStyle::Versus {
            actors.extend(gameplay_stats::build_versus_step_stats(
                state,
                asset_manager,
            ));
        } else if play_style == profile::PlayStyle::Double {
            actors.extend(gameplay_stats::build_double_step_stats(
                state,
                asset_manager,
                playfield_center_x,
            ));
        }
    }
    song_lua_capture_new_actors(&mut underlay_proxy_source, &actors, underlay_tail_start);
    let song_foreground_state = song_lua_song_foreground_state(state);
    let p1_proxy_slices = [
        p1_proxy_sources[0].as_deref(),
        p1_proxy_sources[1].as_deref(),
        p1_proxy_sources[2].as_deref(),
    ];
    let p2_proxy_slices = [
        p2_proxy_sources[0].as_deref(),
        p2_proxy_sources[1].as_deref(),
        p2_proxy_sources[2].as_deref(),
    ];
    let underlay_proxy_slice = underlay_proxy_source.as_deref();
    let overlay_proxy_slice = overlay_proxy_source.as_deref();
    let main_layer_actors = {
        let proxy_sources = song_lua_screen_proxy_sources(
            &actors,
            p1_actor_range,
            p2_actor_range,
            p1_proxy_slices,
            p2_proxy_slices,
            underlay_proxy_slice,
            overlay_proxy_slice,
        );
        build_song_lua_layer_actors(
            &state.song_lua_overlays,
            &overlay_states,
            song_foreground_state,
            &proxy_sources,
            asset_manager,
            song_lua_space_width,
            song_lua_space_height,
            state.current_music_time_display,
            state.current_beat,
            state.total_elapsed_in_screen,
        )
    };
    actors.extend(main_layer_actors);
    if let Some(actor) = build_foreground_media(state, &overlay_states) {
        actors.push(actor);
    }
    for layer in &state.song_lua_foreground_visual_layers {
        if state.current_music_time_display < layer.start_second {
            continue;
        }
        let layer_states = song_lua_overlay_states_from(
            state.current_music_time_display,
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
        );
        let song_foreground_state = song_lua_song_foreground_state_from(
            state.current_music_time_display,
            &layer.song_foreground,
            layer.song_foreground_events.as_slice(),
        );
        let layer_actors = {
            let proxy_sources = song_lua_screen_proxy_sources(
                &actors,
                p1_actor_range,
                p2_actor_range,
                p1_proxy_slices,
                p2_proxy_slices,
                underlay_proxy_slice,
                overlay_proxy_slice,
            );
            build_song_lua_layer_actors(
                &layer.overlays,
                &layer_states,
                song_foreground_state,
                &proxy_sources,
                asset_manager,
                layer.screen_width.max(1.0),
                layer.screen_height.max(1.0),
                state.current_music_time_display,
                state.current_beat,
                state.total_elapsed_in_screen,
            )
        };
        actors.extend(layer_actors);
    }
    actors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::present::actors::SizeSpec;

    fn ensure_i18n() {
        crate::assets::i18n::init("en");
    }

    fn test_proxy_overlay(player_index: usize) -> SongLuaOverlayActor {
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorProxy {
                target: SongLuaProxyTarget::Player { player_index },
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }
    }

    fn test_capture_overlay(name: &str) -> SongLuaOverlayActor {
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorFrameTexture,
            name: Some(name.to_string()),
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }
    }

    fn test_capture_proxy_child(
        parent_index: usize,
        target: SongLuaProxyTarget,
    ) -> SongLuaOverlayActor {
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorProxy { target },
            name: None,
            parent_index: Some(parent_index),
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }
    }

    fn test_aft_overlay(capture_name: &str, visible: bool) -> SongLuaOverlayActor {
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::AftSprite {
                capture_name: capture_name.to_string(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState {
                visible,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        }
    }

    fn test_source_actor() -> Actor {
        Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: Vec::new(),
            background: None,
            z: 0,
        }
    }

    fn test_lobby_player(
        screen_name: &str,
        ready: bool,
    ) -> crate::game::online::lobbies::LobbyPlayer {
        crate::game::online::lobbies::LobbyPlayer {
            label: "Local".to_string(),
            ready,
            screen_name: screen_name.to_string(),
            judgments: None,
            score: None,
            ex_score: None,
        }
    }

    fn test_joined_lobby(
        players: Vec<crate::game::online::lobbies::LobbyPlayer>,
    ) -> crate::game::online::lobbies::JoinedLobby {
        crate::game::online::lobbies::JoinedLobby {
            code: "ABCD".to_string(),
            players,
            song_info: None,
        }
    }

    #[test]
    fn song_lua_proxy_active_players_requires_a_render_source() {
        let overlays = vec![test_proxy_overlay(0)];
        let overlay_states = vec![SongLuaOverlayState::default()];
        let empty_sources = [
            SongLuaPlayerProxySources::default(),
            SongLuaPlayerProxySources::default(),
        ];

        assert_eq!(
            song_lua_proxy_active_players(&overlays, &overlay_states, &empty_sources),
            [false, false]
        );

        let source = vec![test_source_actor()];
        let sources = [
            SongLuaPlayerProxySources {
                player: Some(source.as_slice()),
                ..SongLuaPlayerProxySources::default()
            },
            SongLuaPlayerProxySources::default(),
        ];
        assert_eq!(
            song_lua_proxy_active_players(&overlays, &overlay_states, &sources),
            [true, false]
        );
    }

    #[test]
    fn song_lua_proxy_requests_ignore_unreferenced_capture_children() {
        let overlays = vec![
            test_capture_overlay("cap"),
            test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
        ];
        let overlay_states = vec![SongLuaOverlayState::default(); overlays.len()];
        let requests = song_lua_proxy_requests(&overlays, &overlay_states);

        assert!(!requests.players[0].player);
        assert!(!requests.players[0].note_field);
        assert!(!requests.players[0].judgment);
        assert!(!requests.players[0].combo);
        assert!(!requests.underlay);
        assert!(!requests.overlay);
    }

    #[test]
    fn song_lua_proxy_requests_follow_visible_aft_capture_usage() {
        let overlays = vec![
            test_capture_overlay("cap"),
            test_capture_proxy_child(0, SongLuaProxyTarget::Judgment { player_index: 0 }),
            test_aft_overlay("cap", true),
        ];
        let overlay_states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let requests = song_lua_proxy_requests(&overlays, &overlay_states);

        assert!(!requests.players[0].player);
        assert!(!requests.players[0].note_field);
        assert!(requests.players[0].judgment);
        assert!(!requests.players[0].combo);
    }

    #[test]
    fn song_lua_proxy_requests_skip_hidden_aft_capture_usage() {
        let overlays = vec![
            test_capture_overlay("cap"),
            test_capture_proxy_child(0, SongLuaProxyTarget::Combo { player_index: 0 }),
            test_aft_overlay("cap", false),
        ];
        let overlay_states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let requests = song_lua_proxy_requests(&overlays, &overlay_states);

        assert!(!requests.players[0].combo);
    }

    #[test]
    fn song_lua_overlay_center_coords_stay_centered_under_actorframe() {
        let parent = SongLuaOverlayState {
            x: 427.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        };
        let child = SongLuaOverlayState {
            x: 427.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        };
        let composed = song_lua_overlay_compose_state(
            &SongLuaOverlayKind::ActorFrame,
            parent,
            child,
            854.0,
            480.0,
        );
        assert_eq!(composed.x, 427.0);
        assert_eq!(composed.y, 240.0);
    }

    #[test]
    fn song_lua_overlay_root_actorframe_keeps_absolute_center_child() {
        let parent = SongLuaOverlayState::default();
        let child = SongLuaOverlayState {
            x: 427.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        };
        let composed = song_lua_overlay_compose_state(
            &SongLuaOverlayKind::ActorFrame,
            parent,
            child,
            854.0,
            480.0,
        );
        assert_eq!(composed.x, 427.0);
        assert_eq!(composed.y, 240.0);
    }

    #[test]
    fn song_lua_overlay_local_offsets_still_compose_from_centered_actorframe() {
        let parent = SongLuaOverlayState {
            x: 427.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        };
        let child = SongLuaOverlayState {
            x: -180.0,
            y: 0.0,
            ..SongLuaOverlayState::default()
        };
        let composed = song_lua_overlay_compose_state(
            &SongLuaOverlayKind::ActorFrame,
            parent,
            child,
            854.0,
            480.0,
        );
        assert_eq!(composed.x, 247.0);
        assert_eq!(composed.y, 240.0);
    }

    #[test]
    fn song_lua_actor_proxy_keeps_overlay_z_layer() {
        let source = vec![test_source_actor()];
        let actor =
            song_lua_build_proxy_actor(SongLuaOverlayState::default(), 1234, &source, 640.0, 480.0)
                .expect("actor proxy should render with a source");

        match actor {
            Actor::Frame { z, children, .. } => {
                assert_eq!(z, 1234);
                assert_eq!(children.len(), 1);
            }
            other => panic!("expected frame actor, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_quad_keeps_zoomed_size_in_scale() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([100.0, 50.0]),
                zoom: 0.5,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            321,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect("quad overlay should render");

        match actor {
            Actor::Sprite {
                size,
                scale,
                z,
                visible,
                ..
            } => {
                let expected_scale = [
                    100.0 * 0.5 * screen_width() / 640.0,
                    50.0 * 0.5 * screen_height() / 480.0,
                ];
                assert_eq!(z, 321);
                assert!(visible);
                assert!((scale[0] - expected_scale[0]).abs() <= 0.000_1);
                assert!((scale[1] - expected_scale[1]).abs() <= 0.000_1);
                match size {
                    [SizeSpec::Px(w), SizeSpec::Px(h)] => {
                        assert_eq!(w, 0.0);
                        assert_eq!(h, 0.0);
                    }
                    other => panic!("expected explicit quad size, got {other:?}"),
                }
            }
            other => panic!("expected sprite-backed quad, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_quad_uses_textured_mesh_under_perspective_camera() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([100.0, 50.0]),
                rot_x_deg: 45.0,
                ..SongLuaOverlayState::default()
            },
            Some(SongLuaOverlayState {
                fov: Some(120.0),
                ..SongLuaOverlayState::default()
            }),
            &AssetManager::new(),
            654,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect("perspective song lua quad should render");

        match actor {
            Actor::TexturedMesh { vertices, z, .. } => {
                assert_eq!(z, 654);
                assert_eq!(vertices.len(), 6);
            }
            other => panic!("expected projected textured mesh, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_quad_applies_bounce_effect_offset_at_runtime() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([100.0, 50.0]),
                effect_mode: crate::engine::present::anim::EffectMode::Bounce,
                effect_clock: crate::engine::present::anim::EffectClock::Beat,
                effect_period: 2.0,
                effect_offset: 1.0,
                effect_magnitude: [10.0, 20.0, 5.0],
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            777,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect("effect quad should render");

        match actor {
            Actor::Sprite {
                offset,
                world_z,
                scale,
                z,
                ..
            } => {
                let x_scale = screen_width() / 640.0;
                let y_scale = screen_height() / 480.0;
                assert_eq!(z, 777);
                assert!((offset[0] - (320.0 + 10.0) * x_scale).abs() <= 0.000_1);
                assert!((offset[1] - (240.0 + 20.0) * y_scale).abs() <= 0.000_1);
                assert!((world_z - 5.0).abs() <= 0.000_1);
                assert!(scale[0] > 0.0);
                assert!(scale[1] > 0.0);
            }
            other => panic!("expected sprite-backed quad, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_sprite_setstate_uses_sheet_cell_size_at_runtime() {
        let key = "song-lua-test 4x3.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sprite {
                texture_path: std::path::PathBuf::from(&key),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                sprite_state_index: Some(5),
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            778,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect("setstate sprite should render");

        match actor {
            Actor::Sprite {
                size, uv_rect, z, ..
            } => {
                let expected_w = 10.0 * screen_width() / 640.0;
                let expected_h = 10.0 * screen_height() / 480.0;
                assert_eq!(z, 778);
                assert_eq!(uv_rect, Some([0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0]));
                match size {
                    [SizeSpec::Px(w), SizeSpec::Px(h)] => {
                        assert!((w - expected_w).abs() <= 0.000_1);
                        assert!((h - expected_h).abs() <= 0.000_1);
                    }
                    other => panic!("expected explicit sprite size, got {other:?}"),
                }
            }
            other => panic!("expected sprite overlay, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_sprite_animation_advances_sheet_frames_at_runtime() {
        let key = "song-lua-animate 4x3.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sprite {
                texture_path: std::path::PathBuf::from(&key),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                sprite_state_index: Some(1),
                sprite_animate: true,
                sprite_loop: true,
                sprite_playback_rate: 1.0,
                sprite_state_delay: 0.5,
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            779,
            640.0,
            480.0,
            0.0,
            0.0,
            1.1,
        )
        .expect("animated sprite should render");

        match actor {
            Actor::Sprite { uv_rect, z, .. } => {
                assert_eq!(z, 779);
                assert_eq!(uv_rect, Some([0.75, 0.0, 1.0, 1.0 / 3.0]));
            }
            other => panic!("expected animated sprite overlay, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_sprite_animation_applies_rate_and_loop_controls_at_runtime() {
        let key = "song-lua-animate-rate 4x3.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sprite {
                texture_path: std::path::PathBuf::from(&key),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                sprite_state_index: Some(1),
                sprite_animate: true,
                sprite_loop: false,
                sprite_playback_rate: 2.0,
                sprite_state_delay: 0.5,
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            780,
            640.0,
            480.0,
            0.0,
            0.0,
            10.0,
        )
        .expect("rate-controlled sprite should render");

        match actor {
            Actor::Sprite { uv_rect, z, .. } => {
                assert_eq!(z, 780);
                assert_eq!(uv_rect, Some([0.75, 2.0 / 3.0, 1.0, 1.0]));
            }
            other => panic!("expected animated sprite overlay, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_sprite_applies_texture_translate_to_uv_rect() {
        let key = "song-lua-translate.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sprite {
                texture_path: std::path::PathBuf::from(&key),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                texture_wrapping: true,
                texcoord_offset: Some([0.25, -0.5]),
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            781,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect("translated sprite should render");

        match actor {
            Actor::Sprite { uv_rect, z, .. } => {
                assert_eq!(z, 781);
                assert_eq!(uv_rect, Some([0.25, -0.5, 1.25, 0.5]));
            }
            other => panic!("expected translated sprite overlay, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_sprite_applies_fade_edges_at_runtime() {
        let key = "song-lua-fade-edges.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sprite {
                texture_path: std::path::PathBuf::from(&key),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                fadeleft: 0.1,
                faderight: 0.2,
                fadetop: 0.3,
                fadebottom: 0.4,
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            782,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect("faded sprite should render");

        match actor {
            Actor::Sprite {
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                z,
                ..
            } => {
                assert_eq!(z, 782);
                assert!((fadeleft - 0.1).abs() <= 0.000_1);
                assert!((faderight - 0.2).abs() <= 0.000_1);
                assert!((fadetop - 0.3).abs() <= 0.000_1);
                assert!((fadebottom - 0.4).abs() <= 0.000_1);
            }
            other => panic!("expected faded sprite overlay, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_layer_detects_visible_sprite_texture() {
        let path = std::path::PathBuf::from("badapple.avi");
        let overlays = vec![SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sprite {
                texture_path: path.clone(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }];
        let states = vec![SongLuaOverlayState::default()];

        assert!(song_lua_has_visible_tex(&overlays, &states, path.as_path()));
    }

    #[test]
    fn song_lua_layer_ignores_hidden_sprite_texture() {
        let path = std::path::PathBuf::from("badapple.avi");
        let overlays = vec![SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sprite {
                texture_path: path.clone(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }];
        let states = vec![SongLuaOverlayState {
            visible: false,
            ..SongLuaOverlayState::default()
        }];

        assert!(!song_lua_has_visible_tex(
            &overlays,
            &states,
            path.as_path()
        ));
    }

    #[test]
    fn gameplay_requires_wait_for_solo_joined_lobby() {
        let joined = test_joined_lobby(vec![test_lobby_player("ScreenGameplay", false)]);
        assert!(gameplay_requires_lobby_wait_for(Some(&joined)));
    }

    #[test]
    fn gameplay_wait_text_requires_ready_up_for_solo_lobby_player() {
        ensure_i18n();
        let joined = test_joined_lobby(vec![test_lobby_player("ScreenGameplay", false)]);

        let expected = format!(
            "{}\n{}",
            tr("Lobby", "WaitingForReadyUp"),
            tr("Gameplay", "PressStartToReadyUp"),
        );
        assert_eq!(
            gameplay_lobby_wait_text_for(&joined, false, None).as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn gameplay_wait_text_unlocks_once_solo_lobby_player_is_ready() {
        let joined = test_joined_lobby(vec![test_lobby_player("ScreenGameplay", true)]);

        assert_eq!(gameplay_lobby_wait_text_for(&joined, true, None), None);
    }
}
