use crate::act;
use crate::core::input::{InputEvent, VirtualAction};
use crate::core::space::{
    screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use crate::game::parsing::{noteskin, simfile as song_loading};
use crate::screens::components::heart_bg;
use crate::screens::{Screen, ScreenAction};
use crate::ui::actors::Actor;
use crate::ui::color;
use log::{info, warn};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

/* ----------------------- timing & layout ----------------------- */

/* Show ONLY the hearts bg some time before any other animation starts */
const PRE_ROLL: f32 = 1.25;

/* arrows (matches the simple SM-like splash) */
const ARROW_COUNT: usize = 7;
const ARROW_SPACING: f32 = 50.0;
const ARROW_BASE_DELAY: f32 = 0.20;
const ARROW_STEP_DELAY: f32 = 0.10;
const ARROW_FADE_IN: f32 = 0.75;
const ARROW_FADE_OUT: f32 = 0.75;

/* black bar behind arrows */
const BAR_TARGET_H: f32 = 128.0;
const ARROW_BG_Z: f32 = 106.0; // above hearts, below arrows

/* “squish” bar timings (center line -> open -> close) */
const SQUISH_START_DELAY: f32 = 0.50; // after PRE_ROLL
const SQUISH_IN_DURATION: f32 = 0.35; // 1.0 -> 0.0
pub const BAR_SQUISH_DURATION: f32 = 0.35;

const LOADING_BAR_H: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoadingPhase {
    Songs,
    Courses,
    Finalizing,
}

enum LoadingMsg {
    Phase(LoadingPhase),
    Song {
        done: usize,
        total: usize,
        pack: String,
        song: String,
    },
    Course {
        done: usize,
        total: usize,
        group: String,
        course: String,
    },
    Done,
}

struct LoadingState {
    phase: LoadingPhase,
    line2: String,
    line3: String,
    songs_done: usize,
    songs_total: usize,
    courses_done: usize,
    courses_total: usize,
    done: bool,
    started_at: Instant,
    rx: mpsc::Receiver<LoadingMsg>,
}

impl LoadingState {
    fn new(rx: mpsc::Receiver<LoadingMsg>) -> Self {
        Self {
            phase: LoadingPhase::Songs,
            line2: String::new(),
            line3: String::new(),
            songs_done: 0,
            songs_total: 0,
            courses_done: 0,
            courses_total: 0,
            done: false,
            started_at: Instant::now(),
            rx,
        }
    }
}

/* ----------------------- auto-advance ----------------------- */
#[inline(always)]
fn arrows_finished_at() -> f32 {
    // PRE_ROLL + unsquish end + last arrow fade in/out + tiny pad
    let unsquish_end = SQUISH_START_DELAY + SQUISH_IN_DURATION;
    let last_delay = ARROW_STEP_DELAY.mul_add(ARROW_COUNT as f32, ARROW_BASE_DELAY);
    PRE_ROLL + unsquish_end + last_delay + ARROW_FADE_IN + ARROW_FADE_OUT + 0.05
}

#[inline(always)]
fn maxf(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}
#[inline(always)]
fn remaining(from_time: f32, now: f32) -> f32 {
    maxf(from_time - now, 0.0)
}

/* ---------------------------- state ---------------------------- */

#[derive(PartialEq, Eq)]
enum InitPhase {
    Loading,
    Playing,
    FadingOut,
}

pub struct State {
    elapsed: f32,
    phase: InitPhase,
    loader_started: bool,
    loading: Option<LoadingState>,
    pub active_color_index: i32,
    bg: heart_bg::State,
}

pub fn init() -> State {
    State {
        elapsed: 0.0,
        phase: InitPhase::Loading,
        loader_started: false,
        loading: None,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
    }
}

fn collect_banner_cache_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    {
        let song_cache = crate::game::song::get_song_cache();
        for pack in song_cache.iter() {
            if let Some(path) = pack.banner_path.as_ref() {
                out.push(path.clone());
            }
            for song in &pack.songs {
                if let Some(path) = song.banner_path.as_ref() {
                    out.push(path.clone());
                }
            }
        }
    }
    {
        let course_cache = crate::game::course::get_course_cache();
        for (course_path, course) in course_cache.iter() {
            if let Some(path) =
                rssp::course::resolve_course_banner_path(course_path, &course.banner)
            {
                out.push(path);
            }
        }
    }
    out
}

fn start_loading_thread(state: &mut State) {
    let (tx, rx) = mpsc::channel::<LoadingMsg>();
    state.loading = Some(LoadingState::new(rx));

    std::thread::spawn(move || {
        let _ = tx.send(LoadingMsg::Phase(LoadingPhase::Songs));
        let mut on_song = |done: usize, total: usize, pack: &str, song: &str| {
            let _ = tx.send(LoadingMsg::Song {
                done,
                total,
                pack: pack.to_owned(),
                song: song.to_owned(),
            });
        };
        song_loading::scan_and_load_songs_with_progress_counts("songs", &mut on_song);

        let _ = tx.send(LoadingMsg::Phase(LoadingPhase::Courses));
        let mut on_course = |done: usize, total: usize, group: &str, course: &str| {
            let _ = tx.send(LoadingMsg::Course {
                done,
                total,
                group: group.to_owned(),
                course: course.to_owned(),
            });
        };
        song_loading::scan_and_load_courses_with_progress_counts(
            "courses",
            "songs",
            &mut on_course,
        );

        let _ = tx.send(LoadingMsg::Phase(LoadingPhase::Finalizing));
        let banner_paths = collect_banner_cache_paths();
        info!(
            "Init loading: caching banners ({} textures)...",
            banner_paths.len()
        );
        crate::assets::prewarm_banner_cache(&banner_paths);
        info!("Init loading: banner cache prewarm complete.");
        std::thread::spawn(|| {
            if std::panic::catch_unwind(noteskin::prewarm_itg_preview_cache).is_err() {
                warn!("noteskin prewarm thread panicked; first-use preview hitches may occur");
            }
        });
        let _ = tx.send(LoadingMsg::Done);
    });
}

fn poll_loading_state(loading: &mut LoadingState) {
    while let Ok(msg) = loading.rx.try_recv() {
        match msg {
            LoadingMsg::Phase(phase) => {
                loading.phase = phase;
                loading.line2.clear();
                loading.line3.clear();
            }
            LoadingMsg::Song {
                done,
                total,
                pack,
                song,
            } => {
                loading.phase = LoadingPhase::Songs;
                loading.songs_done = done;
                loading.songs_total = total;
                loading.line2 = pack;
                loading.line3 = song;
            }
            LoadingMsg::Course {
                done,
                total,
                group,
                course,
            } => {
                loading.phase = LoadingPhase::Courses;
                loading.courses_done = done;
                loading.courses_total = total;
                loading.line2 = group;
                loading.line3 = course;
            }
            LoadingMsg::Done => {
                loading.done = true;
            }
        }
    }
}

/* -------------------------- input -> nav ----------------------- */

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if state.phase == InitPhase::Loading {
        return ScreenAction::None;
    }
    if ev.pressed {
        match ev.action {
            VirtualAction::p1_start | VirtualAction::p1_back => {
                return ScreenAction::Navigate(Screen::Menu);
            }
            _ => {}
        }
    }
    ScreenAction::None
}

/* ---------------------------- update --------------------------- */

pub fn update(state: &mut State, dt: f32) -> ScreenAction {
    if state.phase == InitPhase::Loading {
        if !state.loader_started {
            state.loader_started = true;
            start_loading_thread(state);
        }

        let done = if let Some(loading) = state.loading.as_mut() {
            poll_loading_state(loading);
            loading.done
        } else {
            false
        };

        if done {
            state.loading = None;
            state.phase = InitPhase::Playing;
            state.elapsed = 0.0;
        }
        return ScreenAction::None;
    }

    state.elapsed += dt.max(0.0);

    if state.phase == InitPhase::Playing && state.elapsed >= arrows_finished_at() {
        state.phase = InitPhase::FadingOut;
        state.elapsed = arrows_finished_at();
    }

    if state.phase == InitPhase::FadingOut {
        let fade_elapsed = state.elapsed - arrows_finished_at();
        if fade_elapsed >= BAR_SQUISH_DURATION {
            return ScreenAction::Navigate(Screen::Menu);
        }
    }
    ScreenAction::None
}

/* --------------------------- drawing helpers --------------------------- */

pub fn build_squish_bar(progress: f32) -> Actor {
    let w = screen_width();
    let cy = screen_center_y();

    let t = progress.clamp(0.0, 1.0);
    let crop = 0.5 * t;

    act!(quad:
        align(0.5, 0.5):
        xy(0.5 * w, cy):
        zoomto(w, BAR_TARGET_H):
        diffuse(0.0, 0.0, 0.0, 1.0):
        croptop(crop): cropbottom(crop):
        z(105.0)
    )
}

/* Backdrop that starts its animation immediately WHEN ADDED (no initial sleep). */
fn build_arrows_backdrop_now() -> Actor {
    let w = screen_width();
    let cy = screen_center_y();

    act!(quad:
        align(0.5, 0.5):
        xy(0.5 * w, cy):
        zoomto(w, 0.0):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(ARROW_BG_Z):

        /* IN: grow to 128px tall and reach 0.9 alpha */
        accelerate(0.30): zoomto(w, BAR_TARGET_H): diffusealpha(0.90):

        /* hold while arrows do their fade in/out */
        sleep(2.10):

        /* OUT: collapse back to 0 height */
        accelerate(0.30): zoomto(w, 0.0):
        linear(0.0): visible(false)
    )
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.5, 0.5):
        xy(0.5 * screen_width(), screen_center_y()):
        zoomto(screen_width(), BAR_TARGET_H):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1200.0):
        croptop(0.0): cropbottom(0.0):
        linear(0.35): croptop(0.5): cropbottom(0.5)
    );
    (vec![actor], 0.35)
}

fn loading_progress(loading: Option<&LoadingState>) -> (usize, usize, f32) {
    let Some(loading) = loading else {
        return (0, 0, 0.0);
    };
    let done = loading.songs_done.saturating_add(loading.courses_done);
    let mut total = loading.songs_total.saturating_add(loading.courses_total);
    if total < done {
        total = done;
    }
    let mut progress = if total > 0 {
        (done as f32 / total as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if !loading.done && total > 0 && progress >= 1.0 {
        progress = 0.999;
    }
    (done, total, progress)
}

fn loading_phase_label(phase: LoadingPhase) -> &'static str {
    match phase {
        LoadingPhase::Songs => "Loading songs...",
        LoadingPhase::Courses => "Loading courses...",
        LoadingPhase::Finalizing => "Caching banners...",
    }
}

fn push_loading_overlay(state: &State, actors: &mut Vec<Actor>) {
    let loading = state.loading.as_ref();
    let phase = loading.map(|l| l.phase).unwrap_or(LoadingPhase::Songs);
    let (line2, line3) = if let Some(loading) = loading {
        (loading.line2.clone(), loading.line3.clone())
    } else {
        (String::new(), String::new())
    };
    let started = loading.map(|l| l.started_at).unwrap_or_else(Instant::now);
    let elapsed = started.elapsed().as_secs_f32().max(0.0);
    let (done, total, progress) = loading_progress(loading);
    let count_text = if total == 0 {
        String::new()
    } else {
        let pct = 100.0 * progress;
        format!("{done}/{total} ({pct:.1}%)")
    };
    let show_speed_row = matches!(phase, LoadingPhase::Songs | LoadingPhase::Courses) && total > 0;
    let speed_text = if elapsed > 0.0 && show_speed_row {
        format!("Current speed: {:.1} items/s", done as f32 / elapsed)
    } else if show_speed_row {
        "Current speed: 0.0 items/s".to_string()
    } else {
        String::new()
    };
    let fill = color::decorative_rgba(state.active_color_index);

    let bar_w = widescale(360.0, 520.0);
    let bar_h = LOADING_BAR_H;
    let bar_cx = screen_center_x();
    let bar_cy = screen_center_y() + 34.0;
    let fill_w = (bar_w - 4.0) * progress.clamp(0.0, 1.0);

    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.8):
        z(104.0)
    ));
    actors.push(act!(text:
        font("wendy"):
        settext("DEAD SYNC"):
        align(0.5, 0.5):
        xy(screen_center_x(), bar_cy - 136.0):
        zoom(0.82):
        horizalign(center):
        z(110.0)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(if total == 0 { "Initilizing..." } else { loading_phase_label(phase) }):
        align(0.5, 0.5):
        xy(screen_center_x(), bar_cy - 96.0):
        zoom(1.05):
        horizalign(center):
        z(110.0)
    ));
    if !line2.is_empty() {
        actors.push(act!(text:
            font("miso"):
            settext(line2):
            align(0.5, 0.5):
            xy(screen_center_x(), bar_cy - 72.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(110.0)
        ));
    }
    if !line3.is_empty() {
        actors.push(act!(text:
            font("miso"):
            settext(line3):
            align(0.5, 0.5):
            xy(screen_center_x(), bar_cy - 48.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(110.0)
        ));
    }

    let mut bar_children = Vec::with_capacity(4);
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w, bar_h):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(0)
    ));
    bar_children.push(act!(quad:
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoomto(bar_w - 4.0, bar_h - 4.0):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1)
    ));
    if fill_w > 0.0 {
        bar_children.push(act!(quad:
            align(0.0, 0.5):
            xy(2.0, bar_h / 2.0):
            zoomto(fill_w, bar_h - 4.0):
            diffuse(fill[0], fill[1], fill[2], 1.0):
            z(2)
        ));
    }
    bar_children.push(act!(text:
        font("miso"):
        settext(count_text):
        align(0.5, 0.5):
        xy(bar_w / 2.0, bar_h / 2.0):
        zoom(0.9):
        horizalign(center):
        z(3)
    ));
    actors.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [bar_cx, bar_cy],
        size: [
            crate::ui::actors::SizeSpec::Px(bar_w),
            crate::ui::actors::SizeSpec::Px(bar_h),
        ],
        background: None,
        z: 110,
        children: bar_children,
    });

    if show_speed_row {
        actors.push(act!(text:
            font("miso"):
            settext(speed_text):
            align(0.5, 0.5):
            xy(screen_center_x(), bar_cy + 36.0):
            zoom(0.9):
            horizalign(center):
            z(110.0)
        ));
    }
}

/* --------------------------- combined build --------------------------- */

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(32 + ARROW_COUNT);

    /* 1) HEART BACKGROUND — visible immediately */
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    if state.phase == InitPhase::Loading {
        push_loading_overlay(state, &mut actors);
        return actors;
    }

    /* If we’re still in pre-roll, stop here: no squish/backdrop/arrows yet. */
    if state.elapsed < PRE_ROLL {
        return actors;
    }

    /* 2) SQUISH BAR — drive by timeline that starts after PRE_ROLL */
    let t_anim = state.elapsed - PRE_ROLL;

    let progress = if state.phase == InitPhase::FadingOut {
        let fade_elapsed = state.elapsed - arrows_finished_at();
        (fade_elapsed / BAR_SQUISH_DURATION).clamp(0.0, 1.0)
    } else if t_anim < SQUISH_START_DELAY {
        1.0
    } else if t_anim < SQUISH_START_DELAY + SQUISH_IN_DURATION {
        1.0 - ((t_anim - SQUISH_START_DELAY) / SQUISH_IN_DURATION)
    } else {
        0.0
    };
    actors.push(build_squish_bar(progress));

    /* 2.5) ARROW BACKDROP — only add once unsquish has completed */
    let unsquish_end = SQUISH_START_DELAY + SQUISH_IN_DURATION;
    if t_anim >= unsquish_end {
        actors.push(build_arrows_backdrop_now());
    }

    /* 3) RAINBOW ARROWS — their sleeps are computed as “remaining time from now” */
    let cx = screen_center_x();
    let cy = screen_center_y();

    for i in 1..=ARROW_COUNT {
        let x = (i as f32 - 4.0) * ARROW_SPACING;

        // absolute start for arrow i (global time)
        let arrow_start_time =
            ARROW_STEP_DELAY.mul_add(i as f32, PRE_ROLL + unsquish_end + ARROW_BASE_DELAY);
        // convert to remaining time from *current* elapsed so late frames still work perfectly
        let delay_from_now = remaining(arrow_start_time, state.elapsed);

        let tint = color::decorative_rgba(state.active_color_index - i as i32 - 4);

        actors.push(act!(sprite("init_arrow.png"):
            align(0.5, 0.5):
            xy(cx + x, cy):
            z(110.0):
            zoom(0.1):
            diffuse(tint[0], tint[1], tint[2], 0.0):
            sleep(delay_from_now):
            linear(ARROW_FADE_IN):  alpha(1.0):
            linear(ARROW_FADE_OUT): alpha(0.0):
            linear(0.0): visible(false)
        ));
    }

    actors
}
