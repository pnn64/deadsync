use crate::act;
use crate::assets::AssetManager;
use crate::core::gfx::{BlendMode, MeshMode};
use crate::core::space::widescale;
use crate::core::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::profile;
use crate::screens::components::gameplay::{gameplay_stats, notefield};
use crate::screens::components::shared::screen_bar::{self, AvatarParams, ScreenBarParams};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::compose::TextLayoutCache;
use crate::ui::font;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::thread::LocalKey;

const TEXT_CACHE_LIMIT: usize = 8192;
type TextCache<K> = HashMap<K, Arc<str>>;
const INTRO_TEXT_SETTLE_SECONDS: f32 = 1.49; // 0.5 + 0.66 + 0.33 (SL OnCommand chain)

pub use crate::game::gameplay::{State, init, update};
use crate::game::gameplay::{
    TRANSITION_IN_DURATION, TRANSITION_OUT_DELAY, TRANSITION_OUT_DURATION,
    TRANSITION_OUT_FADE_DURATION, timing_tick_status_line, toggle_flash_text,
};

thread_local! {
    static SCORE_2DP_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(1024));
    static RATE_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(128));
    static BPM_TEXT_CACHE: RefCell<TextCache<(u64, bool)>> = RefCell::new(HashMap::with_capacity(512));
    static LIFE_PERCENT_TEXT_CACHE: RefCell<TextCache<u32>> =
        RefCell::new(HashMap::with_capacity(1024));
    static METER_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(64));
    static SYNC_OVERLAY_CACHE: RefCell<TextCache<SyncOverlayTextKey>> =
        RefCell::new(HashMap::with_capacity(256));
    static AUTOSYNC_TEXT_CACHE: RefCell<TextCache<AutosyncTextKey>> =
        RefCell::new(HashMap::with_capacity(256));
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct SyncOverlayTextKey {
    replay_tag: u8,
    replay_ptr: usize,
    replay_len: usize,
    timing_ptr: usize,
    timing_len: usize,
    autosync_ptr: usize,
    autosync_len: usize,
    message_ptr: usize,
    message_len: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct AutosyncTextKey {
    mode: u8,
    old_offset_bits: u32,
    new_offset_bits: u32,
    stddev_bits: u32,
    sample_count: u16,
}

#[inline(always)]
fn str_key(line: &str) -> (usize, usize) {
    (line.as_ptr() as usize, line.len())
}

#[inline(always)]
fn empty_text() -> Arc<str> {
    static EMPTY: OnceLock<Arc<str>> = OnceLock::new();
    EMPTY.get_or_init(|| Arc::<str>::from("")).clone()
}

#[inline(always)]
fn cached_text<K, F>(cache: &'static LocalKey<RefCell<TextCache<K>>>, key: K, build: F) -> Arc<str>
where
    K: Copy + Eq + std::hash::Hash,
    F: FnOnce() -> String,
{
    cache.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(text) = cache.get(&key) {
            return text.clone();
        }
        let text: Arc<str> = Arc::<str>::from(build());
        if cache.len() < TEXT_CACHE_LIMIT {
            cache.insert(key, text.clone());
        }
        text
    })
}

#[inline(always)]
fn quantize_centi_u32(value: f64) -> u32 {
    let value = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    ((value * 100.0).round()).clamp(0.0, u32::MAX as f64) as u32
}

#[inline(always)]
fn quantize_tenths_u32(value: f32) -> u32 {
    let value = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    ((value * 10.0).round()).clamp(0.0, u32::MAX as f32) as u32
}

#[inline(always)]
fn cached_score_2dp(value: f64) -> Arc<str> {
    let key = quantize_centi_u32(value);
    cached_text(&SCORE_2DP_CACHE, key, || {
        format!("{:.2}", key as f64 / 100.0)
    })
}

#[inline(always)]
fn cached_rate_text(rate: f32) -> Arc<str> {
    if (rate - 1.0).abs() <= 0.001 {
        return empty_text();
    }
    cached_text(&RATE_TEXT_CACHE, rate.to_bits(), || {
        format!("{rate:.2}x rate")
    })
}

#[inline(always)]
fn cached_bpm_text(bpm: f64, show_decimal: bool) -> Arc<str> {
    if !bpm.is_finite() {
        return Arc::<str>::from("0");
    }
    if !show_decimal {
        let rounded = bpm.round().max(0.0);
        let key = (rounded.to_bits(), false);
        return cached_text(&BPM_TEXT_CACHE, key, || format!("{rounded:.0}"));
    }
    let rounded_tenth = (bpm * 10.0).round() / 10.0;
    let rounded_tenth = rounded_tenth.max(0.0);
    let key = (rounded_tenth.to_bits(), true);
    cached_text(&BPM_TEXT_CACHE, key, || {
        let nearest_int = rounded_tenth.round();
        if (rounded_tenth - nearest_int).abs() <= 0.001 {
            format!("{nearest_int:.0}")
        } else {
            format!("{rounded_tenth:.1}")
        }
    })
}

#[inline(always)]
fn cached_life_percent_text(life_percent: f32) -> Arc<str> {
    let key = quantize_tenths_u32(life_percent);
    cached_text(&LIFE_PERCENT_TEXT_CACHE, key, || {
        format!("{:.1}%", key as f32 / 10.0)
    })
}

#[inline(always)]
fn cached_meter_text(meter: u32) -> Arc<str> {
    cached_text(&METER_TEXT_CACHE, meter, || meter.to_string())
}

fn sync_overlay_text(state: &State) -> Option<(Arc<str>, usize)> {
    let mut lines = [""; 4];
    let mut line_count = 0usize;
    let mut total_len = 0usize;
    if state.autoplay_enabled {
        let line = state.replay_status_text.as_deref().unwrap_or("AutoPlay");
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if let Some(line) = timing_tick_status_line(state) {
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if let Some(line) = crate::game::gameplay::autosync_mode_status_line(state.autosync_mode) {
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if let Some(line) = state.sync_overlay_message.as_deref() {
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if line_count == 0 {
        return None;
    }
    let replay_line = if state.autoplay_enabled {
        Some(state.replay_status_text.as_deref().unwrap_or("AutoPlay"))
    } else {
        None
    };
    let timing_line = timing_tick_status_line(state);
    let autosync_line = crate::game::gameplay::autosync_mode_status_line(state.autosync_mode);
    let message_line = state.sync_overlay_message.as_deref();
    let (replay_ptr, replay_len, replay_tag) = if let Some(line) = replay_line {
        let (ptr, len) = str_key(line);
        (
            ptr,
            len,
            if state.replay_status_text.is_some() {
                2
            } else {
                1
            },
        )
    } else {
        (0, 0, 0)
    };
    let (timing_ptr, timing_len) = timing_line.map_or((0, 0), str_key);
    let (autosync_ptr, autosync_len) = autosync_line.map_or((0, 0), str_key);
    let (message_ptr, message_len) = message_line.map_or((0, 0), str_key);
    let key = SyncOverlayTextKey {
        replay_tag,
        replay_ptr,
        replay_len,
        timing_ptr,
        timing_len,
        autosync_ptr,
        autosync_len,
        message_ptr,
        message_len,
    };
    let text = cached_text(&SYNC_OVERLAY_CACHE, key, || {
        let mut out = String::with_capacity(total_len + line_count.saturating_sub(1));
        out.push_str(lines[0]);
        for line in &lines[1..line_count] {
            out.push('\n');
            out.push_str(line);
        }
        out
    });
    Some((text, line_count))
}

#[inline(always)]
fn cached_autosync_text(state: &State, old_offset: f32, new_offset: f32) -> Arc<str> {
    let key = AutosyncTextKey {
        mode: state.autosync_mode as u8,
        old_offset_bits: old_offset.to_bits(),
        new_offset_bits: new_offset.to_bits(),
        stddev_bits: state.autosync_standard_deviation.to_bits(),
        sample_count: state.autosync_offset_sample_count.min(u16::MAX as usize) as u16,
    };
    cached_text(&AUTOSYNC_TEXT_CACHE, key, || {
        let collecting_sample = state
            .autosync_offset_sample_count
            .saturating_add(1)
            .min(crate::game::gameplay::AUTOSYNC_OFFSET_SAMPLE_COUNT);
        format!(
            "Old offset: {old_offset:0.3}\nNew offset: {new_offset:0.3}\nStandard deviation: {stddev:0.3}\nCollecting sample: {collecting_sample} / {max_samples}",
            stddev = state.autosync_standard_deviation,
            max_samples = crate::game::gameplay::AUTOSYNC_OFFSET_SAMPLE_COUNT,
        )
    })
}

pub fn prewarm_text_layout(
    cache: &mut TextLayoutCache,
    fonts: &HashMap<&'static str, font::Font>,
    state: &State,
) {
    let cfg = crate::config::get();
    for centi in 0..=10_000 {
        let text = cached_score_2dp(centi as f64 / 100.0);
        cache.prewarm_text(fonts, "wendy_monospace_numbers", text.as_ref(), None);
    }
    for tenths in 0..=1_000 {
        let text = cached_life_percent_text(tenths as f32 / 10.0);
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
    for player in 0..state.num_players {
        let chart = &state.charts[player];
        let meter_text = cached_meter_text(chart.meter);
        cache.prewarm_text(fonts, "wendy", meter_text.as_ref(), None);
        let detail = color::difficulty_display_name_for_song(
            &chart.difficulty,
            &state.song.title,
            cfg.zmod_rating_box_text,
        );
        cache.prewarm_text(fonts, "miso", detail, None);
        for &(_, bpm) in &chart.timing_segments.bpms {
            let text = cached_bpm_text(
                f64::from(bpm.max(0.0)) * f64::from(state.music_rate),
                cfg.show_bpm_decimal,
            );
            cache.prewarm_text(fonts, "miso", text.as_ref(), None);
        }
    }
    cache.prewarm_text(fonts, "miso", "Assist Tick", None);
    cache.prewarm_text(fonts, "miso", "Hit Tick", None);
    cache.prewarm_text(fonts, "miso", "AutoSync Song", None);
    cache.prewarm_text(fonts, "miso", "AutoSync Machine", None);
    cache.prewarm_text(fonts, "miso", "Continue holding &START; to give up", None);
    cache.prewarm_text(fonts, "miso", "Continue holding &BACK; to give up", None);
    cache.prewarm_text(fonts, "miso", "Don't go back!", None);
    if let Some(text) = state.replay_status_text.as_ref() {
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
    if let Some(text) = state.sync_overlay_message.as_ref() {
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
    if state.autosync_mode != crate::game::gameplay::AutosyncMode::Off {
        let (old_offset, new_offset) =
            if state.autosync_mode == crate::game::gameplay::AutosyncMode::Machine {
                (
                    state.initial_global_offset_seconds,
                    state.global_offset_seconds,
                )
            } else {
                (state.initial_song_offset_seconds, state.song_offset_seconds)
            };
        let text = cached_autosync_text(state, old_offset, new_offset);
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
}

// --- TRANSITIONS ---
pub fn in_transition(state: Option<&State>) -> (Vec<Actor>, f32) {
    let text = state
        .map(|gs| gs.stage_intro_text.clone())
        .unwrap_or_else(|| Arc::from("EVENT"));
    let intro_color = state.map_or(color::decorative_rgba(0), |gs| gs.player_color);
    let mut mirrored_splode = act!(sprite("gameplayin_splode.png"):
        align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
        diffuse(intro_color[0], intro_color[1], intro_color[2], 0.8):
        rotationz(-10.0): zoom(0.0):
        z(1101):
        sleep(0.4):
        decelerate(0.6): rotationz(0.0): zoom(1.3): alpha(0.0)
    );
    if let Actor::Sprite { flip_x, .. } = &mut mirrored_splode {
        // Simply Love uses rotationy(180) here; in deadsync 2D parity this is horizontal mirroring.
        *flip_x = true;
    }

    let actors = vec![
        act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(1100):
            sleep(1.4):
            accelerate(0.6): alpha(0.0):
            linear(0.0): visible(false)
        ),
        act!(sprite("gameplayin_splode.png"):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            diffuse(intro_color[0], intro_color[1], intro_color[2], 0.9):
            rotationz(10.0): zoom(0.0):
            z(1101):
            sleep(0.4):
            linear(0.6): rotationz(0.0): zoom(1.1): alpha(0.0)
        ),
        mirrored_splode,
        act!(sprite("gameplayin_minisplode.png"):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            diffuse(intro_color[0], intro_color[1], intro_color[2], 1.0):
            rotationz(10.0): zoom(0.0):
            z(1101):
            sleep(0.4):
            decelerate(0.8): rotationz(0.0): zoom(0.9): alpha(0.0)
        ),
        act!(text:
            font("wendy"): settext(text):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, 0.0):
            z(1102):
            accelerate(0.5): alpha(1.0):
            sleep(0.66):
            accelerate(0.33): zoom(0.4): y(screen_height() - 30.0):
            sleep((TRANSITION_IN_DURATION - INTRO_TEXT_SETTLE_SECONDS).max(0.0))
        ),
    ];
    (actors, TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        sleep(TRANSITION_OUT_DELAY):
        linear(TRANSITION_OUT_FADE_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

// --- DRAWING ---

fn build_background(state: &State, bg_brightness: f32) -> Actor {
    let sw = screen_width();
    let sh = screen_height();
    let screen_aspect = if sh > 0.0 { sw / sh } else { 16.0 / 9.0 };
    let bg_brightness = bg_brightness.clamp(0.0, 1.0);

    let (tex_w, tex_h) =
        if let Some(meta) = crate::assets::texture_dims(&state.background_texture_key) {
            (meta.w as f32, meta.h as f32)
        } else {
            (1.0, 1.0) // fallback, will just fill screen
        };

    let tex_aspect = if tex_h > 0.0 { tex_w / tex_h } else { 1.0 };

    if screen_aspect > tex_aspect {
        // screen is wider, match width to cover
        act!(sprite(state.background_texture_key.clone()):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            zoomtowidth(sw):
            diffuse(bg_brightness, bg_brightness, bg_brightness, 1.0):
            z(-100)
        )
    } else {
        // screen is taller/equal, match height to cover
        act!(sprite(state.background_texture_key.clone()):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            zoomtoheight(sh):
            diffuse(bg_brightness, bg_brightness, bg_brightness, 1.0):
            z(-100)
        )
    }
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let cfg = crate::config::get();
    let hud_snapshot = profile::gameplay_hud_snapshot();
    let mut actors = Vec::with_capacity(96);
    let play_style = hud_snapshot.play_style;
    let player_side = hud_snapshot.player_side;
    let is_p2_single =
        play_style == profile::PlayStyle::Single && player_side == profile::PlayerSide::P2;
    let centered_single_notefield = play_style == profile::PlayStyle::Single
        && state.num_players == 1
        && cfg.center_1player_notefield;
    let player_color = if is_p2_single {
        color::decorative_rgba(state.active_color_index - 2)
    } else {
        state.player_color
    };
    // --- Background and Filter ---
    let hide_song_bg = state
        .player_profiles
        .iter()
        .take(state.num_players)
        .any(|p| p.hide_song_bg);
    if hide_song_bg {
        actors.push(act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(-100)
        ));
    } else {
        actors.push(build_background(state, cfg.bg_brightness));
    }

    // ITGmania/Simply Love parity: ScreenSyncOverlay status text.
    {
        let status_line_count = if let Some((status_text, line_count)) = sync_overlay_text(state) {
            actors.push(act!(text:
                font("miso"):
                settext(status_text):
                align(0.5, 0.5):
                xy(screen_center_x(), screen_center_y() + 150.0):
                shadowlength(2.0):
                strokecolor(0.0, 0.0, 0.0, 1.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(901)
            ));
            line_count
        } else {
            0
        };

        if let Some((flash, alpha)) = toggle_flash_text(state) {
            let y = if status_line_count == 0 {
                screen_center_y() + 150.0
            } else {
                screen_center_y() + 150.0 + 20.0 * status_line_count as f32
            };
            actors.push(act!(text:
                font("miso"):
                settext(flash):
                align(0.5, 0.5):
                xy(screen_center_x(), y):
                shadowlength(2.0):
                strokecolor(0.0, 0.0, 0.0, alpha):
                diffuse(1.0, 1.0, 1.0, alpha):
                z(901)
            ));
        }

        if state.autosync_mode != crate::game::gameplay::AutosyncMode::Off {
            let (old_offset, new_offset) =
                if state.autosync_mode == crate::game::gameplay::AutosyncMode::Machine {
                    (
                        state.initial_global_offset_seconds,
                        state.global_offset_seconds,
                    )
                } else {
                    (state.initial_song_offset_seconds, state.song_offset_seconds)
                };
            let adjustments = cached_autosync_text(state, old_offset, new_offset);
            actors.push(act!(text:
                font("miso"):
                settext(adjustments):
                align(0.5, 0.5):
                xy(screen_center_x() + 160.0, screen_center_y()):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(901)
            ));
        }
    }

    // Hold START/BACK prompt (Simply Love parity: ScreenGameplay debug text).
    {
        const HOLD_FADE_IN_S: f32 = 1.0 / 8.0;
        const ABORT_FADE_OUT_S: f32 = 0.5;

        let y = screen_height() - 116.0;
        let msg =
            if let (Some(key), Some(start)) = (state.hold_to_exit_key, state.hold_to_exit_start) {
                let s = match key {
                    crate::game::gameplay::HoldToExitKey::Start => {
                        Some("Continue holding &START; to give up")
                    }
                    crate::game::gameplay::HoldToExitKey::Back => {
                        Some("Continue holding &BACK; to give up")
                    }
                };
                let alpha = (start.elapsed().as_secs_f32() / HOLD_FADE_IN_S).clamp(0.0, 1.0);
                s.map(|text| (text, alpha))
            } else if let Some(exit) = &state.exit_transition {
                let t = exit.started_at.elapsed().as_secs_f32();
                match exit.kind {
                    crate::game::gameplay::ExitTransitionKind::Out => {
                        let alpha = (1.0 - t / ABORT_FADE_OUT_S).clamp(0.0, 1.0);
                        Some(("Continue holding &START; to give up", alpha))
                    }
                    crate::game::gameplay::ExitTransitionKind::Cancel => {
                        Some(("Continue holding &BACK; to give up", 1.0))
                    }
                }
            } else if let Some(at) = state.hold_to_exit_aborted_at {
                let alpha = (1.0 - at.elapsed().as_secs_f32() / ABORT_FADE_OUT_S).clamp(0.0, 1.0);
                Some(("Don't go back!", alpha))
            } else {
                None
            };

        if let Some((text, alpha)) = msg
            && alpha > 0.0
        {
            actors.push(act!(text:
                font("miso"):
                settext(text):
                align(0.5, 0.5):
                xy(screen_center_x(), y):
                zoom(0.75):
                shadowlength(2.0):
                diffuse(1.0, 1.0, 1.0, alpha):
                z(1000)
            ));
        }
    }

    // Fade-to-black when giving up / backing out (Simply Love parity).
    if let Some(exit) = &state.exit_transition {
        let alpha = crate::game::gameplay::exit_transition_alpha(exit);
        if alpha > 0.0 {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(0.0, 0.0):
                zoomto(screen_width(), screen_height()):
                diffuse(0.0, 0.0, 0.0, alpha):
                z(1500)
            ));
        }
    }

    let notefield_width = |player_idx: usize| -> f32 {
        let Some(ns) = state.noteskin[player_idx].as_ref() else {
            return 256.0;
        };
        let cols = state
            .cols_per_player
            .min(ns.column_xs.len())
            .min(ns.receptor_off.len());
        if cols == 0 {
            return 256.0;
        }
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        for x in ns.column_xs.iter().take(cols) {
            let xf = *x as f32;
            min_x = min_x.min(xf);
            max_x = max_x.max(xf);
        }

        // SL parity (GetNotefieldWidth): layout width is style/lane based and must
        // not shrink/grow with Mini (field zoom).
        let target_arrow_px = 64.0;
        let size = ns.receptor_off[0].size();
        let w = size[0].max(0) as f32;
        let h = size[1].max(0) as f32;
        let arrow_w = if h > 0.0 && target_arrow_px > 0.0 {
            w * (target_arrow_px / h)
        } else {
            w
        };
        (max_x - min_x) + arrow_w
    };

    let (p1_actors, p2_actors, playfield_center_x, per_player_fields): (
        Vec<Actor>,
        Option<Vec<Actor>>,
        f32,
        [(usize, f32); 2],
    ) = match play_style {
        profile::PlayStyle::Versus => {
            let (p1, p1_x) = notefield::build(
                state,
                &state.player_profiles[0],
                notefield::FieldPlacement::P1,
                play_style,
                cfg.center_1player_notefield,
            );
            let (p2, p2_x) = notefield::build(
                state,
                &state.player_profiles[1],
                notefield::FieldPlacement::P2,
                play_style,
                cfg.center_1player_notefield,
            );
            (p1, Some(p2), p1_x, [(0, p1_x), (1, p2_x)])
        }
        _ => {
            let placement = if is_p2_single {
                notefield::FieldPlacement::P2
            } else {
                notefield::FieldPlacement::P1
            };
            let (nf, nf_x) = notefield::build(
                state,
                &state.player_profiles[0],
                placement,
                play_style,
                cfg.center_1player_notefield,
            );
            (nf, None, nf_x, [(0, nf_x), (usize::MAX, 0.0)])
        }
    };

    // Danger overlay (Simply Love parity): red flashing in danger + green recovery, optional HideDanger.
    {
        let sw = screen_width();
        let sh = screen_height();
        let cx = screen_center_x();

        for player_idx in 0..state.num_players {
            let Some(rgba) = crate::game::gameplay::danger_overlay_rgba(state, player_idx) else {
                continue;
            };
            let (x, w, fl, fr) = match play_style {
                profile::PlayStyle::Double => (0.0, sw, 0.0, 0.0),
                profile::PlayStyle::Versus => {
                    if player_idx == 0 {
                        (0.0, cx, 0.0, 0.1)
                    } else {
                        (cx, sw - cx, 0.1, 0.0)
                    }
                }
                profile::PlayStyle::Single => {
                    if centered_single_notefield {
                        (0.0, sw, 0.0, 0.0)
                    } else if is_p2_single {
                        (cx, sw - cx, 0.1, 0.0)
                    } else {
                        (0.0, cx, 0.0, 0.1)
                    }
                }
            };

            actors.push(act!(quad:
                align(0.0, 0.0): xy(x, 0.0):
                zoomto(w, sh):
                fadeleft(fl): faderight(fr):
                diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
                z(-99)
            ));
        }
    }

    // Background filter per-player (Simply Love parity): draw behind each notefield, not full-screen.
    for &(player_idx, field_x) in &per_player_fields {
        if player_idx == usize::MAX || player_idx >= state.num_players {
            continue;
        }
        let filter_alpha = match state.player_profiles[player_idx].background_filter {
            crate::game::profile::BackgroundFilter::Off => 0.0,
            crate::game::profile::BackgroundFilter::Dark => 0.5,
            crate::game::profile::BackgroundFilter::Darker => 0.75,
            crate::game::profile::BackgroundFilter::Darkest => 0.95,
        };
        if filter_alpha <= 0.0 {
            continue;
        }
        actors.push(act!(quad:
            align(0.5, 0.5): xy(field_x, screen_center_y()):
            zoomto(notefield_width(player_idx), screen_height()):
            diffuse(0.0, 0.0, 0.0, filter_alpha):
            z(-99)
        ));
    }

    actors.reserve(p1_actors.len() + p2_actors.as_ref().map_or(0, Vec::len) + 48);
    if let Some(p2_actors) = p2_actors {
        actors.extend(p2_actors);
    }
    actors.extend(p1_actors);
    let clamped_width = screen_width().clamp(640.0, 854.0);
    let score_x_p1 = screen_center_x() - clamped_width / 4.3;
    let score_x_p2 = screen_center_x() + clamped_width / 2.75;
    let diff_x_p1 = screen_center_x() - widescale(292.5, 342.5);
    let diff_x_p2 = screen_center_x() + widescale(292.5, 342.5);

    let mut players = [(0usize, profile::PlayerSide::P1, 0.0, 0.0, 0.0, 0.0); 2];
    let player_count = match play_style {
        profile::PlayStyle::Versus => {
            players[0] = (
                0,
                profile::PlayerSide::P1,
                per_player_fields[0].1,
                diff_x_p1,
                score_x_p1,
                score_x_p2,
            );
            players[1] = (
                1,
                profile::PlayerSide::P2,
                per_player_fields[1].1,
                diff_x_p2,
                score_x_p2,
                score_x_p1,
            );
            2
        }
        _ if is_p2_single => {
            players[0] = (
                0,
                profile::PlayerSide::P2,
                per_player_fields[0].1,
                diff_x_p2,
                score_x_p2,
                score_x_p1,
            );
            1
        }
        _ => {
            players[0] = (
                0,
                profile::PlayerSide::P1,
                per_player_fields[0].1,
                diff_x_p1,
                score_x_p1,
                score_x_p2,
            );
            1
        }
    };

    let is_ultrawide = screen_width() / screen_height().max(1.0) > (21.0 / 9.0);
    let graph_center_shift = widescale(45.0, 95.0);

    for &(player_idx, player_side, field_x, _, _, _) in &players[..player_count] {
        if !state.player_profiles[player_idx].nps_graph_at_top {
            continue;
        }
        let graph_w = state.density_graph_top_w[player_idx];
        let graph_h = state.density_graph_top_h;
        let graph_mesh_h = graph_h * state.density_graph_top_scale_y[player_idx].clamp(0.0, 1.0);
        if graph_w <= 0.0 || graph_h <= 0.0 || graph_mesh_h <= 0.0 {
            continue;
        }
        let note_field_is_centered = (field_x - screen_center_x()).abs() < 1.0;
        let x = if note_field_is_centered {
            screen_center_x() - graph_w * 0.5
        } else if player_side == profile::PlayerSide::P1 {
            screen_center_x() - graph_w - graph_center_shift
        } else {
            screen_center_x() + graph_center_shift
        };
        let y_bottom = 71.0;
        let y_top = y_bottom - graph_h;
        let y_mesh_top = y_bottom - graph_mesh_h;
        let graph_bg_alpha = if state.player_profiles[player_idx].transparent_density_graph_bg {
            0.5
        } else {
            1.0
        };

        actors.push(act!(quad:
            align(0.0, 0.0): xy(x, y_top):
            zoomto(graph_w, graph_h):
            diffuse(30.0 / 255.0, 40.0 / 255.0, 47.0 / 255.0, graph_bg_alpha):
            z(84)
        ));

        if let Some(mesh) = &state.density_graph_top_mesh[player_idx]
            && !mesh.is_empty()
        {
            actors.push(Actor::Mesh {
                align: [0.0, 0.0],
                offset: [x, y_mesh_top],
                size: [SizeSpec::Px(graph_w), SizeSpec::Px(graph_mesh_h)],
                vertices: mesh.clone(),
                mode: MeshMode::Triangles,
                visible: true,
                blend: BlendMode::Alpha,
                z: 85,
            });
        }

        let duration =
            (state.density_graph_last_second - state.density_graph_first_second).max(0.001_f32);
        let progress_w = (((state.current_music_time_display - state.density_graph_first_second)
            / duration)
            * graph_w)
            .clamp(0.0, graph_w);
        if progress_w > 0.0 {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(x, y_top):
                zoomto(progress_w, graph_h):
                diffuse(0.0, 0.0, 0.0, 0.85):
                z(86)
            ));
        }
    }

    for &(player_idx, player_side, field_x, diff_x, score_x_normal, score_x_other) in
        &players[..player_count]
    {
        let chart = &state.charts[player_idx];
        let difficulty_color = color::difficulty_rgba(&chart.difficulty, state.active_color_index);
        let meter_text = cached_meter_text(chart.meter as u32);
        let meter_detail_text =
            color::difficulty_display_name_for_song(&chart.difficulty, &state.song.title, true);

        // Difficulty Box
        let y = 56.0;
        let mut diff_children = Vec::with_capacity(if cfg.zmod_rating_box_text { 3 } else { 2 });
        diff_children.push(act!(quad:
            align(0.5, 0.5): xy(0.0, 0.0): zoomto(30.0, 30.0):
            diffuse(difficulty_color[0], difficulty_color[1], difficulty_color[2], 1.0)
        ));
        let meter_y = if cfg.zmod_rating_box_text { -4.0 } else { 0.0 };
        diff_children.push(act!(text:
            font("wendy"): settext(meter_text): align(0.5, 0.5): xy(0.0, meter_y):
            zoom(0.4): diffuse(0.0, 0.0, 0.0, 1.0)
        ));
        if cfg.zmod_rating_box_text {
            diff_children.push(act!(text:
                font("miso"):
                settext(meter_detail_text):
                align(0.5, 0.5): xy(0.0, 9.5):
                zoom(0.5):
                diffuse(0.0, 0.0, 0.0, 1.0)
            ));
        }
        actors.push(Actor::Frame {
            align: [0.5, 0.5],
            offset: [diff_x, y],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            children: diff_children,
            background: None,
            z: 90,
        });

        // Score Display
        let note_field_is_centered = (field_x - screen_center_x()).abs() < 1.0;
        let nps_graph_at_top = state.player_profiles[player_idx].nps_graph_at_top;
        let single_score_swapped = state.num_players == 1
            && play_style != profile::PlayStyle::Double
            && nps_graph_at_top
            && !note_field_is_centered;
        let score_x = if single_score_swapped {
            score_x_other
        } else {
            score_x_normal
        };
        let hide_score_for_top_graph = state.num_players > 1 && nps_graph_at_top && !is_ultrawide;

        if !state.player_profiles[player_idx].hide_score && !hide_score_for_top_graph {
            let score_y = 56.0;
            let show_ex_score = state.player_profiles[player_idx].show_ex_score;
            let show_hard_ex_score =
                show_ex_score && state.player_profiles[player_idx].show_hard_ex_score;
            let (score_text, score_color) = if show_ex_score {
                let ex_percent = crate::game::gameplay::display_ex_score_percent(state, player_idx);
                (
                    cached_score_2dp(ex_percent.max(0.0)),
                    color::JUDGMENT_RGBA[0],
                )
            } else {
                let score_percent =
                    (crate::game::gameplay::display_itg_score_percent(state, player_idx) * 100.0)
                        as f32;
                (cached_score_2dp(score_percent as f64), [1.0, 1.0, 1.0, 1.0])
            };

            let is_p2_side = player_side == profile::PlayerSide::P2;
            // Arrow Cloud parity: EX remains the "normal" score position/anchor.
            // H.EX is placed at a different x on P2 so it appears to the left of EX.
            actors.push(act!(text:
                font("wendy_monospace_numbers"): settext(score_text):
                align(1.0, 1.0): xy(score_x, score_y):
                zoom(0.5): horizalign(right):
                diffuse(score_color[0], score_color[1], score_color[2], score_color[3]):
                z(90)
            ));

            if show_hard_ex_score {
                let hard_ex_percent =
                    crate::game::gameplay::display_hard_ex_score_percent(state, player_idx);
                let hex = color::HARD_EX_SCORE_RGBA;
                let hard_ex_x = if single_score_swapped {
                    let swapped_base = if is_p2_side {
                        screen_center_x() - clamped_width / 4.3
                    } else {
                        screen_center_x() + clamped_width / 4.3
                    };
                    swapped_base + 115.0
                } else if is_p2_side {
                    // Arrow Cloud: HardEX uses /4.3 on P2 (while EX uses /2.75).
                    screen_center_x() + clamped_width / 4.3
                } else {
                    score_x
                };

                if is_p2_side {
                    actors.push(act!(text:
                        font("wendy_monospace_numbers"):
                        settext(cached_score_2dp(hard_ex_percent.max(0.0))):
                        align(1.0, 0.0): xy(hard_ex_x, score_y):
                        zoom(0.25): horizalign(right):
                        diffuse(hex[0], hex[1], hex[2], hex[3]):
                        z(90)
                    ));
                } else {
                    actors.push(act!(text:
                        font("wendy_monospace_numbers"):
                        settext(cached_score_2dp(hard_ex_percent.max(0.0))):
                        align(0.0, 0.0): xy(hard_ex_x, score_y):
                        zoom(0.25): horizalign(left):
                        diffuse(hex[0], hex[1], hex[2], hex[3]):
                        z(90)
                    ));
                }
            }
        }
    }
    // Current BPM Display (1:1 with Simply Love)
    {
        let base_bpm = state.timing.get_bpm_for_beat(state.current_beat_display);
        let rate = if state.music_rate.is_finite() {
            state.music_rate as f64
        } else {
            1.0
        };
        let display_bpm = if base_bpm.is_finite() {
            f64::from(base_bpm) * rate
        } else {
            0.0
        };
        let bpm_text = cached_bpm_text(display_bpm, cfg.show_bpm_decimal);
        // Final world-space positions derived from analyzing the SM Lua transforms.
        // The parent frame is bottom-aligned to y=52, and its children are positioned
        // relative to that y-coordinate, with a zoom of 1.33 applied to the whole group.
        let frame_origin_y = 51.0;
        let frame_zoom = 1.33;
        // The BPM text is at y=0 relative to the frame's origin. Its final position is just the origin.
        let bpm_center_y = frame_origin_y;
        // The Rate text is at y=12 relative to the frame's origin. Its offset is scaled by the frame's zoom.
        let rate_center_y = 12.0f64.mul_add(frame_zoom, frame_origin_y);
        let bpm_final_zoom = 1.0 * frame_zoom;
        let rate_final_zoom = 0.5 * frame_zoom;
        let mut bpm_x = screen_center_x();
        let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;
        if state.num_players == 1
            && note_field_is_centered
            && state.player_profiles[0].nps_graph_at_top
        {
            let side_shift = if player_side == profile::PlayerSide::P1 {
                0.3
            } else {
                -0.3
            };
            bpm_x = screen_center_x() + screen_width() * side_shift;
        }
        actors.push(act!(text:
            font("miso"): settext(bpm_text):
            align(0.5, 0.5): xy(bpm_x, bpm_center_y):
            zoom(bpm_final_zoom): horizalign(center): z(90)
        ));
        let rate = if state.music_rate.is_finite() {
            state.music_rate
        } else {
            1.0
        };
        let rate_text = cached_rate_text(rate);
        actors.push(act!(text:
            font("miso"): settext(rate_text):
            align(0.5, 0.5): xy(bpm_x, rate_center_y):
            zoom(rate_final_zoom): horizalign(center): z(90)
        ));
    }
    // Song Title Box (SongMeter)
    {
        let w = widescale(310.0, 417.0);
        let h = 22.0;
        let box_cx = screen_center_x();
        let box_cy = 20.0;
        let mut frame_children = Vec::with_capacity(4);
        frame_children.push(act!(quad: align(0.5, 0.5): xy(w / 2.0, h / 2.0): zoomto(w, h): diffuse(1.0, 1.0, 1.0, 1.0): z(0) ));
        frame_children.push(act!(quad: align(0.5, 0.5): xy(w / 2.0, h / 2.0): zoomto(w - 4.0, h - 4.0): diffuse(0.0, 0.0, 0.0, 1.0): z(1) ));
        if state.song.total_length_seconds > 0 && state.current_music_time_display >= 0.0 {
            let progress = (state.current_music_time_display
                / state.song.total_length_seconds as f32)
                .clamp(0.0, 1.0);
            frame_children.push(act!(quad:
                align(0.0, 0.5): xy(2.0, h / 2.0): zoomto((w - 4.0) * progress, h - 4.0):
                diffuse(player_color[0], player_color[1], player_color[2], 1.0): z(2)
            ));
        }
        let full_title = state.song_full_title.clone();
        frame_children.push(act!(text:
            font("miso"): settext(full_title): align(0.5, 0.5): xy(w / 2.0, h / 2.0):
            zoom(0.8): maxwidth(screen_width() / 2.5 - 10.0): horizalign(center): z(3)
        ));
        actors.push(Actor::Frame {
            align: [0.5, 0.5],
            offset: [box_cx, box_cy],
            size: [SizeSpec::Px(w), SizeSpec::Px(h)],
            background: None,
            z: 90,
            children: frame_children,
        });
    }
    // --- Life Meter ---
    {
        let player_life_color = |player_idx: usize| -> [f32; 4] {
            match play_style {
                profile::PlayStyle::Versus => {
                    if player_idx == 0 {
                        color::decorative_rgba(state.active_color_index)
                    } else {
                        color::decorative_rgba(state.active_color_index - 2)
                    }
                }
                _ => {
                    if is_p2_single {
                        color::decorative_rgba(state.active_color_index - 2)
                    } else {
                        color::decorative_rgba(state.active_color_index)
                    }
                }
            }
        };
        let rainbow_life_color = |elapsed: f32| -> [f32; 4] {
            let phase = elapsed * 2.0;
            let r = (phase + 0.0).sin() * 0.5 + 0.5;
            let g = (phase + std::f32::consts::TAU / 3.0).sin() * 0.5 + 0.5;
            let b = (phase + (2.0 * std::f32::consts::TAU) / 3.0).sin() * 0.5 + 0.5;
            [r, g, b, 1.0]
        };
        let responsive_life_color = |life: f32| -> [f32; 4] {
            let life = life.clamp(0.0, 1.0);
            if life >= 0.9 {
                [0.0, 1.0, ((life - 0.9) * 10.0).clamp(0.0, 1.0), 1.0]
            } else if life >= 0.5 {
                [((0.9 - life) * 2.5).clamp(0.0, 1.0), 1.0, 0.0, 1.0]
            } else {
                [1.0, ((life - 0.2) * (10.0 / 3.0)).clamp(0.0, 1.0), 0.0, 1.0]
            }
        };
        let fill_life_color = |player_idx: usize, life: f32, dead: bool| -> [f32; 4] {
            let profile = &state.player_profiles[player_idx];
            let is_hot = !dead && life >= 1.0;
            if is_hot {
                if profile.rainbow_max {
                    rainbow_life_color(state.total_elapsed_in_screen)
                } else {
                    [1.0, 1.0, 1.0, 1.0]
                }
            } else if profile.responsive_colors {
                responsive_life_color(life)
            } else {
                player_life_color(player_idx)
            }
        };
        let show_standard_life_percent = screen_width() / screen_height().max(1.0) >= (16.0 / 9.0);

        let mut life_players = [(0usize, profile::PlayerSide::P1); 2];
        let life_player_count = match play_style {
            profile::PlayStyle::Versus => {
                life_players[0] = (0, profile::PlayerSide::P1);
                life_players[1] = (1, profile::PlayerSide::P2);
                2
            }
            _ if is_p2_single => {
                life_players[0] = (0, profile::PlayerSide::P2);
                1
            }
            _ => {
                life_players[0] = (0, profile::PlayerSide::P1);
                1
            }
        };

        for &(player_idx, side) in &life_players[..life_player_count] {
            if state.player_profiles[player_idx].hide_lifebar {
                continue;
            }

            // Latch-to-zero for rendering the very frame we die.
            let dead =
                state.players[player_idx].is_failing || state.players[player_idx].life <= 0.0;
            let life_for_render = if dead {
                0.0
            } else {
                state.players[player_idx].life.clamp(0.0, 1.0)
            };
            let is_hot = !dead && life_for_render >= 1.0;
            let life_color = fill_life_color(player_idx, life_for_render, dead);
            let life_percent = life_for_render * 100.0;
            let life_percent_text = cached_life_percent_text(life_percent);

            match state.player_profiles[player_idx].lifemeter_type {
                profile::LifeMeterType::Standard => {
                    let w = 136.0;
                    let h = 18.0;
                    let meter_cy = 20.0;
                    let meter_cx = screen_center_x()
                        + match play_style {
                            profile::PlayStyle::Versus => match side {
                                profile::PlayerSide::P1 => -widescale(238.0, 288.0),
                                profile::PlayerSide::P2 => widescale(238.0, 288.0),
                            },
                            _ => match side {
                                profile::PlayerSide::P1 => -widescale(238.0, 288.0),
                                profile::PlayerSide::P2 => widescale(238.0, 288.0),
                            },
                        };

                    // Frames/border
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w + 4.0, h + 4.0):
                        diffuse(1.0, 1.0, 1.0, 1.0): z(90)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w, h):
                        diffuse(0.0, 0.0, 0.0, 1.0): z(91)
                    ));

                    let filled_width = w * life_for_render;
                    // Never draw swoosh if dead OR nothing to fill.
                    if filled_width > 0.0 && !dead {
                        // Logic Parity:
                        // velocity = -(songposition:GetCurBPS() * 0.5)
                        // if songposition:GetFreeze() or songposition:GetDelay() then velocity = 0 end
                        let bps = state.timing.get_bpm_for_beat(state.current_beat_display) / 60.0;
                        let velocity_x = if state.is_in_freeze || state.is_in_delay {
                            0.0
                        } else {
                            -(bps * 0.5)
                        };

                        let swoosh_alpha = if is_hot { 1.0 } else { 0.2 };

                        // MeterSwoosh
                        actors.push(act!(sprite("swoosh.png"):
                            align(0.0, 0.5):
                            xy(meter_cx - w / 2.0, meter_cy):
                            zoomto(filled_width, h):
                            diffusealpha(swoosh_alpha):
                            texcoordvelocity(velocity_x, 0.0):
                            z(93)
                        ));

                        // MeterFill
                        actors.push(act!(quad:
                            align(0.0, 0.5):
                            xy(meter_cx - w / 2.0, meter_cy):
                            zoomto(filled_width, h):
                            diffuse(life_color[0], life_color[1], life_color[2], 1.0):
                            z(92)
                        ));
                    }

                    if state.player_profiles[player_idx].show_life_percent
                        && show_standard_life_percent
                        && !is_hot
                    {
                        let life_text_color = player_life_color(player_idx);
                        let (outer_x, inner_x, text_x, align_x) = if side == profile::PlayerSide::P1
                        {
                            (meter_cx - 76.0, meter_cx - 77.0, meter_cx - 77.0, 1.0)
                        } else {
                            (meter_cx + 76.0, meter_cx + 77.0, meter_cx + 78.0, 0.0)
                        };
                        actors.push(act!(quad:
                            align(align_x, 0.5): xy(outer_x, meter_cy):
                            zoomto(44.0, 18.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(94)
                        ));
                        actors.push(act!(quad:
                            align(align_x, 0.5): xy(inner_x, meter_cy):
                            zoomto(42.0, 16.0):
                            diffuse(0.0, 0.0, 0.0, 1.0):
                            z(95)
                        ));
                        actors.push(act!(text:
                            font("miso"): settext(life_percent_text.clone()):
                            align(align_x, 0.5): xy(text_x, meter_cy):
                            zoom(1.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(96)
                        ));
                    }
                }
                profile::LifeMeterType::Surround => {
                    let sw = screen_width();
                    let sh = screen_height();
                    let w = sw * 0.5;
                    let h = sh - 80.0;
                    let y = 80.0;
                    let croptop = 1.0 - life_for_render;

                    if play_style == profile::PlayStyle::Double {
                        // Double: two quads flanking left/right, moving in unison.
                        actors.push(act!(quad:
                            align(0.0, 0.0): xy(0.0, y):
                            zoomto(w, h):
                            diffuse(0.2, 0.2, 0.2, 1.0):
                            faderight(0.8):
                            croptop(croptop):
                            z(-98)
                        ));
                        actors.push(act!(quad:
                            align(1.0, 0.0): xy(sw, y):
                            zoomto(w, h):
                            diffuse(0.2, 0.2, 0.2, 1.0):
                            fadeleft(0.8):
                            croptop(croptop):
                            z(-98)
                        ));
                        // Only one player in Double style.
                        break;
                    }

                    let mut surround_color = if state.player_profiles[player_idx].responsive_colors
                    {
                        let mut c = responsive_life_color(life_for_render);
                        c[3] = 0.2;
                        c
                    } else {
                        [0.2, 0.2, 0.2, 1.0]
                    };
                    if life_for_render >= 1.0 && state.player_profiles[player_idx].rainbow_max {
                        let mut c = rainbow_life_color(state.total_elapsed_in_screen);
                        c[3] = if state.player_profiles[player_idx].responsive_colors {
                            0.2
                        } else {
                            1.0
                        };
                        surround_color = c;
                    }

                    match side {
                        profile::PlayerSide::P1 => {
                            actors.push(act!(quad:
                                align(0.0, 0.0): xy(0.0, y):
                                zoomto(w, h):
                                diffuse(surround_color[0], surround_color[1], surround_color[2], surround_color[3]):
                                faderight(0.8):
                                croptop(croptop):
                                z(-98)
                            ));
                        }
                        profile::PlayerSide::P2 => {
                            actors.push(act!(quad:
                                align(1.0, 0.0): xy(sw, y):
                                zoomto(w, h):
                                diffuse(surround_color[0], surround_color[1], surround_color[2], surround_color[3]):
                                fadeleft(0.8):
                                croptop(croptop):
                                z(-98)
                            ));
                        }
                    }
                }
                profile::LifeMeterType::Vertical => {
                    let bar_w = 16.0;
                    let bar_h = 250.0;

                    let x = {
                        // SL: default to _screen.cx +/- SL_WideScale(302, 400).
                        let mut x = screen_center_x()
                            + match side {
                                profile::PlayerSide::P1 => -widescale(302.0, 400.0),
                                profile::PlayerSide::P2 => widescale(302.0, 400.0),
                            };

                        // SL: if double style, position next to notefield.
                        if play_style == profile::PlayStyle::Double {
                            let half_nf = notefield_width(player_idx) * 0.5;
                            x = screen_center_x()
                                + match side {
                                    profile::PlayerSide::P1 => -(half_nf + 10.0),
                                    profile::PlayerSide::P2 => half_nf + 10.0,
                                };
                        }

                        x
                    };

                    let cy = bar_h + 10.0;
                    // Frames/border
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(x, cy): zoomto(bar_w + 2.0, bar_h + 2.0):
                        diffuse(1.0, 1.0, 1.0, 1.0): z(90)
                    ));
                    actors.push(act!(quad:
                        align(0.5, 0.5): xy(x, cy): zoomto(bar_w, bar_h):
                        diffuse(0.0, 0.0, 0.0, 1.0): z(91)
                    ));

                    let filled_h = bar_h * life_for_render;

                    // MeterFill
                    if filled_h > 0.0 {
                        actors.push(act!(quad:
                            align(0.0, 1.0):
                            xy(x - bar_w * 0.5, cy + bar_h * 0.5):
                            zoomto(bar_w, filled_h):
                            diffuse(life_color[0], life_color[1], life_color[2], 1.0):
                            z(92)
                        ));
                    }

                    // MeterSwoosh
                    if filled_h > 0.0 && !dead {
                        let bps = state.timing.get_bpm_for_beat(state.current_beat_display) / 60.0;
                        let velocity_x = if state.is_in_freeze || state.is_in_delay {
                            0.0
                        } else {
                            -(bps * 0.5)
                        };
                        let swoosh_alpha = if is_hot { 1.0 } else { 0.2 };

                        actors.push(act!(sprite("swoosh.png"):
                            align(0.5, 0.5):
                            xy(x, (cy + bar_h * 0.5) - filled_h * 0.5):
                            zoomto(filled_h, bar_w):
                            diffusealpha(swoosh_alpha):
                            rotationz(90.0):
                            texcoordvelocity(velocity_x, 0.0):
                            z(93)
                        ));
                    }

                    if state.player_profiles[player_idx].show_life_percent && !is_hot {
                        let life_text_color = player_life_color(player_idx);
                        let text_y = cy + bar_h * 0.5 - (bar_h * life_for_render);
                        let (outer_x, inner_x, text_x, align_x) = if side == profile::PlayerSide::P1
                        {
                            (x + 10.0, x + 11.0, x + 12.0, 0.0)
                        } else {
                            (x - 11.0, x - 12.0, x - 13.0, 1.0)
                        };
                        actors.push(act!(quad:
                            align(align_x, 0.5): xy(outer_x, text_y):
                            zoomto(44.0, 18.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(94)
                        ));
                        actors.push(act!(quad:
                            align(align_x, 0.5): xy(inner_x, text_y):
                            zoomto(42.0, 16.0):
                            diffuse(0.0, 0.0, 0.0, 1.0):
                            z(95)
                        ));
                        actors.push(act!(text:
                            font("miso"): settext(life_percent_text.clone()):
                            align(align_x, 0.5): xy(text_x, text_y):
                            zoom(1.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(96)
                        ));
                    }
                }
            }
        }
    }
    // Simply Love parity: keep Stage/Event text visible at the footer after intro animation ends.
    if !state.stage_intro_text.is_empty()
        && state.total_elapsed_in_screen >= INTRO_TEXT_SETTLE_SECONDS
    {
        actors.push(act!(text:
            font("wendy"): settext(state.stage_intro_text.clone()):
            align(0.5, 0.5): xy(screen_center_x(), screen_height() - 30.0):
            zoom(0.4):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(110)
        ));
    }
    let p1_avatar = hud_snapshot
        .p1
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });
    let p2_avatar = hud_snapshot
        .p2
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });

    let p1_joined = hud_snapshot.p1.joined;
    let p2_joined = hud_snapshot.p2.joined;
    let p1_guest = hud_snapshot.p1.guest;
    let p2_guest = hud_snapshot.p2.guest;

    let (p1_footer_text, p1_footer_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                "INSERT CARD"
            } else {
                hud_snapshot.p1.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (None, None)
    };
    let (p2_footer_text, p2_footer_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                "INSERT CARD"
            } else {
                hud_snapshot.p2.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (None, None)
    };

    let (footer_left, footer_right, left_avatar, right_avatar) =
        if play_style == profile::PlayStyle::Versus {
            (
                p1_footer_text,
                p2_footer_text,
                p1_footer_avatar,
                p2_footer_avatar,
            )
        } else {
            match player_side {
                profile::PlayerSide::P1 => (p1_footer_text, None, p1_footer_avatar, None),
                profile::PlayerSide::P2 => (None, p2_footer_text, None, p2_footer_avatar),
            }
        };
    actors.push(screen_bar::build(ScreenBarParams {
        title: "",
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        fg_color: [1.0; 4],
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
    }));
    let show_step_stats = match play_style {
        profile::PlayStyle::Single | profile::PlayStyle::Double => state
            .player_profiles
            .get(0)
            .is_some_and(|p| p.data_visualizations == profile::DataVisualizations::StepStatistics),
        profile::PlayStyle::Versus => {
            state.player_profiles.get(0).is_some_and(|p| {
                p.data_visualizations == profile::DataVisualizations::StepStatistics
            }) || state.player_profiles.get(1).is_some_and(|p| {
                p.data_visualizations == profile::DataVisualizations::StepStatistics
            })
        }
    };
    if show_step_stats {
        if state.num_cols <= 4 && play_style != profile::PlayStyle::Versus {
            actors.extend(gameplay_stats::build(
                state,
                asset_manager,
                playfield_center_x,
                player_side,
            ));
        } else if play_style == profile::PlayStyle::Versus {
            actors.extend(gameplay_stats::build_versus_step_stats(
                state,
                asset_manager,
            ));
        } else if play_style == profile::PlayStyle::Double {
            actors.extend(gameplay_stats::build_double_step_stats(
                state,
                asset_manager,
                playfield_center_x,
            ));
        }
    }
    actors
}
