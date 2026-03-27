use crate::act;
// Screen navigation is handled in app
use crate::engine::input::{InputEvent, RawKeyboardEvent, VirtualAction};
use crate::engine::present::actors::{Actor, TextAlign};
use crate::engine::present::color;
use crate::game::course::get_course_cache;
use crate::game::online::{self as network, ArrowCloudConnectionStatus, ConnectionStatus};
use crate::game::song::get_song_cache;
use crate::screens::components::menu::logo::{self, LogoParams};
use crate::screens::components::menu::menu_list::{self};
use crate::screens::components::menu::menu_splash;
use crate::screens::components::shared::{heart_bg, screen_bar};
use crate::screens::{Screen, ScreenAction};
use std::cell::RefCell;
use std::sync::{Arc, LazyLock};
use winit::keyboard::KeyCode;

use crate::engine::space::{screen_center_x, screen_height, screen_width};

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.5;
const TRANSITION_OUT_DURATION: f32 = 1.0;

const NORMAL_COLOR_HEX: &str = "#888888";

pub const OPTION_COUNT: usize = 3;
const MENU_OPTIONS: [&str; OPTION_COUNT] = ["GAMEPLAY", "OPTIONS", "EXIT"];

// --- CONSTANTS UPDATED FOR NEW ANIMATION-DRIVEN LAYOUT ---
//const MENU_BELOW_LOGO: f32 = 25.0;
//const MENU_ROW_SPACING: f32 = 23.0;

const MENU_BELOW_LOGO: f32 = 29.0;
const MENU_ROW_SPACING: f32 = 28.0;

const INFO_PX: f32 = 15.0;
const INFO_GAP: f32 = 5.0;
const INFO_MARGIN_ABOVE: f32 = 20.0;

#[derive(Clone)]
struct StatusTextCache<K, const N: usize> {
    key: K,
    main: Arc<str>,
    lines: [Option<Arc<str>>; N],
    line_count: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GrooveErrorKind {
    MachineOffline,
    CannotConnect,
    TimedOut,
    Disabled,
    FailedToLoad,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GrooveStatusKey {
    Disabled,
    Pending { boogie: bool },
    Error { boogie: bool, kind: GrooveErrorKind },
    Connected { boogie: bool, disabled_mask: u8 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArrowCloudStatusKey {
    Disabled,
    Pending,
    Connected,
    TimedOut,
    HostBlocked,
    CannotConnect,
}

static DISABLED_LINE_TEXT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::<str>::from("Disabled"));
static FAILED_TO_LOAD_TEXT: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::<str>::from("Failed to Load 😞"));
static MACHINE_OFFLINE_TEXT: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::<str>::from("Machine Offline"));
static CANNOT_CONNECT_TEXT: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::<str>::from("Cannot Connect"));
static TIMED_OUT_TEXT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::<str>::from("Timed Out"));
static HOST_BLOCKED_TEXT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::<str>::from("Host Blocked"));
static GROOVESTATS_DISABLED_TEXT: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::<str>::from("❌ GrooveStats"));
static GROOVESTATS_WARN_TEXT: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::<str>::from("⚠ GrooveStats"));
static GET_SCORES_DISABLED_TEXT: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::<str>::from("❌ Get Scores"));
static LEADERBOARD_DISABLED_TEXT: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::<str>::from("❌ Leaderboard"));
static AUTO_SUBMIT_DISABLED_TEXT: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::<str>::from("❌ Auto-Submit"));
static ARROW_CLOUD_DISABLED_TEXT: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::<str>::from("❌ Arrow Cloud"));
static ARROW_CLOUD_PENDING_TEXT: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::<str>::from("     Arrow Cloud"));
static ARROW_CLOUD_CONNECTED_TEXT: LazyLock<Arc<str>> =
    LazyLock::new(|| Arc::<str>::from("✔ Arrow Cloud"));

pub struct State {
    pub selected_index: usize,
    pub active_color_index: i32,
    pub rainbow_mode: bool,
    pub started_by_p2: bool,
    bg: heart_bg::State,
    info_text_cache: RefCell<Option<Arc<str>>>,
    groovestats_text_cache: RefCell<Option<StatusTextCache<GrooveStatusKey, 3>>>,
    arrowcloud_text_cache: RefCell<Option<StatusTextCache<ArrowCloudStatusKey, 1>>>,
}

pub fn init() -> State {
    State {
        selected_index: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX, // was 0
        rainbow_mode: false,
        started_by_p2: false,
        bg: heart_bg::State::new(),
        info_text_cache: RefCell::new(None),
        groovestats_text_cache: RefCell::new(None),
        arrowcloud_text_cache: RefCell::new(None),
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
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition(active_color_index: i32) -> (Vec<Actor>, f32) {
    let mut actors: Vec<Actor> = Vec::new();

    // Hearts splash, matching Simply Love's ScreenTitleMenu out.lua look.
    actors.extend(menu_splash::build(active_color_index));

    // Full-screen fade to black behind the hearts.
    let fade = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    actors.push(fade);

    (actors, TRANSITION_OUT_DURATION)
}

pub fn clear_render_cache(state: &State) {
    *state.info_text_cache.borrow_mut() = None;
    *state.groovestats_text_cache.borrow_mut() = None;
    *state.arrowcloud_text_cache.borrow_mut() = None;
}

#[inline(always)]
fn menu_info_text(state: &State) -> Arc<str> {
    if let Some(text) = state.info_text_cache.borrow().as_ref() {
        return text.clone();
    }

    let version = env!("CARGO_PKG_VERSION");
    let song_cache = get_song_cache();
    let num_packs = song_cache.len();
    let num_songs: usize = song_cache.iter().map(|pack| pack.songs.len()).sum();
    let num_courses = get_course_cache().len();
    let text = Arc::<str>::from(format!(
        "DeadSync {version}\n{num_songs} songs in {num_packs} groups, {num_courses} courses"
    ));
    *state.info_text_cache.borrow_mut() = Some(text.clone());
    text
}

#[inline(always)]
fn groove_service_name(boogie: bool) -> &'static str {
    if boogie { "BoogieStats" } else { "GrooveStats" }
}

#[inline(always)]
fn groove_error_kind(msg: &str) -> GrooveErrorKind {
    match msg {
        "Machine Offline" => GrooveErrorKind::MachineOffline,
        "Cannot Connect" => GrooveErrorKind::CannotConnect,
        "Timed Out" => GrooveErrorKind::TimedOut,
        "Disabled" => GrooveErrorKind::Disabled,
        _ => GrooveErrorKind::FailedToLoad,
    }
}

#[inline(always)]
fn groove_status_key() -> GrooveStatusKey {
    let cfg = crate::config::get();
    if !cfg.enable_groovestats {
        return GrooveStatusKey::Disabled;
    }
    let boogie = network::is_boogiestats_active();
    match network::get_status() {
        ConnectionStatus::Pending => GrooveStatusKey::Pending { boogie },
        ConnectionStatus::Error(msg) => GrooveStatusKey::Error {
            boogie,
            kind: groove_error_kind(msg.as_str()),
        },
        ConnectionStatus::Connected(services) => GrooveStatusKey::Connected {
            boogie,
            disabled_mask: (!services.get_scores) as u8
                | (((!services.leaderboard) as u8) << 1)
                | (((!services.auto_submit) as u8) << 2),
        },
    }
}

#[inline(always)]
fn groove_error_text(kind: GrooveErrorKind) -> Arc<str> {
    match kind {
        GrooveErrorKind::MachineOffline => MACHINE_OFFLINE_TEXT.clone(),
        GrooveErrorKind::CannotConnect => CANNOT_CONNECT_TEXT.clone(),
        GrooveErrorKind::TimedOut => TIMED_OUT_TEXT.clone(),
        GrooveErrorKind::Disabled => DISABLED_LINE_TEXT.clone(),
        GrooveErrorKind::FailedToLoad => FAILED_TO_LOAD_TEXT.clone(),
    }
}

fn build_groovestats_text(key: GrooveStatusKey) -> StatusTextCache<GrooveStatusKey, 3> {
    let mut lines = [None, None, None];
    let (main, line_count) = match key {
        GrooveStatusKey::Disabled => {
            lines[0] = Some(DISABLED_LINE_TEXT.clone());
            (GROOVESTATS_DISABLED_TEXT.clone(), 1)
        }
        GrooveStatusKey::Pending { boogie } => (
            Arc::<str>::from(format!("     {}", groove_service_name(boogie))),
            0,
        ),
        GrooveStatusKey::Error { boogie, kind } => {
            lines[0] = Some(groove_error_text(kind));
            (
                Arc::<str>::from(format!("{} not connected", groove_service_name(boogie))),
                1,
            )
        }
        GrooveStatusKey::Connected {
            boogie,
            disabled_mask,
        } => {
            if disabled_mask == 0 {
                (
                    Arc::<str>::from(format!("✔ {}", groove_service_name(boogie))),
                    0,
                )
            } else if disabled_mask == 0b111 {
                (GROOVESTATS_DISABLED_TEXT.clone(), 0)
            } else {
                let mut line_count = 0;
                if disabled_mask & 0b001 != 0 {
                    lines[line_count] = Some(GET_SCORES_DISABLED_TEXT.clone());
                    line_count += 1;
                }
                if disabled_mask & 0b010 != 0 {
                    lines[line_count] = Some(LEADERBOARD_DISABLED_TEXT.clone());
                    line_count += 1;
                }
                if disabled_mask & 0b100 != 0 {
                    lines[line_count] = Some(AUTO_SUBMIT_DISABLED_TEXT.clone());
                    line_count += 1;
                }
                (GROOVESTATS_WARN_TEXT.clone(), line_count)
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
    if !crate::config::get().enable_arrowcloud {
        return ArrowCloudStatusKey::Disabled;
    }
    match network::get_arrowcloud_status() {
        ArrowCloudConnectionStatus::Pending => ArrowCloudStatusKey::Pending,
        ArrowCloudConnectionStatus::Connected => ArrowCloudStatusKey::Connected,
        ArrowCloudConnectionStatus::Error(msg) => {
            let low = msg.to_ascii_lowercase();
            if low.contains("timed out") {
                ArrowCloudStatusKey::TimedOut
            } else if low.contains("blocked") {
                ArrowCloudStatusKey::HostBlocked
            } else {
                ArrowCloudStatusKey::CannotConnect
            }
        }
    }
}

fn build_arrowcloud_text(key: ArrowCloudStatusKey) -> StatusTextCache<ArrowCloudStatusKey, 1> {
    let mut lines = [None];
    let (main, line_count) = match key {
        ArrowCloudStatusKey::Disabled => {
            lines[0] = Some(DISABLED_LINE_TEXT.clone());
            (ARROW_CLOUD_DISABLED_TEXT.clone(), 1)
        }
        ArrowCloudStatusKey::Pending => (ARROW_CLOUD_PENDING_TEXT.clone(), 0),
        ArrowCloudStatusKey::Connected => (ARROW_CLOUD_CONNECTED_TEXT.clone(), 0),
        ArrowCloudStatusKey::TimedOut => {
            lines[0] = Some(TIMED_OUT_TEXT.clone());
            (ARROW_CLOUD_DISABLED_TEXT.clone(), 1)
        }
        ArrowCloudStatusKey::HostBlocked => {
            lines[0] = Some(HOST_BLOCKED_TEXT.clone());
            (ARROW_CLOUD_DISABLED_TEXT.clone(), 1)
        }
        ArrowCloudStatusKey::CannotConnect => {
            lines[0] = Some(CANNOT_CONNECT_TEXT.clone());
            (ARROW_CLOUD_DISABLED_TEXT.clone(), 1)
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
    let lp = LogoParams::default();
    let mut actors: Vec<Actor> = Vec::with_capacity(96);

    // 1) background component (never fades)
    let backdrop = if state.rainbow_mode {
        [1.0, 1.0, 1.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    actors.extend(state.bg.build(heart_bg::Params {
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

    // --- UPDATED PARAMS FOR THE NEW MENU LIST BUILDER ---
    let params = menu_list::MenuParams {
        options: &MENU_OPTIONS,
        selected_index: state.selected_index,
        start_center_y: base_y,
        row_spacing: MENU_ROW_SPACING,
        selected_color: selected,
        normal_color: normal,
        font: "wendy",
    };
    actors.extend(menu_list::build_vertical_menu(params));

    // --- footer bar ---
    let mut footer_fg = [1.0, 1.0, 1.0, 1.0];
    footer_fg[3] *= alpha_multiplier;

    actors.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: "EVENT MODE",
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        left_text: Some("PRESS START"),
        center_text: None,
        right_text: Some("PRESS START"),
        left_avatar: None,
        right_avatar: None,
        fg_color: footer_fg,
    }));

    // --- GrooveStats Info Pane (top-left) ---
    let frame_zoom = 0.8;
    let base_x = 10.0;
    let base_y = 15.0;
    let gs_text = groovestats_text(state);
    actors.push(status_text_actor(
        gs_text.main.clone(),
        0.0,
        base_x,
        base_y,
        frame_zoom,
        alpha_multiplier,
        TextAlign::Left,
    ));
    let line_height_offset = 18.0;
    for line_idx in 0..gs_text.line_count {
        if let Some(text) = gs_text.lines[line_idx].as_ref() {
            actors.push(status_text_actor(
                text.clone(),
                0.0,
                base_x,
                (line_height_offset * (line_idx as f32 + 1.0)).mul_add(frame_zoom, base_y),
                frame_zoom,
                alpha_multiplier,
                TextAlign::Left,
            ));
        }
    }

    // --- Arrow Cloud Info Pane (top-right) ---
    let ac_frame_zoom = 0.8;
    let ac_base_x = screen_width() - 10.0;
    let ac_base_y = 15.0;
    let ac_text = arrowcloud_text(state);
    actors.push(status_text_actor(
        ac_text.main.clone(),
        1.0,
        ac_base_x,
        ac_base_y,
        ac_frame_zoom,
        alpha_multiplier,
        TextAlign::Right,
    ));
    let ac_line_height_offset = 18.0;
    for line_idx in 0..ac_text.line_count {
        if let Some(text) = ac_text.lines[line_idx].as_ref() {
            actors.push(status_text_actor(
                text.clone(),
                1.0,
                ac_base_x,
                (ac_line_height_offset * (line_idx as f32 + 1.0)).mul_add(ac_frame_zoom, ac_base_y),
                ac_frame_zoom,
                alpha_multiplier,
                TextAlign::Right,
            ));
        }
    }

    actors
}

// Event-driven virtual input handler
pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }
    match ev.action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            crate::engine::audio::play_sfx("assets/sounds/start.ogg");
            state.started_by_p2 = matches!(ev.action, VirtualAction::p2_start);
            match state.selected_index {
                0 => ScreenAction::Navigate(Screen::SelectProfile),
                1 => ScreenAction::Navigate(Screen::Options),
                2 => ScreenAction::Exit,
                _ => ScreenAction::None,
            }
        }
        VirtualAction::p1_back | VirtualAction::p2_back => ScreenAction::Exit,
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => {
            let n = OPTION_COUNT as isize;
            let cur = state.selected_index as isize;
            state.selected_index = ((cur - 1 + n) % n) as usize;
            crate::engine::audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => {
            let n = OPTION_COUNT as isize;
            let cur = state.selected_index as isize;
            state.selected_index = ((cur + 1 + n) % n) as usize;
            crate::engine::audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}
