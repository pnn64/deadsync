use crate::act;
use crate::assets::AssetManager;
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::profile;
use crate::game::stage_stats;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::components::heart_bg;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

const STAGE_CYCLE_SECONDS: f32 = 4.0;

const CHARACTER_LIMIT: usize = 4;

const WHEEL_CHAR_WIDTH: f32 = 40.0;
const WHEEL_NUM_ITEMS: usize = 7;
const WHEEL_FOCUS_POS: usize = 3; // Simply Love's sick_wheel focus_pos for num_items=7
const WHEEL_SLIDE_SECONDS: f32 = 0.075; // SL: AlphabetCharacterMT.lua linear(0.075)
const WHEEL_HIDE_FADE_SECONDS: f32 = 0.25;

// Layout (Simply Love semantics)
const PLAYER_FRAME_X_OFF: f32 = 160.0;
const PLAYER_FRAME_Y_OFF: f32 = -20.0;
const WHEEL_X_IN_FRAME: f32 = 40.0;
const WHEEL_Y_IN_FRAME: f32 = 58.0;
const PLAYERNAME_X: f32 = -80.0;
const CURSOR_Y_IN_FRAME: f32 = 58.0;

// Cursor (approximate Cursor.png: 496x92 at zoom 0.5 -> 248x46)
const CURSOR_TOTAL_W: f32 = 248.0;
const CURSOR_BOX_W: f32 = 46.0;
const CURSOR_BOX_H: f32 = 46.0;
const CURSOR_BOX_THICK: f32 = 2.0;
const CURSOR_ARROW_ZOOM: f32 = 0.5;

const POSSIBLE_CHARS: [&str; 40] = [
    "&BACK;",
    "&OK;",
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "?",
    "!",
];

#[derive(Clone, Copy, Debug)]
struct WheelItem {
    info_index: usize,
    x: f32,
    x0: f32,
    x1: f32,
}

#[derive(Clone, Debug)]
struct Wheel {
    info_pos: i32,
    anim_elapsed: Option<f32>,
    items: [WheelItem; WHEEL_NUM_ITEMS],
}

#[derive(Clone, Debug)]
struct PlayerEntry {
    joined: bool,
    can_enter: bool,
    done: bool,
    hide_elapsed: f32,
    name: String,
    wheel: Wheel,
}

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    elapsed: f32,
    finish_hold_elapsed: Option<f32>,
    players: [PlayerEntry; 2],
}

#[inline(always)]
const fn side_ix(side: profile::PlayerSide) -> usize {
    match side {
        profile::PlayerSide::P1 => 0,
        profile::PlayerSide::P2 => 1,
    }
}

fn player_color_rgba(side: profile::PlayerSide, active_color_index: i32) -> [f32; 4] {
    match side {
        profile::PlayerSide::P1 => color::simply_love_rgba(active_color_index),
        profile::PlayerSide::P2 => color::simply_love_rgba(active_color_index - 2),
    }
}

fn sanitize_name(raw: &str) -> String {
    let mut out = String::with_capacity(CHARACTER_LIMIT);
    for ch in raw.chars() {
        if out.len() >= CHARACTER_LIMIT {
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '?' || ch == '!' {
            out.push(ch.to_ascii_uppercase());
        }
    }
    out
}

#[inline(always)]
fn wrap_info_index(info_pos: i32, slot_index1: usize, len: usize) -> usize {
    // Simply Love's wrapped_index(start, offset, set_size):
    // ((start - 1 + offset) % set_size) + 1
    let len_i = len as i32;
    let start = info_pos;
    let offset = slot_index1 as i32;
    let idx1 = (start - 1 + offset).rem_euclid(len_i) + 1;
    (idx1 - 1) as usize
}

#[inline(always)]
fn slot_x(slot_index1: usize) -> f32 {
    let center = (WHEEL_NUM_ITEMS as f32 / 2.0).ceil();
    WHEEL_CHAR_WIDTH * ((slot_index1 as f32) - center)
}

impl Wheel {
    fn new(starting_char_index1: i32) -> Self {
        let len = POSSIBLE_CHARS.len();
        let start_pos = starting_char_index1 - WHEEL_FOCUS_POS as i32;

        Self {
            info_pos: start_pos,
            anim_elapsed: None,
            items: std::array::from_fn(|i| {
                let slot = i + 1;
                let x = slot_x(slot);
                WheelItem {
                    info_index: wrap_info_index(start_pos, slot, len),
                    x,
                    x0: x,
                    x1: x,
                }
            }),
        }
    }

    #[inline(always)]
    fn focused_info_index(&self) -> usize {
        self.items[WHEEL_FOCUS_POS - 1].info_index
    }

    fn finish_tweens(&mut self) {
        let Some(t) = self.anim_elapsed else {
            return;
        };
        let p = (t / WHEEL_SLIDE_SECONDS).clamp(0.0, 1.0);
        for it in &mut self.items {
            it.x = it.x0 + (it.x1 - it.x0) * p;
            it.x0 = it.x;
        }
        self.anim_elapsed = None;
    }

    fn start_tween_to_slots(&mut self) {
        for (i, it) in self.items.iter_mut().enumerate() {
            let slot = i + 1;
            it.x0 = it.x;
            it.x1 = slot_x(slot);
        }
        self.anim_elapsed = Some(0.0);
    }

    fn sync_info_indices(&mut self) {
        let len = POSSIBLE_CHARS.len();
        for (i, it) in self.items.iter_mut().enumerate() {
            let slot = i + 1;
            it.info_index = wrap_info_index(self.info_pos, slot, len);
        }
    }

    fn scroll_by(&mut self, dir: i32) {
        if dir == 0 || POSSIBLE_CHARS.is_empty() {
            return;
        }

        self.finish_tweens();

        self.info_pos = self.info_pos.saturating_add(dir);
        if dir > 0 {
            self.items.rotate_left(dir as usize);
        } else {
            self.items.rotate_right((-dir) as usize);
        }
        self.sync_info_indices();
        self.start_tween_to_slots();
    }

    fn scroll_to_pos(&mut self, focused_char_index1: i32) {
        if POSSIBLE_CHARS.is_empty() {
            return;
        }

        let start_pos = focused_char_index1 - WHEEL_FOCUS_POS as i32;
        let shift_amount = start_pos - self.info_pos;
        if shift_amount == 0 {
            return;
        }

        self.finish_tweens();
        self.info_pos = start_pos;

        if shift_amount.abs() < WHEEL_NUM_ITEMS as i32 {
            if shift_amount > 0 {
                self.items.rotate_left(shift_amount as usize);
            } else {
                self.items.rotate_right((-shift_amount) as usize);
            }
        }

        self.sync_info_indices();
        self.start_tween_to_slots();
    }

    fn update(&mut self, dt: f32) {
        let Some(t) = &mut self.anim_elapsed else {
            return;
        };
        *t = (*t + dt).max(0.0);
        let p = (*t / WHEEL_SLIDE_SECONDS).clamp(0.0, 1.0);
        for it in &mut self.items {
            it.x = it.x0 + (it.x1 - it.x0) * p;
        }
        if p >= 1.0 {
            for it in &mut self.items {
                it.x = it.x1;
                it.x0 = it.x;
            }
            self.anim_elapsed = None;
        }
    }
}

fn player_entry_for(side: profile::PlayerSide) -> PlayerEntry {
    let joined = profile::is_session_side_joined(side);
    let persistent = joined && !profile::is_session_side_guest(side);
    let can_enter = persistent;

    let name = if persistent {
        profile::get_for_side(side).player_initials
    } else {
        String::new()
    };

    // Simply Love: focus starts on OK if a persistent profile has a previous name.
    let focus_char_index1 = if persistent && !name.is_empty() {
        2 // "&OK;"
    } else {
        3 // "A"
    };

    PlayerEntry {
        joined,
        can_enter,
        done: !can_enter,
        hide_elapsed: 0.0,
        name,
        wheel: Wheel::new(focus_char_index1),
    }
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX, // overwritten by app.rs
        bg: heart_bg::State::new(),
        elapsed: 0.0,
        finish_hold_elapsed: None,
        players: [
            player_entry_for(profile::PlayerSide::P1),
            player_entry_for(profile::PlayerSide::P2),
        ],
    }
}

fn all_done(state: &State) -> bool {
    state
        .players
        .iter()
        .filter(|p| p.can_enter)
        .all(|p| p.done)
}

fn start_finish(state: &mut State) {
    if state.finish_hold_elapsed.is_some() {
        return;
    }

    for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
        let ix = side_ix(side);
        let p = &state.players[ix];
        if !(p.joined && p.can_enter) {
            continue;
        }
        if profile::is_session_side_guest(side) {
            continue;
        }
        let name = sanitize_name(&p.name);
        if name.is_empty() {
            continue;
        }
        profile::update_player_initials_for_side(side, &name);
    }

    state.finish_hold_elapsed = Some(0.0);
}

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    state.elapsed = (state.elapsed + dt).max(0.0);

    for p in &mut state.players {
        if p.can_enter {
            p.wheel.update(dt);
        }
        if p.can_enter && p.done && p.hide_elapsed < WHEEL_HIDE_FADE_SECONDS {
            p.hide_elapsed = (p.hide_elapsed + dt).max(0.0);
        }
    }

    if let Some(t) = &mut state.finish_hold_elapsed {
        *t = (*t + dt).max(0.0);
        if *t >= WHEEL_HIDE_FADE_SECONDS {
            return Some(ScreenAction::Navigate(Screen::Menu));
        }
        return None;
    }

    None
}

fn remove_last_char(p: &mut PlayerEntry) -> bool {
    p.name.pop().is_some()
}

fn handle_start(p: &mut PlayerEntry) {
    let selected = POSSIBLE_CHARS
        .get(p.wheel.focused_info_index())
        .copied()
        .unwrap_or("&OK;");
    match selected {
        "&OK;" => {
            p.done = true;
            p.hide_elapsed = 0.0;
            audio::play_sfx("assets/sounds/start.ogg");
        }
        "&BACK;" => {
            if remove_last_char(p) {
                audio::play_sfx("assets/sounds/change_value.ogg");
            } else {
                audio::play_sfx("assets/sounds/boom.ogg");
            }
        }
        ch => {
            if p.name.len() < CHARACTER_LIMIT {
                p.name.push_str(ch);
                audio::play_sfx("assets/sounds/start.ogg");
            } else {
                audio::play_sfx("assets/sounds/boom.ogg");
            }

            if p.name.len() >= CHARACTER_LIMIT {
                // Simply Love: auto scroll focus to "&OK;" when limit reached.
                p.wheel.scroll_to_pos(2);
            }
        }
    }
}

fn handle_delete(p: &mut PlayerEntry) {
    if remove_last_char(p) {
        audio::play_sfx("assets/sounds/change_value.ogg");
    } else {
        audio::play_sfx("assets/sounds/boom.ogg");
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if state.finish_hold_elapsed.is_some() || !ev.pressed {
        return ScreenAction::None;
    }

    let mut handle_for = |side: profile::PlayerSide, f: fn(&mut PlayerEntry)| {
        let ix = side_ix(side);
        let p = &mut state.players[ix];
        if !(p.joined && p.can_enter) || p.done {
            return;
        }
        f(p);
        if all_done(state) {
            return;
        }
    };

    match ev.action {
        VirtualAction::p1_menu_left
        | VirtualAction::p1_left
        | VirtualAction::p1_menu_up
        | VirtualAction::p1_up => {
            let ix = side_ix(profile::PlayerSide::P1);
            let p = &mut state.players[ix];
            if p.joined && p.can_enter && !p.done {
                p.wheel.scroll_by(-1);
                audio::play_sfx("assets/sounds/change.ogg");
            }
        }
        VirtualAction::p1_menu_right
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_down
        | VirtualAction::p1_down => {
            let ix = side_ix(profile::PlayerSide::P1);
            let p = &mut state.players[ix];
            if p.joined && p.can_enter && !p.done {
                p.wheel.scroll_by(1);
                audio::play_sfx("assets/sounds/change.ogg");
            }
        }
        VirtualAction::p1_start => handle_for(profile::PlayerSide::P1, handle_start),
        VirtualAction::p1_select => handle_for(profile::PlayerSide::P1, handle_delete),

        VirtualAction::p2_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_up
        | VirtualAction::p2_up => {
            let ix = side_ix(profile::PlayerSide::P2);
            let p = &mut state.players[ix];
            if p.joined && p.can_enter && !p.done {
                p.wheel.scroll_by(-1);
                audio::play_sfx("assets/sounds/change.ogg");
            }
        }
        VirtualAction::p2_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_down
        | VirtualAction::p2_down => {
            let ix = side_ix(profile::PlayerSide::P2);
            let p = &mut state.players[ix];
            if p.joined && p.can_enter && !p.done {
                p.wheel.scroll_by(1);
                audio::play_sfx("assets/sounds/change.ogg");
            }
        }
        VirtualAction::p2_start => handle_for(profile::PlayerSide::P2, handle_start),
        VirtualAction::p2_select => handle_for(profile::PlayerSide::P2, handle_delete),

        _ => {}
    }

    if all_done(state) {
        start_finish(state);
    }

    ScreenAction::None
}

fn stage_index_for(elapsed: f32, num_stages: usize) -> usize {
    if num_stages == 0 {
        return 0;
    }
    let t = if elapsed.is_finite() && elapsed >= 0.0 {
        elapsed
    } else {
        0.0
    };
    ((t / STAGE_CYCLE_SECONDS).floor() as usize) % num_stages
}

fn fallback_banner_key(active_color_index: i32) -> String {
    let banner_num = active_color_index.rem_euclid(12) + 1;
    format!("banner{banner_num}.png")
}

fn build_banner_and_title(
    state: &State,
    stages: &[stage_stats::StageSummary],
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(4);
    let cx = screen_center_x();

    let fallback_key = fallback_banner_key(state.active_color_index);
    actors.push(act!(sprite(fallback_key):
        align(0.5, 0.5):
        xy(cx, 121.5):
        setsize(418.0, 164.0):
        zoom(0.7):
        z(10)
    ));

    if stages.is_empty() {
        actors.push(act!(text:
            font("miso"):
            settext("NO STAGE DATA AVAILABLE"):
            align(0.5, 0.5):
            xy(cx, 54.0):
            zoom(0.8):
            z(11):
            diffuse(1.0, 1.0, 1.0, 1.0):
            horizalign(center)
        ));
        return actors;
    }

    let idx = stage_index_for(state.elapsed, stages.len());
    let stage = &stages[idx];

    let banner_key = stage
        .song
        .banner_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| fallback_banner_key(state.active_color_index));
    actors.push(act!(sprite(banner_key):
        align(0.5, 0.5):
        xy(cx, 121.5):
        setsize(418.0, 164.0):
        zoom(0.7):
        z(11)
    ));

    let title = stage.song.display_title(crate::config::get().translated_titles);
    actors.push(act!(text:
        font("miso"):
        settext(title):
        align(0.5, 0.5):
        xy(cx, 54.0):
        zoom(0.8):
        maxwidth(294.0):
        shadowlength(0.333):
        z(12):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(center)
    ));

    actors
}

fn build_wheel(p: &PlayerEntry, alpha: f32) -> Actor {
    let mut children = Vec::with_capacity(WHEEL_NUM_ITEMS);

    for item_index in 1..=WHEEL_NUM_ITEMS {
        let it = &p.wheel.items[item_index - 1];
        let x = it.x;
        let content = POSSIBLE_CHARS[it.info_index];

        // Mirror AlphabetCharacterMT.lua visibility: hide the right-most two items.
        let visible = item_index < (WHEEL_NUM_ITEMS - 1);
        let a = if visible { alpha } else { 0.0 };
        let (r, g, b) = if item_index == WHEEL_FOCUS_POS {
            (1.0, 1.0, 1.0)
        } else {
            (0.75, 0.75, 0.75)
        };

        children.push(act!(text:
            font("wendy_white"):
            settext(content):
            align(0.5, 0.5):
            xy(x, 0.0):
            zoom(0.5):
            z(12):
            diffuse(r, g, b, a):
            horizalign(center)
        ));
    }

    Actor::Frame {
        align: [0.5, 0.5],
        offset: [WHEEL_X_IN_FRAME, WHEEL_Y_IN_FRAME],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z: 12,
    }
}

fn build_player_frame(side: profile::PlayerSide, state: &State) -> Actor {
    let ix = side_ix(side);
    let p = &state.players[ix];
    let cx = screen_center_x();
    let cy = screen_center_y();

    let px = match side {
        profile::PlayerSide::P1 => cx - PLAYER_FRAME_X_OFF,
        profile::PlayerSide::P2 => cx + PLAYER_FRAME_X_OFF,
    };

    let mut children: Vec<Actor> = Vec::with_capacity(32);

    // Quads: behind name, wheel, and (future) high score list.
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, 0.0):
        zoomto(300.0, screen_height() / 7.0):
        diffuse(0.0, 0.0, 0.0, 0.75):
        z(10)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, CURSOR_Y_IN_FRAME):
        zoomto(300.0, screen_height() / 10.0):
        diffuse(0.0, 0.0, 0.0, 0.5):
        z(10)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, 142.0):
        zoomto(300.0, screen_height() / 4.0):
        diffuse(0.0, 0.0, 0.0, 0.25):
        z(10)
    ));

    if p.can_enter {
        // PlayerName text (stays visible even after finishing input).
        children.push(act!(text:
            font("wendy_white"):
            settext(p.name.clone()):
            align(0.0, 0.5):
            xy(PLAYERNAME_X, 0.0):
            zoom(0.75):
            z(12):
            diffuse(1.0, 1.0, 1.0, 1.0):
            horizalign(left)
        ));

        let alpha = if p.done {
            1.0 - (p.hide_elapsed / WHEEL_HIDE_FADE_SECONDS).clamp(0.0, 1.0)
        } else {
            1.0
        };

        if alpha > 0.0 {
            let pc = player_color_rgba(side, state.active_color_index);

            // Cursor outline around the focused character (always centered).
            let hw = CURSOR_BOX_W * 0.5;
            let hh = CURSOR_BOX_H * 0.5;
            let t = CURSOR_BOX_THICK;

            // Top
            children.push(act!(quad:
                align(0.5, 0.5):
                xy(0.0, CURSOR_Y_IN_FRAME - hh + t * 0.5):
                zoomto(CURSOR_BOX_W, t):
                diffuse(pc[0], pc[1], pc[2], alpha):
                z(11)
            ));
            // Bottom
            children.push(act!(quad:
                align(0.5, 0.5):
                xy(0.0, CURSOR_Y_IN_FRAME + hh - t * 0.5):
                zoomto(CURSOR_BOX_W, t):
                diffuse(pc[0], pc[1], pc[2], alpha):
                z(11)
            ));
            // Left
            children.push(act!(quad:
                align(0.5, 0.5):
                xy(-hw + t * 0.5, CURSOR_Y_IN_FRAME):
                zoomto(t, CURSOR_BOX_H):
                diffuse(pc[0], pc[1], pc[2], alpha):
                z(11)
            ));
            // Right
            children.push(act!(quad:
                align(0.5, 0.5):
                xy(hw - t * 0.5, CURSOR_Y_IN_FRAME):
                zoomto(t, CURSOR_BOX_H):
                diffuse(pc[0], pc[1], pc[2], alpha):
                z(11)
            ));

            // Cursor arrows (Simply Love Cursor.png edges).
            let arrow_x = CURSOR_TOTAL_W * 0.5 - 12.0;
            children.push(act!(sprite("meter_arrow.png"):
                align(0.5, 0.5):
                xy(-arrow_x, CURSOR_Y_IN_FRAME):
                rotationz(0.0):
                zoom(CURSOR_ARROW_ZOOM):
                z(11):
                diffuse(pc[0], pc[1], pc[2], alpha)
            ));
            children.push(act!(sprite("meter_arrow.png"):
                align(0.5, 0.5):
                xy(arrow_x, CURSOR_Y_IN_FRAME):
                rotationz(180.0):
                zoom(CURSOR_ARROW_ZOOM):
                z(11):
                diffuse(pc[0], pc[1], pc[2], alpha)
            ));

            children.push(build_wheel(p, alpha));
        }
    } else if p.joined {
        let pc = player_color_rgba(side, state.active_color_index);
        children.push(act!(text:
            font("miso"):
            settext("Out of Ranking"):
            align(0.5, 0.5):
            xy(0.0, CURSOR_Y_IN_FRAME):
            zoom(0.7):
            z(12):
            diffuse(pc[0], pc[1], pc[2], 1.0):
            horizalign(center)
        ));
    }

    Actor::Frame {
        align: [0.5, 0.5],
        offset: [px, cy + PLAYER_FRAME_Y_OFF],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z: 100,
    }
}

pub fn get_actors(
    state: &State,
    stages: &[stage_stats::StageSummary],
    _asset_manager: &AssetManager,
) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(64);

    // Background
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    // Banner + title cycling (Simply Love behavior)
    actors.extend(build_banner_and_title(state, stages));

    for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
        if !state.players[side_ix(side)].joined {
            continue;
        }
        actors.push(build_player_frame(side, state));
    }

    actors
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0): z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0): z(1100):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}
