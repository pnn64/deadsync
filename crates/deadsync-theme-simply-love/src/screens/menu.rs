use crate::act;
use crate::assets::i18n::{self, tr, tr_fmt};
use crate::assets::{FontRole, machine_font_key};
// Screen navigation is handled in app
use crate::screens::components::menu::logo::{self, LogoParams};
use crate::screens::components::shared::{screen_bar, transitions, visual_style_bg};
use crate::screens::input as screen_input;
use crate::screens::{Screen, ThemeEffect};
use crate::views::{
    MainMenuArrowCloudError, MainMenuArrowCloudStatus, MainMenuGrooveError, MainMenuGrooveStatus,
    MainMenuRuntimeView,
};
use deadlib_present::actors::{Actor, TextAlign};
use deadlib_present::color;
use deadsync_input::KeyCode;
use deadsync_input::RawKeyboardEvent;
use deadsync_input::{InputEvent, VirtualAction};
use std::cell::RefCell;
use std::sync::Arc;

use deadlib_present::space::{screen_center_x, screen_height, screen_width};

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.5;
const TRANSITION_OUT_DURATION: f32 = 1.0;
const BRAND_EXIT_DURATION: f32 = 0.65;
// Graphics/ScreenTitleMenu scroll.lua staggers each row's OnCommand by 75 ms.
const ROW_STAGGER: f32 = 0.075;
const ROW_ENTRY_DURATION: f32 = 0.2;
const ROW_EXIT_DURATION: f32 = 0.18;
// GainFocusCommand queues a glow-in, then changes color during the glow-out.
const FOCUS_GLOW_IN: f32 = 0.1;
const FOCUS_GLOW_OUT: f32 = 0.05;
const FOCUS_GLOW_ALPHA: f32 = 0.5;

const NORMAL_COLOR_HEX: &str = "#888888";

pub const OPTION_COUNT: usize = 3;
const MAX_OPTION_COUNT: usize = OPTION_COUNT + 1;

#[inline]
fn option_count(state: &State) -> usize {
    if state.runtime_view.allow_shutdown_host {
        OPTION_COUNT + 1
    } else {
        OPTION_COUNT
    }
}

#[inline]
fn shutdown_index(state: &State) -> Option<usize> {
    state
        .runtime_view
        .allow_shutdown_host
        .then_some(OPTION_COUNT)
}

// --- CONSTANTS UPDATED FOR NEW ANIMATION-DRIVEN LAYOUT ---
//const MENU_BELOW_LOGO: f32 = 25.0;
//const MENU_ROW_SPACING: f32 = 23.0;

const MENU_BELOW_LOGO: f32 = 29.0;
const MENU_ROW_SPACING: f32 = 28.0;
const MENU_BASE_PX: f32 = 32.0;
const MENU_FOCUS_ZOOM: f32 = 0.5;
const MENU_UNFOCUSED_ZOOM: f32 = 0.4;

const INFO_PX: f32 = 15.0;
const INFO_GAP: f32 = 5.0;
const INFO_MARGIN_ABOVE: f32 = 20.0;
const STATUS_BASE_X: f32 = 10.0;
const STATUS_BASE_Y: f32 = 15.0;
const STATUS_ZOOM: f32 = 0.8;
const STATUS_LINE_HEIGHT: f32 = 18.0;
const STATUS_BLOCK_GAP: f32 = 6.0;

#[derive(Clone)]
struct StatusTextCache<K, const N: usize> {
    key: K,
    main: Arc<str>,
    lines: [Option<Arc<str>>; N],
    line_count: usize,
}

fn groove_error_text(kind: MainMenuGrooveError) -> Arc<str> {
    match kind {
        MainMenuGrooveError::Disabled => tr("Menu", "Disabled"),
        MainMenuGrooveError::MachineOffline => tr("Menu", "MachineOffline"),
        MainMenuGrooveError::CannotConnect => tr("Menu", "CannotConnect"),
        MainMenuGrooveError::TimedOut => tr("Menu", "TimedOut"),
        MainMenuGrooveError::InvalidResponse => tr("Menu", "FailedToLoad"),
    }
}

fn arrowcloud_error_text(kind: MainMenuArrowCloudError) -> Arc<str> {
    match kind {
        MainMenuArrowCloudError::Disabled => tr("Menu", "Disabled"),
        MainMenuArrowCloudError::TimedOut => tr("Menu", "TimedOut"),
        MainMenuArrowCloudError::HostBlocked => tr("Menu", "HostBlocked"),
        MainMenuArrowCloudError::CannotConnect => tr("Menu", "CannotConnect"),
    }
}

#[derive(Clone, PartialEq, Eq)]
struct InfoTextKey {
    banner_tag: Option<String>,
    song_count: usize,
    pack_count: usize,
    course_count: usize,
}

/// Locale-owned chrome compiled once and replaced only when translations change.
struct MenuChromeText {
    i18n_revision: u64,
    options: [Arc<str>; OPTION_COUNT + 1],
    event_mode: Arc<str>,
    press_start: Arc<str>,
    smx_warnings: [Arc<str>; 2],
}

#[derive(Clone, Copy)]
enum FocusTween {
    Idle,
    Gain {
        elapsed: f32,
        focus_from: f32,
        glow_from: f32,
    },
    Lose {
        elapsed: f32,
        focus_from: f32,
        glow_from: f32,
    },
}

#[derive(Clone, Copy)]
struct RowAnim {
    focus: f32,
    glow_alpha: f32,
    tween: FocusTween,
}

const ROW_FOCUSED: RowAnim = RowAnim {
    focus: 1.0,
    glow_alpha: 0.0,
    tween: FocusTween::Idle,
};
const ROW_UNFOCUSED: RowAnim = RowAnim {
    focus: 0.0,
    glow_alpha: 0.0,
    tween: FocusTween::Idle,
};

fn build_chrome_text(i18n_revision: u64) -> MenuChromeText {
    MenuChromeText {
        i18n_revision,
        options: [
            tr("Menu", "Gameplay"),
            tr("Menu", "Options"),
            tr("Menu", "Exit"),
            tr("Menu", "Shutdown"),
        ],
        event_mode: tr("Common", "EventMode"),
        press_start: tr("Common", "PressStart"),
        smx_warnings: [
            tr("Menu", "SmxAssignWarning1"),
            tr("Menu", "SmxAssignWarning2"),
        ],
    }
}

pub struct State {
    pub selected_index: usize,
    pub active_color_index: i32,
    pub rainbow_mode: bool,
    pub started_by_p2: bool,
    runtime_view: MainMenuRuntimeView,
    bg: visual_style_bg::State,
    chrome_text: RefCell<MenuChromeText>,
    info_text_cache: RefCell<Option<(InfoTextKey, Arc<str>)>>,
    local_ip_text_cache: RefCell<Option<(Arc<str>, Arc<str>)>>,
    groovestats_text_cache: RefCell<Option<StatusTextCache<MainMenuGrooveStatus, 3>>>,
    arrowcloud_text_cache: RefCell<Option<StatusTextCache<MainMenuArrowCloudStatus, 1>>>,
    menu_lr_chord: screen_input::MenuLrChordTracker,
    menu_lr_undo: [i8; 2],
    row_anims: [RowAnim; MAX_OPTION_COUNT],
}

pub fn init() -> State {
    let i18n_revision = i18n::revision();
    State {
        selected_index: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX, // was 0
        rainbow_mode: false,
        started_by_p2: false,
        runtime_view: MainMenuRuntimeView::default(),
        bg: visual_style_bg::State::new(),
        chrome_text: RefCell::new(build_chrome_text(i18n_revision)),
        info_text_cache: RefCell::new(None),
        local_ip_text_cache: RefCell::new(None),
        groovestats_text_cache: RefCell::new(None),
        arrowcloud_text_cache: RefCell::new(None),
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        menu_lr_undo: [0; 2],
        row_anims: [ROW_FOCUSED, ROW_UNFOCUSED, ROW_UNFOCUSED, ROW_UNFOCUSED],
    }
}

pub fn reset_for_entry(state: &mut State) {
    let active_color_index = state.active_color_index;
    *state = init();
    state.active_color_index = active_color_index;
}

pub fn sync_runtime_view(state: &mut State, view: MainMenuRuntimeView) {
    state.runtime_view = view;
    let selected_index = state
        .selected_index
        .min(option_count(state).saturating_sub(1));
    set_selected(state, selected_index);
}

#[inline(always)]
fn accelerate(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t
}

#[inline(always)]
fn decelerate(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

#[inline(always)]
fn mix(from: f32, to: f32, amount: f32) -> f32 {
    (to - from).mul_add(amount, from)
}

fn update_gain(row: &mut RowAnim, elapsed: f32, focus_from: f32, glow_from: f32) {
    if elapsed < FOCUS_GLOW_IN {
        row.focus = focus_from;
        row.glow_alpha = mix(
            glow_from,
            FOCUS_GLOW_ALPHA,
            accelerate(elapsed / FOCUS_GLOW_IN),
        );
        row.tween = FocusTween::Gain {
            elapsed,
            focus_from,
            glow_from,
        };
        return;
    }

    let progress = decelerate((elapsed - FOCUS_GLOW_IN) / FOCUS_GLOW_OUT);
    row.focus = mix(focus_from, 1.0, progress);
    row.glow_alpha = mix(FOCUS_GLOW_ALPHA, 0.0, progress);
    row.tween = if elapsed < FOCUS_GLOW_IN + FOCUS_GLOW_OUT {
        FocusTween::Gain {
            elapsed,
            focus_from,
            glow_from,
        }
    } else {
        FocusTween::Idle
    };
}

fn update_lose(row: &mut RowAnim, elapsed: f32, focus_from: f32, glow_from: f32) {
    let progress = accelerate(elapsed / FOCUS_GLOW_IN);
    row.focus = mix(focus_from, 0.0, progress);
    row.glow_alpha = mix(glow_from, 0.0, progress);
    row.tween = if elapsed < FOCUS_GLOW_IN {
        FocusTween::Lose {
            elapsed,
            focus_from,
            glow_from,
        }
    } else {
        FocusTween::Idle
    };
}

fn update_row_anim(row: &mut RowAnim, dt: f32) {
    match row.tween {
        FocusTween::Idle => {}
        FocusTween::Gain {
            elapsed,
            focus_from,
            glow_from,
        } => update_gain(row, elapsed + dt, focus_from, glow_from),
        FocusTween::Lose {
            elapsed,
            focus_from,
            glow_from,
        } => update_lose(row, elapsed + dt, focus_from, glow_from),
    }
}

pub fn update(state: &mut State, dt: f32) {
    if !dt.is_finite() || dt <= 0.0 {
        return;
    }
    for row in &mut state.row_anims {
        update_row_anim(row, dt);
    }
}

// Keyboard input is handled centrally via the virtual dispatcher in app
// Screen-specific raw keyboard handling for Menu (e.g., F4 to Sandbox)
pub fn handle_raw_key_event(_state: &mut State, key: &RawKeyboardEvent) -> ThemeEffect {
    if !key.pressed {
        return ThemeEffect::None;
    }
    match key.code {
        KeyCode::F4 => return ThemeEffect::Navigate(Screen::Sandbox),
        KeyCode::Escape => return ThemeEffect::Navigate(Screen::Init),
        _ => {}
    }
    ThemeEffect::None
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    // Simply Love's ScreenTitleMenu out actor only holds the screen for one
    // second. The menu owns its actor fades and the shell adds the fly burst.
    (Vec::new(), TRANSITION_OUT_DURATION)
}

pub fn cancel_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(1.0, 1.0, 1.0, 0.0):
        z(1200):
        decelerate(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

fn smooth_p(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.5 {
        2.0 * t * t
    } else {
        1.0 - 2.0 * (1.0 - t) * (1.0 - t)
    }
}

fn brand_exit_alpha(elapsed: Option<f32>) -> f32 {
    elapsed.map_or(1.0, |t| 1.0 - smooth_p(t / BRAND_EXIT_DURATION))
}

fn row_exit_alpha(index: usize, elapsed: Option<f32>) -> f32 {
    elapsed.map_or(1.0, |t| {
        let fade_t = (t - ROW_STAGGER * index as f32) / ROW_EXIT_DURATION;
        1.0 - fade_t.clamp(0.0, 1.0)
    })
}

fn row_entry_alpha(index: usize, elapsed: Option<f32>) -> f32 {
    elapsed.map_or(1.0, |t| {
        ((t - ROW_STAGGER * index as f32) / ROW_ENTRY_DURATION).clamp(0.0, 1.0)
    })
}

#[inline(always)]
fn mix_rgba(from: [f32; 4], to: [f32; 4], amount: f32) -> [f32; 4] {
    [
        mix(from[0], to[0], amount),
        mix(from[1], to[1], amount),
        mix(from[2], to[2], amount),
        mix(from[3], to[3], amount),
    ]
}

pub fn clear_render_cache(state: &State) {
    *state.info_text_cache.borrow_mut() = None;
    *state.local_ip_text_cache.borrow_mut() = None;
    *state.groovestats_text_cache.borrow_mut() = None;
    *state.arrowcloud_text_cache.borrow_mut() = None;
}

fn sync_i18n_cache(state: &State) {
    let revision = i18n::revision();
    if state.chrome_text.borrow().i18n_revision == revision {
        return;
    }
    clear_render_cache(state);
    *state.chrome_text.borrow_mut() = build_chrome_text(revision);
}

#[inline(always)]
fn menu_info_text(state: &State, update_banner_tag: Option<&str>) -> Arc<str> {
    let key = InfoTextKey {
        banner_tag: update_banner_tag.map(str::to_owned),
        song_count: state.runtime_view.song_count,
        pack_count: state.runtime_view.pack_count,
        course_count: state.runtime_view.course_count,
    };
    if let Some((cached_key, text)) = state.info_text_cache.borrow().as_ref()
        && cached_key == &key
    {
        return text.clone();
    }

    let version = deadsync_version::current().to_string();
    let mut version_line = tr_fmt("Menu", "VersionLine", &[("version", &version)]).to_string();
    if let Some(tag) = key.banner_tag.as_deref() {
        let suffix = tr_fmt("Menu", "UpdateAvailableSuffix", &[("version", tag)]);
        version_line.push(' ');
        version_line.push_str(&suffix);
    }
    let songs = key.song_count.to_string();
    let packs = key.pack_count.to_string();
    let courses = key.course_count.to_string();
    let summary = tr_fmt(
        "Menu",
        "SongSummary",
        &[("songs", &songs), ("packs", &packs), ("courses", &courses)],
    );
    let text = Arc::<str>::from(format!("{version_line}\n{summary}"));
    *state.info_text_cache.borrow_mut() = Some((key, text.clone()));
    text
}

#[inline(always)]
fn groove_service_name(boogie: bool) -> Arc<str> {
    if boogie {
        tr("Menu", "BoogieStatsName")
    } else {
        tr("Menu", "GrooveStatsName")
    }
}

fn build_groovestats_text(key: MainMenuGrooveStatus) -> StatusTextCache<MainMenuGrooveStatus, 3> {
    let mut lines = [None, None, None];
    let (main, line_count) = match key {
        MainMenuGrooveStatus::Pending { boogie } => {
            let service = groove_service_name(boogie);
            (
                tr_fmt("Menu", "ServicePending", &[("service", service.as_ref())]),
                0,
            )
        }
        MainMenuGrooveStatus::Error { boogie, kind } => {
            lines[0] = Some(groove_error_text(kind));
            if kind == MainMenuGrooveError::Disabled {
                (tr("Menu", "GrooveStatsDisabled"), 1)
            } else {
                let service = groove_service_name(boogie);
                (
                    tr_fmt(
                        "Menu",
                        "ServiceNotConnected",
                        &[("service", service.as_ref())],
                    ),
                    1,
                )
            }
        }
        MainMenuGrooveStatus::Connected {
            boogie,
            get_scores,
            leaderboard,
            auto_submit,
        } => {
            let disabled_mask =
                (!get_scores) as u8 | (((!leaderboard) as u8) << 1) | (((!auto_submit) as u8) << 2);
            if disabled_mask == 0 {
                let service = groove_service_name(boogie);
                (
                    tr_fmt("Menu", "ServiceConnected", &[("service", service.as_ref())]),
                    0,
                )
            } else if disabled_mask == 0b111 {
                (tr("Menu", "GrooveStatsDisabled"), 0)
            } else {
                let mut line_count = 0;
                if disabled_mask & 0b001 != 0 {
                    lines[line_count] = Some(tr("Menu", "GetScoresDisabled"));
                    line_count += 1;
                }
                if disabled_mask & 0b010 != 0 {
                    lines[line_count] = Some(tr("Menu", "LeaderboardDisabled"));
                    line_count += 1;
                }
                if disabled_mask & 0b100 != 0 {
                    lines[line_count] = Some(tr("Menu", "AutoSubmitDisabled"));
                    line_count += 1;
                }
                (tr("Menu", "GrooveStatsWarn"), line_count)
            }
        }
    };
    StatusTextCache {
        key,
        main,
        lines,
        line_count,
    }
}

fn groovestats_text(state: &State) -> StatusTextCache<MainMenuGrooveStatus, 3> {
    let key = state.runtime_view.groovestats;
    if let Some(cache) = state.groovestats_text_cache.borrow().as_ref()
        && cache.key == key
    {
        return cache.clone();
    }
    let cache = build_groovestats_text(key);
    *state.groovestats_text_cache.borrow_mut() = Some(cache.clone());
    cache
}

fn build_arrowcloud_text(
    key: MainMenuArrowCloudStatus,
) -> StatusTextCache<MainMenuArrowCloudStatus, 1> {
    let mut lines = [None];
    let (main, line_count) = match key {
        MainMenuArrowCloudStatus::Pending => (tr("Menu", "ArrowCloudPending"), 0),
        MainMenuArrowCloudStatus::Connected => (tr("Menu", "ArrowCloudConnected"), 0),
        MainMenuArrowCloudStatus::Error(kind) => {
            lines[0] = Some(arrowcloud_error_text(kind));
            (tr("Menu", "ArrowCloudDisabled"), 1)
        }
    };
    StatusTextCache {
        key,
        main,
        lines,
        line_count,
    }
}

fn arrowcloud_text(state: &State) -> StatusTextCache<MainMenuArrowCloudStatus, 1> {
    let key = state.runtime_view.arrowcloud;
    if let Some(cache) = state.arrowcloud_text_cache.borrow().as_ref()
        && cache.key == key
    {
        return cache.clone();
    }
    let cache = build_arrowcloud_text(key);
    *state.arrowcloud_text_cache.borrow_mut() = Some(cache.clone());
    cache
}

fn local_ip_text(state: &State) -> Option<Arc<str>> {
    let ip = state.runtime_view.local_ip.as_ref()?;
    if let Some((cached_ip, text)) = state.local_ip_text_cache.borrow().as_ref()
        && cached_ip == ip
    {
        return Some(text.clone());
    }
    let text = tr_fmt("Menu", "LocalIpAddress", &[("address", ip.as_ref())]);
    *state.local_ip_text_cache.borrow_mut() = Some((ip.clone(), text.clone()));
    Some(text)
}

#[inline(always)]
fn status_text_actor(
    text: Arc<str>,
    align_x: f32,
    x: f32,
    y: f32,
    zoom: f32,
    alpha: f32,
    align_text: TextAlign,
) -> Actor {
    let mut actor = act!(text:
        font("miso"):
        settext(text):
        align(align_x, 0.0):
        xy(x, y):
        zoom(zoom):
        z(200)
    );
    if let Actor::Text {
        color,
        align_text: text_align,
        ..
    } = &mut actor
    {
        color[3] = alpha;
        *text_align = align_text;
    }
    actor
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    update_banner_tag: Option<&str>,
    alpha_multiplier: f32,
    entry_elapsed: Option<f32>,
    exit_elapsed: Option<f32>,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    sync_i18n_cache(state);
    let chrome_text = state.chrome_text.borrow();
    let lp = LogoParams::default();
    actors.reserve(96);

    // 1) background component (never fades)
    let backdrop = if state.rainbow_mode {
        [1.0, 1.0, 1.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: backdrop,
            alpha_mul: 1.0,
            visual_policy,
        },
    );

    // If fully faded, don't create the other actors
    if alpha_multiplier <= 0.0 {
        return;
    }

    // --- The rest of the function is the same, but uses the passed-in alpha_multiplier ---

    // 2) logo + info
    let info2_y_tl = lp.top_margin - INFO_MARGIN_ABOVE - INFO_PX;
    let info1_y_tl = info2_y_tl - INFO_PX - INFO_GAP;

    let brand_alpha = alpha_multiplier * brand_exit_alpha(exit_elapsed);
    let logo_actors = logo::build_logo_default(
        visual_policy.title_logo_texture_key,
        state.runtime_view.game,
    );
    for mut actor in logo_actors {
        if let Actor::Sprite { tint, .. } = &mut actor {
            tint[3] *= brand_alpha;
        }
        actors.push(actor);
    }

    let mut info_color = [1.0, 1.0, 1.0, 1.0];
    info_color[3] *= brand_alpha;

    actors.push(act!(text:
        align(0.5, 0.0): xy(screen_center_x(), info1_y_tl): zoom(0.8):
        font("miso"): settext(menu_info_text(state, update_banner_tag)): horizalign(center):
        diffuse(info_color[0], info_color[1], info_color[2], info_color[3])
    ));

    // 3) menu list
    let base_y = lp.top_margin + lp.target_h + MENU_BELOW_LOGO;
    let selected = color::menu_selected_rgba(state.active_color_index);
    let normal = color::rgba_hex(NORMAL_COLOR_HEX);

    let menu_font = machine_font_key(visual_policy.machine_font, FontRole::Bold);
    let menu_center_x = screen_center_x();
    for (index, label) in chrome_text.options[..option_count(state)]
        .iter()
        .enumerate()
    {
        let is_selected = index == state.selected_index;
        let zoom = if is_selected {
            MENU_FOCUS_ZOOM
        } else {
            MENU_UNFOCUSED_ZOOM
        };
        let row_anim = state.row_anims[index];
        let mut row_color = mix_rgba(normal, selected, row_anim.focus);
        let row_alpha = alpha_multiplier
            * row_entry_alpha(index, entry_elapsed)
            * row_exit_alpha(index, exit_elapsed);
        row_color[3] *= row_alpha;
        let glow_alpha = row_anim.glow_alpha * row_alpha;
        let center_y = (index as f32).mul_add(MENU_ROW_SPACING, base_y);
        actors.push(act!(text:
            align(0.5, 0.5):
            xy(menu_center_x, center_y):
            zoomtoheight(MENU_BASE_PX * zoom):
            diffuse(row_color[0], row_color[1], row_color[2], row_color[3]):
            glow(1.0, 1.0, 1.0, glow_alpha):
            shadowlength(0.8):
            font(menu_font):
            settext(Arc::clone(label)):
            horizalign(center)
        ));
    }

    // --- footer bar ---
    let mut footer_fg = [1.0, 1.0, 1.0, 1.0];
    footer_fg[3] *= alpha_multiplier;
    actors.push(screen_bar::build_title_menu(screen_bar::ScreenBarParams {
        visual_policy,
        title: chrome_text.event_mode.as_ref(),
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        left_text: Some(chrome_text.press_start.as_ref()),
        center_text: None,
        right_text: Some(chrome_text.press_start.as_ref()),
        left_avatar: None,
        right_avatar: None,
        fg_color: footer_fg,
    }));

    // --- Local IP (optional, top-left) ---
    let mut gs_base_y = STATUS_BASE_Y;
    if let Some(text) = local_ip_text(state) {
        actors.push(status_text_actor(
            text,
            0.0,
            STATUS_BASE_X,
            STATUS_BASE_Y,
            STATUS_ZOOM,
            alpha_multiplier,
            TextAlign::Left,
        ));
        gs_base_y += STATUS_LINE_HEIGHT.mul_add(STATUS_ZOOM, STATUS_BLOCK_GAP);
    }

    // --- GrooveStats Info Pane (below the optional local IP) ---
    let gs_text = groovestats_text(state);
    actors.push(status_text_actor(
        gs_text.main.clone(),
        0.0,
        STATUS_BASE_X,
        gs_base_y,
        STATUS_ZOOM,
        alpha_multiplier,
        TextAlign::Left,
    ));
    for line_idx in 0..gs_text.line_count {
        if let Some(text) = gs_text.lines[line_idx].as_ref() {
            actors.push(status_text_actor(
                text.clone(),
                0.0,
                STATUS_BASE_X,
                (STATUS_LINE_HEIGHT * (line_idx as f32 + 1.0)).mul_add(STATUS_ZOOM, gs_base_y),
                STATUS_ZOOM,
                alpha_multiplier,
                TextAlign::Left,
            ));
        }
    }

    // --- Arrow Cloud Info Pane (below GrooveStats/BoogieStats) ---
    let ac_base_y = (STATUS_LINE_HEIGHT * (gs_text.line_count as f32 + 1.0))
        .mul_add(STATUS_ZOOM, gs_base_y + STATUS_BLOCK_GAP);
    let ac_text = arrowcloud_text(state);
    actors.push(status_text_actor(
        ac_text.main.clone(),
        0.0,
        STATUS_BASE_X,
        ac_base_y,
        STATUS_ZOOM,
        alpha_multiplier,
        TextAlign::Left,
    ));
    for line_idx in 0..ac_text.line_count {
        if let Some(text) = ac_text.lines[line_idx].as_ref() {
            actors.push(status_text_actor(
                text.clone(),
                0.0,
                STATUS_BASE_X,
                (STATUS_LINE_HEIGHT * (line_idx as f32 + 1.0)).mul_add(STATUS_ZOOM, ac_base_y),
                STATUS_ZOOM,
                alpha_multiplier,
                TextAlign::Left,
            ));
        }
    }

    // --- StepManiaX pad warning (only when two pads share a P1/P2 jumper and no
    // assignment resolves them, so the user knows to assign their pads). ---
    if let Some(conflict) = state.runtime_view.smx_conflict {
        let smx_base_y = (STATUS_LINE_HEIGHT * (ac_text.line_count as f32 + 1.0))
            .mul_add(STATUS_ZOOM, ac_base_y + STATUS_BLOCK_GAP);
        // Two short lines (kept compact for the main screen).
        for (i, text) in chrome_text.smx_warnings.iter().enumerate() {
            let y = (STATUS_LINE_HEIGHT * i as f32).mul_add(STATUS_ZOOM, smx_base_y);
            let mut actor = status_text_actor(
                Arc::clone(text),
                0.0,
                STATUS_BASE_X,
                y,
                STATUS_ZOOM,
                alpha_multiplier,
                TextAlign::Left,
            );
            if let Actor::Text { color, .. } = &mut actor {
                // Amber warning (alpha already applied by status_text_actor).
                color[..3].copy_from_slice(&conflict.color_rgb);
            }
            actors.push(actor);
        }
    }
}

// Signature changed to accept the alpha_multiplier
pub fn get_actors(
    state: &State,
    update_banner_tag: Option<&str>,
    alpha_multiplier: f32,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(96);
    push_actors(
        &mut actors,
        state,
        update_banner_tag,
        alpha_multiplier,
        None,
        None,
        Default::default(),
    );
    actors
}

#[inline(always)]
fn move_selection(state: &mut State, delta: isize) {
    let n = option_count(state) as isize;
    let cur = state.selected_index as isize;
    set_selected(state, (cur + delta).rem_euclid(n) as usize);
}

fn set_selected(state: &mut State, selected_index: usize) {
    let old_index = state.selected_index;
    if old_index == selected_index {
        return;
    }

    let old = state.row_anims[old_index];
    state.row_anims[old_index].tween = FocusTween::Lose {
        elapsed: 0.0,
        focus_from: old.focus,
        glow_from: old.glow_alpha,
    };
    let new = state.row_anims[selected_index];
    state.row_anims[selected_index].tween = FocusTween::Gain {
        elapsed: 0.0,
        focus_from: new.focus,
        glow_from: new.glow_alpha,
    };
    state.selected_index = selected_index;
}

#[inline(always)]
fn start_selected(state: &mut State, started_by_p2: bool) -> ThemeEffect {
    state.started_by_p2 = started_by_p2;
    let effect = if Some(state.selected_index) == shutdown_index(state) {
        ThemeEffect::Shutdown
    } else {
        match state.selected_index {
            0 => ThemeEffect::Navigate(Screen::SelectProfile),
            1 => ThemeEffect::Navigate(Screen::Options),
            2 => ThemeEffect::Exit,
            _ => ThemeEffect::None,
        }
    };
    crate::effects::sfx_then("assets/sounds/start.ogg", effect)
}

#[inline(always)]
const fn menu_nav_delta(action: VirtualAction) -> Option<isize> {
    match action {
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => Some(-1),
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => Some(1),
        _ => None,
    }
}

// Event-driven virtual input handler
pub fn handle_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    if let Some(side) = screen_input::menu_lr_side(ev.action)
        && !ev.pressed
    {
        state.menu_lr_undo[deadsync_profile::player_side_index(side)] = 0;
    }
    if let Some((side, nav)) = screen_input::three_key_menu_action(
        &mut state.menu_lr_chord,
        ev,
        state.runtime_view.dedicated_three_key_nav,
    ) {
        let side_ix = deadsync_profile::player_side_index(side);
        return match nav {
            screen_input::ThreeKeyMenuAction::Prev => {
                move_selection(state, -1);
                state.menu_lr_undo[side_ix] = 1;
                crate::effects::sfx("assets/sounds/change.ogg")
            }
            screen_input::ThreeKeyMenuAction::Next => {
                move_selection(state, 1);
                state.menu_lr_undo[side_ix] = -1;
                crate::effects::sfx("assets/sounds/change.ogg")
            }
            screen_input::ThreeKeyMenuAction::Confirm => {
                state.menu_lr_undo[side_ix] = 0;
                start_selected(state, side_ix == 1)
            }
            screen_input::ThreeKeyMenuAction::Cancel => {
                let undo = state.menu_lr_undo[side_ix];
                if undo != 0 {
                    move_selection(state, undo as isize);
                    state.menu_lr_undo[side_ix] = 0;
                    crate::effects::sfx_then(
                        "assets/sounds/change.ogg",
                        ThemeEffect::Navigate(Screen::Init),
                    )
                } else {
                    ThemeEffect::Navigate(Screen::Init)
                }
            }
        };
    }
    if !ev.pressed {
        return ThemeEffect::None;
    }
    if let Some(delta) = menu_nav_delta(ev.action) {
        move_selection(state, delta);
        return crate::effects::sfx("assets/sounds/change.ogg");
    }
    match ev.action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            start_selected(state, matches!(ev.action, VirtualAction::p2_start))
        }
        VirtualAction::p1_back | VirtualAction::p2_back => ThemeEffect::Navigate(Screen::Init),
        _ => ThemeEffect::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::InputSource;
    use std::time::Instant;

    fn input(action: VirtualAction) -> InputEvent {
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

    fn approx(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn title_brand_uses_simply_love_smooth_fade() {
        approx(brand_exit_alpha(None), 1.0);
        approx(brand_exit_alpha(Some(0.0)), 1.0);
        approx(brand_exit_alpha(Some(BRAND_EXIT_DURATION * 0.5)), 0.5);
        approx(brand_exit_alpha(Some(BRAND_EXIT_DURATION)), 0.0);
    }

    #[test]
    fn title_rows_use_simply_love_stagger() {
        approx(row_exit_alpha(0, Some(0.0)), 1.0);
        approx(row_exit_alpha(0, Some(ROW_EXIT_DURATION * 0.5)), 0.5);
        approx(row_exit_alpha(0, Some(ROW_EXIT_DURATION)), 0.0);

        approx(row_exit_alpha(3, Some(ROW_STAGGER * 3.0)), 1.0);
        approx(
            row_exit_alpha(3, Some(ROW_STAGGER * 3.0 + ROW_EXIT_DURATION * 0.5)),
            0.5,
        );
        approx(
            row_exit_alpha(3, Some(ROW_STAGGER * 3.0 + ROW_EXIT_DURATION)),
            0.0,
        );
    }

    #[test]
    fn title_rows_stagger_in_downward_like_simply_love() {
        approx(row_entry_alpha(0, Some(0.0)), 0.0);
        approx(row_entry_alpha(0, Some(ROW_ENTRY_DURATION * 0.5)), 0.5);
        approx(row_entry_alpha(0, Some(ROW_ENTRY_DURATION)), 1.0);

        approx(row_entry_alpha(3, Some(ROW_STAGGER * 3.0)), 0.0);
        approx(
            row_entry_alpha(3, Some(ROW_STAGGER * 3.0 + ROW_ENTRY_DURATION * 0.5)),
            0.5,
        );
        approx(
            row_entry_alpha(3, Some(ROW_STAGGER * 3.0 + ROW_ENTRY_DURATION)),
            1.0,
        );
        approx(row_entry_alpha(3, None), 1.0);
    }

    #[test]
    fn focus_color_waits_for_glow_in_before_changing() {
        let mut state = init();
        move_selection(&mut state, 1);

        update(&mut state, FOCUS_GLOW_IN * 0.5);
        approx(state.row_anims[0].focus, 0.75);
        approx(state.row_anims[1].focus, 0.0);
        approx(state.row_anims[1].glow_alpha, 0.125);

        update(&mut state, FOCUS_GLOW_IN * 0.5);
        approx(state.row_anims[0].focus, 0.0);
        approx(state.row_anims[1].focus, 0.0);
        approx(state.row_anims[1].glow_alpha, FOCUS_GLOW_ALPHA);

        update(&mut state, FOCUS_GLOW_OUT * 0.5);
        approx(state.row_anims[1].focus, 0.75);
        approx(state.row_anims[1].glow_alpha, 0.125);

        update(&mut state, FOCUS_GLOW_OUT * 0.5);
        approx(state.row_anims[1].focus, 1.0);
        approx(state.row_anims[1].glow_alpha, 0.0);
    }

    #[test]
    fn title_menu_left_and_up_move_previous() {
        assert_eq!(menu_nav_delta(VirtualAction::p1_left), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_menu_left), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_up), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_menu_up), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_left), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_menu_left), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_up), Some(-1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_menu_up), Some(-1));
    }

    #[test]
    fn title_menu_right_and_down_move_next() {
        assert_eq!(menu_nav_delta(VirtualAction::p1_right), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_menu_right), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_down), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p1_menu_down), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_right), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_menu_right), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_down), Some(1));
        assert_eq!(menu_nav_delta(VirtualAction::p2_menu_down), Some(1));
    }

    #[test]
    fn movement_emits_change_sfx() {
        let mut state = init();
        let effect = handle_input(&mut state, &input(VirtualAction::p1_right));

        assert_eq!(state.selected_index, 1);
        assert!(matches!(
            effect,
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                deadsync_theme::AudioRequest::PlaySfx(path)
            )) if path == "assets/sounds/change.ogg"
        ));
    }

    #[test]
    fn start_emits_audio_before_navigation() {
        let mut state = init();
        let effect = handle_input(&mut state, &input(VirtualAction::p2_start));
        let ThemeEffect::Batch(effects) = effect else {
            panic!("expected batched start effect");
        };

        assert!(state.started_by_p2);
        assert!(matches!(
            &effects[0],
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Audio(
                deadsync_theme::AudioRequest::PlaySfx(path)
            )) if *path == "assets/sounds/start.ogg"
        ));
        assert!(matches!(
            effects[1],
            ThemeEffect::Navigate(Screen::SelectProfile)
        ));
    }

    #[test]
    fn prepared_shutdown_capability_adds_shutdown_action() {
        let mut state = init();
        sync_runtime_view(
            &mut state,
            MainMenuRuntimeView {
                allow_shutdown_host: true,
                ..MainMenuRuntimeView::default()
            },
        );
        state.selected_index = OPTION_COUNT;

        let ThemeEffect::Batch(effects) = handle_input(&mut state, &input(VirtualAction::p1_start))
        else {
            panic!("expected batched shutdown effect");
        };
        assert!(matches!(effects[1], ThemeEffect::Shutdown));
    }

    #[test]
    fn removing_shutdown_capability_clamps_selection() {
        let mut state = init();
        state.selected_index = OPTION_COUNT;
        sync_runtime_view(&mut state, MainMenuRuntimeView::default());
        assert_eq!(state.selected_index, OPTION_COUNT - 1);
    }

    #[test]
    fn prepared_chrome_preserves_optional_shutdown_row() {
        fn has_text(actors: &[Actor], expected: &str) -> bool {
            actors.iter().any(|actor| {
                matches!(actor, Actor::Text { content, .. } if content.as_str() == expected)
            })
        }

        let mut state = init();
        let gameplay = tr("Menu", "Gameplay");
        let options = tr("Menu", "Options");
        let exit = tr("Menu", "Exit");
        let shutdown = tr("Menu", "Shutdown");
        let actors = get_actors(&state, None, 1.0);
        assert!(has_text(&actors, &gameplay));
        assert!(has_text(&actors, &options));
        assert!(has_text(&actors, &exit));
        assert!(!has_text(&actors, &shutdown));

        sync_runtime_view(
            &mut state,
            MainMenuRuntimeView {
                allow_shutdown_host: true,
                ..MainMenuRuntimeView::default()
            },
        );
        let actors = get_actors(&state, None, 1.0);
        assert!(has_text(&actors, &shutdown));
    }

    #[test]
    fn local_ip_renders_before_online_services_when_present() {
        fn text_index(actors: &[Actor], expected: &str) -> Option<usize> {
            actors.iter().position(|actor| {
                matches!(actor, Actor::Text { content, .. } if content.as_str() == expected)
            })
        }

        let mut state = init();
        sync_runtime_view(
            &mut state,
            MainMenuRuntimeView {
                local_ip: Some(Arc::from("192.168.1.42")),
                ..MainMenuRuntimeView::default()
            },
        );
        let actors = get_actors(&state, None, 1.0);
        let ip_text = tr_fmt("Menu", "LocalIpAddress", &[("address", "192.168.1.42")]);
        let gs_text = build_groovestats_text(MainMenuGrooveStatus::default()).main;

        let ip_index = text_index(&actors, &ip_text).expect("local IP text");
        let gs_index = text_index(&actors, &gs_text).expect("GrooveStats text");
        assert!(ip_index < gs_index);

        sync_runtime_view(&mut state, MainMenuRuntimeView::default());
        let actors = get_actors(&state, None, 1.0);
        assert_eq!(text_index(&actors, &ip_text), None);
    }

    #[test]
    fn title_menu_back_replays_intro_without_changing_exit_item() {
        let mut state = init();
        assert!(matches!(
            handle_raw_key_event(
                &mut state,
                &RawKeyboardEvent {
                    code: KeyCode::Escape,
                    pressed: true,
                    repeat: false,
                    timestamp: Instant::now(),
                    host_nanos: 0,
                }
            ),
            ThemeEffect::Navigate(Screen::Init)
        ));
        assert!(matches!(
            handle_input(&mut state, &input(VirtualAction::p1_back)),
            ThemeEffect::Navigate(Screen::Init)
        ));

        state.runtime_view.dedicated_three_key_nav = true;
        let _ = handle_input(&mut state, &input(VirtualAction::p1_menu_left));
        let ThemeEffect::Batch(effects) =
            handle_input(&mut state, &input(VirtualAction::p1_menu_right))
        else {
            panic!("expected three-key cancel batch");
        };
        assert!(matches!(effects[1], ThemeEffect::Navigate(Screen::Init)));

        state.selected_index = 2;
        let ThemeEffect::Batch(effects) = handle_input(&mut state, &input(VirtualAction::p1_start))
        else {
            panic!("expected batched exit effect");
        };
        assert!(matches!(effects[1], ThemeEffect::Exit));
    }

    #[test]
    fn cancel_transition_matches_simply_love_white_fade() {
        let (actors, duration) = cancel_transition();
        assert_eq!(duration, 1.0);
        assert_eq!(actors.len(), 1);
        let Actor::Sprite { tint, .. } = &actors[0] else {
            panic!("expected white cancel quad");
        };
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn reset_for_entry_preserves_color_and_resets_selection() {
        let mut state = init();
        state.selected_index = 2;
        state.active_color_index = 7;
        state.rainbow_mode = true;

        reset_for_entry(&mut state);

        assert_eq!(state.selected_index, 0);
        assert_eq!(state.active_color_index, 7);
        assert!(!state.rainbow_mode);
    }
}
