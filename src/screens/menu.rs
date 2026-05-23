use crate::act;
use crate::assets::i18n::{self, tr, tr_fmt};
use crate::assets::{FontRole, current_machine_font_key};
// Screen navigation is handled in app
use crate::engine::input::{InputEvent, MouseButton, PointerEvent, PointerKind, RawKeyboardEvent, VirtualAction};
use crate::engine::present::actors::{Actor, TextAlign};
use crate::engine::present::color;
use crate::game::course::get_course_cache;
use crate::game::online::{
    self as network, ArrowCloudConnectionStatus, ArrowCloudError, ConnectionStatus,
    GrooveStatsError,
};
use crate::game::song::get_song_cache;
use crate::screens::components::menu::logo::{self, LogoParams};
use crate::screens::components::menu::menu_list::{self};
use crate::screens::components::menu::menu_splash;
use crate::screens::components::shared::hitbox::{HitRect, hit_test};
use crate::screens::components::shared::{screen_bar, transitions, visual_style_bg};
use crate::screens::input as screen_input;
use crate::screens::{Screen, ScreenAction};
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use winit::keyboard::KeyCode;

use crate::engine::space::screen_center_x;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.5;
const TRANSITION_OUT_DURATION: f32 = 1.0;

const NORMAL_COLOR_HEX: &str = "#888888";

pub const OPTION_COUNT: usize = 3;

// --- CONSTANTS UPDATED FOR NEW ANIMATION-DRIVEN LAYOUT ---
//const MENU_BELOW_LOGO: f32 = 25.0;
//const MENU_ROW_SPACING: f32 = 23.0;

const MENU_BELOW_LOGO: f32 = 29.0;
const MENU_ROW_SPACING: f32 = 28.0;

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum GrooveStatusKey {
    Pending {
        boogie: bool,
    },
    Error {
        boogie: bool,
        kind: GrooveStatsError,
    },
    Connected {
        boogie: bool,
        disabled_mask: u8,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArrowCloudStatusKey {
    Pending,
    Connected,
    Error(ArrowCloudError),
}

fn groove_error_text(kind: GrooveStatsError) -> Arc<str> {
    match kind {
        GrooveStatsError::Disabled => tr("Menu", "Disabled"),
        GrooveStatsError::MachineOffline => tr("Menu", "MachineOffline"),
        GrooveStatsError::CannotConnect => tr("Menu", "CannotConnect"),
        GrooveStatsError::TimedOut => tr("Menu", "TimedOut"),
        GrooveStatsError::InvalidResponse => tr("Menu", "FailedToLoad"),
    }
}

fn arrowcloud_error_text(kind: ArrowCloudError) -> Arc<str> {
    match kind {
        ArrowCloudError::Disabled => tr("Menu", "Disabled"),
        ArrowCloudError::TimedOut => tr("Menu", "TimedOut"),
        ArrowCloudError::HostBlocked => tr("Menu", "HostBlocked"),
        ArrowCloudError::CannotConnect => tr("Menu", "CannotConnect"),
    }
}

pub struct State {
    pub selected_index: usize,
    pub active_color_index: i32,
    pub rainbow_mode: bool,
    pub started_by_p2: bool,
    bg: visual_style_bg::State,
    i18n_revision: Cell<u64>,
    info_text_cache: RefCell<Option<(Option<String>, Arc<str>)>>,
    groovestats_text_cache: RefCell<Option<StatusTextCache<GrooveStatusKey, 3>>>,
    arrowcloud_text_cache: RefCell<Option<StatusTextCache<ArrowCloudStatusKey, 1>>>,
    menu_lr_chord: screen_input::MenuLrChordTracker,
    menu_lr_undo: [i8; 2],
    /// Index currently under the mouse pointer, or `None` if the cursor is
    /// outside the menu rows (or mouse input is disabled).
    hovered_index: Option<usize>,
}

pub fn init() -> State {
    State {
        selected_index: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX, // was 0
        rainbow_mode: false,
        started_by_p2: false,
        bg: visual_style_bg::State::new(),
        i18n_revision: Cell::new(i18n::revision()),
        info_text_cache: RefCell::new(None),
        groovestats_text_cache: RefCell::new(None),
        arrowcloud_text_cache: RefCell::new(None),
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        menu_lr_undo: [0; 2],
        hovered_index: None,
    }
}

// Keyboard input is handled centrally via the virtual dispatcher in app
// Screen-specific raw keyboard handling for Menu (e.g., F4 to Sandbox)
pub fn handle_raw_key_event(_state: &mut State, key: &RawKeyboardEvent) -> ScreenAction {
    if !key.pressed {
        return ScreenAction::None;
    }
    match key.code {
        KeyCode::F4 => return ScreenAction::Navigate(Screen::Sandbox),
        KeyCode::Escape => return ScreenAction::Exit,
        _ => {}
    }
    ScreenAction::None
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition(active_color_index: i32) -> (Vec<Actor>, f32) {
    let mut actors: Vec<Actor> = Vec::new();

    // Hearts splash, matching Simply Love's ScreenTitleMenu out.lua look.
    actors.extend(menu_splash::build(active_color_index));

    // Full-screen fade to black behind the hearts.
    let fade = transitions::fade_out_black_actor(TRANSITION_OUT_DURATION, 1200);
    actors.push(fade);

    (actors, TRANSITION_OUT_DURATION)
}

pub fn clear_render_cache(state: &State) {
    *state.info_text_cache.borrow_mut() = None;
    *state.groovestats_text_cache.borrow_mut() = None;
    *state.arrowcloud_text_cache.borrow_mut() = None;
}

fn sync_i18n_cache(state: &State) {
    let revision = i18n::revision();
    if state.i18n_revision.get() == revision {
        return;
    }
    clear_render_cache(state);
    state.i18n_revision.set(revision);
}

#[inline(always)]
fn menu_info_text(state: &State) -> Arc<str> {
    let banner_tag = update_banner_tag();
    if let Some((cached_tag, text)) = state.info_text_cache.borrow().as_ref()
        && cached_tag == &banner_tag
    {
        return text.clone();
    }

    let version = crate::engine::version::current().to_string();
    let song_cache = get_song_cache();
    let num_packs = song_cache.len();
    let num_songs: usize = song_cache.iter().map(|pack| pack.songs.len()).sum();
    let num_courses = get_course_cache().len();
    let mut version_line = tr_fmt("Menu", "VersionLine", &[("version", &version)]).to_string();
    if let Some(tag) = banner_tag.as_deref() {
        let suffix = tr_fmt("Menu", "UpdateAvailableSuffix", &[("version", tag)]);
        version_line.push(' ');
        version_line.push_str(&suffix);
    }
    let songs = num_songs.to_string();
    let packs = num_packs.to_string();
    let courses = num_courses.to_string();
    let summary = tr_fmt(
        "Menu",
        "SongSummary",
        &[("songs", &songs), ("packs", &packs), ("courses", &courses)],
    );
    let text = Arc::<str>::from(format!("{version_line}\n{summary}"));
    *state.info_text_cache.borrow_mut() = Some((banner_tag, text.clone()));
    text
}

fn update_banner_tag() -> Option<String> {
    match crate::engine::updater::state::snapshot()? {
        crate::engine::updater::UpdateState::Available(info) => Some(info.tag),
        _ => None,
    }
}

#[inline(always)]
fn groove_service_name(boogie: bool) -> Arc<str> {
    if boogie {
        tr("Menu", "BoogieStatsName")
    } else {
        tr("Menu", "GrooveStatsName")
    }
}

#[inline(always)]
fn groove_status_key() -> GrooveStatusKey {
    let boogie = network::is_boogiestats_active();
    match network::get_status() {
        ConnectionStatus::Pending => GrooveStatusKey::Pending { boogie },
        ConnectionStatus::Error(kind) => GrooveStatusKey::Error { boogie, kind },
        ConnectionStatus::Connected(services) => GrooveStatusKey::Connected {
            boogie,
            disabled_mask: (!services.get_scores) as u8
                | (((!services.leaderboard) as u8) << 1)
                | (((!services.auto_submit) as u8) << 2),
        },
    }
}

fn build_groovestats_text(key: GrooveStatusKey) -> StatusTextCache<GrooveStatusKey, 3> {
    let mut lines = [None, None, None];
    let (main, line_count) = match key {
        GrooveStatusKey::Pending { boogie } => {
            let service = groove_service_name(boogie);
            (
                tr_fmt("Menu", "ServicePending", &[("service", service.as_ref())]),
                0,
            )
        }
        GrooveStatusKey::Error { boogie, kind } => {
            lines[0] = Some(groove_error_text(kind));
            if kind == GrooveStatsError::Disabled {
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
        GrooveStatusKey::Connected {
            boogie,
            disabled_mask,
        } => {
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

fn groovestats_text(state: &State) -> StatusTextCache<GrooveStatusKey, 3> {
    let key = groove_status_key();
    if let Some(cache) = state.groovestats_text_cache.borrow().as_ref()
        && cache.key == key
    {
        return cache.clone();
    }
    let cache = build_groovestats_text(key);
    *state.groovestats_text_cache.borrow_mut() = Some(cache.clone());
    cache
}

#[inline(always)]
fn arrowcloud_status_key() -> ArrowCloudStatusKey {
    match network::get_arrowcloud_status() {
        ArrowCloudConnectionStatus::Pending => ArrowCloudStatusKey::Pending,
        ArrowCloudConnectionStatus::Connected => ArrowCloudStatusKey::Connected,
        ArrowCloudConnectionStatus::Error(kind) => ArrowCloudStatusKey::Error(kind),
    }
}

fn build_arrowcloud_text(key: ArrowCloudStatusKey) -> StatusTextCache<ArrowCloudStatusKey, 1> {
    let mut lines = [None];
    let (main, line_count) = match key {
        ArrowCloudStatusKey::Pending => (tr("Menu", "ArrowCloudPending"), 0),
        ArrowCloudStatusKey::Connected => (tr("Menu", "ArrowCloudConnected"), 0),
        ArrowCloudStatusKey::Error(kind) => {
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

fn arrowcloud_text(state: &State) -> StatusTextCache<ArrowCloudStatusKey, 1> {
    let key = arrowcloud_status_key();
    if let Some(cache) = state.arrowcloud_text_cache.borrow().as_ref()
        && cache.key == key
    {
        return cache.clone();
    }
    let cache = build_arrowcloud_text(key);
    *state.arrowcloud_text_cache.borrow_mut() = Some(cache.clone());
    cache
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

// Signature changed to accept the alpha_multiplier
pub fn get_actors(state: &State, alpha_multiplier: f32) -> Vec<Actor> {
    sync_i18n_cache(state);
    let lp = LogoParams::default();
    let mut actors: Vec<Actor> = Vec::with_capacity(96);

    // 1) background component (never fades)
    let backdrop = if state.rainbow_mode {
        [1.0, 1.0, 1.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    actors.extend(state.bg.build(visual_style_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: backdrop,
        alpha_mul: 1.0,
    }));

    // If fully faded, don't create the other actors
    if alpha_multiplier <= 0.0 {
        return actors;
    }

    // --- The rest of the function is the same, but uses the passed-in alpha_multiplier ---

    // 2) logo + info
    let info2_y_tl = lp.top_margin - INFO_MARGIN_ABOVE - INFO_PX;
    let info1_y_tl = info2_y_tl - INFO_PX - INFO_GAP;

    let logo_actors = logo::build_logo_default();
    for mut actor in logo_actors {
        if let Actor::Sprite { tint, .. } = &mut actor {
            tint[3] *= alpha_multiplier;
        }
        actors.push(actor);
    }

    let mut info_color = [1.0, 1.0, 1.0, 1.0];
    info_color[3] *= alpha_multiplier;

    actors.push(act!(text:
        align(0.5, 0.0): xy(screen_center_x(), info1_y_tl): zoom(0.8):
        font("miso"): settext(menu_info_text(state)): horizalign(center):
        diffuse(info_color[0], info_color[1], info_color[2], info_color[3])
    ));

    // 3) menu list
    let base_y = lp.top_margin + lp.target_h + MENU_BELOW_LOGO;
    let mut selected = color::menu_selected_rgba(state.active_color_index);
    let mut normal = color::rgba_hex(NORMAL_COLOR_HEX);
    selected[3] *= alpha_multiplier;
    normal[3] *= alpha_multiplier;

    let menu_labels = [
        tr("Menu", "Gameplay"),
        tr("Menu", "Options"),
        tr("Menu", "Exit"),
    ];

    // --- UPDATED PARAMS FOR THE NEW MENU LIST BUILDER ---
    let params = menu_list::MenuParams {
        options: &menu_labels,
        selected_index: state.selected_index,
        start_center_y: base_y,
        row_spacing: MENU_ROW_SPACING,
        selected_color: selected,
        normal_color: normal,
        font: current_machine_font_key(FontRole::Bold),
    };
    actors.extend(menu_list::build_vertical_menu(params));

    // --- footer bar ---
    let mut footer_fg = [1.0, 1.0, 1.0, 1.0];
    footer_fg[3] *= alpha_multiplier;
    let event_mode = tr("Common", "EventMode");
    let press_start = tr("Common", "PressStart");

    actors.push(screen_bar::build_title_menu(screen_bar::ScreenBarParams {
        title: event_mode.as_ref(),
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        left_text: Some(press_start.as_ref()),
        center_text: None,
        right_text: Some(press_start.as_ref()),
        left_avatar: None,
        right_avatar: None,
        fg_color: footer_fg,
    }));

    // --- GrooveStats Info Pane (top-left) ---
    let gs_text = groovestats_text(state);
    actors.push(status_text_actor(
        gs_text.main.clone(),
        0.0,
        STATUS_BASE_X,
        STATUS_BASE_Y,
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
                (STATUS_LINE_HEIGHT * (line_idx as f32 + 1.0)).mul_add(STATUS_ZOOM, STATUS_BASE_Y),
                STATUS_ZOOM,
                alpha_multiplier,
                TextAlign::Left,
            ));
        }
    }

    // --- Arrow Cloud Info Pane (below GrooveStats/BoogieStats) ---
    let ac_base_y = (STATUS_LINE_HEIGHT * (gs_text.line_count as f32 + 1.0))
        .mul_add(STATUS_ZOOM, STATUS_BASE_Y + STATUS_BLOCK_GAP);
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

    actors
}

#[inline(always)]
fn move_selection(state: &mut State, delta: isize) {
    let n = OPTION_COUNT as isize;
    let cur = state.selected_index as isize;
    state.selected_index = (cur + delta).rem_euclid(n) as usize;
    crate::engine::audio::play_sfx("assets/sounds/change.ogg");
}

/// Update the selection without playing the change sfx. Used for pointer
/// hover, which should follow the cursor silently rather than chirping on
/// every pixel of movement.
#[inline(always)]
fn set_selection_silent(state: &mut State, index: usize) {
    if index < OPTION_COUNT {
        state.selected_index = index;
    }
}

#[inline(always)]
fn start_selected(state: &mut State, started_by_p2: bool) -> ScreenAction {
    start_index(state, state.selected_index, started_by_p2)
}

#[inline(always)]
fn start_index(state: &mut State, index: usize, started_by_p2: bool) -> ScreenAction {
    crate::engine::audio::play_sfx("assets/sounds/start.ogg");
    state.started_by_p2 = started_by_p2;
    match index {
        0 => ScreenAction::Navigate(Screen::SelectProfile),
        1 => ScreenAction::Navigate(Screen::Options),
        2 => ScreenAction::Exit,
        _ => ScreenAction::None,
    }
}

/// Width of each menu-row hit target in logical units. Picked generously so
/// users don't need pixel-perfect aim, while staying narrower than the
/// 16:9 logical width (854) so it never reaches the screen edges.
const MENU_HIT_WIDTH: f32 = 240.0;

/// Compute the hit rectangles for the three main-menu rows. Mirrors the
/// layout that `get_actors_with_alpha` uses to draw `menu_list`.
fn menu_item_rects() -> [HitRect; OPTION_COUNT] {
    let lp = LogoParams::default();
    let base_y = lp.top_margin + lp.target_h + MENU_BELOW_LOGO;
    let center_x = screen_center_x();
    let mut rects = [HitRect {
        min: crate::engine::space::LogicalPos::new(0.0, 0.0),
        max: crate::engine::space::LogicalPos::new(0.0, 0.0),
        id: 0,
    }; OPTION_COUNT];
    for i in 0..OPTION_COUNT {
        let center_y = (i as f32).mul_add(MENU_ROW_SPACING, base_y);
        rects[i] = HitRect::from_center(
            center_x,
            center_y,
            MENU_HIT_WIDTH,
            MENU_ROW_SPACING,
            i as u32,
        );
    }
    rects
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
pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if let Some(side) = screen_input::menu_lr_side(ev.action)
        && !ev.pressed
    {
        state.menu_lr_undo[screen_input::player_side_ix(side)] = 0;
    }
    if let Some((side, nav)) = screen_input::three_key_menu_action(&mut state.menu_lr_chord, ev) {
        let side_ix = screen_input::player_side_ix(side);
        return match nav {
            screen_input::ThreeKeyMenuAction::Prev => {
                move_selection(state, -1);
                state.menu_lr_undo[side_ix] = 1;
                ScreenAction::None
            }
            screen_input::ThreeKeyMenuAction::Next => {
                move_selection(state, 1);
                state.menu_lr_undo[side_ix] = -1;
                ScreenAction::None
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
                }
                ScreenAction::Exit
            }
        };
    }
    if !ev.pressed {
        return ScreenAction::None;
    }
    if let Some(delta) = menu_nav_delta(ev.action) {
        move_selection(state, delta);
        return ScreenAction::None;
    }
    match ev.action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            start_selected(state, matches!(ev.action, VirtualAction::p2_start))
        }
        VirtualAction::p1_back | VirtualAction::p2_back => ScreenAction::Exit,
        _ => ScreenAction::None,
    }
}

/// Returns true if `pos` (logical coords) falls on one of the main-menu
/// rows. Used by the app to swap the cursor to its hover variant — kept as
/// a thin wrapper so the cursor logic doesn't have to reach into private
/// menu layout details.
pub fn pointer_hits_item(pos: crate::engine::space::LogicalPos) -> bool {
    hit_test(&menu_item_rects(), pos).is_some()
}

/// Pointer-driven navigation for the main menu.
///
/// * Hover moves the highlight silently to the row under the cursor.
/// * Left-click on a row launches that row (same effect as `Start`).
/// * Right-click anywhere is a `back` (exit) consistent with keyboard.
pub fn handle_pointer(state: &mut State, ev: &PointerEvent) -> ScreenAction {
    match ev.kind {
        PointerKind::Move => {
            if let Some(pos) = ev.pos {
                let rects = menu_item_rects();
                if let Some(id) = hit_test(&rects, pos) {
                    let ix = id as usize;
                    state.hovered_index = Some(ix);
                    set_selection_silent(state, ix);
                } else {
                    state.hovered_index = None;
                }
            } else {
                state.hovered_index = None;
            }
            ScreenAction::None
        }
        PointerKind::Leave => {
            state.hovered_index = None;
            ScreenAction::None
        }
        PointerKind::Down(MouseButton::Left) => {
            let Some(pos) = ev.pos else {
                return ScreenAction::None;
            };
            let rects = menu_item_rects();
            match hit_test(&rects, pos) {
                Some(id) => start_index(state, id as usize, false),
                None => ScreenAction::None,
            }
        }
        PointerKind::Down(MouseButton::Right) => ScreenAction::Exit,
        _ => ScreenAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MENU_BELOW_LOGO, MENU_HIT_WIDTH, MENU_ROW_SPACING, OPTION_COUNT, menu_item_rects,
        menu_nav_delta,
    };
    use crate::engine::input::VirtualAction;
    use crate::engine::space::LogicalPos;
    use crate::screens::components::menu::logo::LogoParams;
    use crate::screens::components::shared::hitbox::hit_test;

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
    fn menu_item_rects_are_stacked_and_hit_test_per_row() {
        // Pin the logical surface to a known 16:9 design size so
        // screen_center_x() is stable across machines running the tests.
        crate::engine::space::ortho_for_window(854, 480);
        let rects = menu_item_rects();
        let lp = LogoParams::default();
        let base_y = lp.top_margin + lp.target_h + MENU_BELOW_LOGO;
        let center_x = 0.5 * 854.0;
        for (i, r) in rects.iter().enumerate() {
            let expected_cy = (i as f32).mul_add(MENU_ROW_SPACING, base_y);
            // Rect is centered on the row center.
            assert!((0.5 * (r.min.x + r.max.x) - center_x).abs() < 1e-3);
            assert!((0.5 * (r.min.y + r.max.y) - expected_cy).abs() < 1e-3);
            assert!((r.max.x - r.min.x - MENU_HIT_WIDTH).abs() < 1e-3);
            assert!((r.max.y - r.min.y - MENU_ROW_SPACING).abs() < 1e-3);
            assert_eq!(r.id, i as u32);
        }

        // Centerpoint of each row hits its own id, and falls into a single
        // unique row.
        for i in 0..OPTION_COUNT {
            let center_y = (i as f32).mul_add(MENU_ROW_SPACING, base_y);
            assert_eq!(
                hit_test(&rects, LogicalPos::new(center_x, center_y)),
                Some(i as u32)
            );
        }

        // Far outside the menu region returns no hit.
        assert_eq!(hit_test(&rects, LogicalPos::new(0.0, 0.0)), None);
        assert_eq!(hit_test(&rects, LogicalPos::new(center_x, base_y - 50.0)), None);
    }
}
