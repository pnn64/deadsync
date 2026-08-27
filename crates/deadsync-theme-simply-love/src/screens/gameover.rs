use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::{FontRole, machine_font_key};
use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::{Screen, ThemeEffect};
use crate::views::{PostSongPlayerView, PostSongRuntimeView};
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile as profile_data;
use deadsync_score::stage_stats;
use std::sync::Arc;

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

const fn player_color_rgba(side: profile_data::PlayerSide, active_color_index: i32) -> [f32; 4] {
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

struct GameOverPlayerText {
    profile_lines: Option<[Arc<str>; 3]>,
    general_lines: [Arc<str>; 3],
}

/// Retained Game Over text owned exclusively by the game thread.
///
/// The cache lives for one Game Over screen and warms before actor construction.
/// Capacity is fixed at three labels and six lines per joined player; stable
/// frames compare a dirty bit, locale revision, and stage count. A miss scans
/// the session stages once per joined side and replaces the fixed data without
/// eviction or background work. Screen teardown drops all values; the sync
/// result supports cadence tests, and steady-state frame cost is O(1).
struct GameOverText {
    game: Arc<str>,
    over: Arc<str>,
    no_avatar: Arc<str>,
    players: [Option<GameOverPlayerText>; 2],
    dirty: bool,
    i18n_revision: u64,
    stage_count: usize,
}

impl GameOverText {
    fn new() -> Self {
        Self {
            game: Arc::from(""),
            over: Arc::from(""),
            no_avatar: Arc::from(""),
            players: std::array::from_fn(|_| None),
            dirty: true,
            i18n_revision: u64::MAX,
            stage_count: 0,
        }
    }

    fn sync(
        &mut self,
        runtime: &PostSongRuntimeView,
        stages: &[stage_stats::StageSummary],
    ) -> bool {
        let i18n_revision = crate::assets::i18n::revision();
        if !self.dirty && self.i18n_revision == i18n_revision && self.stage_count == stages.len() {
            return false;
        }

        self.game = tr("GameOver", "GameText");
        self.over = tr("GameOver", "OverText");
        self.no_avatar = tr("GameOver", "NoAvatar");
        self.players = std::array::from_fn(|index| {
            let side = if index == 0 {
                profile_data::PlayerSide::P1
            } else {
                profile_data::PlayerSide::P2
            };
            build_player_lines(&runtime.players[index], side, stages)
        });
        self.dirty = false;
        self.i18n_revision = crate::assets::i18n::revision();
        self.stage_count = stages.len();
        true
    }
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

fn format_time_spent(seconds_total: f32) -> Arc<str> {
    let total = seconds_total.max(0.0).round() as u32;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    let hours = TextContent::inline_u32(hours);
    let minutes = TextContent::inline_u32(minutes);
    let seconds = TextContent::inline_u32(seconds);

    if total >= 3600 {
        tr_fmt(
            "GameOver",
            "TimeFormatHMS",
            &[
                ("hours", hours.as_str()),
                ("minutes", minutes.as_str()),
                ("seconds", seconds.as_str()),
            ],
        )
    } else {
        tr_fmt(
            "GameOver",
            "TimeFormatMS",
            &[("minutes", minutes.as_str()), ("seconds", seconds.as_str())],
        )
    }
}

fn build_player_lines(
    player: &PostSongPlayerView,
    side: profile_data::PlayerSide,
    stages: &[stage_stats::StageSummary],
) -> Option<GameOverPlayerText> {
    if !player.joined {
        return None;
    }

    // Profile stats (only for persistent profiles)
    let profile_lines = if player.guest {
        None
    } else {
        let calories = if player.ignore_step_count_calories {
            Arc::from("")
        } else {
            let cals = if player.calories_burned_today.is_finite()
                && player.calories_burned_today >= 0.0
            {
                player.calories_burned_today.round() as u32
            } else {
                0
            };
            Arc::from(format!("{}\n{cals}", tr("GameOver", "CaloriesBurnedToday")))
        };
        Some([
            Arc::from(player.display_name.as_str()),
            calories,
            Arc::from(format!(
                "{}\n{}",
                tr("GameOver", "TotalSongsPlayed"),
                player.total_songs_played,
            )),
        ])
    };

    // General stats (no profile required)
    let stats = session_stats_for_side(side, stages);
    let general_lines = [
        Arc::from(format!(
            "{}\n{}",
            tr("GameOver", "SongsPlayedThisGame"),
            stats.songs_played
        )),
        Arc::from(format!(
            "{}\n{}",
            tr("GameOver", "NotesHitThisGame"),
            stats.notes_hit
        )),
        Arc::from(format!(
            "{}\n{}",
            tr("GameOver", "TimeSpentThisGame"),
            format_time_spent(stats.duration_seconds)
        )),
    ];

    Some(GameOverPlayerText {
        profile_lines,
        general_lines,
    })
}

pub struct State {
    pub active_color_index: i32,
    bg: visual_style_bg::State,
    elapsed: f32,
    text: GameOverText,
    runtime: PostSongRuntimeView,
}

#[must_use]
pub fn init(runtime: PostSongRuntimeView) -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX, // overwritten by app
        bg: visual_style_bg::State::new(),
        elapsed: 0.0,
        text: GameOverText::new(),
        runtime,
    }
}

pub fn sync_text(state: &mut State, stages: &[stage_stats::StageSummary]) -> bool {
    state.text.sync(&state.runtime, stages)
}

pub fn update(state: &mut State, dt: f32) -> Option<ThemeEffect> {
    state.elapsed = (state.elapsed + dt).max(0.0);
    (state.elapsed >= gameover_seconds(state.runtime.srpg10_visuals))
        .then_some(ThemeEffect::Navigate(Screen::Menu))
}

#[inline(always)]
const fn gameover_seconds(srpg10: bool) -> f32 {
    if srpg10 {
        SRPG10_GAMEOVER_SECONDS
    } else {
        GAMEOVER_SECONDS
    }
}

pub const fn handle_input(_state: &mut State, ev: &InputEvent) -> ThemeEffect {
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
    _asset_manager: &AssetManager,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(64);

    // Background (Simply Love: ScreenWithMenuElements background)
    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
            visual_policy,
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
        let headline_font = machine_font_key(state.runtime.machine_font, FontRole::Headline);
        let zoom = match state.runtime.machine_font {
            deadsync_config::prelude::MachineFont::Wendy => 1.2,
            deadsync_config::prelude::MachineFont::Mega => 1.95,
        };

        actors.push(act!(text:
            font(headline_font):
            settext(Arc::clone(&state.text.game)):
            align(0.5, 0.5):
            xy(cx, cy - 40.0):
            croptop(1.0): fadetop(1.0):
            zoom(zoom):
            shadowlength(1.0):
            z(20):
            decelerate(0.5): croptop(0.0): fadetop(0.0)
        ));
        actors.push(act!(text:
            font(headline_font):
            settext(Arc::clone(&state.text.over)):
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
        let player_index = profile_data::player_side_index(side);
        let player = &state.runtime.players[player_index];
        let Some(lines) = state.text.players[player_index].as_ref() else {
            continue;
        };

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
                    xy(AVATAR_DIM.mul_add(-0.5, x_pos), AVATAR_Y):
                    zoomto(AVATAR_DIM, AVATAR_DIM):
                    z(12)
                ));
            } else {
                actors.push(act!(quad:
                    align(0.0, 0.0):
                    xy(AVATAR_DIM.mul_add(-0.5, x_pos), AVATAR_Y):
                    zoomto(AVATAR_DIM, AVATAR_DIM):
                    diffuse(0.157, 0.196, 0.224, 0.667):
                    z(12)
                ));
                actors.push(act!(text:
                    font("miso"):
                    settext(Arc::clone(&state.text.no_avatar)):
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

        if let Some(profile_lines) = lines.profile_lines.as_ref() {
            for (i, line) in profile_lines.iter().enumerate() {
                let y = LINE_HEIGHT.mul_add(i as f32, PROFILE_STATS_Y);
                actors.push(act!(text:
                    font("miso"):
                    settext(Arc::clone(line)):
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

        for (i, line) in lines.general_lines.iter().enumerate() {
            let y = LINE_HEIGHT.mul_add((i + 1) as f32, NORMAL_STATS_Y);
            actors.push(act!(text:
                font("miso"):
                settext(Arc::clone(line)):
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
    state: &mut State,
    stages: &[stage_stats::StageSummary],
    asset_manager: &AssetManager,
) -> Vec<Actor> {
    sync_text(state, stages);
    let mut actors = Vec::with_capacity(64);
    push_actors(
        &mut actors,
        state,
        asset_manager,
        crate::views::SimplyLoveVisualPolicyView::default(),
    );
    actors
}

#[must_use]
pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

#[must_use]
pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gameover_text_builds_once_for_stable_session_data() {
        let mut runtime = PostSongRuntimeView::default();
        runtime.players[0].joined = true;
        runtime.players[0].display_name = "Player One".to_owned();
        runtime.players[0].total_songs_played = 42;
        let mut state = init(runtime);

        assert!(sync_text(&mut state, &[]));
        assert!(!sync_text(&mut state, &[]));

        let player = state.text.players[0].as_ref().expect("joined player text");
        let profile = player
            .profile_lines
            .as_ref()
            .expect("persistent profile lines");
        assert_eq!(profile[0].as_ref(), "Player One");
        assert!(profile[2].contains("42"));
        assert_eq!(player.general_lines.len(), 3);
    }

    #[test]
    fn gameover_text_omits_profile_lines_for_guests() {
        let mut runtime = PostSongRuntimeView::default();
        runtime.players[1].joined = true;
        runtime.players[1].guest = true;
        let mut state = init(runtime);

        assert!(sync_text(&mut state, &[]));
        let player = state.text.players[1].as_ref().expect("joined guest text");
        assert!(player.profile_lines.is_none());
    }
}
