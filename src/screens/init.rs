use crate::act;
use crate::assets::i18n::tr;
use crate::assets::{FontRole, current_machine_font_key_for_text};
use crate::config::dirs;
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{
    screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use crate::game::{
    course,
    parsing::{noteskin, simfile as song_loading},
};
use crate::screens::components::shared::{loading_bar, visual_style_bg};
use crate::screens::{Screen, ScreenAction};
use log::info;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock};
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
    Artwork,
    Noteskins,
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
    Artwork {
        done: usize,
        total: usize,
        line2: String,
        line3: String,
    },
    Noteskins {
        done: usize,
        total: usize,
        skin: String,
        status: String,
    },
    Done,
}

#[derive(Clone)]
struct SpeedTextCache {
    done: usize,
    tenths_per_sec: u32,
    text: Arc<str>,
}

struct LoadingState {
    phase: LoadingPhase,
    line2: Arc<str>,
    line3: Arc<str>,
    count_text: Arc<str>,
    songs_done: usize,
    songs_total: usize,
    courses_done: usize,
    courses_total: usize,
    artwork_done: usize,
    artwork_total: usize,
    noteskins_done: usize,
    noteskins_total: usize,
    done: bool,
    started_at: Instant,
    rx: mpsc::Receiver<LoadingMsg>,
    speed_text_cache: RefCell<Option<SpeedTextCache>>,
}

impl LoadingState {
    fn new(rx: mpsc::Receiver<LoadingMsg>) -> Self {
        Self {
            phase: LoadingPhase::Songs,
            line2: EMPTY_TEXT.clone(),
            line3: EMPTY_TEXT.clone(),
            count_text: EMPTY_TEXT.clone(),
            songs_done: 0,
            songs_total: 0,
            courses_done: 0,
            courses_total: 0,
            artwork_done: 0,
            artwork_total: 0,
            noteskins_done: 0,
            noteskins_total: 0,
            done: false,
            started_at: Instant::now(),
            rx,
            speed_text_cache: RefCell::new(None),
        }
    }
}

static EMPTY_TEXT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::<str>::from(""));

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
    bg: visual_style_bg::State,
}

pub fn init() -> State {
    State {
        elapsed: 0.0,
        phase: InitPhase::Loading,
        loader_started: false,
        loading: None,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn bench_loading_state() -> State {
    let (_tx, rx) = mpsc::channel::<LoadingMsg>();
    let mut loading = LoadingState::new(rx);
    loading.phase = LoadingPhase::Artwork;
    loading.line2 = Arc::<str>::from("ITL Online 2024");
    loading.line3 = Arc::<str>::from("Deep Down");
    loading.artwork_done = 4821;
    loading.artwork_total = 9372;
    refresh_loading_count_text(&mut loading);
    State {
        elapsed: 0.0,
        phase: InitPhase::Loading,
        loader_started: true,
        loading: Some(loading),
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
    }
}

pub(crate) fn clear_render_cache(state: &State) {
    if let Some(loading) = state.loading.as_ref() {
        *loading.speed_text_cache.borrow_mut() = None;
    }
}

fn collect_artwork_cache_paths() -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut banner = Vec::new();
    let mut cdtitle = Vec::new();
    {
        let song_cache = crate::game::song::get_song_cache();
        for pack in song_cache.iter() {
            if let Some(path) = pack.banner_path.as_ref() {
                banner.push(path.clone());
            }
            for song in &pack.songs {
                if let Some(path) = song.banner_path.as_ref() {
                    banner.push(path.clone());
                }
                if let Some(path) = song.cdtitle_path.as_ref() {
                    cdtitle.push(path.clone());
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
                banner.push(path);
            }
        }
    }
    (banner, cdtitle)
}

fn cache_progress_lines(path: Option<&Path>) -> (String, String) {
    let Some(path) = path else {
        return (String::new(), String::new());
    };
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let file_stem = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or(file_name)
        .to_owned();

    let mut parts: Vec<&str> = Vec::with_capacity(16);
    for component in path.components() {
        if let std::path::Component::Normal(name) = component
            && let Some(name) = name.to_str()
        {
            parts.push(name);
        }
    }

    if let Some(idx) = parts.iter().position(|p| p.eq_ignore_ascii_case("songs"))
        && let Some(pack) = parts.get(idx + 1)
    {
        let line3 = parts
            .get(idx + 2)
            .copied()
            .filter(|name| !name.eq_ignore_ascii_case(file_name))
            .map(str::to_owned)
            .unwrap_or(file_stem);
        return ((*pack).to_owned(), line3);
    }

    if let Some(idx) = parts.iter().position(|p| p.eq_ignore_ascii_case("courses"))
        && let Some(group) = parts.get(idx + 1)
    {
        return ((*group).to_owned(), file_stem);
    }

    let line2 = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_owned();
    (line2, file_stem)
}

#[inline(always)]
fn arc_phase_label(phase: LoadingPhase) -> Arc<str> {
    match phase {
        LoadingPhase::Songs => tr("Init", "LoadingSongsText"),
        LoadingPhase::Courses => tr("Init", "LoadingCoursesText"),
        LoadingPhase::Artwork => tr("Init", "CachingArtworkText"),
        LoadingPhase::Noteskins => tr("Init", "CompilingNoteskinsText"),
    }
}

#[inline(always)]
fn loading_progress_values(loading: &LoadingState) -> (usize, usize, f32) {
    let (done, mut total) = match loading.phase {
        LoadingPhase::Songs => (loading.songs_done, loading.songs_total),
        LoadingPhase::Courses => (loading.courses_done, loading.courses_total),
        LoadingPhase::Artwork => (loading.artwork_done, loading.artwork_total),
        LoadingPhase::Noteskins => (loading.noteskins_done, loading.noteskins_total),
    };
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

fn refresh_loading_count_text(loading: &mut LoadingState) {
    let (done, total, _) = loading_progress_values(loading);
    loading.count_text = if loading.done || (total > 0 && done >= total) {
        tr("Init", "DoneText")
    } else if total == 0 {
        EMPTY_TEXT.clone()
    } else {
        Arc::<str>::from(crate::screens::progress_count_text(done, total))
    };
    *loading.speed_text_cache.borrow_mut() = None;
}

fn speed_text(loading: &LoadingState, done: usize, elapsed_s: f32) -> Arc<str> {
    let tenths_per_sec = if elapsed_s > 0.0 {
        ((done as f32 / elapsed_s) * 10.0).round().max(0.0) as u32
    } else {
        0
    };
    if let Some(cache) = loading.speed_text_cache.borrow().as_ref()
        && cache.done == done
        && cache.tenths_per_sec == tenths_per_sec
    {
        return cache.text.clone();
    }
    let text = Arc::<str>::from(format!(
        "Current speed: {}.{} items/s",
        tenths_per_sec / 10,
        tenths_per_sec % 10
    ));
    *loading.speed_text_cache.borrow_mut() = Some(SpeedTextCache {
        done,
        tenths_per_sec,
        text: text.clone(),
    });
    text
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
        let dirs = dirs::app_dirs();
        song_loading::scan_and_load_songs_with_progress_counts(&dirs.songs_dir(), &mut on_song);

        let _ = tx.send(LoadingMsg::Phase(LoadingPhase::Courses));
        let mut on_course = |done: usize, total: usize, group: &str, course: &str| {
            let _ = tx.send(LoadingMsg::Course {
                done,
                total,
                group: group.to_owned(),
                course: course.to_owned(),
            });
        };
        course::scan_and_load_courses_with_progress_counts(
            &dirs.courses_dir(),
            &dirs.songs_dir(),
            &mut on_course,
        );

        let (banner_paths, cdtitle_paths) = collect_artwork_cache_paths();
        let artwork_total =
            crate::app::media_cache::artwork_cache_jobs(&banner_paths, &cdtitle_paths);

        let _ = tx.send(LoadingMsg::Phase(LoadingPhase::Artwork));
        info!(
            "Init loading: caching artwork in one pass (banner={}, cdtitle={}, total jobs={})...",
            banner_paths.len(),
            cdtitle_paths.len(),
            artwork_total
        );
        let mut on_artwork = |done: usize, _total: usize, path: Option<&Path>| {
            let (line2, line3) = cache_progress_lines(path);
            let _ = tx.send(LoadingMsg::Artwork {
                done,
                total: artwork_total,
                line2,
                line3,
            });
        };
        crate::app::media_cache::prewarm_artwork_cache_with_progress(
            &banner_paths,
            &cdtitle_paths,
            &mut on_artwork,
        );
        info!("Init loading: artwork cache prewarm complete.");

        let _ = tx.send(LoadingMsg::Phase(LoadingPhase::Noteskins));
        info!("Init loading: compiling noteskin cache before UI...");
        let mut on_noteskin = |done: usize, total: usize, skin: &str, status: &str| {
            let _ = tx.send(LoadingMsg::Noteskins {
                done,
                total,
                skin: skin.to_owned(),
                status: status.to_owned(),
            });
        };
        let noteskin_summary = noteskin::compile_all_itg_caches_with_progress(&mut on_noteskin);
        info!(
            "Init loading: noteskin cache compile complete (total={}, built={}, reused={}, failed={}).",
            noteskin_summary.total,
            noteskin_summary.built,
            noteskin_summary.reused,
            noteskin_summary.failed
        );
        let _ = tx.send(LoadingMsg::Done);
    });
}

fn poll_loading_state(loading: &mut LoadingState) {
    while let Ok(msg) = loading.rx.try_recv() {
        match msg {
            LoadingMsg::Phase(phase) => {
                loading.phase = phase;
                loading.line2 = EMPTY_TEXT.clone();
                loading.line3 = EMPTY_TEXT.clone();
                refresh_loading_count_text(loading);
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
                loading.line2 = Arc::<str>::from(pack);
                loading.line3 = Arc::<str>::from(song);
                refresh_loading_count_text(loading);
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
                loading.line2 = Arc::<str>::from(group);
                loading.line3 = Arc::<str>::from(course);
                refresh_loading_count_text(loading);
            }
            LoadingMsg::Artwork {
                done,
                total,
                line2,
                line3,
            } => {
                loading.phase = LoadingPhase::Artwork;
                loading.artwork_done = done;
                loading.artwork_total = total;
                loading.line2 = Arc::<str>::from(line2);
                loading.line3 = Arc::<str>::from(line3);
                refresh_loading_count_text(loading);
            }
            LoadingMsg::Noteskins {
                done,
                total,
                skin,
                status,
            } => {
                loading.phase = LoadingPhase::Noteskins;
                loading.noteskins_done = done;
                loading.noteskins_total = total;
                loading.line2 = Arc::<str>::from(skin);
                loading.line3 = Arc::<str>::from(status);
                refresh_loading_count_text(loading);
            }
            LoadingMsg::Done => {
                loading.done = true;
                refresh_loading_count_text(loading);
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
    loading_progress_values(loading)
}

fn push_loading_overlay(state: &State, actors: &mut Vec<Actor>, loading_elapsed_s: f32) {
    let loading = state.loading.as_ref();
    let phase = loading.map(|l| l.phase).unwrap_or(LoadingPhase::Songs);
    let (done, total, progress) = loading_progress(loading);
    let show_speed_row = matches!(
        phase,
        LoadingPhase::Songs
            | LoadingPhase::Courses
            | LoadingPhase::Artwork
            | LoadingPhase::Noteskins
    ) && total > 0;
    let speed_text = loading
        .filter(|_| show_speed_row)
        .map(|loading| speed_text(loading, done, loading_elapsed_s.max(0.0)));
    let fill = color::decorative_rgba(state.active_color_index);

    let bar_w = widescale(360.0, 520.0);
    let bar_h = LOADING_BAR_H;
    let bar_cx = screen_center_x();
    let bar_cy = screen_center_y() + 34.0;
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.8):
        z(104.0)
    ));
    let title_text = tr("Init", "TitleText");
    let title_font = current_machine_font_key_for_text(FontRole::Header, &title_text);
    actors.push(act!(text:
        font(title_font):
        settext(title_text):
        align(0.5, 0.5):
        xy(screen_center_x(), bar_cy - 136.0):
        zoom(0.82):
        horizalign(center):
        z(110.0)
    ));
    let phase_label = loading.map_or_else(
        || tr("Init", "InitializingText"),
        |_| arc_phase_label(phase),
    );
    actors.push(act!(text:
        font("miso"):
        settext(phase_label):
        align(0.5, 0.5):
        xy(screen_center_x(), bar_cy - 96.0):
        zoom(1.05):
        horizalign(center):
        z(110.0)
    ));
    if let Some(loading) = loading
        && !loading.line2.is_empty()
    {
        actors.push(act!(text:
            font("miso"):
            settext(loading.line2.clone()):
            align(0.5, 0.5):
            xy(screen_center_x(), bar_cy - 72.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(110.0)
        ));
    }
    if let Some(loading) = loading
        && !loading.line3.is_empty()
    {
        actors.push(act!(text:
            font("miso"):
            settext(loading.line3.clone()):
            align(0.5, 0.5):
            xy(screen_center_x(), bar_cy - 48.0):
            zoom(0.95):
            maxwidth(screen_width() * 0.9):
            horizalign(center):
            z(110.0)
        ));
    }

    actors.push(loading_bar::build(loading_bar::LoadingBarParams {
        align: [0.5, 0.5],
        offset: [bar_cx, bar_cy],
        width: bar_w,
        height: bar_h,
        progress,
        label: loading
            .map_or_else(|| EMPTY_TEXT.clone(), |loading| loading.count_text.clone())
            .into(),
        fill_rgba: [fill[0], fill[1], fill[2], 1.0],
        bg_rgba: [0.0, 0.0, 0.0, 1.0],
        border_rgba: [1.0, 1.0, 1.0, 1.0],
        text_rgba: [1.0, 1.0, 1.0, 1.0],
        text_zoom: 0.9,
        z: 110,
    }));

    if let Some(speed_text) = speed_text {
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

fn get_actors_with_elapsed_overrides(
    state: &State,
    loading_elapsed_override: Option<f32>,
    bg_elapsed_override: Option<f32>,
) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(32 + ARROW_COUNT);

    /* 1) HEART BACKGROUND — visible immediately */
    let bg_params = visual_style_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    };
    actors.extend(if let Some(bg_elapsed_s) = bg_elapsed_override {
        state.bg.build_at_elapsed(bg_params, bg_elapsed_s)
    } else {
        state.bg.build(bg_params)
    });

    if state.phase == InitPhase::Loading {
        let loading_elapsed_s = loading_elapsed_override.unwrap_or_else(|| {
            state.loading.as_ref().map_or(0.0, |loading| {
                loading.started_at.elapsed().as_secs_f32().max(0.0)
            })
        });
        push_loading_overlay(state, &mut actors, loading_elapsed_s);
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
            tweensalt(i):
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

pub(crate) fn get_actors_at_loading_elapsed(state: &State, loading_elapsed_s: f32) -> Vec<Actor> {
    get_actors_with_elapsed_overrides(state, Some(loading_elapsed_s), Some(loading_elapsed_s))
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    get_actors_with_elapsed_overrides(state, None, None)
}
