use crate::SimplyLoveEffect as ThemeEffect;
use crate::act;
use crate::color;
use crate::fonts::machine_font_key_for_text;
use crate::i18n::{tr, tr_fmt};
use crate::screens::Screen;
use crate::screens::components::shared::{loading_bar, visual_style_bg};
use deadlib_present::actors::Actor;
use deadlib_present::space::{
    screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use deadsync_assets::progress::{ContentReloadPhase, LibraryLoadView};
use deadsync_config::app_config::Config;
use deadsync_theme::startup::StartupPresentation;

use crate::views::SimplyLoveVisualPolicyView;
use deadsync_theme::FontRole;
use std::cell::RefCell;
use std::sync::{Arc, LazyLock};

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

struct CountTextCache {
    done: usize,
    total: usize,
    finished: bool,
    text: Arc<str>,
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
    Playing,
    FadingOut,
}

pub struct State {
    elapsed: f32,
    phase: InitPhase,
    active_color_index: i32,
    bg: visual_style_bg::State,
    count_text_cache: RefCell<Option<CountTextCache>>,
    speed_text_cache: RefCell<Option<SpeedTextCache>>,
}

impl State {
    #[must_use]
    pub const fn new(color_index: i32) -> Self {
        Self {
            elapsed: 0.0,
            phase: InitPhase::Playing,
            active_color_index: color_index,
            bg: visual_style_bg::State::new(),
            count_text_cache: RefCell::new(None),
            speed_text_cache: RefCell::new(None),
        }
    }

    fn count_text(&self, loading: &LibraryLoadView) -> Arc<str> {
        let LibraryLoadView {
            done,
            total,
            finished,
            ..
        } = *loading;
        if let Some(cache) = self.count_text_cache.borrow().as_ref()
            && (cache.done, cache.total, cache.finished) == (done, total, finished)
        {
            return cache.text.clone();
        }
        let text = if finished || (total > 0 && done >= total) {
            tr("Init", "DoneText")
        } else if total == 0 {
            EMPTY_TEXT.clone()
        } else {
            Arc::<str>::from(crate::screens::progress_count_text(done, total))
        };
        *self.count_text_cache.borrow_mut() = Some(CountTextCache {
            done,
            total,
            finished,
            text: text.clone(),
        });
        text
    }
}

impl StartupPresentation<Actor, LibraryLoadView, Config> for State {
    fn reset_intro(&mut self, color_index: i32) {
        self.elapsed = 0.0;
        self.phase = InitPhase::Playing;
        self.active_color_index = color_index;
    }

    fn advance_intro(&mut self, delta_time: f32) -> ThemeEffect {
        update(self, delta_time)
    }

    fn push_loading(
        &self,
        actors: &mut Vec<Actor>,
        progress: Option<&LibraryLoadView>,
        config: &Config,
    ) {
        let visual_policy = SimplyLoveVisualPolicyView::from_config(config);
        push_background(actors, self, visual_policy);
        push_loading_overlay(self, actors, progress, visual_policy.machine_font);
    }

    fn push_intro(&self, actors: &mut Vec<Actor>, config: &Config) {
        push_intro(
            actors,
            self,
            SimplyLoveVisualPolicyView::from_config(config),
        );
    }
}

#[inline(always)]
fn arc_phase_label(phase: ContentReloadPhase) -> Arc<str> {
    match phase {
        ContentReloadPhase::Songs => tr("Init", "LoadingSongsText"),
        ContentReloadPhase::Courses => tr("Init", "LoadingCoursesText"),
        ContentReloadPhase::Artwork => tr("Init", "CachingArtworkText"),
        ContentReloadPhase::Noteskins => tr("Init", "CompilingNoteskinsText"),
        ContentReloadPhase::ReplayGain => tr("Init", "AnalyzingLoudnessText"),
    }
}

fn speed_text(state: &State, done: usize, elapsed_s: f32) -> Arc<str> {
    let tenths_per_sec = if elapsed_s > 0.0 {
        ((done as f32 / elapsed_s) * 10.0).round().max(0.0) as u32
    } else {
        0
    };
    if let Some(cache) = state.speed_text_cache.borrow().as_ref()
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
    *state.speed_text_cache.borrow_mut() = Some(SpeedTextCache {
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

fn update(state: &mut State, dt: f32) -> ThemeEffect {
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

#[must_use]
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

#[must_use]
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

fn loading_progress(loading: &LibraryLoadView) -> f32 {
    if loading.total == 0 {
        return 0.0;
    }
    let progress = loading.done as f32 / loading.total as f32;
    progress.min(if loading.finished { 1.0 } else { 0.999 })
}

fn push_loading_overlay(
    state: &State,
    actors: &mut Vec<Actor>,
    loading: Option<&LibraryLoadView>,
    machine_font: deadsync_config::theme::MachineFont,
) {
    let speed_text = loading.filter(|loading| loading.total > 0).map(|loading| {
        let speed = speed_text(
            state,
            loading.done,
            loading.started_at.elapsed().as_secs_f32(),
        );
        let phase_elapsed_s = loading.phase_started_at.elapsed().as_secs_f32();
        match eta_seconds(loading.done, loading.total, phase_elapsed_s) {
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
    let show_skip_hint = loading.is_some_and(|loading| loading.can_skip_replaygain);
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
        |loading| arc_phase_label(loading.phase),
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
        progress: loading.map_or(0.0, loading_progress),
        label: loading
            .map_or_else(|| EMPTY_TEXT.clone(), |loading| state.count_text(loading))
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
            settext(crate::i18n::tr("Init", "SkipHint")):
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

fn push_background(
    actors: &mut Vec<Actor>,
    state: &State,
    visual_policy: SimplyLoveVisualPolicyView,
) {
    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
            visual_policy,
        },
    );
}

fn push_intro(actors: &mut Vec<Actor>, state: &State, visual_policy: SimplyLoveVisualPolicyView) {
    actors.reserve(32 + ARROW_COUNT);
    push_background(actors, state, visual_policy);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_intro_returns_to_menu() {
        let mut state = State::new(2);
        assert!(matches!(
            state.advance_intro(arrows_finished_at()),
            ThemeEffect::None
        ));
        assert!(matches!(state.phase, InitPhase::FadingOut));

        state.reset_intro(7);

        assert!(matches!(state.phase, InitPhase::Playing));
        assert_eq!(state.active_color_index, 7);
        assert!(matches!(state.advance_intro(0.0), ThemeEffect::None));
        assert!(matches!(
            state.advance_intro(arrows_finished_at()),
            ThemeEffect::None
        ));
        assert!(matches!(state.phase, InitPhase::FadingOut));
        assert!(matches!(
            state.advance_intro(BAR_SQUISH_DURATION + 0.001),
            ThemeEffect::Navigate(Screen::Menu)
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
