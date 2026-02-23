use crate::act;
use crate::assets::AssetManager;
use crate::core::audio;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{screen_center_x, screen_center_y};
use crate::screens::components::screen_bar::{
    AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::{heart_bg, screen_bar};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;
use crate::ui::font;

/* ------------------------------ layout ------------------------------- */
const ROOT_X_OFF: f32 = 90.0;
const ROOT_ZOOM: f32 = 1.25;

// Simply Love: Graphics/ScreenSelectPlayMode Icon.lua
const CHOICE_ZOOM_FOCUSED: f32 = 0.75;
const CHOICE_ZOOM_UNFOCUSED: f32 = 0.3;
const CHOICE_ZOOM_TWEEN_DUR: f32 = 0.1;

const CURSOR_H: f32 = 40.0;
const CURSOR_MIN_W: f32 = 90.0;
const CURSOR_MAX_W: f32 = 170.0;
const CURSOR_TWEEN_DUR: f32 = 0.1;

const EXIT_TOTAL_DUR: f32 = 0.9; // SL: out.lua sleeps 0.9 to allow OffCommands to complete.

const TIME_PER_ARROW: f32 = 0.2;
const ARROW_H: f32 = 20.0;
const ARROW_PAD_Y: f32 = 5.0;
const LOOP_RESET_Y: f32 = 24.0 * ARROW_H;
const ARROW_SPRITE_SZ: f32 = 150.0;
const ARROW_SPRITE_ZOOM: f32 = 0.18;

const CHOICES: [&str; 2] = ["Regular", "Marathon"];
const REGULAR_DESC: &str = "Choose your songs with a\nshort break between each.\n\nThese are dance games\nas we know and love them!";
const MARATHON_DESC: &str = "Play a predetermined course\nof songs without any\nbreak between.\n\nMany courses have scripted\nmodifiers!";

const PATTERN: [&str; 24] = [
    "left", "down", "left", "right", "down", "up", "left", "right", "left", "down", "up", "right",
    "left", "right", "down", "up", "down", "right", "left", "right", "up", "down", "up", "right",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Regular,
    Marathon,
}

#[inline(always)]
const fn choice_from_index(idx: usize) -> Choice {
    match idx {
        0 => Choice::Regular,
        _ => Choice::Marathon,
    }
}

#[inline(always)]
const fn choice_desc(choice: Choice) -> &'static str {
    match choice {
        Choice::Regular => REGULAR_DESC,
        Choice::Marathon => MARATHON_DESC,
    }
}

#[inline(always)]
const fn choice_cursor_label_width(choice: Choice) -> f32 {
    // Approximation of SM's `choice_actor:GetWidth()`, clamped after dividing by 1.4.
    match choice {
        Choice::Regular => 140.0,
        Choice::Marathon => 168.0,
    }
}

#[inline(always)]
const fn choice_play_mode(choice: Choice) -> crate::game::profile::PlayMode {
    match choice {
        Choice::Regular => crate::game::profile::PlayMode::Regular,
        Choice::Marathon => crate::game::profile::PlayMode::Marathon,
    }
}

pub struct State {
    pub active_color_index: i32,
    pub selected_index: usize,
    cursor_y: f32,
    choice_zooms: [f32; CHOICES.len()],
    demo_time: f32,
    exit_requested: bool,
    exit_target: Option<Screen>,
    bg: heart_bg::State,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        selected_index: 0,
        cursor_y: -60.0,
        choice_zooms: [CHOICE_ZOOM_UNFOCUSED; CHOICES.len()],
        demo_time: 0.0,
        exit_requested: false,
        exit_target: None,
        bg: heart_bg::State::new(),
    }
}

pub fn on_enter(state: &mut State) {
    state.selected_index = match crate::game::profile::get_session_play_mode() {
        crate::game::profile::PlayMode::Regular => 0,
        crate::game::profile::PlayMode::Marathon => 1,
    };
    // Match SL behavior where switching mode requeues FirstLoopRegular/Marathon.
    state.demo_time = 0.0;
    state.cursor_y = -60.0 + CURSOR_H * (state.selected_index as f32);
    for (i, z) in state.choice_zooms.iter_mut().enumerate() {
        *z = if i == state.selected_index {
            CHOICE_ZOOM_FOCUSED
        } else {
            CHOICE_ZOOM_UNFOCUSED
        };
    }
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    // Simply Love handles transitions via per-actor OffCommands and a sleep in out.lua.
    (vec![], 0.0)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    // Simply Love handles transitions via per-actor OffCommands and a sleep in out.lua.
    (vec![], 0.0)
}

#[inline(always)]
fn exit_anim_t(exiting: bool) -> f32 {
    if !exiting {
        return 0.0;
    }
    use crate::ui::{anim, runtime};
    static STEPS: std::sync::OnceLock<Vec<anim::Step>> = std::sync::OnceLock::new();
    let steps = STEPS.get_or_init(|| vec![anim::linear(EXIT_TOTAL_DUR).x(EXIT_TOTAL_DUR).build()]);

    let mut init = anim::TweenState::default();
    init.x = 0.0;
    let sid = runtime::site_id(file!(), line!(), column!(), 0x53504D4F44455849u64); // "SPMODEXI"
    runtime::materialize(sid, init, steps).x.max(0.0)
}

#[inline(always)]
fn fade_after(exit_t: f32, delay: f32, dur: f32) -> f32 {
    if exit_t <= delay {
        1.0
    } else if exit_t >= delay + dur {
        0.0
    } else {
        1.0 - (exit_t - delay) / dur
    }
}

#[inline(always)]
fn cropleft_after(exit_t: f32, delay: f32, dur: f32) -> f32 {
    if exit_t <= delay {
        0.0
    } else if exit_t >= delay + dur {
        1.0
    } else {
        (exit_t - delay) / dur
    }
}

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    let dt = dt.max(0.0);
    state.demo_time = (state.demo_time + dt).rem_euclid(60.0);

    let target_y = -60.0 + CURSOR_H * (state.selected_index as f32);
    let speed = CURSOR_H / CURSOR_TWEEN_DUR;
    let max_step = speed * dt;
    let dy = target_y - state.cursor_y;
    if dy.abs() <= max_step {
        state.cursor_y = target_y;
    } else {
        state.cursor_y += dy.signum() * max_step;
    }

    // Choice zoom (GainFocus/LoseFocus linear(0.1) in SL).
    let zoom_speed = (CHOICE_ZOOM_FOCUSED - CHOICE_ZOOM_UNFOCUSED) / CHOICE_ZOOM_TWEEN_DUR;
    let zoom_max_step = zoom_speed * dt;
    for (i, z) in state.choice_zooms.iter_mut().enumerate() {
        let target = if i == state.selected_index {
            CHOICE_ZOOM_FOCUSED
        } else {
            CHOICE_ZOOM_UNFOCUSED
        };
        let dz = target - *z;
        if dz.abs() <= zoom_max_step {
            *z = target;
        } else {
            *z += dz.signum() * zoom_max_step;
        }
    }

    if state.exit_requested {
        if let Some(target) = state.exit_target
            && exit_anim_t(true) >= EXIT_TOTAL_DUR
        {
            state.exit_target = None;
            return Some(ScreenAction::Navigate(target));
        }
    }
    None
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }
    if state.exit_requested {
        return ScreenAction::None;
    }

    let nav = match crate::game::profile::get_session_player_side() {
        crate::game::profile::PlayerSide::P2 => match ev.action {
            VirtualAction::p2_left
            | VirtualAction::p2_menu_left
            | VirtualAction::p2_up
            | VirtualAction::p2_menu_up => Some(-1),
            VirtualAction::p2_right
            | VirtualAction::p2_menu_right
            | VirtualAction::p2_down
            | VirtualAction::p2_menu_down => Some(1),
            VirtualAction::p2_start => Some(0),
            VirtualAction::p2_back => Some(9),
            _ => None,
        },
        crate::game::profile::PlayerSide::P1 => match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p1_up
            | VirtualAction::p1_menu_up => Some(-1),
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p1_down
            | VirtualAction::p1_menu_down => Some(1),
            VirtualAction::p1_start => Some(0),
            VirtualAction::p1_back => Some(9),
            _ => None,
        },
    };

    match nav {
        Some(-1) => {
            state.selected_index = (state.selected_index + CHOICES.len() - 1) % CHOICES.len();
            state.demo_time = 0.0;
            audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }
        Some(1) => {
            state.selected_index = (state.selected_index + 1) % CHOICES.len();
            state.demo_time = 0.0;
            audio::play_sfx("assets/sounds/change.ogg");
            ScreenAction::None
        }
        Some(0) => {
            state.exit_requested = true;
            state.exit_target = Some(Screen::ProfileLoad);
            let _ = exit_anim_t(true);
            crate::game::profile::set_session_play_mode(choice_play_mode(choice_from_index(
                state.selected_index,
            )));
            audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::None
        }
        Some(9) => {
            state.exit_requested = true;
            state.exit_target = Some(Screen::Menu);
            let _ = exit_anim_t(true);
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}

#[inline(always)]
fn root_pt(x: f32, y: f32) -> (f32, f32) {
    (
        screen_center_x() + ROOT_X_OFF + x * ROOT_ZOOM,
        screen_center_y() + y * ROOT_ZOOM,
    )
}

#[inline(always)]
fn root_sz(w: f32, h: f32) -> (f32, f32) {
    (w * ROOT_ZOOM, h * ROOT_ZOOM)
}

#[inline(always)]
fn arrow_rotation(dir: &str) -> f32 {
    match dir {
        "center" => 0.0,
        // StepMania's positive Z rotation is opposite of our renderer.
        // Use mirrored angles so SL's direction mapping stays 1:1.
        "up" => 315.0,
        "upright" => 270.0,
        "right" => 225.0,
        "downright" => 180.0,
        "down" => 135.0,
        "downleft" => 90.0,
        "left" => 45.0,
        "upleft" => 0.0,
        _ => 315.0,
    }
}

#[inline(always)]
fn ease01(x: f32, f_ease: f32) -> f32 {
    use crate::ui::anim;
    let x = x.clamp(0.0, 1.0);
    // Use the same curve implementation as tween segments.
    anim::eval_ease_p_for_f_ease(x, f_ease)
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(256);
    let exit_t = exit_anim_t(state.exit_requested);

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    actors.push(screen_bar::build(ScreenBarParams {
        title: "SELECT MODE",
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
    }));

    let p1_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P1);
    let p2_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P2);
    let p1_avatar = p1_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });
    let p2_avatar = p2_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });

    let p1_joined =
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P1);
    let p2_joined =
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P2);
    let p1_guest =
        crate::game::profile::is_session_side_guest(crate::game::profile::PlayerSide::P1);
    let p2_guest =
        crate::game::profile::is_session_side_guest(crate::game::profile::PlayerSide::P2);

    let (footer_left, left_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                "INSERT CARD"
            } else {
                p1_profile.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (Some("PRESS START"), None)
    };
    let (footer_right, right_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                "INSERT CARD"
            } else {
                p2_profile.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (Some("PRESS START"), None)
    };
    actors.push(screen_bar::build(ScreenBarParams {
        title: "EVENT MODE",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
    }));

    // Grey backgrounds (SL: ScreenSelectPlayMode underlay/default.lua).
    let bg1_a = fade_after(exit_t, 0.4, 0.1);
    let bg2_a = fade_after(exit_t, 0.3, 0.1);
    let (gw, gh) = root_sz(90.0, 38.0);
    let (g1x, g1y) = root_pt(-188.0, -60.0);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(g1x, g1y):
        zoomto(gw, gh):
        diffuse(0.2, 0.2, 0.2, bg1_a)
    ));
    let (g2x, g2y) = root_pt(-188.0, -20.0);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(g2x, g2y):
        zoomto(gw, gh):
        diffuse(0.2, 0.2, 0.2, bg2_a)
    ));

    // Border and background of the SelectMode box.
    let border_crop = cropleft_after(exit_t, 0.6, 0.2);
    let (bx, by) = root_pt(0.0, 0.0);
    let (bw, bh) = root_sz(302.0, 162.0);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(bx, by):
        zoomto(bw, bh):
        diffuse(1.0, 1.0, 1.0, 1.0):
        cropleft(border_crop)
    ));
    let (iw, ih) = root_sz(300.0, 160.0);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(bx, by):
        zoomto(iw, ih):
        diffuse(0.0, 0.0, 0.0, 1.0):
        cropleft(border_crop)
    ));

    // Description text.
    let desc_alpha = fade_after(exit_t, 0.4, 0.2);
    let (dx, dy) = root_pt(-130.0, -60.0);
    let choice = choice_from_index(state.selected_index);
    actors.push(act!(text:
        font("miso"):
        settext(choice_desc(choice)):
        align(0.0, 0.0):
        xy(dx, dy):
        zoom(0.825 * ROOT_ZOOM):
        diffuse(1.0, 1.0, 1.0, desc_alpha):
        horizalign(left)
    ));

    // Cursor highlight.
    let cursor_crop = cropleft_after(exit_t, 0.4, 0.2);
    let cursor_alpha = 1.0;
    let label = CHOICES[state.selected_index];
    let measured_w = asset_manager.with_fonts(|all_fonts| {
        asset_manager
            .with_font("wendy", |f| {
                font::measure_line_width_logical(f, label, all_fonts) as f32
            })
            .unwrap_or(0.0)
    });
    let base_w = if measured_w > 0.0 {
        measured_w
    } else {
        choice_cursor_label_width(choice)
    };
    let cursor_w = (base_w / 1.4).clamp(CURSOR_MIN_W, CURSOR_MAX_W);
    let (cw, ch_outer) = root_sz(cursor_w, CURSOR_H + 2.0);
    let (cw2, ch_inner) = root_sz(cursor_w, CURSOR_H);
    let (cx_outer, cy_outer) = root_pt(-151.0, state.cursor_y);
    let (cx_inner, cy_inner) = root_pt(-150.0, state.cursor_y);
    actors.push(act!(quad:
        align(1.0, 0.5): xy(cx_outer, cy_outer):
        zoomto(cw, ch_outer):
        diffuse(1.0, 1.0, 1.0, cursor_alpha):
        cropleft(cursor_crop)
    ));
    actors.push(act!(quad:
        align(1.0, 0.5): xy(cx_inner, cy_inner):
        zoomto(cw2, ch_inner):
        diffuse(0.0, 0.0, 0.0, cursor_alpha):
        cropleft(cursor_crop)
    ));

    // Choice labels (SL: Graphics/ScreenSelectPlayMode Icon.lua).
    let label_alpha = fade_after(exit_t, 0.0, 0.2);
    let label_selected = color::simply_love_rgba(state.active_color_index);
    let label_unselected = color::rgba_hex("#888888");
    let zoom_den = (CHOICE_ZOOM_FOCUSED - CHOICE_ZOOM_UNFOCUSED).max(f32::EPSILON);
    for (i, &label) in CHOICES.iter().enumerate() {
        let (x, y) = root_pt(-160.0, -60.0 + CURSOR_H * (i as f32));
        let zoom = state.choice_zooms[i];
        let t = ((zoom - CHOICE_ZOOM_UNFOCUSED) / zoom_den).clamp(0.0, 1.0);

        let rgb = [
            label_unselected[0] + (label_selected[0] - label_unselected[0]) * t,
            label_unselected[1] + (label_selected[1] - label_unselected[1]) * t,
            label_unselected[2] + (label_selected[2] - label_unselected[2]) * t,
        ];

        actors.push(act!(text:
            font("wendy"):
            settext(label):
            align(1.0, 0.5):
            xy(x, y):
            zoom(zoom):
            diffuse(rgb[0], rgb[1], rgb[2], label_alpha):
            horizalign(right)
        ));
    }

    // Score.
    let score_alpha = fade_after(exit_t, 0.4, 0.2);
    let (sx, sy) = root_pt(124.0, -68.0);
    actors.push(act!(text:
        font("wendy_monospace_numbers"):
        settext("77.41"):
        align(0.5, 0.5):
        xy(sx, sy):
        zoom(0.225 * ROOT_ZOOM):
        diffuse(1.0, 1.0, 1.0, score_alpha)
    ));

    // Life meter (static sample).
    let life_alpha = fade_after(exit_t, 0.4, 0.2);
    let (lmw1, lmh1) = root_sz(60.0, 16.0);
    let (lmw2, lmh2) = root_sz(58.0, 14.0);
    let (lmw3, lmh3) = root_sz(40.0, 14.0);
    let (lbx, lby) = root_pt(68.0, -64.0);
    let (bbx, bby) = root_pt(68.0, -64.0);
    let (cbx, cby) = root_pt(59.0, -64.0);
    let life_color = color::simply_love_rgba(state.active_color_index);

    actors.push(act!(quad:
        align(0.5, 0.5): xy(lbx, lby):
        zoomto(lmw1, lmh1):
        diffuse(1.0, 1.0, 1.0, life_alpha)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(bbx, bby):
        zoomto(lmw2, lmh2):
        diffuse(0.0, 0.0, 0.0, life_alpha)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(cbx, cby):
        zoomto(lmw3, lmh3):
        diffuse(life_color[0], life_color[1], life_color[2], life_alpha)
    ));
    actors.push(act!(sprite("swoosh.png"):
        align(0.5, 0.5): xy(cbx, cby):
        zoomto(lmw3, lmh3):
        diffuse(1.0, 1.0, 1.0, 0.45 * life_alpha):
        texcoordvelocity(-2.0, 0.0)
    ));

    // Gameplay demo: faux playfield (dance).
    let field_alpha = fade_after(exit_t, 0.4, 0.2);
    let marathon = state.selected_index == 1;
    let f_ease = if marathon { 75.0 } else { 0.0 };
    let cycle_dur = TIME_PER_ARROW * (PATTERN.len() as f32);
    let base_t = state.demo_time;

    let (nfx, nfy) = root_pt(90.0, 15.0);
    // Use a mask source quad for SM-style MaskSource/MaskDest clipping.
    let (mx, my) = root_pt(0.0, 0.0);
    let (mw, mh) = root_sz(300.0, 160.0);
    actors.push(act!(quad:
        align(0.5, 0.5): xy(mx, my):
        zoomto(mw, mh):
        diffuse(1.0, 1.0, 1.0, 1.0):
        MaskSource()
    ));

    let columns = [
        ("left", -36.0),
        ("down", -12.0),
        ("up", 12.0),
        ("right", 36.0),
    ];

    for (dir, x_off) in columns {
        let (x, y) = (nfx + x_off * ROOT_ZOOM, nfy + (-55.0) * ROOT_ZOOM);
        let (aw, ah) = root_sz(ARROW_SPRITE_SZ, ARROW_SPRITE_SZ);
        actors.push(act!(sprite("select_mode/arrow-body.png"):
            align(0.5, 0.5): xy(x, y):
            setsize(aw, ah):
            zoom(ARROW_SPRITE_ZOOM):
            rotationz(arrow_rotation(dir)):
            diffuse(1.0, 1.0, 1.0, field_alpha):
            MaskDest():
        ));
    }

    for (i, dir) in PATTERN.iter().enumerate() {
        let (_, col_x) = columns
            .iter()
            .find(|(d, _)| d == dir)
            .copied()
            .unwrap_or(("up", 12.0));

        let i1 = (i as f32) + 1.0;
        let first_dur = TIME_PER_ARROW * i1;
        let (y0, t_local, spin_base) = if base_t < first_dur {
            (
                -55.0 + i1 * (ARROW_H + ARROW_PAD_Y),
                (base_t / first_dur).clamp(0.0, 1.0),
                0.0,
            )
        } else {
            let loop_t = (base_t - first_dur).rem_euclid(cycle_dur);
            let loops = ((base_t - first_dur) / cycle_dur).floor().max(0.0);
            (
                LOOP_RESET_Y,
                (loop_t / cycle_dur).clamp(0.0, 1.0),
                720.0 * (1.0 + loops),
            )
        };

        let p = if marathon {
            ease01(t_local, f_ease)
        } else {
            t_local
        };
        let y = y0 + (-55.0 - y0) * p;
        let rot = if marathon {
            // Match SL's clockwise spin in our opposite-sign rotation space.
            arrow_rotation(dir) - spin_base - 720.0 * p
        } else {
            arrow_rotation(dir)
        };

        let tint = color::decorative_rgba(state.active_color_index + i as i32);
        let (x, y) = (nfx + col_x * ROOT_ZOOM, nfy + y * ROOT_ZOOM);
        let (aw, ah) = root_sz(ARROW_SPRITE_SZ, ARROW_SPRITE_SZ);
        actors.push(act!(sprite("select_mode/arrow-border.png"):
            align(0.5, 0.5): xy(x, y):
            setsize(aw, ah):
            zoom(ARROW_SPRITE_ZOOM):
            diffuse(1.0, 1.0, 1.0, field_alpha):
            MaskDest():
            rotationz(rot):
        ));
        actors.push(act!(sprite("select_mode/arrow-body.png"):
            align(0.5, 0.5): xy(x, y):
            setsize(aw, ah):
            zoom(ARROW_SPRITE_ZOOM):
            diffuse(tint[0], tint[1], tint[2], tint[3] * field_alpha):
            MaskDest():
            rotationz(rot):
        ));
        actors.push(act!(sprite("select_mode/arrow-stripes.png"):
            align(0.5, 0.5): xy(x, y):
            setsize(aw, ah):
            zoom(ARROW_SPRITE_ZOOM):
            diffuse(1.0, 1.0, 1.0, field_alpha):
            blend(multiply):
            MaskDest():
            rotationz(rot):
        ));
    }

    actors
}
