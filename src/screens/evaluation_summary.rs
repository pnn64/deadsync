use crate::act;
use crate::assets::AssetManager;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{screen_center_x, screen_height, screen_width, widescale};
use crate::game::profile;
use crate::game::scores;
use crate::game::song::SongData;
use crate::game::stage_stats;
use crate::screens::components::screen_bar::{
    ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::{eval_grades, heart_bg, screen_bar};
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use chrono::Local;
use std::collections::HashSet;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

const ROWS_PER_PAGE: usize = 4;

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    pub page: usize,
    pub elapsed: f32,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX, // overwritten by app.rs
        bg: heart_bg::State::new(),
        page: 1,
        elapsed: 0.0,
    }
}

pub fn update(state: &mut State, dt: f32) {
    state.elapsed = (state.elapsed + dt).max(0.0);
}

pub fn handle_input(state: &mut State, num_stages: usize, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_back
        | VirtualAction::p1_start
        | VirtualAction::p2_back
        | VirtualAction::p2_start => ScreenAction::Navigate(Screen::Initials),

        VirtualAction::p1_menu_left
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_menu_left
        | VirtualAction::p2_menu_up
        | VirtualAction::p1_left
        | VirtualAction::p1_up
        | VirtualAction::p2_left
        | VirtualAction::p2_up => {
            let pages = pages_for(num_stages);
            if pages > 1 && state.page > 1 {
                state.page = state.page.saturating_sub(1).max(1);
            }
            ScreenAction::None
        }

        VirtualAction::p1_menu_right
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_menu_right
        | VirtualAction::p2_menu_down
        | VirtualAction::p1_right
        | VirtualAction::p1_down
        | VirtualAction::p2_right
        | VirtualAction::p2_down => {
            let pages = pages_for(num_stages);
            if pages > 1 {
                state.page = (state.page + 1).min(pages.max(1));
            }
            ScreenAction::None
        }

        _ => ScreenAction::None,
    }
}

#[inline(always)]
fn pages_for(num_stages: usize) -> usize {
    let pages = num_stages.div_ceil(ROWS_PER_PAGE);
    pages.max(1)
}

fn format_rate_x(rate: f32) -> String {
    let r = if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    };
    let s = format!("{r:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn display_bpm_range(song: &SongData) -> Option<(f64, f64)> {
    let s = song.display_bpm.trim();
    if !s.is_empty() && s != "*" {
        let parts: Vec<&str> = s.split([':', '-']).map(str::trim).collect();
        if parts.len() == 2 {
            if let (Ok(a), Ok(b)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                let lo = a.min(b);
                let hi = a.max(b);
                if lo.is_finite() && hi.is_finite() && lo > 0.0 && hi > 0.0 {
                    return Some((lo, hi));
                }
            }
        } else if let Ok(v) = s.parse::<f64>() {
            if v.is_finite() && v > 0.0 {
                return Some((v, v));
            }
        }
    }

    let lo = song.min_bpm;
    let hi = song.max_bpm;
    if lo.is_finite() && hi.is_finite() && lo > 0.0 && hi > 0.0 {
        Some((lo.min(hi), lo.max(hi)))
    } else {
        None
    }
}

fn stringify_display_bpms(song: &SongData, music_rate: f32) -> String {
    let Some((mut lo, mut hi)) = display_bpm_range(song) else {
        return String::new();
    };

    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate as f64
    } else {
        1.0
    };
    lo *= rate;
    hi *= rate;

    let use_decimals = (music_rate - 1.0).abs() > 0.001;
    let fmt_one = |v: f64| {
        if use_decimals {
            let s = format!("{v:.1}");
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            format!("{v:.0}")
        }
    };

    if (lo - hi).abs() < 1.0e-6 {
        fmt_one(lo)
    } else {
        format!("{} - {}", fmt_one(lo), fmt_one(hi))
    }
}

fn steps_type_label(chart_type: &str) -> &'static str {
    if chart_type.eq_ignore_ascii_case("dance-single") {
        "Single"
    } else if chart_type.eq_ignore_ascii_case("dance-double") {
        "Double"
    } else {
        "Unknown"
    }
}

fn difficulty_display_name(difficulty: &str) -> &'static str {
    if difficulty.eq_ignore_ascii_case("edit") {
        return "Edit";
    }
    let difficulty_index = color::FILE_DIFFICULTY_NAMES
        .iter()
        .position(|&n| n.eq_ignore_ascii_case(difficulty))
        .unwrap_or(2);
    color::DISPLAY_DIFFICULTY_NAMES[difficulty_index]
}

fn should_display_profile_names(stages: &[stage_stats::StageSummary]) -> bool {
    let mut p1: HashSet<&str> = HashSet::new();
    let mut p2: HashSet<&str> = HashSet::new();
    for s in stages {
        if let Some(p) = s.players.get(0).and_then(|p| p.as_ref()) {
            p1.insert(p.profile_name.as_str());
        }
        if let Some(p) = s.players.get(1).and_then(|p| p.as_ref()) {
            p2.insert(p.profile_name.as_str());
        }
    }
    p1.len() > 1 || p2.len() > 1
}

fn build_player_stats(
    side: profile::PlayerSide,
    p: &stage_stats::PlayerStageSummary,
    show_profile_names: bool,
    active_color_index: i32,
    elapsed: f32,
) -> Vec<Actor> {
    let (col1x, col2x, grade_x, align1_x, align2_x, align1_text, align2_text, col1_eps) = match side
    {
        profile::PlayerSide::P1 => (
            -90.0,
            -(screen_width() / 2.5),
            -widescale(194.0, 250.0),
            1.0,
            0.0,
            crate::ui::actors::TextAlign::Right,
            crate::ui::actors::TextAlign::Left,
            -1.0,
        ),
        profile::PlayerSide::P2 => (
            90.0,
            screen_width() / 2.5,
            widescale(194.0, 250.0),
            0.0,
            1.0,
            crate::ui::actors::TextAlign::Left,
            crate::ui::actors::TextAlign::Right,
            1.0,
        ),
    };

    let mut out = Vec::with_capacity(24);

    // Profile name (only if there were any profile switches this session)
    if show_profile_names {
        let mut a = act!(text:
            font("miso"):
            settext(p.profile_name.clone()):
            align(align1_x, 0.5):
            xy(col1x, -43.0):
            zoom(0.5):
            z(3):
            diffuse(1.0, 1.0, 1.0, 1.0)
        );
        if let Actor::Text { align_text, .. } = &mut a {
            *align_text = align1_text;
        }
        out.push(a);
    }

    let show_w0 = p.show_w0;
    let showex = p.show_ex_score;

    // Percent score (trim '%' and remove leading whitespace, like Simply Love)
    let percent_text = format!("{:.2}", (p.score_percent * 100.0).max(0.0));
    let percent_rgba = if p.grade == scores::Grade::Failed {
        [1.0, 0.0, 0.0, 1.0]
    } else {
        [1.0; 4]
    };

    let (percent_zoom, percent_y) = if showex {
        (0.38, -12.0)
    } else if show_w0 {
        (0.48, -32.0)
    } else {
        (0.5, -24.0)
    };
    let mut percent_actor = act!(text:
        font("wendy"):
        settext(percent_text):
        align(align1_x, 0.5):
        xy(col1x, percent_y):
        zoom(percent_zoom):
        z(3):
        diffuse(percent_rgba[0], percent_rgba[1], percent_rgba[2], percent_rgba[3])
    );
    if let Actor::Text { align_text, .. } = &mut percent_actor {
        *align_text = align1_text;
    }
    out.push(percent_actor);

    // EX score (only if W0 is enabled)
    if show_w0 {
        let ex_color = color::JUDGMENT_RGBA[0];
        let ex_text = format!("{:.2}", p.ex_score_percent.max(0.0));
        let (ex_zoom, ex_y) = if showex { (0.48, -32.0) } else { (0.38, -12.0) };
        let mut ex_actor = act!(text:
            font("wendy"):
            settext(ex_text):
            align(align1_x, 0.5):
            xy(col1x, ex_y):
            zoom(ex_zoom):
            z(3):
            diffuse(ex_color[0], ex_color[1], ex_color[2], ex_color[3])
        );
        if let Actor::Text { align_text, .. } = &mut ex_actor {
            *align_text = align1_text;
        }
        out.push(ex_actor);
    }

    // Stepchart style + difficulty text
    {
        let style = steps_type_label(&p.chart.chart_type);
        let diff = difficulty_display_name(&p.chart.difficulty);
        let text = format!("{style} / {diff}");
        let mut a = act!(text:
            font("miso"):
            settext(text):
            align(align1_x, 0.5):
            xy(col1x + col1_eps, 17.0):
            zoom(0.65):
            z(3):
            diffuse(1.0, 1.0, 1.0, 1.0)
        );
        if let Actor::Text { align_text, .. } = &mut a {
            *align_text = align1_text;
        }
        out.push(a);
    }

    // Difficulty meter
    {
        let diff_color = color::difficulty_rgba(&p.chart.difficulty, active_color_index);
        let (meter_zoom, meter_y) = if show_w0 { (0.3, 5.0) } else { (0.4, -1.0) };
        let mut a = act!(text:
            font("wendy"):
            settext(p.chart.meter.to_string()):
            align(align1_x, 0.5):
            xy(col1x, meter_y):
            zoom(meter_zoom):
            z(3):
            diffuse(diff_color[0], diff_color[1], diff_color[2], 1.0)
        );
        if let Actor::Text { align_text, .. } = &mut a {
            *align_text = align1_text;
        }
        out.push(a);
    }

    // Step artist
    {
        let mut a = act!(text:
            font("miso"):
            settext(p.chart.step_artist.clone()):
            align(align1_x, 0.5):
            xy(col1x, 32.0):
            zoom(0.65):
            z(3):
            diffuse(1.0, 1.0, 1.0, 1.0)
        );
        if let Actor::Text { align_text, .. } = &mut a {
            *align_text = align1_text;
        }
        out.push(a);
    }

    // Letter grade
    out.extend(eval_grades::actors(
        p.grade,
        eval_grades::EvalGradeParams {
            x: grade_x,
            y: -6.0,
            z: 4,
            zoom: widescale(0.275, 0.3),
            elapsed,
        },
    ));

    // Judgment numbers: W0..W5, Miss
    let wc = p.window_counts;
    let mut counts: [u32; 7] = [wc.w0, wc.w1, wc.w2, wc.w3, wc.w4, wc.w5, wc.miss];
    if !show_w0 {
        counts[1] = counts[0].saturating_add(counts[1]); // W1 includes W0 when FA+/EX is disabled
    }
    let y_base = if show_w0 { -58.0 } else { -63.0 };

    for i in 0..counts.len() {
        if i == 0 && !show_w0 {
            continue;
        }
        let y = ((i as f32) + 1.0).mul_add(13.0, y_base);
        let rgba = match i {
            0 => color::JUDGMENT_RGBA[0], // W0
            1 => {
                if show_w0 {
                    color::JUDGMENT_FA_PLUS_WHITE_RGBA
                } else {
                    color::JUDGMENT_RGBA[0]
                }
            }
            2 => color::JUDGMENT_RGBA[1],
            3 => color::JUDGMENT_RGBA[2],
            4 => color::JUDGMENT_RGBA[3],
            5 => color::JUDGMENT_RGBA[4],
            _ => color::JUDGMENT_RGBA[5],
        };

        let mut a = act!(text:
            font("wendy"):
            settext(counts[i].to_string()):
            align(align2_x, 0.5):
            xy(col2x, y):
            zoom(0.28):
            z(3):
            diffuse(rgba[0], rgba[1], rgba[2], rgba[3])
        );
        if let Actor::Text { align_text, .. } = &mut a {
            *align_text = align2_text;
        }
        out.push(a);
    }

    out
}

fn build_row(
    row_pos: usize,
    stage: &stage_stats::StageSummary,
    show_profile_names: bool,
    active_color_index: i32,
    elapsed: f32,
) -> Actor {
    let cx = screen_center_x();
    let y = (screen_height() / 4.75) * (row_pos as f32);

    let banner_key = stage
        .song
        .banner_path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            let banner_num = active_color_index.rem_euclid(12) + 1;
            format!("banner{banner_num}.png")
        });

    let full_title = stage
        .song
        .display_full_title(crate::config::get().translated_titles);

    let bpm_str = stringify_display_bpms(&stage.song, stage.music_rate);
    let bpm_line = if bpm_str.is_empty() {
        String::new()
    } else if (stage.music_rate - 1.0).abs() > 0.001 {
        format!(
            "{} bpm ({}x Music Rate)",
            bpm_str,
            format_rate_x(stage.music_rate)
        )
    } else {
        format!("{bpm_str} bpm")
    };

    let mut children: Vec<Actor> = Vec::with_capacity(64);

    // Black quad background
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, -6.0):
        zoomto(screen_width() - 40.0, 94.0):
        diffuse(0.0, 0.0, 0.0, 0.5):
        z(0)
    ));

    // Banner
    children.push(act!(sprite(banner_key):
        align(0.5, 0.5):
        xy(0.0, -6.0):
        setsize(418.0, 164.0):
        zoom(0.333):
        z(1)
    ));

    // Song title
    children.push(act!(text:
        font("miso"):
        settext(full_title):
        align(0.5, 0.5):
        xy(0.0, -43.0):
        zoom(0.8):
        maxwidth(350.0):
        z(2):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(center)
    ));

    // BPM(s)
    children.push(act!(text:
        font("miso"):
        settext(bpm_line):
        align(0.5, 0.5):
        xy(0.0, 32.0):
        zoom(0.65):
        maxwidth(350.0):
        z(2):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(center)
    ));

    for (idx, side) in [(0, profile::PlayerSide::P1), (1, profile::PlayerSide::P2)] {
        let Some(p) = stage.players.get(idx).and_then(|p| p.as_ref()) else {
            continue;
        };
        children.extend(build_player_stats(
            side,
            p,
            show_profile_names,
            active_color_index,
            elapsed,
        ));
    }

    Actor::Frame {
        align: [0.5, 0.5],
        offset: [cx, y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z: 50,
    }
}

pub fn get_actors(
    state: &State,
    stages: &[stage_stats::StageSummary],
    _asset_manager: &AssetManager,
) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(32);

    // Background
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    // Top Bar
    actors.push(screen_bar::build(ScreenBarParams {
        title: "EVALUATION",
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

    if stages.is_empty() {
        actors.push(act!(text:
            font("wendy"):
            settext("NO STAGE DATA AVAILABLE"):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_height() * 0.5):
            zoom(0.8):
            z(100):
            diffuse(1.0, 1.0, 1.0, 1.0):
            horizalign(center)
        ));
        return actors;
    }

    let pages = pages_for(stages.len());
    let page = state.page.clamp(1, pages);

    // Centered "Page x/y"
    actors.push(act!(text:
        font("wendy"):
        settext(format!("Page {page}/{pages}")):
        align(0.5, 0.5):
        xy(screen_center_x(), 15.0):
        zoom(widescale(0.5, 0.6)):
        z(121):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(center)
    ));

    // --- "ITG" text (top right, no pads) ---
    {
        let itg_text_x = screen_width() - 10.0;
        actors.push(act!(text:
                font("wendy"):
                settext("ITG"):
                align(1.0, 0.5):
            xy(itg_text_x, 15.0):
            zoom(widescale(0.5, 0.6)):
            z(121):
            diffuse(1.0, 1.0, 1.0, 1.0)
        ));
    }

    let show_profile_names = should_display_profile_names(stages);
    for row in 1..=ROWS_PER_PAGE {
        let stage_index = (page - 1) * ROWS_PER_PAGE + (row - 1);
        let Some(stage) = stages.get(stage_index) else {
            continue;
        };
        actors.push(build_row(
            row,
            stage,
            show_profile_names,
            state.active_color_index,
            state.elapsed,
        ));
    }

    // --- Footer decorations (avatars + date/time) ---
    {
        let play_style = profile::get_session_play_style();
        let player_side = profile::get_session_player_side();

        let p1_profile = profile::get_for_side(profile::PlayerSide::P1);
        let p2_profile = profile::get_for_side(profile::PlayerSide::P2);

        let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
        let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
        let p1_guest = profile::is_session_side_guest(profile::PlayerSide::P1);
        let p2_guest = profile::is_session_side_guest(profile::PlayerSide::P2);

        let p1_avatar_key = if p1_joined && !p1_guest {
            p1_profile.avatar_texture_key
        } else {
            None
        };
        let p2_avatar_key = if p2_joined && !p2_guest {
            p2_profile.avatar_texture_key
        } else {
            None
        };

        let (left_avatar, right_avatar) = if play_style == profile::PlayStyle::Versus {
            (p1_avatar_key.as_deref(), p2_avatar_key.as_deref())
        } else {
            match player_side {
                profile::PlayerSide::P1 => (p1_avatar_key.as_deref(), None),
                profile::PlayerSide::P2 => (None, p2_avatar_key.as_deref()),
            }
        };

        if let Some(key) = left_avatar {
            actors.push(act!(sprite(key):
                align(0.0, 1.0):
                xy(0.0, screen_height()):
                setsize(32.0, 32.0):
                z(121)
            ));
        }
        if let Some(key) = right_avatar {
            actors.push(act!(sprite(key):
                align(1.0, 1.0):
                xy(screen_width(), screen_height()):
                setsize(32.0, 32.0):
                z(121)
            ));
        }

        let timestamp_text = Local::now().format("%Y/%m/%d %H:%M").to_string();
        actors.push(act!(text:
            font("wendy_monospace_numbers"):
            settext(timestamp_text):
            align(0.5, 1.0):
            xy(screen_center_x(), screen_height() - 14.0):
            zoom(0.18):
            horizalign(center):
            z(121)
        ));
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
