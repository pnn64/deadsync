use crate::act;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::{FontRole, machine_font_key_for_text};
use crate::screens::components::shared::{loading_bar, visual_style_bg};
use crate::screens::{Screen, ThemeEffect};
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{
    screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use deadsync_input::{InputEvent, VirtualAction};
use std::cell::RefCell;
use std::path::PathBuf;
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

#[derive(Clone)]
struct SpeedTextCache {
    done: usize,
    tenths_per_sec: u32,
    text: Arc<str>,
}

struct LoadingState {
    phase: crate::views::SimplyLoveContentReloadPhase,
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
    replaygain_done: usize,
    replaygain_total: usize,
    replaygain_skip_requested: bool,
    done: bool,
    started_at: Instant,
    phase_started_at: Instant,
    speed_text_cache: RefCell<Option<SpeedTextCache>>,
}

impl LoadingState {
    fn new() -> Self {
        Self {
            phase: crate::views::SimplyLoveContentReloadPhase::Songs,
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
            replaygain_done: 0,
            replaygain_total: 0,
            replaygain_skip_requested: false,
            done: false,
            started_at: Instant::now(),
            phase_started_at: Instant::now(),
            speed_text_cache: RefCell::new(None),
        }
    }

    /// Switch the active phase, resetting the per-phase timer (used for the ETA
    /// estimate) whenever the phase actually changes so each phase's rate is
    /// measured from its own start rather than from boot.
    fn set_phase(&mut self, phase: crate::views::SimplyLoveContentReloadPhase) {
        if self.phase != phase {
            self.phase = phase;
            self.phase_started_at = Instant::now();
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
    songs_root: PathBuf,
    courses_root: PathBuf,
    pub active_color_index: i32,
    bg: visual_style_bg::State,
}

pub fn init(songs_root: PathBuf, courses_root: PathBuf) -> State {
    State {
        elapsed: 0.0,
        phase: InitPhase::Loading,
        loader_started: false,
        loading: None,
        songs_root,
        courses_root,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
    }
}

#[inline(always)]
fn arc_phase_label(phase: crate::views::SimplyLoveContentReloadPhase) -> Arc<str> {
    match phase {
        crate::views::SimplyLoveContentReloadPhase::Songs => tr("Init", "LoadingSongsText"),
        crate::views::SimplyLoveContentReloadPhase::Courses => tr("Init", "LoadingCoursesText"),
        crate::views::SimplyLoveContentReloadPhase::Artwork => tr("Init", "CachingArtworkText"),
        crate::views::SimplyLoveContentReloadPhase::Noteskins => {
            tr("Init", "CompilingNoteskinsText")
        }
        crate::views::SimplyLoveContentReloadPhase::ReplayGain => {
            tr("Init", "AnalyzingLoudnessText")
        }
    }
}

#[inline(always)]
fn loading_progress_values(loading: &LoadingState) -> (usize, usize, f32) {
    let (done, mut total) = match loading.phase {
        crate::views::SimplyLoveContentReloadPhase::Songs => {
            (loading.songs_done, loading.songs_total)
        }
        crate::views::SimplyLoveContentReloadPhase::Courses => {
            (loading.courses_done, loading.courses_total)
        }
        crate::views::SimplyLoveContentReloadPhase::Artwork => {
            (loading.artwork_done, loading.artwork_total)
        }
        crate::views::SimplyLoveContentReloadPhase::Noteskins => {
            (loading.noteskins_done, loading.noteskins_total)
        }
        crate::views::SimplyLoveContentReloadPhase::ReplayGain => {
            (loading.replaygain_done, loading.replaygain_total)
        }
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

/// Estimated seconds remaining for the current phase, derived from how many
/// items have completed since the phase started. Returns `None` until there's
/// enough signal to project (nothing done yet, or already finished).
fn eta_seconds(done: usize, total: usize, phase_elapsed_s: f32) -> Option<u64> {
    if done == 0 || total <= done || phase_elapsed_s <= 0.0 {
        return None;
    }
    let per_item_s = phase_elapsed_s / done as f32;
    Some((per_item_s * (total - done) as f32).round() as u64)
}

/// Format a remaining-seconds estimate as a compact human string (e.g. `45s`,
/// `2m 13s`, `1h 04m`), matching the ETA readout used by the score-import and
/// profile-save modals. Returns `--` for absurdly large values.
fn format_eta(secs: u64) -> String {
    if secs >= 24 * 60 * 60 {
        return "--".to_owned();
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    let rem_s = secs % 60;
    if mins < 60 {
        return format!("{mins}m {rem_s:02}s");
    }
    let hours = mins / 60;
    let rem_m = mins % 60;
    format!("{hours}h {rem_m:02}m")
}

fn start_loading(state: &mut State) -> ThemeEffect {
    state.loading = Some(LoadingState::new());
    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Content(
        crate::SimplyLoveContentRequest::InitializeLibrary {
            songs_root: state.songs_root.clone(),
            courses_root: state.courses_root.clone(),
        },
    ))
}

pub fn sync_loading_events(
    state: &mut State,
    events: Vec<crate::views::SimplyLoveContentReloadEvent>,
) {
    let Some(loading) = state.loading.as_mut() else {
        return;
    };
    for event in events {
        match event {
            crate::views::SimplyLoveContentReloadEvent::Phase(phase) => {
                loading.set_phase(phase);
                loading.line2 = EMPTY_TEXT.clone();
                loading.line3 = EMPTY_TEXT.clone();
                refresh_loading_count_text(loading);
            }
            crate::views::SimplyLoveContentReloadEvent::Song {
                done,
                total,
                pack,
                song,
            } => {
                loading.set_phase(crate::views::SimplyLoveContentReloadPhase::Songs);
                loading.songs_done = done;
                loading.songs_total = total;
                loading.line2 = Arc::<str>::from(pack);
                loading.line3 = Arc::<str>::from(song);
                refresh_loading_count_text(loading);
            }
            crate::views::SimplyLoveContentReloadEvent::Course {
                done,
                total,
                group,
                course,
            } => {
                loading.set_phase(crate::views::SimplyLoveContentReloadPhase::Courses);
                loading.courses_done = done;
                loading.courses_total = total;
                loading.line2 = Arc::<str>::from(group);
                loading.line3 = Arc::<str>::from(course);
                refresh_loading_count_text(loading);
            }
            crate::views::SimplyLoveContentReloadEvent::Artwork {
                done,
                total,
                line2,
                line3,
            } => {
                loading.set_phase(crate::views::SimplyLoveContentReloadPhase::Artwork);
                loading.artwork_done = done;
                loading.artwork_total = total;
                loading.line2 = Arc::<str>::from(line2);
                loading.line3 = Arc::<str>::from(line3);
                refresh_loading_count_text(loading);
            }
            crate::views::SimplyLoveContentReloadEvent::Noteskins {
                done,
                total,
                skin,
                status,
            } => {
                loading.set_phase(crate::views::SimplyLoveContentReloadPhase::Noteskins);
                loading.noteskins_done = done;
                loading.noteskins_total = total;
                loading.line2 = Arc::<str>::from(skin);
                loading.line3 = Arc::<str>::from(status);
                refresh_loading_count_text(loading);
            }
            crate::views::SimplyLoveContentReloadEvent::ReplayGain {
                done,
                total,
                line2,
                line3,
            } => {
                loading.set_phase(crate::views::SimplyLoveContentReloadPhase::ReplayGain);
                loading.replaygain_done = done;
                loading.replaygain_total = total;
                loading.line2 = Arc::<str>::from(line2);
                loading.line3 = Arc::<str>::from(line3);
                refresh_loading_count_text(loading);
            }
            crate::views::SimplyLoveContentReloadEvent::Finished { .. } => {
                loading.done = true;
                refresh_loading_count_text(loading);
            }
        }
    }
}

/* -------------------------- input -> nav ----------------------- */

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    if state.phase == InitPhase::Loading {
        // While loading, Start/Back skips only the ReplayGain (loudness) pass,
        // which is the last and longest phase; the rest of the library is
        // already usable and the skipped songs get analyzed lazily on preview.
        if ev.pressed && is_start_or_back(ev.action) {
            if let Some(loading) = state.loading.as_mut()
                && loading.phase == crate::views::SimplyLoveContentReloadPhase::ReplayGain
                && !loading.replaygain_skip_requested
            {
                loading.replaygain_skip_requested = true;
                return ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Content(
                    crate::SimplyLoveContentRequest::SkipReplayGain,
                ));
            }
        }
        return ThemeEffect::None;
    }
    if ev.pressed && is_start_or_back(ev.action) {
        return ThemeEffect::Navigate(Screen::Menu);
    }
    ThemeEffect::None
}

#[inline(always)]
fn is_start_or_back(action: VirtualAction) -> bool {
    matches!(
        action,
        VirtualAction::p1_start
            | VirtualAction::p1_back
            | VirtualAction::p2_start
            | VirtualAction::p2_back
    )
}

/* ---------------------------- update --------------------------- */

pub fn update(state: &mut State, dt: f32) -> ThemeEffect {
    if state.phase == InitPhase::Loading {
        if !state.loader_started {
            state.loader_started = true;
            return start_loading(state);
        }

        let done = state.loading.as_ref().is_some_and(|loading| loading.done);

        if done {
            state.loading = None;
            state.phase = InitPhase::Playing;
            state.elapsed = 0.0;
        }
        return ThemeEffect::None;
    }

    state.elapsed += dt.max(0.0);

    if state.phase == InitPhase::Playing && state.elapsed >= arrows_finished_at() {
        state.phase = InitPhase::FadingOut;
        state.elapsed = arrows_finished_at();
    }

    if state.phase == InitPhase::FadingOut {
        let fade_elapsed = state.elapsed - arrows_finished_at();
        if fade_elapsed >= BAR_SQUISH_DURATION {
            return ThemeEffect::Navigate(Screen::Menu);
        }
    }
    ThemeEffect::None
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

fn push_loading_overlay(
    state: &State,
    actors: &mut Vec<Actor>,
    loading_elapsed_s: f32,
    machine_font: crate::config::MachineFont,
) {
    let loading = state.loading.as_ref();
    let phase = loading
        .map(|loading| loading.phase)
        .unwrap_or(crate::views::SimplyLoveContentReloadPhase::Songs);
    let (done, total, progress) = loading_progress(loading);
    let show_speed_row = total > 0;
    let speed_text = loading.filter(|_| show_speed_row).map(|loading| {
        let speed = speed_text(loading, done, loading_elapsed_s.max(0.0));
        let phase_elapsed_s = loading.phase_started_at.elapsed().as_secs_f32().max(0.0);
        match eta_seconds(done, total, phase_elapsed_s) {
            Some(eta_secs) => Arc::<str>::from(format!(
                "{speed}  \u{2022}  {}",
                tr_fmt(
                    "OptionsScoreImport",
                    "ImportEta",
                    &[("eta", &format_eta(eta_secs))]
                ),
            )),
            None => speed,
        }
    });
    let show_skip_hint = matches!(
        phase,
        crate::views::SimplyLoveContentReloadPhase::ReplayGain
    ) && loading
        .is_some_and(|loading| !loading.done && !loading.replaygain_skip_requested);
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
    let title_font = machine_font_key_for_text(machine_font, FontRole::Header, &title_text);
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

    if show_skip_hint {
        actors.push(act!(text:
            font("miso"):
            settext("Press &START; to skip".to_owned()):
            align(0.5, 0.5):
            xy(screen_center_x(), bar_cy + 58.0):
            zoom(0.8):
            diffuse(0.7, 0.7, 0.7, 1.0):
            horizalign(center):
            z(110.0)
        ));
    }
}

/* --------------------------- combined build --------------------------- */

fn push_actors_with_elapsed_overrides(
    actors: &mut Vec<Actor>,
    state: &State,
    loading_elapsed_override: Option<f32>,
    bg_elapsed_override: Option<f32>,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(32 + ARROW_COUNT);

    /* 1) HEART BACKGROUND — visible immediately */
    let bg_params = visual_style_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
        visual_policy,
    };
    if let Some(bg_elapsed_s) = bg_elapsed_override {
        state
            .bg
            .push_at_elapsed(actors, bg_params, f64::from(bg_elapsed_s));
    } else {
        state.bg.push(actors, bg_params);
    }

    if state.phase == InitPhase::Loading {
        let loading_elapsed_s = loading_elapsed_override.unwrap_or_else(|| {
            state.loading.as_ref().map_or(0.0, |loading| {
                loading.started_at.elapsed().as_secs_f32().max(0.0)
            })
        });
        push_loading_overlay(state, actors, loading_elapsed_s, visual_policy.machine_font);
        return;
    }

    /* If we’re still in pre-roll, stop here: no squish/backdrop/arrows yet. */
    if state.elapsed < PRE_ROLL {
        return;
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
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    push_actors_with_elapsed_overrides(actors, state, None, None, visual_policy);
}

pub fn get_actors(state: &State) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(32 + ARROW_COUNT);
    push_actors(&mut actors, state, Default::default());
    actors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_loading_emits_shell_request() {
        let mut state = init(PathBuf::from("Songs"), PathBuf::from("Courses"));

        let effect = update(&mut state, 0.0);

        assert!(matches!(
            effect,
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Content(
                crate::SimplyLoveContentRequest::InitializeLibrary {
                    songs_root,
                    courses_root,
                }
            )) if songs_root == PathBuf::from("Songs")
                && courses_root == PathBuf::from("Courses")
        ));
        assert!(state.loading.is_some());
    }

    #[test]
    fn shell_events_drive_startup_progress_and_completion() {
        let mut state = init(PathBuf::from("Songs"), PathBuf::from("Courses"));
        let _ = update(&mut state, 0.0);

        sync_loading_events(
            &mut state,
            vec![
                crate::views::SimplyLoveContentReloadEvent::Artwork {
                    done: 4,
                    total: 9,
                    line2: "Pack".to_owned(),
                    line3: "Song".to_owned(),
                },
                crate::views::SimplyLoveContentReloadEvent::Noteskins {
                    done: 2,
                    total: 3,
                    skin: "metal".to_owned(),
                    status: "built".to_owned(),
                },
                crate::views::SimplyLoveContentReloadEvent::Finished {
                    song_packs: Vec::new(),
                },
            ],
        );

        let loading = state
            .loading
            .as_ref()
            .expect("loading chrome should remain");
        assert_eq!((loading.artwork_done, loading.artwork_total), (4, 9));
        assert_eq!((loading.noteskins_done, loading.noteskins_total), (2, 3));
        assert_eq!(
            (loading.line2.as_ref(), loading.line3.as_ref()),
            ("metal", "built")
        );
        assert!(loading.done);

        assert!(matches!(update(&mut state, 0.0), ThemeEffect::None));
        assert!(state.loading.is_none());
        assert!(matches!(state.phase, InitPhase::Playing));
    }

    fn press(action: VirtualAction) -> InputEvent {
        let now = std::time::Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed: true,
            source: deadsync_core::input::InputSource::Gamepad,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    #[test]
    fn start_skips_only_during_replaygain_phase() {
        let mut state = init(PathBuf::from("Songs"), PathBuf::from("Courses"));
        let _ = update(&mut state, 0.0);

        // Earlier phases ignore Start; nothing is skipped.
        sync_loading_events(
            &mut state,
            vec![crate::views::SimplyLoveContentReloadEvent::Artwork {
                done: 1,
                total: 10,
                line2: "Pack".to_owned(),
                line3: "Song".to_owned(),
            }],
        );
        assert!(matches!(
            handle_input(&mut state, &press(VirtualAction::p1_start)),
            ThemeEffect::None
        ));

        // Once in the ReplayGain phase, Start requests a skip exactly once.
        sync_loading_events(
            &mut state,
            vec![crate::views::SimplyLoveContentReloadEvent::ReplayGain {
                done: 2,
                total: 100,
                line2: "Pack".to_owned(),
                line3: "Song".to_owned(),
            }],
        );
        assert!(matches!(
            handle_input(&mut state, &press(VirtualAction::p1_start)),
            ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Content(
                crate::SimplyLoveContentRequest::SkipReplayGain,
            ))
        ));
        // A second press is a no-op (skip already requested).
        assert!(matches!(
            handle_input(&mut state, &press(VirtualAction::p1_back)),
            ThemeEffect::None
        ));
    }

    #[test]
    fn eta_seconds_projects_remaining_time() {
        // 2 done in 4s => 2s/item; 3 remaining => 6s.
        assert_eq!(eta_seconds(2, 5, 4.0), Some(6));
        // 10 done in 300s => 30s/item; 2 remaining => 60s.
        assert_eq!(eta_seconds(10, 12, 300.0), Some(60));
        // No projection before any progress or once complete.
        assert!(eta_seconds(0, 5, 4.0).is_none());
        assert!(eta_seconds(5, 5, 4.0).is_none());
    }

    #[test]
    fn format_eta_matches_modal_style() {
        assert_eq!(format_eta(6), "6s");
        assert_eq!(format_eta(60), "1m 00s");
        assert_eq!(format_eta(133), "2m 13s");
        assert_eq!(format_eta(3840), "1h 04m");
        assert_eq!(format_eta(24 * 60 * 60), "--");
    }
}
