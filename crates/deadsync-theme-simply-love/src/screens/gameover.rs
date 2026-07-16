use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::{FontRole, current_machine_font_key, visual_styles};
use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::{Screen, ThemeEffect};
use crate::views::{PostSongPlayerView, PostSongRuntimeView};
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile as profile_data;
use deadsync_score::stage_stats;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;
const GAMEOVER_SECONDS: f32 = 23.0;
const SRPG10_GAMEOVER_SECONDS: f32 = 135.0;

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

fn player_color_rgba(side: profile_data::PlayerSide, active_color_index: i32) -> [f32; 4] {
    match side {
        profile_data::PlayerSide::P1 => color::simply_love_rgba(active_color_index),
        profile_data::PlayerSide::P2 => color::simply_love_rgba(active_color_index - 2),
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SessionStats {
    songs_played: u32,
    notes_hit: u32,
    duration_seconds: f32,
}

#[inline(always)]
fn is_course_summary_stage(stage: &stage_stats::StageSummary) -> bool {
    stage
        .players
        .iter()
        .flatten()
        .any(|player| player.chart.short_hash.starts_with("course-"))
}

fn session_stats_for_side(
    side: profile_data::PlayerSide,
    stages: &[stage_stats::StageSummary],
) -> SessionStats {
    let mut out = SessionStats::default();
    for stage in stages {
        if is_course_summary_stage(stage) {
            continue;
        }
        let Some(player) = stage
            .players
            .get(profile_data::player_side_index(side))
            .and_then(Option::as_ref)
        else {
            continue;
        };
        out.songs_played = out.songs_played.saturating_add(1);
        out.notes_hit = out.notes_hit.saturating_add(player.notes_hit);
        out.duration_seconds += stage.duration_seconds.max(0.0);
    }
    out
}

fn format_time_spent(seconds_total: f32) -> String {
    let total = seconds_total.max(0.0).round() as u32;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;

    if hours > 0 {
        tr_fmt(
            "GameOver",
            "TimeFormatHMS",
            &[
                ("hours", &hours.to_string()),
                ("minutes", &minutes.to_string()),
                ("seconds", &seconds.to_string()),
            ],
        )
        .to_string()
    } else {
        tr_fmt(
            "GameOver",
            "TimeFormatMS",
            &[
                ("minutes", &minutes.to_string()),
                ("seconds", &seconds.to_string()),
            ],
        )
        .to_string()
    }
}

fn build_player_lines(
    player: &PostSongPlayerView,
    side: profile_data::PlayerSide,
    stages: &[stage_stats::StageSummary],
) -> (Vec<String>, Vec<String>) {
    // Profile stats (only for persistent profiles)
    let mut profile_lines: Vec<String> = Vec::with_capacity(3);
    if player.joined && !player.guest {
        profile_lines.push(player.display_name.clone());

        if player.ignore_step_count_calories {
            profile_lines.push(String::new());
        } else {
            let cals = if player.calories_burned_today.is_finite()
                && player.calories_burned_today >= 0.0
            {
                player.calories_burned_today.round() as u32
            } else {
                0
            };
            profile_lines.push(format!("{}\n{cals}", tr("GameOver", "CaloriesBurnedToday")));
        }

        profile_lines.push(format!(
            "{}\n{}",
            tr("GameOver", "TotalSongsPlayed"),
            player.total_songs_played,
        ));
    }

    // General stats (no profile required)
    let stats = session_stats_for_side(side, stages);
    let general_lines: Vec<String> = vec![
        format!(
            "{}\n{}",
            tr("GameOver", "SongsPlayedThisGame"),
            stats.songs_played
        ),
        format!(
            "{}\n{}",
            tr("GameOver", "NotesHitThisGame"),
            stats.notes_hit
        ),
        format!(
            "{}\n{}",
            tr("GameOver", "TimeSpentThisGame"),
            format_time_spent(stats.duration_seconds)
        ),
    ];

    (profile_lines, general_lines)
}

pub struct State {
    pub active_color_index: i32,
    bg: visual_style_bg::State,
    elapsed: f32,
    runtime: PostSongRuntimeView,
}

pub fn init(runtime: PostSongRuntimeView) -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX, // overwritten by app
        bg: visual_style_bg::State::new(),
        elapsed: 0.0,
        runtime,
    }
}

pub fn update(state: &mut State, dt: f32) -> Option<ThemeEffect> {
    state.elapsed = (state.elapsed + dt).max(0.0);
    (state.elapsed >= gameover_seconds()).then_some(ThemeEffect::Navigate(Screen::Menu))
}

#[inline(always)]
fn gameover_seconds() -> f32 {
    if visual_styles::srpg10_active() {
        SRPG10_GAMEOVER_SECONDS
    } else {
        GAMEOVER_SECONDS
    }
}

pub fn handle_input(_state: &mut State, ev: &InputEvent) -> ThemeEffect {
    if !ev.pressed {
        return ThemeEffect::None;
    }
    match ev.action {
        VirtualAction::p1_start
        | VirtualAction::p1_back
        | VirtualAction::p2_start
        | VirtualAction::p2_back => ThemeEffect::Navigate(Screen::Menu),
        _ => ThemeEffect::None,
    }
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    stages: &[stage_stats::StageSummary],
    _asset_manager: &AssetManager,
) {
    actors.reserve(64);

    // Background (Simply Love: ScreenWithMenuElements background)
    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
        },
    );

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

    // GAME OVER text (Arrow Cloud: ThemeFont headline, crop reveal)
    {
        let cx = screen_center_x();
        let cy = screen_center_y();
        let zoom = match state.runtime.machine_font {
            deadsync_config::prelude::MachineFont::Wendy => 1.2,
            deadsync_config::prelude::MachineFont::Mega => 1.95,
        };

        actors.push(act!(text:
            font(current_machine_font_key(FontRole::Headline)):
            settext(tr("GameOver", "GameText")):
            align(0.5, 0.5):
            xy(cx, cy - 40.0):
            croptop(1.0): fadetop(1.0):
            zoom(zoom):
            shadowlength(1.0):
            z(20):
            decelerate(0.5): croptop(0.0): fadetop(0.0)
        ));
        actors.push(act!(text:
            font(current_machine_font_key(FontRole::Headline)):
            settext(tr("GameOver", "OverText")):
            align(0.5, 0.5):
            xy(cx, cy + 40.0):
            croptop(1.0): fadetop(1.0):
            zoom(zoom):
            shadowlength(1.0):
            z(20):
            decelerate(0.5): croptop(0.0): fadetop(0.0)
        ));
    }

    for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
        let player = &state.runtime.players[profile_data::player_side_index(side)];
        if !player.joined {
            continue;
        }

        let pc = player_color_rgba(side, state.active_color_index);
        let x_pos = match side {
            profile_data::PlayerSide::P1 => SIDE_BG_X_PAD,
            profile_data::PlayerSide::P2 => screen_width() - SIDE_BG_X_PAD,
        };

        // Avatar (persistent profiles only)
        if !player.guest {
            if let Some(key) = player.avatar_texture_key.as_deref() {
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
                    settext(tr("GameOver", "NoAvatar")):
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

        let (profile_lines, general_lines) = build_player_lines(player, side, stages);

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
}

pub fn get_actors(
    state: &State,
    stages: &[stage_stats::StageSummary],
    asset_manager: &AssetManager,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(64);
    push_actors(&mut actors, state, stages, asset_manager);
    actors
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1100)
}
