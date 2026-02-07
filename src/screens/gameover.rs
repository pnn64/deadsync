use crate::act;
use crate::assets::AssetManager;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::profile;
use crate::game::scores;
use crate::game::stage_stats;
use crate::screens::components::heart_bg;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

// Simply Love: ScreenGameOver TimerSeconds = 23 (non-SRPG9)
const GAMEOVER_SECONDS: f32 = 23.0;

// Layout (Simply Love)
const SIDE_BG_W: f32 = 160.0;
const SIDE_BG_X_PAD: f32 = 80.0;
const SIDE_LINE_W: f32 = 120.0;
const SIDE_LINE_Y: f32 = 288.0;
const SIDE_LINE_H: f32 = 1.0;
const LINE_HEIGHT: f32 = 58.0;
const PROFILE_STATS_Y: f32 = 138.0;
const NORMAL_STATS_Y: f32 = 268.0;
const STATS_TEXT_ZOOM: f32 = 0.95;

const AVATAR_DIM: f32 = 110.0;
const AVATAR_Y: f32 = 12.0;

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

#[derive(Clone, Copy, Debug, Default)]
struct SessionStats {
    songs_played: u32,
    notes_hit: u32,
    duration_seconds: f32,
}

fn session_stats_for_side(
    side: profile::PlayerSide,
    stages: &[stage_stats::StageSummary],
) -> SessionStats {
    let mut out = SessionStats::default();
    for s in stages {
        let Some(p) = s.players.get(side_ix(side)).and_then(|p| p.as_ref()) else {
            continue;
        };
        out.songs_played = out.songs_played.saturating_add(1);
        out.notes_hit = out.notes_hit.saturating_add(p.notes_hit);
        out.duration_seconds += s.duration_seconds.max(0.0);
    }
    out
}

fn format_time_spent(seconds_total: f32) -> String {
    let total = seconds_total.max(0.0).round() as u32;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;

    if hours > 0 {
        format!("{hours}hr. {minutes}min. {seconds}sec.")
    } else {
        format!("{minutes}min. {seconds}sec.")
    }
}

fn build_player_lines(
    side: profile::PlayerSide,
    stages: &[stage_stats::StageSummary],
    total_songs_played: u32,
) -> (Vec<String>, Vec<String>) {
    // Profile stats (only for persistent profiles)
    let mut profile_lines: Vec<String> = Vec::with_capacity(3);
    if profile::is_session_side_joined(side) && !profile::is_session_side_guest(side) {
        let p = profile::get_for_side(side);
        profile_lines.push(p.display_name);

        if p.ignore_step_count_calories {
            profile_lines.push(String::new());
        } else {
            let cals = if p.calories_burned_today.is_finite() && p.calories_burned_today >= 0.0 {
                p.calories_burned_today.round() as u32
            } else {
                0
            };
            profile_lines.push(format!("Calories Burned Today\n{cals}"));
        }

        profile_lines.push(format!("Total Songs Played\n{total_songs_played}"));
    }

    // General stats (no profile required)
    let stats = session_stats_for_side(side, stages);
    let general_lines: Vec<String> = vec![
        format!("Songs Played This Game\n{}", stats.songs_played),
        format!("Notes Hit This Game\n{}", stats.notes_hit),
        format!(
            "Time Spent This Game\n{}",
            format_time_spent(stats.duration_seconds)
        ),
    ];

    (profile_lines, general_lines)
}

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    elapsed: f32,
    total_songs_played: [u32; 2],
}

fn init_inner(scan_totals: bool) -> State {
    let total_songs_played = if scan_totals {
        [
            scores::total_songs_played_for_side(profile::PlayerSide::P1),
            scores::total_songs_played_for_side(profile::PlayerSide::P2),
        ]
    } else {
        [0, 0]
    };

    State {
        active_color_index: color::DEFAULT_COLOR_INDEX, // overwritten by app.rs
        bg: heart_bg::State::new(),
        elapsed: 0.0,
        total_songs_played,
    }
}

pub fn init() -> State {
    init_inner(true)
}

pub fn init_blank() -> State {
    init_inner(false)
}

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    state.elapsed = (state.elapsed + dt).max(0.0);
    if state.elapsed >= GAMEOVER_SECONDS {
        return Some(ScreenAction::Navigate(Screen::Menu));
    }
    None
}

pub fn handle_input(_state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_start
        | VirtualAction::p1_back
        | VirtualAction::p2_start
        | VirtualAction::p2_back => ScreenAction::Navigate(Screen::Menu),
        _ => ScreenAction::None,
    }
}

pub fn get_actors(
    state: &State,
    stages: &[stage_stats::StageSummary],
    _asset_manager: &AssetManager,
) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(64);

    // Background (Simply Love: ScreenWithMenuElements background)
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    // Side stat backdrops (Simply Love: two quads at x=80 and x=w-80)
    {
        let sh = screen_height();
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(SIDE_BG_X_PAD, sh * 0.5):
            zoomto(SIDE_BG_W, sh):
            diffuse(0.0, 0.0, 0.0, 0.6):
            z(10)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(screen_width() - SIDE_BG_X_PAD, sh * 0.5):
            zoomto(SIDE_BG_W, sh):
            diffuse(0.0, 0.0, 0.0, 0.6):
            z(10)
        ));
    }

    // GAME OVER text (Simply Love: Wendy/_wendy white, crop reveal)
    {
        let cx = screen_center_x();
        let cy = screen_center_y();

        actors.push(act!(text:
            font("wendy_white"):
            settext("GAME"):
            align(0.5, 0.5):
            xy(cx, cy - 40.0):
            croptop(1.0): fadetop(1.0):
            zoom(1.2):
            shadowlength(1.0):
            z(20):
            decelerate(0.5): croptop(0.0): fadetop(0.0)
        ));
        actors.push(act!(text:
            font("wendy_white"):
            settext("OVER"):
            align(0.5, 0.5):
            xy(cx, cy + 40.0):
            croptop(1.0): fadetop(1.0):
            zoom(1.2):
            shadowlength(1.0):
            z(20):
            decelerate(0.5): croptop(0.0): fadetop(0.0)
        ));
    }

    for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
        if !profile::is_session_side_joined(side) {
            continue;
        }

        let pc = player_color_rgba(side, state.active_color_index);
        let x_pos = match side {
            profile::PlayerSide::P1 => SIDE_BG_X_PAD,
            profile::PlayerSide::P2 => screen_width() - SIDE_BG_X_PAD,
        };

        // Avatar (persistent profiles only)
        if !profile::is_session_side_guest(side) {
            let p = profile::get_for_side(side);
            if let Some(key) = p.avatar_texture_key {
                actors.push(act!(sprite(key):
                    align(0.0, 0.0):
                    xy(x_pos - AVATAR_DIM * 0.5, AVATAR_Y):
                    zoomto(AVATAR_DIM, AVATAR_DIM):
                    z(12)
                ));
            } else {
                actors.push(act!(quad:
                    align(0.0, 0.0):
                    xy(x_pos - AVATAR_DIM * 0.5, AVATAR_Y):
                    zoomto(AVATAR_DIM, AVATAR_DIM):
                    diffuse(0.157, 0.196, 0.224, 0.667):
                    z(12)
                ));
                actors.push(act!(text:
                    font("miso"):
                    settext("No Avatar"):
                    align(0.5, 0.5):
                    xy(x_pos, AVATAR_Y + AVATAR_DIM - 18.0):
                    zoom(0.9):
                    diffuse(1.0, 1.0, 1.0, 0.9):
                    z(13)
                ));
            }
        }

        // Horizontal divider line
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x_pos, SIDE_LINE_Y):
            zoomto(SIDE_LINE_W, SIDE_LINE_H):
            diffuse(pc[0], pc[1], pc[2], 1.0):
            z(12)
        ));

        let (profile_lines, general_lines) =
            build_player_lines(side, stages, state.total_songs_played[side_ix(side)]);

        for (i, line) in profile_lines.iter().enumerate() {
            let y = (LINE_HEIGHT * (i as f32)) + PROFILE_STATS_Y;
            actors.push(act!(text:
                font("miso"):
                settext(line.clone()):
                align(0.5, 0.5):
                xy(x_pos, y):
                zoom(STATS_TEXT_ZOOM):
                maxwidth(150.0):
                diffuse(pc[0], pc[1], pc[2], 1.0):
                z(13):
                horizalign(center)
            ));
        }

        for (i, line) in general_lines.iter().enumerate() {
            let y = (LINE_HEIGHT * ((i + 1) as f32)) + NORMAL_STATS_Y;
            actors.push(act!(text:
                font("miso"):
                settext(line.clone()):
                align(0.5, 0.5):
                xy(x_pos, y):
                zoom(STATS_TEXT_ZOOM):
                maxwidth(150.0):
                diffuse(pc[0], pc[1], pc[2], 1.0):
                z(13):
                horizalign(center)
            ));
        }
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
