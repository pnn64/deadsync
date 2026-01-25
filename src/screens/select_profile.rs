use crate::act;
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{screen_width, screen_height, screen_center_x, screen_center_y};
use crate::game::parsing::noteskin::{self, Noteskin, Quantization, NUM_QUANTIZATIONS};
use crate::game::profile::{self, ActiveProfile};
use crate::game::scroll::ScrollSpeedSetting;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::{self, Actor};
use crate::ui::color;
use crate::ui::components::screen_bar::{ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement};
use crate::ui::components::{heart_bg, screen_bar};
use std::path::Path;
use std::str::FromStr;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;
// Simply Love:
// - PlayerFrame.lua: bouncebegin(0.35):zoom(0)
// - default.lua: OffCommand sleep(0.5) to let PlayerFrames tween out
const EXIT_ANIM_DURATION: f32 = 0.5;
const PLAYERFRAME_EXIT_ZOOM_OUT_DURATION: f32 = 0.35;

// Simply Love:
// PlayerFrame.lua: PlayerJoinedMessageCommand -> zoom(1.15):bounceend(0.175):zoom(1)
const JOIN_PULSE_ZOOM_IN: f32 = 1.15;
const JOIN_PULSE_DURATION: f32 = 0.175;

pub const fn exit_anim_duration() -> f32 {
    EXIT_ANIM_DURATION
}

/* ------------------------------ layout ------------------------------- */
const ROW_H: f32 = 35.0;
const ROWS_VISIBLE: i32 = 9;
const FRAME_BASE_W: f32 = 200.0;
const FRAME_W_SCROLLER: f32 = FRAME_BASE_W * 1.1;
const FRAME_W_JOIN: f32 = FRAME_BASE_W * 0.9;
const FRAME_H: f32 = 214.0;
const FRAME_BORDER: f32 = 2.0;
const FRAME_CX_OFF: f32 = 150.0;
const FRAME_IN_CROP_DUR: f32 = 0.30; // SL: smooth(0.3):cropbottom(0)
const OVERLAY_IN_DELAY: f32 = 0.30; // SL: sleep(0.3)
const OVERLAY_IN_DUR: f32 = 0.10; // SL: linear(0.1)

const INFO_W: f32 = FRAME_BASE_W * 0.475;
const INFO_X0_OFF: f32 = 15.5;
const INFO_PAD: f32 = 4.0;

const SCROLLER_W: f32 = FRAME_W_SCROLLER - INFO_W;
const SCROLLER_CX_OFF: f32 = -47.0;
const SCROLLER_TEXT_PAD_X: f32 = 6.0;

const AVATAR_BG_HEX: &str = "#283239aa";
const AVATAR_X_OFF: f32 = INFO_PAD * 1.125;
const AVATAR_Y_OFF: f32 = -103.5;
const AVATAR_HEART_X: f32 = 13.0;
const AVATAR_HEART_Y: f32 = 8.0;
const AVATAR_HEART_ZOOM: f32 = 0.09;
const AVATAR_TEXT_Y: f32 = 67.0;
const AVATAR_LABEL_ZOOM: f32 = 0.815; // SL: fallback avatar label zoom(0.815)

const INFO_LINE_Y_OFF: f32 = 18.0;
// Unified Y offset for side-by-side previews
const PREVIEW_Y_OFF: f32 = 42.0;

const TOTAL_SONGS_ZOOM: f32 = 0.65; // SL: TotalSongs zoom(0.65)
const MODS_ZOOM: f32 = 0.625; // SL: RecentMods zoom(0.625)
const MODS_Y_OFF: f32 = 47.0; // SL: RecentMods xy(...,47)
const TOTAL_SONGS_STATIC: &str = "123 Songs Played";

const JOIN_TEXT: &str = "Press &START; to join!";
const WAITING_TEXT: &str = "Waiting ...";
const SELECTED_NAME_Y_OFF: f32 = 160.0; // SL: SelectedProfileText y(160)
const SELECTED_NAME_ZOOM: f32 = 1.35; // SL: SelectedProfileText zoom(1.35)

const SHAKE_STEP_DUR: f32 = 0.1; // SL: bounceend(0.1) x3
const SHAKE_DUR: f32 = SHAKE_STEP_DUR * 3.0;

#[derive(Clone)]
struct Choice {
    kind: ActiveProfile,
    display_name: String,
    speed_mod: String,
    avatar_key: Option<String>,
    scroll_option: profile::ScrollOption,
    noteskin: profile::NoteSkin,
    judgment: profile::JudgmentGraphic,
}

pub struct State {
    pub active_color_index: i32,
    p1_joined: bool,
    p2_joined: bool,
    p1_ready: bool,
    p2_ready: bool,
    p1_selected_index: usize,
    p2_selected_index: usize,
    exit_anim: bool,
    choices: Vec<Choice>,
    bg: heart_bg::State,
    p1_preview_noteskin: Option<Noteskin>,
    p2_preview_noteskin: Option<Noteskin>,
    preview_time: f32,
    preview_beat: f32,
    p1_join_pulse_t: f32,
    p2_join_pulse_t: f32,
    p1_shake_t: f32,
    p2_shake_t: f32,
}

fn load_noteskin(kind: profile::NoteSkin) -> Option<Noteskin> {
    let style = noteskin::Style {
        num_cols: 4,
        num_players: 1,
    };

    let path_str = match kind {
        profile::NoteSkin::Cel => "assets/noteskins/cel/dance-single.txt",
        profile::NoteSkin::Metal => "assets/noteskins/metal/dance-single.txt",
        profile::NoteSkin::EnchantmentV2 => "assets/noteskins/enchantment-v2/dance-single.txt",
        profile::NoteSkin::DevCel2024V3 => "assets/noteskins/devcel-2024-v3/dance-single.txt",
    };

    noteskin::load(Path::new(path_str), &style)
        .ok()
        .or_else(|| noteskin::load(Path::new("assets/noteskins/cel/dance-single.txt"), &style).ok())
        .or_else(|| noteskin::load(Path::new("assets/noteskins/fallback.txt"), &style).ok())
}

fn preview_noteskin_for_choice(choices: &[Choice], selected_index: usize) -> Option<Noteskin> {
    let Some(choice) = choices.get(selected_index) else {
        return None;
    };
    match choice.kind {
        ActiveProfile::Guest => None,
        ActiveProfile::Local { .. } => load_noteskin(choice.noteskin),
    }
}

#[inline(always)]
fn format_recent_mods(speed_mod: &str, scroll: profile::ScrollOption) -> String {
    let mut out = String::new();
    let mut first = true;

    let mut push = |s: &str| {
        if s.is_empty() {
            return;
        }
        if !first {
            out.push_str(", ");
        }
        first = false;
        out.push_str(s);
    };

    push(speed_mod.trim());
    if scroll.contains(profile::ScrollOption::Reverse) {
        push("Reverse");
    }
    if scroll.contains(profile::ScrollOption::Split) {
        push("Split");
    }
    if scroll.contains(profile::ScrollOption::Alternate) {
        push("Alternate");
    }
    if scroll.contains(profile::ScrollOption::Cross) {
        push("Cross");
    }
    if scroll.contains(profile::ScrollOption::Centered) {
        push("Centered");
    }
    push("Overhead");
    out
}

fn build_choices() -> Vec<Choice> {
    let mut out = Vec::new();

    let default_profile = crate::game::profile::Profile::default();
    let default_speed_mod = format!("{}", default_profile.scroll_speed);
    let guest_speed_mod = format!("{}", crate::game::profile::GUEST_SCROLL_SPEED);
    let default_scroll_option = default_profile.scroll_option;
    out.push(Choice {
        kind: ActiveProfile::Guest,
        display_name: "[ GUEST ]".to_string(),
        speed_mod: guest_speed_mod,
        avatar_key: None,
        scroll_option: default_scroll_option,
        noteskin: profile::NoteSkin::default(),
        judgment: profile::JudgmentGraphic::default(),
    });
    for p in profile::scan_local_profiles() {
        let mut speed_mod = default_speed_mod.clone();
        let mut scroll_option = default_scroll_option;
        let mut noteskin = profile::NoteSkin::default();
        let mut judgment = profile::JudgmentGraphic::default();
        let ini_path = std::path::Path::new("save/profiles")
            .join(&p.id)
            .join("profile.ini");
        let mut ini = crate::config::SimpleIni::new();
        if ini.load(&ini_path).is_ok() {
            if let Some(raw) = ini.get("PlayerOptions", "ScrollSpeed") {
                let trimmed = raw.trim();
                speed_mod = if let Ok(setting) = ScrollSpeedSetting::from_str(trimmed) {
                    format!("{setting}")
                } else {
                    trimmed.to_string()
                };
            }

            scroll_option = ini
                .get("PlayerOptions", "Scroll")
                .and_then(|s| profile::ScrollOption::from_str(&s).ok())
                .unwrap_or_else(|| {
                    let reverse_enabled = ini
                        .get("PlayerOptions", "ReverseScroll")
                        .and_then(|v| v.parse::<u8>().ok())
                        .is_some_and(|v| v != 0);
                    if reverse_enabled {
                        profile::ScrollOption::Reverse
                    } else {
                        default_scroll_option
                    }
                });
        }
        if let Ok(value) = ini
            .get("PlayerOptions", "NoteSkin")
            .unwrap_or_default()
            .parse::<profile::NoteSkin>()
        {
            noteskin = value;
        }
        if let Ok(value) = ini
            .get("PlayerOptions", "JudgmentGraphic")
            .unwrap_or_default()
            .parse::<profile::JudgmentGraphic>()
        {
            judgment = value;
        }

        out.push(Choice {
            kind: ActiveProfile::Local { id: p.id },
            display_name: p.display_name,
            speed_mod,
            avatar_key: p.avatar_path.map(|path| path.to_string_lossy().into_owned()),
            scroll_option,
            noteskin,
            judgment,
        });
    }
    out
}

pub fn init() -> State {
    let choices = build_choices();
    let active = profile::get_active_profile();
    let active_color_index = crate::config::get().simply_love_color;

    let mut selected_index = 0usize;
    if let ActiveProfile::Local { id } = active
        && let Some(i) = choices.iter().position(|c| match &c.kind {
            ActiveProfile::Local { id: cid } => cid == &id,
            ActiveProfile::Guest => false,
        }) {
            selected_index = i;
        }

    let mut state = State {
        active_color_index,
        p1_joined: true,
        p2_joined: false,
        p1_ready: false,
        p2_ready: false,
        p1_selected_index: selected_index,
        p2_selected_index: selected_index,
        exit_anim: false,
        choices,
        bg: heart_bg::State::new(),
        p1_preview_noteskin: None,
        p2_preview_noteskin: None,
        preview_time: 0.0,
        preview_beat: 0.0,
        p1_join_pulse_t: JOIN_PULSE_DURATION,
        p2_join_pulse_t: JOIN_PULSE_DURATION,
        p1_shake_t: SHAKE_DUR,
        p2_shake_t: SHAKE_DUR,
    };
    state.p1_preview_noteskin =
        preview_noteskin_for_choice(&state.choices, state.p1_selected_index);
    state.p2_preview_noteskin =
        preview_noteskin_for_choice(&state.choices, state.p2_selected_index);
    state
}

pub fn set_joined(state: &mut State, p1_joined: bool, p2_joined: bool) {
    state.p1_joined = p1_joined;
    state.p2_joined = p2_joined;
    state.p1_ready = false;
    state.p2_ready = false;
    state.p1_join_pulse_t = JOIN_PULSE_DURATION;
    state.p2_join_pulse_t = JOIN_PULSE_DURATION;

    state.p1_preview_noteskin =
        preview_noteskin_for_choice(&state.choices, state.p1_selected_index);
    state.p2_preview_noteskin =
        preview_noteskin_for_choice(&state.choices, state.p2_selected_index);
}

pub fn update(state: &mut State, dt: f32) {
    const BPM: f32 = 120.0;
    let dt = dt.max(0.0);
    state.preview_time += dt;
    state.preview_beat += dt * (BPM / 60.0);

    state.p1_join_pulse_t = (state.p1_join_pulse_t + dt).min(JOIN_PULSE_DURATION);
    state.p2_join_pulse_t = (state.p2_join_pulse_t + dt).min(JOIN_PULSE_DURATION);
    state.p1_shake_t = (state.p1_shake_t + dt).min(SHAKE_DUR);
    state.p2_shake_t = (state.p2_shake_t + dt).min(SHAKE_DUR);
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

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

#[inline(always)]
const fn both_ready(state: &State) -> bool {
    (state.p1_ready || !state.p1_joined) && (state.p2_ready || !state.p2_joined)
}

#[inline(always)]
fn active_choices(state: &State) -> (ActiveProfile, ActiveProfile) {
    let p1 = if state.p1_joined {
        state
            .choices
            .get(state.p1_selected_index)
            .map_or(ActiveProfile::Guest, |c| c.kind.clone())
    } else {
        ActiveProfile::Guest
    };
    let p2 = if state.p2_joined {
        state
            .choices
            .get(state.p2_selected_index)
            .map_or(ActiveProfile::Guest, |c| c.kind.clone())
    } else {
        ActiveProfile::Guest
    };
    (p1, p2)
}

fn trigger_invalid_choice(state: &mut State, is_p1: bool) {
    if is_p1 {
        state.p1_shake_t = 0.0;
        // Simply Love `InvalidChoiceMessageCommand` starts with `finishtweening()`,
        // so ensure any join pulse is fully settled before we shake.
        state.p1_join_pulse_t = JOIN_PULSE_DURATION;
    } else {
        state.p2_shake_t = 0.0;
        state.p2_join_pulse_t = JOIN_PULSE_DURATION;
    }
    audio::play_sfx("assets/sounds/boom.ogg");
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed || state.exit_anim {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            if !state.p1_joined || state.p1_ready {
                return ScreenAction::None;
            }
            if state.p1_selected_index > 0 {
                state.p1_selected_index -= 1;
                state.p1_preview_noteskin =
                    preview_noteskin_for_choice(&state.choices, state.p1_selected_index);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            ScreenAction::None
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            if !state.p1_joined || state.p1_ready {
                return ScreenAction::None;
            }
            if state.p1_selected_index + 1 < state.choices.len() {
                state.p1_selected_index += 1;
                state.p1_preview_noteskin =
                    preview_noteskin_for_choice(&state.choices, state.p1_selected_index);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            ScreenAction::None
        }
        VirtualAction::p1_start => {
            if !state.p1_joined {
                state.p1_joined = true;
                state.p1_ready = false;
                state.p1_join_pulse_t = 0.0;
                state.p1_preview_noteskin =
                    preview_noteskin_for_choice(&state.choices, state.p1_selected_index);
                audio::play_sfx("assets/sounds/start.ogg");
                return ScreenAction::None;
            }

            if state.p1_ready {
                return ScreenAction::None;
            }

            if state.p2_joined
                && state.p2_ready
                && state.choices.get(state.p1_selected_index).is_some_and(|c| {
                    !matches!(&c.kind, ActiveProfile::Guest)
                        && state
                            .choices
                            .get(state.p2_selected_index)
                            .is_some_and(|o| o.kind == c.kind)
                })
            {
                trigger_invalid_choice(state, true);
                return ScreenAction::None;
            }

            state.p1_ready = true;
            if both_ready(state) {
                audio::play_sfx("assets/sounds/start.ogg");
                state.exit_anim = true;
                let _ = exit_anim_t(true);
                profile::set_session_player_side(if state.p1_joined {
                    profile::PlayerSide::P1
                } else {
                    profile::PlayerSide::P2
                });
                profile::set_session_joined(state.p1_joined, state.p2_joined);
                let (p1, p2) = active_choices(state);
                return ScreenAction::SelectProfiles { p1, p2 };
            }
            ScreenAction::None
        }
        VirtualAction::p1_back | VirtualAction::p1_select => {
            if state.p1_joined && state.p1_ready {
                state.p1_ready = false;
                audio::play_sfx("assets/sounds/unjoin.ogg");
                return ScreenAction::None;
            }
            if state.p1_joined {
                state.p1_joined = false;
                state.p1_ready = false;
                audio::play_sfx("assets/sounds/unjoin.ogg");
                return ScreenAction::None;
            }
            if state.p2_joined {
                return ScreenAction::None;
            }
            state.exit_anim = true;
            let _ = exit_anim_t(true);
            ScreenAction::Navigate(Screen::Menu)
        }
        VirtualAction::p2_up | VirtualAction::p2_menu_up => {
            if !state.p2_joined || state.p2_ready {
                return ScreenAction::None;
            }
            if state.p2_selected_index > 0 {
                state.p2_selected_index -= 1;
                state.p2_preview_noteskin =
                    preview_noteskin_for_choice(&state.choices, state.p2_selected_index);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            ScreenAction::None
        }
        VirtualAction::p2_down | VirtualAction::p2_menu_down => {
            if !state.p2_joined || state.p2_ready {
                return ScreenAction::None;
            }
            if state.p2_selected_index + 1 < state.choices.len() {
                state.p2_selected_index += 1;
                state.p2_preview_noteskin =
                    preview_noteskin_for_choice(&state.choices, state.p2_selected_index);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            ScreenAction::None
        }
        VirtualAction::p2_start => {
            if !state.p2_joined {
                state.p2_joined = true;
                state.p2_ready = false;
                state.p2_join_pulse_t = 0.0;
                state.p2_preview_noteskin =
                    preview_noteskin_for_choice(&state.choices, state.p2_selected_index);
                audio::play_sfx("assets/sounds/start.ogg");
                return ScreenAction::None;
            }

            if state.p2_ready {
                return ScreenAction::None;
            }

            if state.p1_joined
                && state.p1_ready
                && state.choices.get(state.p2_selected_index).is_some_and(|c| {
                    !matches!(&c.kind, ActiveProfile::Guest)
                        && state
                            .choices
                            .get(state.p1_selected_index)
                            .is_some_and(|o| o.kind == c.kind)
                })
            {
                trigger_invalid_choice(state, false);
                return ScreenAction::None;
            }

            state.p2_ready = true;
            if both_ready(state) {
                audio::play_sfx("assets/sounds/start.ogg");
                state.exit_anim = true;
                let _ = exit_anim_t(true);
                profile::set_session_player_side(if state.p1_joined {
                    profile::PlayerSide::P1
                } else {
                    profile::PlayerSide::P2
                });
                profile::set_session_joined(state.p1_joined, state.p2_joined);
                let (p1, p2) = active_choices(state);
                return ScreenAction::SelectProfiles { p1, p2 };
            }
            ScreenAction::None
        }
        VirtualAction::p2_back | VirtualAction::p2_select => {
            if state.p2_joined && state.p2_ready {
                state.p2_ready = false;
                audio::play_sfx("assets/sounds/unjoin.ogg");
                return ScreenAction::None;
            }
            if state.p2_joined {
                state.p2_joined = false;
                state.p2_ready = false;
                audio::play_sfx("assets/sounds/unjoin.ogg");
                return ScreenAction::None;
            }
            if state.p1_joined {
                return ScreenAction::None;
            }
            state.exit_anim = true;
            let _ = exit_anim_t(true);
            ScreenAction::Navigate(Screen::Menu)
        }
        _ => ScreenAction::None,
    }
}

fn apply_alpha_to_actor(actor: &mut Actor, alpha: f32) {
    match actor {
        Actor::Sprite { tint, .. } => tint[3] *= alpha,
        Actor::Text { color, .. } => color[3] *= alpha,
        Actor::Frame {
            background,
            children,
            ..
        } => {
            if let Some(actors::Background::Color(c)) = background {
                c[3] *= alpha;
            }
            for child in children {
                apply_alpha_to_actor(child, alpha);
            }
        }
        Actor::Shadow { color, child, .. } => {
            color[3] *= alpha;
            apply_alpha_to_actor(child, alpha);
        }
    }
}

#[inline(always)]
fn exit_anim_t(exiting: bool) -> f32 {
    if !exiting {
        return 0.0;
    }

    use crate::ui::{anim, runtime};
    static STEPS: std::sync::OnceLock<Vec<anim::Step>> = std::sync::OnceLock::new();
    let dur = EXIT_ANIM_DURATION.max(0.0);
    let steps = STEPS.get_or_init(|| vec![anim::linear(dur).x(dur).build()]);

    let mut init = anim::TweenState::default();
    init.x = 0.0;
    let sid = runtime::site_id(file!(), line!(), column!(), 0x53454C5052455849u64); // "SELPREXI"
    runtime::materialize(sid, init, steps).x.max(0.0)
}

#[inline(always)]
fn exit_zoom(exit_t: f32) -> f32 {
    let p = crate::ui::anim::bouncebegin_p(
        (exit_t / PLAYERFRAME_EXIT_ZOOM_OUT_DURATION).clamp(0.0, 1.0),
    );
    (1.0 - p).max(0.0)
}

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
fn join_pulse_zoom(join_t: f32) -> f32 {
    if join_t >= JOIN_PULSE_DURATION {
        return 1.0;
    }
    let p = crate::ui::anim::bounceend_p((join_t / JOIN_PULSE_DURATION).clamp(0.0, 1.0));
    lerp(JOIN_PULSE_ZOOM_IN, 1.0, p).max(0.0)
}

#[inline(always)]
fn shake_x(shake_t: f32) -> f32 {
    if shake_t >= SHAKE_DUR {
        return 0.0;
    }
    let p = crate::ui::anim::bounceend_p((shake_t / SHAKE_STEP_DUR).clamp(0.0, 1.0));
    if shake_t < SHAKE_STEP_DUR {
        lerp(0.0, 5.0, p)
    } else if shake_t < SHAKE_STEP_DUR * 2.0 {
        let t = (shake_t - SHAKE_STEP_DUR).clamp(0.0, SHAKE_STEP_DUR);
        let p = crate::ui::anim::bounceend_p((t / SHAKE_STEP_DUR).clamp(0.0, 1.0));
        lerp(5.0, -5.0, p)
    } else {
        let t = SHAKE_STEP_DUR.mul_add(-2.0, shake_t).clamp(0.0, SHAKE_STEP_DUR);
        let p = crate::ui::anim::bounceend_p((t / SHAKE_STEP_DUR).clamp(0.0, 1.0));
        lerp(-5.0, 0.0, p)
    }
}

#[inline(always)]
fn scale_about(v: f32, pivot: f32, zoom: f32) -> f32 {
    (v - pivot).mul_add(zoom, pivot)
}

fn apply_zoom_to_actor(actor: &mut Actor, pivot: [f32; 2], zoom: f32) {
    match actor {
        Actor::Sprite {
            offset, size, scale, ..
        } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            for s in size.iter_mut() {
                if let actors::SizeSpec::Px(v) = s {
                    *v *= zoom;
                }
            }
            scale[0] *= zoom;
            scale[1] *= zoom;
        }
        Actor::Text {
            offset,
            scale,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            clip,
            ..
        } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            scale[0] *= zoom;
            scale[1] *= zoom;

            if let Some(r) = clip.as_mut() {
                r[0] = scale_about(r[0], pivot[0], zoom);
                r[1] = scale_about(r[1], pivot[1], zoom);
                r[2] *= zoom;
                r[3] *= zoom;
            }

            if !*max_w_pre_zoom
                && let Some(w) = max_width {
                    *max_width = Some(*w * zoom);
                }
            if !*max_h_pre_zoom
                && let Some(h) = max_height {
                    *max_height = Some(*h * zoom);
                }
        }
        Actor::Frame {
            offset,
            size,
            children,
            ..
        } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            for s in size.iter_mut() {
                if let actors::SizeSpec::Px(v) = s {
                    *v *= zoom;
                }
            }
            for child in children {
                apply_zoom_to_actor(child, pivot, zoom);
            }
        }
        Actor::Shadow { len, child, .. } => {
            len[0] *= zoom;
            len[1] *= zoom;
            apply_zoom_to_actor(child, pivot, zoom);
        }
    }
}

fn apply_offset_to_actor(actor: &mut Actor, dx: f32, dy: f32) {
    match actor {
        Actor::Sprite { offset, .. } => {
            offset[0] += dx;
            offset[1] += dy;
        }
        Actor::Text { offset, clip, .. } => {
            offset[0] += dx;
            offset[1] += dy;
            if let Some(r) = clip.as_mut() {
                r[0] += dx;
                r[1] += dy;
            }
        }
        // Frame children are already in the frame's coordinate space; shifting the
        // frame moves the whole subtree in compose.
        Actor::Frame { offset, .. } => {
            offset[0] += dx;
            offset[1] += dy;
        }
        Actor::Shadow { child, .. } => apply_offset_to_actor(child, dx, dy),
    }
}

fn apply_clip_rect_to_actor(actor: &mut Actor, rect: [f32; 4]) {
    match actor {
        Actor::Text { clip, .. } => *clip = Some(rect),
        Actor::Frame { children, .. } => {
            for child in children {
                apply_clip_rect_to_actor(child, rect);
            }
        }
        Actor::Shadow { child, .. } => apply_clip_rect_to_actor(child, rect),
        Actor::Sprite { .. } => {}
    }
}

#[inline(always)]
fn box_inner_alpha() -> f32 {
    use crate::ui::{anim, runtime};
    static STEPS: std::sync::OnceLock<Vec<anim::Step>> = std::sync::OnceLock::new();

    let steps = STEPS.get_or_init(|| {
        vec![
            anim::sleep(FRAME_IN_CROP_DUR),
            anim::linear(OVERLAY_IN_DUR).x(1.0).build(),
        ]
    });

    let mut init = anim::TweenState::default();
    init.x = 0.0;
    let sid = runtime::site_id(file!(), line!(), column!(), 0x53454C50524F4649u64); // "SELPROFI"
    runtime::materialize(sid, init, steps).x.clamp(0.0, 1.0)
}

fn push_join_prompt(
    out: &mut Vec<Actor>,
    cx: f32,
    cy: f32,
    frame_h: f32,
    border_rgba: [f32; 4],
    inner_alpha: f32,
    time: f32,
    text: &str,
) {
    // ITGmania diffuse_shift: period=1, color1=white, color2=gray.
    // f = sin((t + 0.25) * 2π) / 2 + 0.5
    let t = time.rem_euclid(1.0);
    let f = ((t + 0.25) * std::f32::consts::PI * 2.0).sin().mul_add(0.5, 0.5);
    let shade = 0.5f32.mul_add(f, 0.5);

    out.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(FRAME_W_JOIN + FRAME_BORDER, frame_h + FRAME_BORDER):
        diffuse(border_rgba[0], border_rgba[1], border_rgba[2], border_rgba[3]):
        cropbottom(1.0):
        ease(FRAME_IN_CROP_DUR, 0.0): cropbottom(0.0):
        z(100)
    ));
    out.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(FRAME_W_JOIN, frame_h):
        diffuse(0.0, 0.0, 0.0, 1.0):
        cropbottom(1.0):
        ease(FRAME_IN_CROP_DUR, 0.0): cropbottom(0.0):
        z(101)
    ));
    out.push(act!(text:
        align(0.5, 0.5):
        xy(cx, cy):
        font("miso"):
        zoomtoheight(18.0):
        maxwidth(FRAME_W_JOIN - 20.0):
        settext(text):
        diffuse(shade, shade, shade, inner_alpha):
        z(103)
    ));
}

#[allow(clippy::too_many_arguments)]
fn push_scroller_frame(
    out: &mut Vec<Actor>,
    choices: &[Choice],
    selected_index: usize,
    preview_noteskin: Option<&Noteskin>,
    preview_time: f32,
    preview_beat: f32,
    frame_cx: f32,
    frame_cy: f32,
    frame_y0: f32,
    frame_h: f32,
    color_index: i32,
    inner_alpha: f32,
    border_rgba: [f32; 4],
    col_overlay: [f32; 4],
) {
    // Simply Love parity:
    // - Frame bg uses PlayerColor(P1) => SL.Colors[ActiveColorIndex]
    // - Top edge is LightenColor(c) (rgb * 1.25), producing a subtle vertical gradient
    // - Scroller highlight + info pane use semi-transparent black overlays (alpha 0.5)
    let col_frame = color::simply_love_rgba(color_index);
    let col_frame_top = color::lighten_rgba(col_frame);

    // Frame border.
    out.push(act!(quad:
        align(0.5, 0.5):
        xy(frame_cx, frame_cy):
        zoomto(FRAME_W_SCROLLER + FRAME_BORDER, frame_h + FRAME_BORDER):
        diffuse(border_rgba[0], border_rgba[1], border_rgba[2], border_rgba[3]):
        cropbottom(1.0):
        ease(FRAME_IN_CROP_DUR, 0.0): cropbottom(0.0):
        z(100)
    ));
    // Base fill.
    out.push(act!(quad:
        align(0.5, 0.5):
        xy(frame_cx, frame_cy):
        zoomto(FRAME_W_SCROLLER, frame_h):
        diffuse(col_frame[0], col_frame[1], col_frame[2], col_frame[3]):
        cropbottom(1.0):
        ease(FRAME_IN_CROP_DUR, 0.0): cropbottom(0.0):
        z(101)
    ));
    // Top-edge lighten gradient (approx for diffusetopedge()).
    out.push(act!(quad:
        align(0.5, 0.5):
        xy(frame_cx, frame_cy):
        zoomto(FRAME_W_SCROLLER, frame_h):
        diffuse(col_frame_top[0], col_frame_top[1], col_frame_top[2], col_frame_top[3]):
        fadebottom(1.0):
        cropbottom(1.0):
        ease(FRAME_IN_CROP_DUR, 0.0): cropbottom(0.0):
        z(101)
    ));

    // Info pane background (semi-transparent black overlay).
    let info_x0 = frame_cx + INFO_X0_OFF;
    let info_text_x = INFO_PAD.mul_add(1.25, info_x0);
    let info_max_w = INFO_PAD.mul_add(-2.5, INFO_W);

    out.push(act!(quad:
        align(0.0, 0.0):
        xy(info_x0, frame_y0):
        zoomto(INFO_W, frame_h):
        diffuse(0.0, 0.0, 0.0, 0.0):
        sleep(OVERLAY_IN_DELAY):
        linear(OVERLAY_IN_DUR): diffusealpha(col_overlay[3]):
        z(102)
    ));

    // Scroller highlight bar.
    let scroller_cx = frame_cx + SCROLLER_CX_OFF;
    out.push(act!(quad:
        align(0.5, 0.5):
        xy(scroller_cx, frame_cy):
        zoomto(SCROLLER_W, ROW_H):
        diffuse(0.0, 0.0, 0.0, 0.0):
        sleep(OVERLAY_IN_DELAY):
        linear(OVERLAY_IN_DUR): diffusealpha(col_overlay[3]):
        z(102)
    ));

    // Scroller rows.
    let scroller_clip = [
        SCROLLER_W.mul_add(-0.5, scroller_cx),
        frame_y0,
        SCROLLER_W,
        frame_h,
    ];
    let rows_half = ROWS_VISIBLE / 2;
    for d in -rows_half..=rows_half {
        let idx_i = selected_index as i32 + d;
        if idx_i < 0 || idx_i >= choices.len() as i32 {
            continue;
        }
        let choice = &choices[idx_i as usize];
        let y = (d as f32).mul_add(ROW_H, frame_cy);

        let mut row = act!(text:
            align(0.5, 0.5):
            xy(scroller_cx, y):
            font("miso"):
            maxwidth(SCROLLER_TEXT_PAD_X.mul_add(-2.0, SCROLLER_W)):
            zoom(1.0):
            settext(choice.display_name.clone()):
            diffuse(1.0, 1.0, 1.0, inner_alpha):
            shadowlength(0.5):
            z(103):
            horizalign(center)
        );
        apply_clip_rect_to_actor(&mut row, scroller_clip);
        out.push(row);
    }

    let selected = choices.get(selected_index);
    let selected_is_local = selected.is_some_and(|c| matches!(&c.kind, ActiveProfile::Local { .. }));

    // Avatar slot (SL-style): show profile.png if present, else heart + text.
    let avatar_dim = INFO_PAD.mul_add(-2.25, INFO_W);
    let avatar_x = info_x0 + AVATAR_X_OFF;
    let avatar_y = frame_cy + AVATAR_Y_OFF;

    if let Some(choice) = selected {
        let is_guest = matches!(&choice.kind, ActiveProfile::Guest);
        let show_fallback = is_guest || choice.avatar_key.is_none();
        if show_fallback {
            let bg = color::rgba_hex(AVATAR_BG_HEX);
            out.push(act!(quad:
                align(0.0, 0.0):
                xy(avatar_x, avatar_y):
                zoomto(avatar_dim, avatar_dim):
                diffuse(bg[0], bg[1], bg[2], bg[3] * inner_alpha):
                z(103)
            ));
            out.push(act!(sprite("heart.png"):
                align(0.0, 0.0):
                xy(avatar_x + AVATAR_HEART_X, avatar_y + AVATAR_HEART_Y):
                zoom(AVATAR_HEART_ZOOM):
                diffuse(1.0, 1.0, 1.0, 0.9 * inner_alpha):
                z(104)
            ));

            let label = if is_guest { "[ GUEST ]" } else { "No Avatar" };
            out.push(act!(text:
                align(0.5, 0.0):
                xy(avatar_x + avatar_dim * 0.5, avatar_y + AVATAR_TEXT_Y):
                font("miso"):
                maxwidth(avatar_dim - 8.0):
                zoom(AVATAR_LABEL_ZOOM):
                settext(label):
                diffuse(1.0, 1.0, 1.0, 0.9 * inner_alpha):
                z(105):
                horizalign(center)
            ));
        } else if let Some(key) = &choice.avatar_key {
            out.push(act!(sprite(key.clone()):
                align(0.0, 0.0):
                xy(avatar_x, avatar_y):
                zoomto(avatar_dim, avatar_dim):
                diffusealpha(inner_alpha):
                z(104)
            ));
        }
    }

    if selected_is_local {
        out.push(act!(text:
            align(0.0, 0.0):
            xy(info_text_x, frame_cy):
            font("miso"):
            zoom(TOTAL_SONGS_ZOOM):
            maxwidth(info_max_w):
            settext(TOTAL_SONGS_STATIC):
            diffuse(1.0, 1.0, 1.0, inner_alpha):
            z(103)
        ));
    }

    // Thin white line separating stats from mods (SL-style).
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(INFO_PAD.mul_add(1.25, info_x0), frame_cy + INFO_LINE_Y_OFF):
        zoomto(info_max_w, 1.0):
        diffuse(1.0, 1.0, 1.0, 0.5 * inner_alpha):
        z(103)
    ));

    // NoteSkin + JudgmentGraphic previews (SL-style placement).
    if selected_is_local {
        let selected_mods = selected
            .map(|c| format_recent_mods(&c.speed_mod, c.scroll_option))
            .unwrap_or_default();
        let preview_y = frame_cy + PREVIEW_Y_OFF;

        if let Some(ns) = preview_noteskin {
            let note_idx = 2 * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
            if let Some(note_slot) = ns.notes.get(note_idx) {
                let frame = note_slot.frame_index(preview_time, preview_beat);
                let uv = note_slot.uv_for_frame(frame);
                let size = note_slot.size();
                let width = size[0].max(1) as f32;
                let height = size[1].max(1) as f32;

                const TARGET_ARROW_PIXEL_SIZE: f32 = 40.0;
                const PREVIEW_SCALE: f32 = 0.4;
                let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
                let scale = if height > 0.0 {
                    target_height / height
                } else {
                    PREVIEW_SCALE
                };

                let ns_x = INFO_W.mul_add(0.13, info_x0);
                let ns_y = preview_y - 10.0;

                out.push(act!(sprite(note_slot.texture_key().to_string()):
                    align(0.5, 0.5):
                    xy(ns_x, ns_y):
                    zoomto(width * scale, target_height):
                    rotationz(-note_slot.def.rotation_deg as f32):
                    customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                    diffusealpha(inner_alpha):
                    z(104)
                ));
            }
        }

        let judgment_texture = selected
            .and_then(|c| match c.judgment {
                profile::JudgmentGraphic::Love => Some("judgements/Love 2x7 (doubleres).png"),
                profile::JudgmentGraphic::LoveChroma => {
                    Some("judgements/Love Chroma 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Rainbowmatic => {
                    Some("judgements/Rainbowmatic 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::GrooveNights => {
                    Some("judgements/GrooveNights 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Emoticon => {
                    Some("judgements/Emoticon 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Censored => {
                    Some("judgements/Censored 1x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Chromatic => {
                    Some("judgements/Chromatic 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::ITG2 => Some("judgements/ITG2 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Bebas => Some("judgements/Bebas 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Code => Some("judgements/Code 2x7 (doubleres).png"),
                profile::JudgmentGraphic::ComicSans => {
                    Some("judgements/Comic Sans 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Focus => Some("judgements/Focus 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Grammar => {
                    Some("judgements/Grammar 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Miso => Some("judgements/Miso 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Papyrus => {
                    Some("judgements/Papyrus 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Roboto => Some("judgements/Roboto 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Shift => Some("judgements/Shift 2x7 (doubleres).png"),
                profile::JudgmentGraphic::Tactics => {
                    Some("judgements/Tactics 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::Wendy => Some("judgements/Wendy 2x7 (doubleres).png"),
                profile::JudgmentGraphic::WendyChroma => {
                    Some("judgements/Wendy Chroma 2x7 (doubleres).png")
                }
                profile::JudgmentGraphic::None => None,
            });

        if let Some(texture) = judgment_texture {
            let jd_x = INFO_W.mul_add(0.61, info_x0);
            let jd_y = preview_y - 10.0;
            out.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(jd_x, jd_y):
                setstate(0):
                zoom(0.160):
                diffusealpha(inner_alpha):
                z(104)
            ));
        }

        out.push(act!(text:
            align(0.0, 0.0):
            xy(info_text_x, frame_cy + MODS_Y_OFF):
            font("miso"):
            zoom(MODS_ZOOM):
            maxwidth(info_max_w):
            settext(selected_mods):
            diffuse(1.0, 1.0, 1.0, inner_alpha):
            z(103)
        ));
    }
}

pub fn get_actors(state: &State, alpha_multiplier: f32) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(128);

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    let fg = [1.0, 1.0, 1.0, 1.0];

    actors.push(screen_bar::build(ScreenBarParams {
        title: "SELECT PROFILE",
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: fg,
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
    }));

    let (footer_left, footer_right) = match (state.p1_joined, state.p2_joined) {
        (false, false) => (Some("PRESS START"), Some("PRESS START")),
        (true, false) => (None, Some("NOT PRESENT")),
        (false, true) => (Some("NOT PRESENT"), None),
        (true, true) => (None, None),
    };
    actors.push(screen_bar::build(ScreenBarParams {
        title: "EVENT MODE",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: fg,
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar: None,
        right_avatar: None,
    }));

    if alpha_multiplier <= 0.0 {
        return actors;
    }

    let mut ui: Vec<Actor> = Vec::new();
    let inner_alpha = box_inner_alpha();
    let exit_t = exit_anim_t(state.exit_anim);
    let exit_zoom = if state.exit_anim {
        exit_zoom(exit_t)
    } else {
        1.0
    };

    let frame_h = FRAME_H;
    let cx = screen_center_x();
    let cy = screen_center_y();

    let frame_y0 = frame_h.mul_add(-0.5, cy);

    // IMPORTANT: Apply shake as a post-transform, otherwise the changing X affects
    // act! tween site_ids (salt includes init.x) and restarts tweens every frame.
    let p1_cx = cx - FRAME_CX_OFF;
    let p2_cx = cx + FRAME_CX_OFF;
    let p1_shake_dx = shake_x(state.p1_shake_t);
    let p2_shake_dx = shake_x(state.p2_shake_t);

    let col_overlay = [0.0, 0.0, 0.0, 0.5];
    let border_rgba = [1.0, 1.0, 1.0, 1.0];

    // P1: keep both frames alive (visibility via alpha) so tween state doesn't reset.
    {
        let mut p1_ui: Vec<Actor> = Vec::new();

        let show_scroller = state.p1_joined && !state.p1_ready;
        let show_join = !state.p1_joined || state.p1_ready;
        let show_selected_name = state.p1_joined && state.p1_ready;

        let mut scroller_ui: Vec<Actor> = Vec::new();
        push_scroller_frame(
            &mut scroller_ui,
            &state.choices,
            state.p1_selected_index,
            state.p1_preview_noteskin.as_ref(),
            state.preview_time,
            state.preview_beat,
            p1_cx,
            cy,
            frame_y0,
            frame_h,
            state.active_color_index,
            inner_alpha,
            border_rgba,
            col_overlay,
        );
        for a in &mut scroller_ui {
            apply_alpha_to_actor(a, if show_scroller { 1.0 } else { 0.0 });
        }
        p1_ui.extend(scroller_ui);

        let mut join_ui: Vec<Actor> = Vec::new();
        push_join_prompt(
            &mut join_ui,
            p1_cx,
            cy,
            frame_h,
            border_rgba,
            inner_alpha,
            state.preview_time,
            if state.p1_ready { WAITING_TEXT } else { JOIN_TEXT },
        );
        for a in &mut join_ui {
            apply_alpha_to_actor(a, if show_join { 1.0 } else { 0.0 });
        }
        p1_ui.extend(join_ui);

        if show_selected_name {
            let name = state
                .choices
                .get(state.p1_selected_index).map_or_else(|| "[ GUEST ]".to_string(), |c| c.display_name.clone());
            let a = act!(text:
                align(0.5, 0.5):
                xy(p1_cx, cy + SELECTED_NAME_Y_OFF):
                font("miso"):
                zoom(SELECTED_NAME_ZOOM):
                maxwidth(FRAME_W_SCROLLER):
                settext(name):
                diffuse(1.0, 1.0, 1.0, inner_alpha):
                shadowlength(0.5):
                z(106):
                horizalign(center)
            );
            p1_ui.push(a);
        }

        let zoom = exit_zoom * join_pulse_zoom(state.p1_join_pulse_t);
        if (zoom - 1.0).abs() > f32::EPSILON {
            for a in &mut p1_ui {
                apply_zoom_to_actor(a, [p1_cx, cy], zoom);
            }
        }
        if p1_shake_dx != 0.0 {
            for a in &mut p1_ui {
                apply_offset_to_actor(a, p1_shake_dx, 0.0);
            }
        }
        ui.extend(p1_ui);
    }

    // P2
    {
        let mut p2_ui: Vec<Actor> = Vec::new();

        let show_scroller = state.p2_joined && !state.p2_ready;
        let show_join = !state.p2_joined || state.p2_ready;
        let show_selected_name = state.p2_joined && state.p2_ready;

        let mut scroller_ui: Vec<Actor> = Vec::new();
        push_scroller_frame(
            &mut scroller_ui,
            &state.choices,
            state.p2_selected_index,
            state.p2_preview_noteskin.as_ref(),
            state.preview_time,
            state.preview_beat,
            p2_cx,
            cy,
            frame_y0,
            frame_h,
            state.active_color_index - 2,
            inner_alpha,
            border_rgba,
            col_overlay,
        );
        for a in &mut scroller_ui {
            apply_alpha_to_actor(a, if show_scroller { 1.0 } else { 0.0 });
        }
        p2_ui.extend(scroller_ui);

        let mut join_ui: Vec<Actor> = Vec::new();
        push_join_prompt(
            &mut join_ui,
            p2_cx,
            cy,
            frame_h,
            border_rgba,
            inner_alpha,
            state.preview_time,
            if state.p2_ready { WAITING_TEXT } else { JOIN_TEXT },
        );
        for a in &mut join_ui {
            apply_alpha_to_actor(a, if show_join { 1.0 } else { 0.0 });
        }
        p2_ui.extend(join_ui);

        if show_selected_name {
            let name = state
                .choices
                .get(state.p2_selected_index).map_or_else(|| "[ GUEST ]".to_string(), |c| c.display_name.clone());
            let a = act!(text:
                align(0.5, 0.5):
                xy(p2_cx, cy + SELECTED_NAME_Y_OFF):
                font("miso"):
                zoom(SELECTED_NAME_ZOOM):
                maxwidth(FRAME_W_SCROLLER):
                settext(name):
                diffuse(1.0, 1.0, 1.0, inner_alpha):
                shadowlength(0.5):
                z(106):
                horizalign(center)
            );
            p2_ui.push(a);
        }

        let zoom = exit_zoom * join_pulse_zoom(state.p2_join_pulse_t);
        if (zoom - 1.0).abs() > f32::EPSILON {
            for a in &mut p2_ui {
                apply_zoom_to_actor(a, [p2_cx, cy], zoom);
            }
        }
        if p2_shake_dx != 0.0 {
            for a in &mut p2_ui {
                apply_offset_to_actor(a, p2_shake_dx, 0.0);
            }
        }
        ui.extend(p2_ui);
    }

    for mut a in ui {
        apply_alpha_to_actor(&mut a, alpha_multiplier);
        actors.push(a);
    }

    actors
}
