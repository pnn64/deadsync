use crate::act;
use crate::assets::{self, AssetManager};
use crate::core::space::*;
use crate::core::space::{is_wide, widescale};
use crate::game::judgment;
use crate::game::judgment::JudgeGrade;
use crate::game::note::HoldResult;
use crate::game::note::NoteType;
use crate::game::parsing::noteskin::{Quantization, SpriteSlot, NUM_QUANTIZATIONS};
use crate::game::{profile, scroll::ScrollSpeedSetting};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::components::screen_bar::{self, ScreenBarParams};
use crate::ui::font;
use log::warn;
use std::array::from_fn;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

pub use crate::game::gameplay::{init, update, State};
use crate::game::gameplay::active_hold_is_engaged;
use crate::game::gameplay::{
    ComboMilestoneKind, COMBO_HUNDRED_MILESTONE_DURATION, COMBO_THOUSAND_MILESTONE_DURATION,
    HOLD_JUDGMENT_TOTAL_DURATION, MINE_EXPLOSION_DURATION, RECEPTOR_GLOW_DURATION,
    RECEPTOR_Y_OFFSET_FROM_CENTER, TRANSITION_IN_DURATION, TRANSITION_OUT_DURATION,
};

// --- CONSTANTS ---

// Gameplay Layout & Feel
const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0; // Match Simply Love's on-screen arrow height
const TARGET_EXPLOSION_PIXEL_SIZE: f32 = 125.0; // Simply Love tap explosions top out around 125px tall
const HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER: f32 = -90.0; // Mirrors Simply Love metrics for hold judgments
const LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT: f32 = 140.0; // Each frame in Love 1x2 (doubleres).png is 140px tall
const HOLD_JUDGMENT_FINAL_HEIGHT: f32 = 32.0; // Matches Simply Love's final on-screen size
const HOLD_JUDGMENT_INITIAL_HEIGHT: f32 = HOLD_JUDGMENT_FINAL_HEIGHT * 0.8; // Mirrors 0.4->0.5 zoom ramp in metrics
const HOLD_JUDGMENT_FINAL_ZOOM: f32 =
    HOLD_JUDGMENT_FINAL_HEIGHT / LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT;
const HOLD_JUDGMENT_INITIAL_ZOOM: f32 =
    HOLD_JUDGMENT_INITIAL_HEIGHT / LOVE_HOLD_JUDGMENT_NATIVE_FRAME_HEIGHT;

//const DANGER_THRESHOLD: f32 = 0.2; // For implementation of red/green flashing light

// Visual Feedback
const SHOW_COMBO_AT: u32 = 4; // From Simply Love metrics

// Z-order layers for key gameplay visuals (higher draws on top)
const Z_RECEPTOR: i32 = 100;
const Z_HOLD_BODY: i32 = 110;
const Z_HOLD_CAP: i32 = 110;
const Z_HOLD_EXPLOSION: i32 = 120;
const Z_HOLD_GLOW: i32 = 130;
const Z_MINE_EXPLOSION: i32 = 101;
const Z_TAP_NOTE: i32 = 140;
const MINE_CORE_SIZE_RATIO: f32 = 0.45;
const MINE_FILL_LAYERS: usize = 32;
const MINE_GRADIENT_SAMPLES: usize = 64;

#[derive(Hash, PartialEq, Eq, Clone)]
struct MineGradientKey {
    texture_key: String,
    src: [i32; 2],
    size: [i32; 2],
}

type MineGradientCache = HashMap<MineGradientKey, Arc<Vec<[f32; 4]>>>;

static MINE_GRADIENT_CACHE: LazyLock<Mutex<MineGradientCache>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Debug)]
struct MineFillState {
    layers: [[f32; 4]; MINE_FILL_LAYERS],
}

fn mine_fill_state(slot: &SpriteSlot, beat: f32) -> Option<MineFillState> {
    let colors = {
        let key = MineGradientKey {
            texture_key: slot.texture_key().to_string(),
            src: slot.def.src,
            size: slot.def.size,
        };

        let mut cache = MINE_GRADIENT_CACHE.lock().ok()?;
        match cache.entry(key.clone()) {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {
                let colors = Arc::new(load_mine_gradient_colors(slot)?);
                entry.insert(colors.clone());
                colors
            }
        }
    };

    if colors.is_empty() {
        return None;
    }

    let phase = beat.rem_euclid(1.0);
    let len = colors.len();
    if len == 0 {
        return None;
    }

    let idx_float = phase * len as f32;
    let idx = (idx_float.floor() as usize) % len;

    let layers = from_fn(|layer| {
        let offset = layer % len;
        let sample_index = (idx + len - offset) % len;
        let mut color = colors[sample_index];
        color[3] = 1.0;
        color
    });

    Some(MineFillState { layers })
}

fn load_mine_gradient_colors(slot: &SpriteSlot) -> Option<Vec<[f32; 4]>> {
    let texture_key = slot.texture_key();
    let path = Path::new("assets").join(texture_key);
    let image = image::open(&path).ok()?.to_rgba8();

    let mut width = slot.def.size[0];
    let mut height = slot.def.size[1];
    if width <= 0 || height <= 0 {
        if let Some(frame) = slot.source.frame_size() {
            width = frame[0];
            height = frame[1];
        }
    }

    if width <= 0 || height <= 0 {
        warn!("Mine fill slot has invalid size for gradient sampling");
        return None;
    }

    let src_x = slot.def.src[0].max(0) as u32;
    let src_y = slot.def.src[1].max(0) as u32;
    let mut sample_width = width as u32;
    let mut sample_height = height as u32;

    if src_x >= image.width() || src_y >= image.height() {
        warn!(
            "Mine fill region ({}, {}) is outside of texture {}",
            src_x, src_y, texture_key
        );
        return None;
    }

    if src_x + sample_width > image.width() {
        sample_width = image.width().saturating_sub(src_x);
    }
    if src_y + sample_height > image.height() {
        sample_height = image.height().saturating_sub(src_y);
    }

    if sample_width == 0 || sample_height == 0 {
        warn!(
            "Mine fill region has zero sample size for texture {}",
            texture_key
        );
        return None;
    }

    let mut colors = Vec::with_capacity(sample_width as usize);
    for dx in 0..sample_width {
        let mut r = 0.0_f32;
        let mut g = 0.0_f32;
        let mut b = 0.0_f32;
        let mut alpha_weight = 0.0_f32;

        for dy in 0..sample_height {
            let pixel = image.get_pixel(src_x + dx, src_y + dy);
            let a = pixel[3] as f32 / 255.0;
            if a <= f32::EPSILON {
                continue;
            }
            r += pixel[0] as f32 * a;
            g += pixel[1] as f32 * a;
            b += pixel[2] as f32 * a;
            alpha_weight += a;
        }

        if alpha_weight <= f32::EPSILON {
            colors.push([0.0, 0.0, 0.0, 0.0]);
        } else {
            let inv = 1.0 / alpha_weight;
            colors.push([
                (r * inv) / 255.0,
                (g * inv) / 255.0,
                (b * inv) / 255.0,
                (alpha_weight / sample_height as f32).clamp(0.0, 1.0),
            ]);
        }
    }

    if colors.is_empty() {
        return None;
    }

    if colors.len() == 1 {
        let mut color = colors[0];
        color[3] = 1.0;
        return Some(vec![color; MINE_GRADIENT_SAMPLES.max(1)]);
    }

    let max_index = (colors.len() - 1) as f32;
    let mut samples = Vec::with_capacity(MINE_GRADIENT_SAMPLES);
    let divisor = (MINE_GRADIENT_SAMPLES.saturating_sub(1)).max(1) as f32;
    for i in 0..MINE_GRADIENT_SAMPLES {
        let t = i as f32 / divisor;
        let position = t * max_index;
        let base_index = position.floor() as usize;
        let next_index = (base_index + 1).min(colors.len() - 1);
        let frac = (position - base_index as f32).clamp(0.0, 1.0);

        let c0 = colors[base_index];
        let c1 = colors[next_index];
        let mut sampled = [
            c0[0] + (c1[0] - c0[0]) * frac,
            c0[1] + (c1[1] - c0[1]) * frac,
            c0[2] + (c1[2] - c0[2]) * frac,
            1.0,
        ];

        sampled[0] = sampled[0].clamp(0.0, 1.0);
        sampled[1] = sampled[1].clamp(0.0, 1.0);
        sampled[2] = sampled[2].clamp(0.0, 1.0);

        samples.push(sampled);
    }

    Some(samples)
}

// --- TRANSITIONS ---
pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

// --- DRAWING ---

fn build_background(state: &State) -> Actor {
    let sw = screen_width();
    let sh = screen_height();
    let screen_aspect = if sh > 0.0 { sw / sh } else { 16.0 / 9.0 };

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
            z(-100)
        )
    } else {
        // screen is taller/equal, match height to cover
        act!(sprite(state.background_texture_key.clone()):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            zoomtoheight(sh):
            z(-100)
        )
    }
}

// --- Statics for Judgment Counter Display ---

static JUDGMENT_ORDER: [JudgeGrade; 6] = [
    JudgeGrade::Fantastic,
    JudgeGrade::Excellent,
    JudgeGrade::Great,
    JudgeGrade::Decent,
    JudgeGrade::WayOff,
    JudgeGrade::Miss,
];

struct JudgmentDisplayInfo {
    label: &'static str,
    color: [f32; 4],
}

static JUDGMENT_INFO: LazyLock<HashMap<JudgeGrade, JudgmentDisplayInfo>> = LazyLock::new(|| {
    HashMap::from([
        (JudgeGrade::Fantastic, JudgmentDisplayInfo { label: "FANTASTIC", color: color::rgba_hex(color::JUDGMENT_HEX[0]) }),
        (JudgeGrade::Excellent, JudgmentDisplayInfo { label: "EXCELLENT", color: color::rgba_hex(color::JUDGMENT_HEX[1]) }),
        (JudgeGrade::Great,     JudgmentDisplayInfo { label: "GREAT",     color: color::rgba_hex(color::JUDGMENT_HEX[2]) }),
        (JudgeGrade::Decent,    JudgmentDisplayInfo { label: "DECENT",    color: color::rgba_hex(color::JUDGMENT_HEX[3]) }),
        (JudgeGrade::WayOff,    JudgmentDisplayInfo { label: "WAY OFF",   color: color::rgba_hex(color::JUDGMENT_HEX[4]) }),
        (JudgeGrade::Miss,      JudgmentDisplayInfo { label: "MISS",      color: color::rgba_hex(color::JUDGMENT_HEX[5]) }),
    ])
});

fn format_game_time(s: f32, total_seconds: f32) -> String {
    if s < 0.0 {
        return format_game_time(0.0, total_seconds);
    }
    let s_u64 = s as u64;

    let minutes = s_u64 / 60;
    let seconds = s_u64 % 60;

    if total_seconds >= 3600.0 {
        // Over an hour total? use H:MM:SS
        let hours = s_u64 / 3600;
        let minutes = (s_u64 % 3600) / 60;
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else if total_seconds >= 600.0 {
        // Over 10 mins total? use MM:SS
        format!("{:02}:{:02}", minutes, seconds)
    } else {
        // Under 10 mins total? use M:SS
        format!("{}:{:02}", minutes, seconds)
    }
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::new();
    let profile = profile::get();

    // --- Background and Filter ---
    actors.push(build_background(state));

    let filter_alpha = match profile.background_filter {
        crate::game::profile::BackgroundFilter::Off => 0.0,
        crate::game::profile::BackgroundFilter::Dark => 0.5,
        crate::game::profile::BackgroundFilter::Darker => 0.75,
        crate::game::profile::BackgroundFilter::Darkest => 0.95,
    };

    if filter_alpha > 0.0 {
        actors.push(act!(quad:
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, filter_alpha):
            z(-99) // Draw just above the background
        ));
    }

    // --- Playfield Positioning (1:1 with Simply Love) ---
    let logical_screen_width = screen_width();
    let clamped_width = logical_screen_width.clamp(640.0, 854.0);
    let playfield_center_x = screen_center_x() - (clamped_width * 0.25);

    let receptor_y = screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER;

    // --- Banner (1:1 with Simply Love, including parent frame logic) ---
    if let Some(banner_path) = &state.song.banner_path {
        let banner_key = banner_path.to_string_lossy().into_owned();
        let wide = is_wide();

        let sidepane_center_x = screen_width() * 0.75;
        let sidepane_center_y = screen_center_y() + 80.0;
        let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;
        let is_ultrawide = screen_width() / screen_height() > (21.0 / 9.0);
        let banner_data_zoom = if note_field_is_centered && wide && !is_ultrawide {
            let ar = screen_width() / screen_height();
            let t = ((ar - (16.0 / 10.0)) / ((16.0 / 9.0) - (16.0 / 10.0))).clamp(0.0, 1.0);
            0.825 + (0.925 - 0.825) * t
        } else {
            1.0
        };
        let mut local_banner_x = 70.0;
        if note_field_is_centered && wide {
            local_banner_x = 72.0;
        }
        let local_banner_y = -200.0;

        let banner_x = sidepane_center_x + (local_banner_x * banner_data_zoom);
        let banner_y = sidepane_center_y + (local_banner_y * banner_data_zoom);
        let final_zoom = 0.4 * banner_data_zoom;

        actors.push(act!(sprite(banner_key):
            align(0.5, 0.5): xy(banner_x, banner_y):
            setsize(418.0, 164.0): zoom(final_zoom):
            z(-50)
        ));
    }

    if let Some(ns) = &state.noteskin {
        let scale_sprite = |size: [i32; 2]| -> [f32; 2] {
            let width = size[0].max(0) as f32;
            let height = size[1].max(0) as f32;
            if height <= 0.0 || TARGET_ARROW_PIXEL_SIZE <= 0.0 {
                [width, height]
            } else {
                let scale = TARGET_ARROW_PIXEL_SIZE / height;
                [width * scale, TARGET_ARROW_PIXEL_SIZE]
            }
        };
        let scale_explosion = |size: [i32; 2]| -> [f32; 2] {
            let width = size[0].max(0) as f32;
            let height = size[1].max(0) as f32;
            if height <= 0.0 || TARGET_EXPLOSION_PIXEL_SIZE <= 0.0 {
                [width, height]
            } else {
                let scale = TARGET_EXPLOSION_PIXEL_SIZE / height;
                [width * scale, TARGET_EXPLOSION_PIXEL_SIZE]
            }
        };
        let current_time = state.current_music_time;
        // Precompute per-frame values used for converting beat/time to Y positions
        let (rate, cmod_pps_opt, curr_disp_beat, beatmod_multiplier) = match state.scroll_speed {
            ScrollSpeedSetting::CMod(c_bpm) => {
                let pps = (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING;
                let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
                (rate, Some(pps), 0.0, 0.0)
            }
            ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                let curr_disp = state.timing.get_displayed_beat(state.current_beat);
                let speed_multiplier = state.timing.get_speed_multiplier(state.current_beat, state.current_music_time);
                let player_multiplier = state.scroll_speed.beat_multiplier(state.scroll_reference_bpm, state.music_rate);
                let final_multiplier = player_multiplier * speed_multiplier;
                (1.0, None, curr_disp, final_multiplier)
            }
        };
        // For dynamic values (e.g., last_held_beat while letting go), fall back to timing for that beat
        let compute_lane_y = |beat: f32| -> f32 {
            match state.scroll_speed {
                ScrollSpeedSetting::CMod(_) => {
                    let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                    let note_time_chart = state.timing.get_time_for_beat(beat);
                    let time_diff_real = (note_time_chart - current_time) / rate;
                    receptor_y + time_diff_real * pps_chart
                }
                ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                    let note_disp_beat = state.timing.get_displayed_beat(beat);
                    let beat_diff_disp = note_disp_beat - curr_disp_beat;
                    receptor_y + (beat_diff_disp * ScrollSpeedSetting::ARROW_SPACING * beatmod_multiplier)
                }
            }
        };

        let mine_explosion_size = {
            let base = assets::texture_dims("hit_mine_explosion.png")
                .map(|meta| [meta.w.max(1) as f32, meta.h.max(1) as f32])
                .unwrap_or([TARGET_EXPLOSION_PIXEL_SIZE, TARGET_EXPLOSION_PIXEL_SIZE]);

            if base[1] <= 0.0 {
                base
            } else {
                let scale = TARGET_EXPLOSION_PIXEL_SIZE / base[1];
                [base[0] * scale, TARGET_EXPLOSION_PIXEL_SIZE]
            }
        };

        // Receptors + glow
        for i in 0..4 {
            let col_x_offset = ns.column_xs[i];

            let bop_timer = state.receptor_bop_timers[i];
            let bop_zoom = if bop_timer > 0.0 {
                let t = (0.11 - bop_timer) / 0.11;
                0.75 + (1.0 - 0.75) * t
            } else {
                1.0
            };

            let receptor_slot = &ns.receptor_off[i];
            let receptor_frame =
                receptor_slot.frame_index(state.total_elapsed_in_screen, state.current_beat);
            let receptor_uv = receptor_slot.uv_for_frame(receptor_frame);
            let receptor_size = scale_sprite(receptor_slot.size());
            let receptor_color = ns.receptor_pulse.color_for_beat(state.current_beat);
            actors.push(act!(sprite(receptor_slot.texture_key().to_string()):
                align(0.5, 0.5):
                xy(playfield_center_x + col_x_offset as f32, receptor_y):
                zoomto(receptor_size[0] as f32, receptor_size[1] as f32):
                zoom(bop_zoom):
                diffuse(
                    receptor_color[0],
                    receptor_color[1],
                    receptor_color[2],
                    receptor_color[3]
                ):
                rotationz(-receptor_slot.def.rotation_deg as f32):
                customtexturerect(
                    receptor_uv[0],
                    receptor_uv[1],
                    receptor_uv[2],
                    receptor_uv[3]
                ):
                z(Z_RECEPTOR)
            ));

            if let Some(hold_slot) = state.active_holds[i]
                .as_ref()
                .filter(|active| active_hold_is_engaged(active))
                .and_then(|active| {
                    let note_type = &state.notes[active.note_index].note_type;
                    let visuals = if matches!(note_type, NoteType::Roll) {
                        &ns.roll
                    } else {
                        &ns.hold
                    };
                    visuals
                        .explosion
                        .as_ref()
                        .or_else(|| ns.hold.explosion.as_ref())
                })
            {
                let hold_uv = hold_slot.uv_for_frame(0);
                let hold_size = scale_explosion(hold_slot.size());
                let receptor_rotation = ns
                    .receptor_off
                    .get(i)
                    .map(|slot| slot.def.rotation_deg as f32)
                    .unwrap_or(0.0);
                let base_rotation = hold_slot.def.rotation_deg as f32;
                let final_rotation = base_rotation + receptor_rotation;
                actors.push(act!(sprite(hold_slot.texture_key().to_string()):
                    align(0.5, 0.5):
                    xy(playfield_center_x + col_x_offset as f32, receptor_y):
                    zoomto(hold_size[0], hold_size[1]):
                    rotationz(-final_rotation):
                    customtexturerect(hold_uv[0], hold_uv[1], hold_uv[2], hold_uv[3]):
                    blend(normal):
                    z(Z_HOLD_EXPLOSION)
                ));
            }

            let glow_timer = state.receptor_glow_timers[i];
            if glow_timer > 0.0 {
                if let Some(glow_slot) = ns.receptor_glow.get(i).and_then(|slot| slot.as_ref()) {
                    let glow_frame =
                        glow_slot.frame_index(state.total_elapsed_in_screen, state.current_beat);
                    let glow_uv = glow_slot.uv_for_frame(glow_frame);
                    let glow_size = glow_slot.size();
                    let alpha = (glow_timer / RECEPTOR_GLOW_DURATION).powf(0.75);
                    actors.push(act!(sprite(glow_slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(playfield_center_x + col_x_offset as f32, receptor_y):
                        zoomto(glow_size[0] as f32, glow_size[1] as f32):
                        rotationz(-glow_slot.def.rotation_deg as f32):
                        customtexturerect(glow_uv[0], glow_uv[1], glow_uv[2], glow_uv[3]):
                        diffuse(1.0, 1.0, 1.0, alpha):
                        blend(add):
                        z(Z_HOLD_GLOW)
                    ));
                }
            }
        }

        // Tap explosions
        for i in 0..4 {
            if let Some(active) = state.tap_explosions[i].as_ref() {
                if let Some(explosion) = ns.tap_explosions.get(&active.window) {
                    let col_x_offset = ns.column_xs[i];
                    let anim_time = active.elapsed;
                    let slot = &explosion.slot;
                    let beat_for_anim = if slot.source.is_beat_based() {
                        (state.current_beat - active.start_beat).max(0.0)
                    } else {
                        state.current_beat
                    };
                    let frame = slot.frame_index(anim_time, beat_for_anim);
                    let uv = slot.uv_for_frame(frame);
                    let size = scale_explosion(slot.size());
                    let visual = explosion.animation.state_at(active.elapsed);
                    let rotation_deg = ns
                        .receptor_off
                        .get(i)
                        .map(|slot| slot.def.rotation_deg)
                        .unwrap_or(0);

                    actors.push(act!(sprite(slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(playfield_center_x + col_x_offset as f32, receptor_y):
                        zoomto(size[0], size[1]):
                        zoom(visual.zoom):
                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                        diffuse(
                            visual.diffuse[0],
                            visual.diffuse[1],
                            visual.diffuse[2],
                            visual.diffuse[3]
                        ):
                        rotationz(-(rotation_deg as f32)):
                        blend(normal):
                        z(101)
                    ));

                    let glow = visual.glow;
                    let glow_strength =
                        glow[0].abs() + glow[1].abs() + glow[2].abs() + glow[3].abs();
                    if glow_strength > f32::EPSILON {
                        actors.push(act!(sprite(slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(playfield_center_x + col_x_offset as f32, receptor_y):
                            zoomto(size[0], size[1]):
                            zoom(visual.zoom):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(glow[0], glow[1], glow[2], glow[3]):
                            rotationz(-(rotation_deg as f32)):
                            blend(add):
                            z(101)
                        ));
                    }
                }
            }
        }

        // Mine explosions
        for i in 0..4 {
            if let Some(active) = state.mine_explosions[i].as_ref() {
                let duration = MINE_EXPLOSION_DURATION.max(f32::EPSILON);
                let progress = (active.elapsed / duration).clamp(0.0, 1.0);
                let alpha = if progress < 0.5 {
                    1.0
                } else {
                    1.0 - ((progress - 0.5) / 0.5)
                }
                .clamp(0.0, 1.0);

                if alpha <= f32::EPSILON {
                    continue;
                }

                let rotation_progress = 180.0 * progress;
                let col_x_offset = ns.column_xs[i];
                let base_rotation = ns
                    .receptor_off
                    .get(i)
                    .map(|slot| slot.def.rotation_deg as f32)
                    .unwrap_or(0.0);
                let final_rotation = base_rotation + rotation_progress;

                actors.push(act!(sprite("hit_mine_explosion.png"):
                    align(0.5, 0.5):
                    xy(playfield_center_x + col_x_offset as f32, receptor_y):
                    zoomto(mine_explosion_size[0], mine_explosion_size[1]):
                    rotationz(-final_rotation):
                    diffuse(1.0, 1.0, 1.0, alpha):
                    blend(add):
                    z(Z_MINE_EXPLOSION)
                ));
            }
        }

        // Only consider notes that are currently in or near the lookahead window.
        let min_visible_index = state
            .arrows
            .iter()
            .filter_map(|v| v.first())
            .map(|a| a.note_index)
            .min()
            .unwrap_or(0);
        let max_visible_index = state.note_spawn_cursor.min(state.notes.len());
        // Build candidate indices: visible heads + active holds + decaying holds
        let mut render_indices: Vec<usize> = (min_visible_index..max_visible_index).collect();
        for a in &state.active_holds {
            if let Some(h) = a.as_ref() {
                if h.note_index < state.notes.len() { render_indices.push(h.note_index); }
            }
        }
        for &idx in &state.decaying_hold_indices {
            if idx < state.notes.len() { render_indices.push(idx); }
        }
        // Deduplicate while roughly preserving order
        {
            use std::collections::HashSet;
            let mut seen = HashSet::with_capacity(render_indices.len());
            render_indices.retain(|i| seen.insert(*i));
        }
        for note_index in render_indices {
            let note = &state.notes[note_index];
            if !matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                continue;
            }
            let Some(hold) = &note.hold else {
                continue;
            };

            if matches!(hold.result, Some(HoldResult::Held)) {
                continue;
            }

            let mut head_beat = note.beat;
            if hold.let_go_started_at.is_some() || hold.result == Some(HoldResult::LetGo) {
                head_beat = hold.last_held_beat.clamp(note.beat, hold.end_beat);
            }
            let head_y = compute_lane_y(head_beat);
            let tail_y = compute_lane_y(hold.end_beat);
            let head_is_top = head_y <= tail_y;
            let mut top = head_y.min(tail_y);
            let mut bottom = head_y.max(tail_y);
            if bottom < -200.0 || top > screen_height() + 200.0 {
                continue;
            }
            top = top.max(-400.0);
            bottom = bottom.min(screen_height() + 400.0);
            if bottom <= top {
                continue;
            }

            let col_x_offset = ns.column_xs[note.column];
            let active_state = state.active_holds[note.column]
                .as_ref()
                .filter(|h| h.note_index == note_index);
            let engaged = active_state.map(|h| active_hold_is_engaged(h)).unwrap_or(false);
            let use_active = active_state
                .map(|h| h.is_pressed && !h.let_go)
                .unwrap_or(false);

            let let_go_gray = ns.hold_let_go_gray_percent.clamp(0.0, 1.0);
            let hold_life = hold.life.clamp(0.0, 1.0);
            let hold_color_scale = let_go_gray + (1.0 - let_go_gray) * hold_life;
            let hold_diffuse = [hold_color_scale, hold_color_scale, hold_color_scale, 1.0];

            if engaged {
                if head_is_top {
                    top = top.max(receptor_y);
                } else {
                    bottom = bottom.min(receptor_y);
                }
            }

            if bottom <= top {
                continue;
            }

            let visuals = if matches!(note.note_type, NoteType::Roll) {
                &ns.roll
            } else {
                &ns.hold
            };

            let tail_slot = if use_active {
                visuals
                    .bottomcap_active
                    .as_ref()
                    .or_else(|| visuals.bottomcap_inactive.as_ref())
            } else {
                visuals
                    .bottomcap_inactive
                    .as_ref()
                    .or_else(|| visuals.bottomcap_active.as_ref())
            };

            // Prepare clipped body extents that respect the tail cap on the side
            // where the tail visually exists. For normal orientation (head above
            // tail), we clip the body against the tail cap at the bottom. For
            // reverse orientation (head below tail), we clip the body against the
            // tail cap at the top.
            let mut body_top = top;
            let mut body_bottom = bottom;
            if let Some(cap_slot) = tail_slot {
                let cap_size = scale_sprite(cap_slot.size());
                let cap_height = cap_size[1];
                if cap_height > std::f32::EPSILON {
                    // Keep the body from poking through the bottom cap, but allow
                    // a tiny overlap so the seam stays hidden like ITGmania.
                    if head_is_top {
                        // Tail visually at the bottom; trim the body bottom.
                        let cap_top = tail_y - cap_height * 0.5;
                        body_bottom = body_bottom.min(cap_top + 1.0);
                    } else {
                        // Tail visually at the top; trim the body top.
                        let cap_bottom = tail_y + cap_height * 0.5;
                        body_top = body_top.max(cap_bottom - 1.0);
                    }
                }
            }

            // Track the actual drawn body extents to decide whether the tail cap
            // should be rendered (prevents floating caps when no body segments were drawn).
            let mut rendered_body_top: Option<f32> = None;
            let mut rendered_body_bottom: Option<f32> = None;

            if body_bottom > body_top {
                if let Some(body_slot) = if use_active {
                    visuals
                        .body_active
                        .as_ref()
                        .or_else(|| visuals.body_inactive.as_ref())
                } else {
                    visuals
                        .body_inactive
                        .as_ref()
                        .or_else(|| visuals.body_active.as_ref())
                } {
                    let texture_size = body_slot.size();
                    let texture_width = texture_size[0].max(1) as f32;
                    let texture_height = texture_size[1].max(1) as f32;
                    if texture_width > std::f32::EPSILON && texture_height > std::f32::EPSILON {
                        let body_width = TARGET_ARROW_PIXEL_SIZE;
                        let scale = body_width / texture_width;
                        let segment_height = (texture_height * scale).max(std::f32::EPSILON);
                        let body_uv = body_slot.uv_for_frame(0);
                        let u0 = body_uv[0];
                        let u1 = body_uv[2];
                        let v_top = body_uv[1];
                        let v_bottom = body_uv[3];
                        let v_range = v_bottom - v_top;
                        let natural_top = if head_is_top { head_y } else { tail_y };
                        let natural_bottom = if head_is_top { tail_y } else { head_y };
                        let hold_length = (natural_bottom - natural_top).abs();
                        let visible_top_distance = if head_is_top {
                            (body_top - natural_top).clamp(0.0, hold_length)
                        } else {
                            (natural_bottom - body_top).clamp(0.0, hold_length)
                        };
                        let visible_bottom_distance = if head_is_top {
                            (body_bottom - natural_top).clamp(0.0, hold_length)
                        } else {
                            (natural_bottom - body_bottom).clamp(0.0, hold_length)
                        };

                        const SEGMENT_PHASE_EPS: f32 = 1e-4;
                        let max_segments = 2048;
                        let mut emitted = 0;

                        if head_is_top {
                            let mut phase = visible_top_distance / segment_height;
                            let phase_end = visible_bottom_distance / segment_height;

                            // Shift the fractional remainder of the hold body height to the first
                            // segment so the final segment can remain a full tile that lines up with
                            // the tail cap. This avoids a visible seam between the last two body
                            // segments. Base the offset on the full hold length so the amount trimmed
                            // from the first segment stays consistent even when the hold is only
                            // partially visible on screen.
                            let mut phase_offset = 0.0_f32;
                            let total_phase = hold_length / segment_height;
                            if total_phase >= 1.0 + SEGMENT_PHASE_EPS {
                                let fractional = total_phase.fract();
                                if fractional > SEGMENT_PHASE_EPS
                                    && (1.0 - fractional) > SEGMENT_PHASE_EPS
                                {
                                    phase_offset = 1.0 - fractional;
                                }
                            }

                            phase += phase_offset;
                            let phase_end_adjusted = phase_end + phase_offset;

                            while phase + SEGMENT_PHASE_EPS < phase_end_adjusted
                                && emitted < max_segments
                            {
                                let mut next_phase = (phase.floor() + 1.0).min(phase_end_adjusted);
                                if next_phase - phase < SEGMENT_PHASE_EPS {
                                    next_phase = phase_end_adjusted;
                                }
                                if next_phase - phase < SEGMENT_PHASE_EPS {
                                    break;
                                }

                                let distance_start = (phase - phase_offset) * segment_height;
                                let distance_end = (next_phase - phase_offset) * segment_height;
                                let y_start = natural_top + distance_start;
                                let y_end = natural_top + distance_end;
                                let segment_top = y_start.max(body_top);
                                let segment_bottom = y_end.min(body_bottom);
                                if segment_bottom - segment_top <= std::f32::EPSILON {
                                    phase = next_phase;
                                    continue;
                                }

                                let base_floor = phase.floor();
                                let start_fraction = (phase - base_floor).clamp(0.0, 1.0);
                                let end_fraction = (next_phase - base_floor).clamp(0.0, 1.0);
                                let mut v0 = v_top + v_range * start_fraction;
                                let mut v1 = v_top + v_range * end_fraction;
                                let segment_center = (segment_top + segment_bottom) * 0.5;
                                let segment_size = segment_bottom - segment_top;
                                let portion = (segment_size / segment_height).clamp(0.0, 1.0);
                                let is_last_segment = (body_bottom - segment_bottom).abs() <= 0.5
                                    || next_phase >= phase_end_adjusted - SEGMENT_PHASE_EPS;

                                if is_last_segment {
                                    if v_range >= 0.0 {
                                        v1 = v_bottom;
                                        v0 = v_bottom - v_range.abs() * portion;
                                    } else {
                                        v1 = v_bottom;
                                        v0 = v_bottom + v_range.abs() * portion;
                                    }
                                }

                                // Update drawn body extents
                                rendered_body_top = Some(match rendered_body_top {
                                    None => segment_top,
                                    Some(v) => v.min(segment_top),
                                });
                                rendered_body_bottom = Some(match rendered_body_bottom {
                                    None => segment_bottom,
                                    Some(v) => v.max(segment_bottom),
                                });

                                actors.push(act!(sprite(body_slot.texture_key().to_string()):
                                    align(0.5, 0.5):
                                    xy(playfield_center_x + col_x_offset as f32, segment_center):
                                    zoomto(body_width, segment_size):
                                    customtexturerect(u0, v0, u1, v1):
                                    diffuse(
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3]
                                    ):
                                    z(Z_HOLD_BODY)
                                ));

                                phase = next_phase;
                                emitted += 1;
                            }
                        } else {
                            // Reverse-oriented holds (head visually below tail): use a symmetric
                            // segmenter measured from the natural_top (tail) downward.
                            let mut phase = visible_top_distance / segment_height;
                            let phase_end = visible_bottom_distance / segment_height;
                            let phase_offset = 0.0_f32; // anchor seam at the tail side
                            let phase_end_adjusted = phase_end + phase_offset;

                            while phase + SEGMENT_PHASE_EPS < phase_end_adjusted
                                && emitted < max_segments
                            {
                                let mut next_phase = (phase.floor() + 1.0).min(phase_end_adjusted);
                                if next_phase - phase < SEGMENT_PHASE_EPS { next_phase = phase_end_adjusted; }
                                if next_phase - phase < SEGMENT_PHASE_EPS { break; }

                                let distance_start = (phase - phase_offset) * segment_height;
                                let distance_end = (next_phase - phase_offset) * segment_height;
                                let y_start = natural_top + distance_start;
                                let y_end = natural_top + distance_end;
                                let segment_top = y_start.max(body_top);
                                let segment_bottom = y_end.min(body_bottom);
                                if segment_bottom - segment_top <= std::f32::EPSILON {
                                    phase = next_phase;
                                    continue;
                                }

                                let base_floor = phase.floor();
                                let start_fraction = (phase - base_floor).clamp(0.0, 1.0);
                                let end_fraction = (next_phase - base_floor).clamp(0.0, 1.0);
                                let mut v0 = v_top + v_range * start_fraction;
                                let mut v1 = v_top + v_range * end_fraction;
                                let segment_center = (segment_top + segment_bottom) * 0.5;
                                let segment_size = segment_bottom - segment_top;
                                let portion = (segment_size / segment_height).clamp(0.0, 1.0);
                                let is_last_segment = (body_bottom - segment_bottom).abs() <= 0.5
                                    || next_phase >= phase_end_adjusted - SEGMENT_PHASE_EPS;

                                if is_last_segment {
                                    if v_range >= 0.0 {
                                        v1 = v_bottom;
                                        v0 = v_bottom - v_range.abs() * portion;
                                    } else {
                                        v1 = v_bottom;
                                        v0 = v_bottom + v_range.abs() * portion;
                                    }
                                }

                                // Update drawn body extents
                                rendered_body_top = Some(match rendered_body_top {
                                    None => segment_top,
                                    Some(v) => v.min(segment_top),
                                });
                                rendered_body_bottom = Some(match rendered_body_bottom {
                                    None => segment_bottom,
                                    Some(v) => v.max(segment_bottom),
                                });

                                actors.push(act!(sprite(body_slot.texture_key().to_string()):
                                    align(0.5, 0.5):
                                    xy(playfield_center_x + col_x_offset as f32, segment_center):
                                    zoomto(body_width, segment_size):
                                    customtexturerect(u0, v0, u1, v1):
                                    diffuse(
                                        hold_diffuse[0],
                                        hold_diffuse[1],
                                        hold_diffuse[2],
                                        hold_diffuse[3]
                                    ):
                                    z(Z_HOLD_BODY)
                                ));

                                phase = next_phase;
                                emitted += 1;
                            }
                        }
                    }
                }
            }

            if let Some(cap_slot) = tail_slot {
                let tail_position = tail_y;
                if tail_position > -400.0 && tail_position < screen_height() + 400.0 {
                    let cap_uv = cap_slot.uv_for_frame(0);
                    let cap_size = scale_sprite(cap_slot.size());
                    let cap_width = cap_size[0];
                    let mut cap_height = cap_size[1];
                    let mut cap_center = tail_position;
                    let u0 = cap_uv[0];
                    let u1 = cap_uv[2];
                    let mut v0 = cap_uv[1];
                    let mut v1 = cap_uv[3];

                    // Only draw the tail cap if the rendered body actually reaches
                    // the cap side. This prevents floating caps when no body segments
                    // were drawn near the tail due to scroll gimmicks.
                    let (rt, rb) = match (rendered_body_top, rendered_body_bottom) {
                        (Some(t), Some(b)) if b > t + 0.5 => (t, b),
                        _ => { continue; }
                    };
                    let cap_adjacent_ok = if head_is_top {
                        // Tail visually below; ensure the drawn body bottom is near the tail.
                        let dist = tail_y - rb;
                        dist >= -2.0 && dist <= cap_height + 2.0
                    } else {
                        // Tail visually above; ensure the drawn body top is near the tail.
                        let dist = rt - tail_y;
                        dist >= -2.0 && dist <= cap_height + 2.0
                    };
                    if !cap_adjacent_ok { continue; }

                    if cap_height > std::f32::EPSILON {
                        let mut cap_top = cap_center - cap_height * 0.5;
                        let mut cap_bottom = cap_center + cap_height * 0.5;
                        let v_span = v1 - v0;

                        if head_is_top {
                            let head_limit = top;
                            if head_limit > cap_top {
                                let trimmed = (head_limit - cap_top).clamp(0.0, cap_height);
                                if trimmed >= cap_height - std::f32::EPSILON {
                                    cap_height = 0.0;
                                } else if trimmed > std::f32::EPSILON {
                                    let fraction = trimmed / cap_height;
                                    v0 += v_span * fraction;
                                    cap_top += trimmed;
                                    cap_center = (cap_top + cap_bottom) * 0.5;
                                    cap_height = cap_bottom - cap_top;
                                }
                            }
                        } else {
                            let head_limit = bottom;
                            if head_limit < cap_bottom {
                                let trimmed = (cap_bottom - head_limit).clamp(0.0, cap_height);
                                if trimmed >= cap_height - std::f32::EPSILON {
                                    cap_height = 0.0;
                                } else if trimmed > std::f32::EPSILON {
                                    let fraction = trimmed / cap_height;
                                    v1 -= v_span * fraction;
                                    cap_bottom -= trimmed;
                                    cap_center = (cap_top + cap_bottom) * 0.5;
                                    cap_height = cap_bottom - cap_top;
                                }
                            }
                        }
                    }

                    if cap_height > std::f32::EPSILON {
                        actors.push(act!(sprite(cap_slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(playfield_center_x + col_x_offset as f32, cap_center):
                            zoomto(cap_width, cap_height):
                            customtexturerect(u0, v0, u1, v1):
                            diffuse(
                                hold_diffuse[0],
                                hold_diffuse[1],
                                hold_diffuse[2],
                                hold_diffuse[3]
                            ):
                            z(Z_HOLD_CAP)
                        ));
                    }
                }
            }

            if hold.let_go_started_at.is_some() || hold.result == Some(HoldResult::LetGo) {
                if head_y >= receptor_y - state.draw_distance_after_targets
                    && head_y <= receptor_y + state.draw_distance_before_targets
                {
                    let beat_fraction = note.beat.fract();
                    let quantization = match (beat_fraction * 192.0).round() as u32 {
                        0 | 192 => Quantization::Q4th,
                        96 => Quantization::Q8th,
                        48 | 144 => Quantization::Q16th,
                        24 | 72 | 120 | 168 => Quantization::Q32nd,
                        64 | 128 => Quantization::Q12th,
                        32 | 160 => Quantization::Q24th,
                        _ => Quantization::Q192nd,
                    };

                    let note_idx = (note.column % 4) * NUM_QUANTIZATIONS + quantization as usize;
                    if let Some(note_slot) = ns.notes.get(note_idx) {
                        let frame = note_slot
                            .frame_index(state.total_elapsed_in_screen, state.current_beat);
                        let uv = note_slot.uv_for_frame(frame);
                        let size = scale_sprite(note_slot.size());

                        actors.push(act!(sprite(note_slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(playfield_center_x + col_x_offset as f32, head_y):
                            zoomto(size[0] as f32, size[1] as f32):
                            rotationz(-note_slot.def.rotation_deg as f32):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(
                                hold_diffuse[0],
                                hold_diffuse[1],
                                hold_diffuse[2],
                                hold_diffuse[3]
                            ):
                            z(Z_TAP_NOTE)
                        ));
                    }
                }
            }
        }

        // Active arrows
        for column_arrows in &state.arrows {
            for arrow in column_arrows {
                // Use cached per-note timing to avoid per-frame timing queries
                let y_pos = match state.scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                        let note_time_chart = state.note_time_cache[arrow.note_index];
                        let time_diff_real = (current_time - note_time_chart) / rate;
                        // Negative because earlier times are above the receptor
                        receptor_y - time_diff_real * pps_chart
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = state.note_display_beat_cache[arrow.note_index];
                        let beat_diff_disp = note_disp_beat - curr_disp_beat;
                        receptor_y + beat_diff_disp * ScrollSpeedSetting::ARROW_SPACING * beatmod_multiplier
                    }
                };

                if y_pos < receptor_y - state.draw_distance_after_targets
                    || y_pos > receptor_y + state.draw_distance_before_targets
                {
                    continue;
                }

                let col_x_offset = ns.column_xs[arrow.column];

                if matches!(arrow.note_type, NoteType::Mine) {
                    let fill_slot = ns.mines.get(arrow.column).and_then(|slot| slot.as_ref());
                    let frame_slot = ns
                        .mine_frames
                        .get(arrow.column)
                        .and_then(|slot| slot.as_ref());

                    if fill_slot.is_none() && frame_slot.is_none() {
                        continue;
                    }

                    let base_rotation = fill_slot
                        .map(|slot| -slot.def.rotation_deg as f32)
                        .or_else(|| frame_slot.map(|slot| -slot.def.rotation_deg as f32))
                        .unwrap_or(0.0);
                    let time = state.total_elapsed_in_screen;
                    let beat = state.current_beat;

                    let circle_reference = frame_slot
                        .map(|slot| scale_sprite(slot.size()))
                        .or_else(|| fill_slot.map(|slot| scale_sprite(slot.size())))
                        .unwrap_or([TARGET_ARROW_PIXEL_SIZE, TARGET_ARROW_PIXEL_SIZE]);

                    if let Some(slot) = fill_slot {
                        if let Some(fill_state) = mine_fill_state(slot, state.current_beat) {
                            let width = circle_reference[0] * MINE_CORE_SIZE_RATIO;
                            let height = circle_reference[1] * MINE_CORE_SIZE_RATIO;

                            for layer_idx in (0..MINE_FILL_LAYERS).rev() {
                                let color = fill_state.layers[layer_idx];
                                let scale = (layer_idx as f32 + 1.0) / MINE_FILL_LAYERS as f32;
                                let layer_width = width * scale;
                                let layer_height = height * scale;

                                if layer_width <= 0.0 || layer_height <= 0.0 {
                                    continue;
                                }

                                actors.push(act!(sprite("circle.png"):
                                    align(0.5, 0.5):
                                    xy(playfield_center_x + col_x_offset as f32, y_pos):
                                    zoomto(layer_width, layer_height):
                                    diffuse(color[0], color[1], color[2], 1.0):
                                    z(Z_TAP_NOTE - 2)
                                ));
                            }
                        } else {
                            let frame = slot.frame_index(time, beat);
                            let uv = slot.uv_for_frame(frame);
                            let size = scale_sprite(slot.size());
                            let width = size[0];
                            let height = size[1];
                            let rotation = base_rotation - time * 45.0;

                            actors.push(act!(sprite(slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(playfield_center_x + col_x_offset as f32, y_pos):
                                zoomto(width, height):
                                rotationz(rotation):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                diffuse(1.0, 1.0, 1.0, 0.9):
                                z(Z_TAP_NOTE - 1)
                            ));
                        }
                    }

                    if let Some(slot) = frame_slot {
                        let frame = slot.frame_index(time, beat);
                        let uv = slot.uv_for_frame(frame);
                        let size = scale_sprite(slot.size());
                        let rotation = base_rotation + time * 120.0;

                        actors.push(act!(sprite(slot.texture_key().to_string()):
                            align(0.5, 0.5):
                            xy(playfield_center_x + col_x_offset as f32, y_pos):
                            zoomto(size[0], size[1]):
                            rotationz(rotation):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            z(Z_TAP_NOTE)
                        ));
                    }

                    continue;
                }

                let beat_fraction = arrow.beat.fract();
                let quantization = match (beat_fraction * 192.0).round() as u32 {
                    0 | 192 => Quantization::Q4th,
                    96 => Quantization::Q8th,
                    48 | 144 => Quantization::Q16th,
                    24 | 72 | 120 | 168 => Quantization::Q32nd,
                    64 | 128 => Quantization::Q12th,
                    32 | 160 => Quantization::Q24th,
                    _ => Quantization::Q192nd,
                };

                let note_idx = (arrow.column % 4) * NUM_QUANTIZATIONS + quantization as usize;
                if let Some(note_slot) = ns.notes.get(note_idx) {
                    let note_frame =
                        note_slot.frame_index(state.total_elapsed_in_screen, state.current_beat);
                    let note_uv = note_slot.uv_for_frame(note_frame);
                    let note_size = scale_sprite(note_slot.size());

                    actors.push(act!(sprite(note_slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(playfield_center_x + col_x_offset as f32, y_pos):
                        zoomto(note_size[0] as f32, note_size[1] as f32):
                        rotationz(-note_slot.def.rotation_deg as f32):
                        customtexturerect(note_uv[0], note_uv[1], note_uv[2], note_uv[3]):
                        z(Z_TAP_NOTE)
                    ));
                }
            }
        }
    }

    // Combo Milestone Explosions (100 / 1000 combo)
    if !state.combo_milestones.is_empty() {
        let combo_center_x = playfield_center_x;
        let combo_center_y = screen_center_y() + 30.0;
        let player_color = state.player_color;

        let ease_out_quad = |t: f32| -> f32 {
            let t = t.clamp(0.0, 1.0);
            1.0 - (1.0 - t).powi(2)
        };

        for milestone in &state.combo_milestones {
            match milestone.kind {
                ComboMilestoneKind::Hundred => {
                    let elapsed = milestone.elapsed;
                    let explosion_duration = 0.5_f32;
                    if elapsed <= explosion_duration {
                        let progress = (elapsed / explosion_duration).clamp(0.0, 1.0);
                        let zoom = 2.0 - progress;
                        let alpha = (0.5 * (1.0 - progress)).max(0.0);
                        for &direction in &[1.0_f32, -1.0_f32] {
                            let rotation = 90.0 * direction * progress;
                            actors.push(act!(sprite("combo_explosion.png"):
                                align(0.5, 0.5):
                                xy(combo_center_x, combo_center_y):
                                zoom(zoom):
                                rotationz(rotation):
                                diffuse(1.0, 1.0, 1.0, alpha):
                                blend(add):
                                z(89)
                            ));
                        }
                    }

                    if elapsed <= COMBO_HUNDRED_MILESTONE_DURATION {
                        let progress = (elapsed / COMBO_HUNDRED_MILESTONE_DURATION).clamp(0.0, 1.0);
                        let eased = ease_out_quad(progress);
                        let zoom = 0.25 + (2.0 - 0.25) * eased;
                        let alpha = (0.6 * (1.0 - eased)).max(0.0);
                        let rotation = 10.0 + (0.0 - 10.0) * eased;
                        actors.push(act!(sprite("combo_100milestone_splode.png"):
                            align(0.5, 0.5):
                            xy(combo_center_x, combo_center_y):
                            zoom(zoom):
                            rotationz(rotation):
                            diffuse(
                                player_color[0],
                                player_color[1],
                                player_color[2],
                                alpha
                            ):
                            blend(add):
                            z(89)
                        ));

                        let mini_duration = 0.4_f32;
                        if elapsed <= mini_duration {
                            let mini_progress = (elapsed / mini_duration).clamp(0.0, 1.0);
                            let mini_zoom = 0.25 + (1.8 - 0.25) * mini_progress;
                            let mini_alpha = (1.0 - mini_progress).max(0.0);
                            let mini_rotation = 10.0 + (0.0 - 10.0) * mini_progress;
                            actors.push(act!(sprite("combo_100milestone_minisplode.png"):
                                align(0.5, 0.5):
                                xy(combo_center_x, combo_center_y):
                                zoom(mini_zoom):
                                rotationz(mini_rotation):
                                diffuse(
                                    player_color[0],
                                    player_color[1],
                                    player_color[2],
                                    mini_alpha
                                ):
                                blend(add):
                                z(89)
                            ));
                        }
                    }
                }
                ComboMilestoneKind::Thousand => {
                    let elapsed = milestone.elapsed;
                    if elapsed <= COMBO_THOUSAND_MILESTONE_DURATION {
                        let progress =
                            (elapsed / COMBO_THOUSAND_MILESTONE_DURATION).clamp(0.0, 1.0);
                        let zoom = 0.25 + (3.0 - 0.25) * progress;
                        let alpha = (0.7 * (1.0 - progress)).max(0.0);
                        let x_offset = 100.0 * progress;
                        for &direction in &[1.0_f32, -1.0_f32] {
                            let final_x = combo_center_x + x_offset * direction;
                            actors.push(act!(sprite("combo_1000milestone_swoosh.png"):
                                align(0.5, 0.5):
                                xy(final_x, combo_center_y):
                                zoom(zoom):
                                zoomx(zoom * direction):
                                diffuse(
                                    player_color[0],
                                    player_color[1],
                                    player_color[2],
                                    alpha
                                ):
                                blend(add):
                                z(89)
                            ));
                        }
                    }
                }
            }
        }
    }

    // Combo
    if state.miss_combo >= SHOW_COMBO_AT {
        actors.push(act!(text:
            font("wendy_combo"): settext(state.miss_combo.to_string()):
            align(0.5, 0.5): xy(playfield_center_x, screen_center_y() + 30.0):
            zoom(0.75): horizalign(center): shadowlength(1.0):
            diffuse(1.0, 0.0, 0.0, 1.0):
            z(90)
        ));
    } else if state.combo >= SHOW_COMBO_AT {
        let (color1, color2) = if let Some(fc_grade) = &state.full_combo_grade {
            match fc_grade {
                JudgeGrade::Fantastic => (color::rgba_hex("#C8FFFF"), color::rgba_hex("#6BF0FF")),
                JudgeGrade::Excellent => (color::rgba_hex("#FDFFC9"), color::rgba_hex("#FDDB85")),
                JudgeGrade::Great => (color::rgba_hex("#C9FFC9"), color::rgba_hex("#94FEC1")),
                _ => ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
            }
        } else {
            ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0])
        };

        let effect_period = 0.8;
        let t = (state.total_elapsed_in_screen / effect_period).fract();
        let anim_t = ((t * 2.0 * std::f32::consts::PI).sin() + 1.0) / 2.0;

        let final_color = [
            color1[0] + (color2[0] - color1[0]) * anim_t,
            color1[1] + (color2[1] - color1[1]) * anim_t,
            color1[2] + (color2[2] - color1[2]) * anim_t,
            1.0,
        ];

        actors.push(act!(text:
            font("wendy_combo"): settext(state.combo.to_string()):
            align(0.5, 0.5): xy(playfield_center_x, screen_center_y() + 30.0):
            zoom(0.75): horizalign(center): shadowlength(1.0):
            diffuse(final_color[0], final_color[1], final_color[2], final_color[3]):
            z(90)
        ));
    }

    // Judgment Sprite (Love)
    if let Some(render_info) = &state.last_judgment {
        let judgment = &render_info.judgment;
        let elapsed = render_info.judged_at.elapsed().as_secs_f32();
        if elapsed < 0.9 {
            let zoom = if elapsed < 0.1 {
                let t = elapsed / 0.1;
                let ease_t = 1.0 - (1.0 - t).powi(2);
                0.8 + (0.75 - 0.8) * ease_t
            } else if elapsed < 0.7 {
                0.75
            } else {
                let t = (elapsed - 0.7) / 0.2;
                let ease_t = t.powi(2);
                0.75 * (1.0 - ease_t)
            };

            let offset_sec = judgment.time_error_ms / 1000.0;
            let mut frame_base = judgment.grade as usize;
            if judgment.grade >= JudgeGrade::Excellent {
                frame_base += 1;
            }
            let frame_offset = if offset_sec < 0.0 { 0 } else { 1 };
            let linear_index = (frame_base * 2 + frame_offset) as u32;

            actors.push(act!(sprite("judgements/Love 2x7 (doubleres).png"):
                align(0.5, 0.5): xy(playfield_center_x, screen_center_y() - 30.0):
                z(200): zoomtoheight(76.0): setstate(linear_index): zoom(zoom)
            ));
        }
    }

    let hold_judgment_y = screen_center_y() + HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER;
    for (column, render_info) in state.hold_judgments.iter().enumerate() {
        let Some(render_info) = render_info else {
            continue;
        };

        let elapsed = render_info.triggered_at.elapsed().as_secs_f32();
        if elapsed >= HOLD_JUDGMENT_TOTAL_DURATION {
            continue;
        }

        let zoom = if elapsed < 0.3 {
            let progress = (elapsed / 0.3).clamp(0.0, 1.0);
            HOLD_JUDGMENT_INITIAL_ZOOM
                + progress * (HOLD_JUDGMENT_FINAL_ZOOM - HOLD_JUDGMENT_INITIAL_ZOOM)
        } else {
            HOLD_JUDGMENT_FINAL_ZOOM
        };

        let frame_index = match render_info.result {
            HoldResult::Held => 0,
            HoldResult::LetGo => 1,
        } as u32;

        let column_offset = state
            .noteskin
            .as_ref()
            .and_then(|ns| ns.column_xs.get(column))
            .map(|&x| x as f32)
            .unwrap_or_else(|| ((column as f32) - 1.5) * TARGET_ARROW_PIXEL_SIZE);

        actors.push(act!(sprite("hold_judgements/Love 1x2 (doubleres).png"):
            align(0.5, 0.5):
            xy(playfield_center_x + column_offset, hold_judgment_y):
            z(195):
            setstate(frame_index):
            zoom(zoom):
            diffusealpha(1.0)
        ));
    }

    // Difficulty Box
    let x = screen_center_x() - widescale(292.5, 342.5);
    let y = 56.0;

    let difficulty_index = color::FILE_DIFFICULTY_NAMES
        .iter()
        .position(|&name| name.eq_ignore_ascii_case(&state.chart.difficulty))
        .unwrap_or(2);
    let difficulty_color_index = state.active_color_index - (4 - difficulty_index) as i32;
    let difficulty_color = color::simply_love_rgba(difficulty_color_index);
    let meter_text = state.chart.meter.to_string();

    actors.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [x, y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children: vec![
            act!(quad:
                align(0.5, 0.5): xy(0.0, 0.0): zoomto(30.0, 30.0):
                diffuse(difficulty_color[0], difficulty_color[1], difficulty_color[2], 1.0)
            ),
            act!(text:
                font("wendy"): settext(meter_text): align(0.5, 0.5): xy(0.0, 0.0):
                zoom(0.4): diffuse(0.0, 0.0, 0.0, 1.0)
            ),
        ],
        background: None,
        z: 90,
    });

    // Score Display (P1)
    let clamped_width = screen_width().clamp(640.0, 854.0);
    let score_x = screen_center_x() - clamped_width / 4.3;
    let score_y = 56.0;

    let score_percent = (judgment::calculate_itg_score_percent(
        &state.scoring_counts,
        state.holds_held_for_score,
        state.rolls_held_for_score,
        state.mines_hit_for_score,
        state.possible_grade_points,
    ) * 100.0) as f32;
    let percent_text = format!("{:.2}", score_percent);

    actors.push(act!(text:
        font("wendy_monospace_numbers"): settext(percent_text):
        align(1.0, 1.0): xy(score_x, score_y):
        zoom(0.5): horizalign(right): z(90)
    ));

    // Current BPM Display (1:1 with Simply Love)
    {
        let base_bpm = state.timing.get_bpm_for_beat(state.current_beat);
        let display_bpm = if base_bpm.is_finite() {
            (base_bpm * if state.music_rate.is_finite() { state.music_rate } else { 1.0 }).round() as i32
        } else {
            0
        };

        let bpm_text = display_bpm.to_string();

        // Final world-space positions derived from analyzing the SM Lua transforms.
        // The parent frame is bottom-aligned to y=52, and its children are positioned
        // relative to that y-coordinate, with a zoom of 1.33 applied to the whole group.
        let frame_origin_y = 51.0;
        let frame_zoom = 1.33;

        // The BPM text is at y=0 relative to the frame's origin. Its final position is just the origin.
        let bpm_center_y = frame_origin_y;
        // The Rate text is at y=12 relative to the frame's origin. Its offset is scaled by the frame's zoom.
        let rate_center_y = frame_origin_y + (12.0 * frame_zoom);

        let bpm_final_zoom = 1.0 * frame_zoom;
        let rate_final_zoom = 0.5 * frame_zoom;

        let bpm_x = screen_center_x();

        actors.push(act!(text:
            font("miso"): settext(bpm_text):
            align(0.5, 0.5): xy(bpm_x, bpm_center_y):
            zoom(bpm_final_zoom): horizalign(center): z(90)
        ));

        let rate = if state.music_rate.is_finite() { state.music_rate } else { 1.0 };
        let rate_text = if (rate - 1.0).abs() > 0.001 {
            format!("{rate:.2}x rate")
        } else {
            String::new()
        };

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
        let mut frame_children = Vec::new();

        frame_children.push(act!(quad: align(0.5, 0.5): xy(w / 2.0, h / 2.0): zoomto(w, h): diffuse(1.0, 1.0, 1.0, 1.0): z(0) ));
        frame_children.push(act!(quad: align(0.5, 0.5): xy(w / 2.0, h / 2.0): zoomto(w - 4.0, h - 4.0): diffuse(0.0, 0.0, 0.0, 1.0): z(1) ));

        if state.song.total_length_seconds > 0 && state.current_music_time >= 0.0 {
            let progress =
                (state.current_music_time / state.song.total_length_seconds as f32).clamp(0.0, 1.0);
            frame_children.push(act!(quad:
                align(0.0, 0.5): xy(2.0, h / 2.0): zoomto((w - 4.0) * progress, h - 4.0):
                diffuse(state.player_color[0], state.player_color[1], state.player_color[2], 1.0): z(2)
            ));
        }

        let full_title = if state.song.subtitle.trim().is_empty() {
            state.song.title.clone()
        } else {
            format!("{} {}", state.song.title, state.song.subtitle)
        };
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

    // --- Life Meter (P1) ---
    {
        let w = 136.0;
        let h = 18.0;
        let meter_cx = screen_center_x() - widescale(238.0, 288.0);
        let meter_cy = 20.0;

        // Frames/border
        actors.push(act!(quad: align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w + 4.0, h + 4.0): diffuse(1.0, 1.0, 1.0, 1.0): z(90) ));
        actors.push(act!(quad: align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w, h): diffuse(0.0, 0.0, 0.0, 1.0): z(91) ));

        // Latch-to-zero for rendering the very frame we die.
        let dead = state.is_failing || state.life <= 0.0;
        let life_for_render = if dead {
            0.0
        } else {
            state.life.clamp(0.0, 1.0)
        };

        let is_hot = !dead && life_for_render >= 1.0;
        let life_color = if is_hot {
            [1.0, 1.0, 1.0, 1.0]
        } else {
            state.player_color
        };

        let filled_width = w * life_for_render;

        // Never draw swoosh if dead OR nothing to fill.
        if filled_width > 0.0 && !dead {
            let bps = state.timing.get_bpm_for_beat(state.current_beat) / 60.0;
            let swoosh_alpha = if is_hot { 1.0 } else { 0.2 };
            actors.push(act!(sprite("swoosh.png"):
                align(0.0, 0.5):
                xy(meter_cx - w / 2.0, meter_cy):
                zoomto(filled_width, h):
                diffusealpha(swoosh_alpha):
                texcoordvelocity(-(bps * 0.5), 0.0):
                z(93)
            ));

            actors.push(act!(quad:
                align(0.0, 0.5):
                xy(meter_cx - w / 2.0, meter_cy):
                zoomto(filled_width, h):
                diffuse(life_color[0], life_color[1], life_color[2], 1.0):
                z(92)
            ));
        }
    }

    actors.push(screen_bar::build(ScreenBarParams {
        title: "",
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        fg_color: [1.0; 4],
        left_text: Some(&profile.display_name),
        center_text: None,
        right_text: None,
        left_avatar: None,
    }));

    actors.extend(build_side_pane(state, asset_manager));
    actors.extend(build_holds_mines_rolls_pane(state, asset_manager));

    actors
}

fn build_holds_mines_rolls_pane(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    if !is_wide() {
        return vec![];
    }
    let mut actors = Vec::new();

    let sidepane_center_x = screen_width() * 0.75;
    let sidepane_center_y = screen_center_y() + 80.0;
    let logical_screen_width = screen_width();
    let clamped_width = logical_screen_width.clamp(640.0, 854.0);
    let nf_center_x = screen_center_x() - (clamped_width * 0.25);
    let note_field_is_centered = (nf_center_x - screen_center_x()).abs() < 1.0;
    let is_ultrawide = screen_width() / screen_height() > (21.0 / 9.0);
    let banner_data_zoom = if note_field_is_centered && is_wide() && !is_ultrawide {
        let ar = screen_width() / screen_height();
        let t = ((ar - (16.0 / 10.0)) / ((16.0 / 9.0) - (16.0 / 10.0))).clamp(0.0, 1.0);
        0.825 + (0.925 - 0.825) * t
    } else {
        1.0
    };
    let local_x = 155.0;
    let local_y = -112.0;
    let frame_cx = sidepane_center_x + (local_x * banner_data_zoom);
    let frame_cy = sidepane_center_y + (local_y * banner_data_zoom);
    let frame_zoom = banner_data_zoom;

    let categories = [
        ("holds", state.holds_held, state.holds_total),
        ("mines", state.mines_avoided, state.mines_total),
        ("rolls", state.rolls_held, state.rolls_total),
    ];

    let largest_count = categories
        .iter()
        .map(|(_, achieved, total)| (*achieved).max(*total))
        .max()
        .unwrap_or(0);
    let digits_needed = if largest_count == 0 {
        1
    } else {
        (largest_count as f32).log10().floor() as usize + 1
    };
    let digits_to_fmt = digits_needed.clamp(3, 4);
    let row_height = 28.0 * frame_zoom;
    let mut children = Vec::new();

    asset_manager.with_fonts(|all_fonts| asset_manager.with_font("wendy_screenevaluation", |metrics_font| {
        let value_zoom = 0.4 * frame_zoom;
        let label_zoom = 0.833 * frame_zoom;
        let gray = color::rgba_hex("#5A6166");
        let white = [1.0, 1.0, 1.0, 1.0];

        // --- HYBRID LAYOUT LOGIC ---
        // 1. Measure real character widths for number layout.
        let digit_width = font::measure_line_width_logical(metrics_font, "0", all_fonts) as f32 * value_zoom;
        if digit_width <= 0.0 { return; }
        let slash_width = font::measure_line_width_logical(metrics_font, "/", all_fonts) as f32 * value_zoom;

        // 2. Use a hardcoded width for calculating the label's position (for theme parity).
        const LOGICAL_CHAR_WIDTH_FOR_LABEL: f32 = 36.0;
        let fixed_char_width_scaled_for_label = LOGICAL_CHAR_WIDTH_FOR_LABEL * value_zoom;

        for (i, (label_text, achieved, total)) in categories.iter().enumerate() {
            let item_y = (i as f32 - 1.0) * row_height;
            let right_anchor_x = 0.0;
            let mut cursor_x = right_anchor_x;

            let possible_str = format!("{:0width$}", *total as usize, width = digits_to_fmt);
            let achieved_str = format!("{:0width$}", *achieved as usize, width = digits_to_fmt);

            // --- Layout Numbers using MEASURED widths ---
            // 1. Draw "possible" number (right-most part)
            let first_nonzero_possible = possible_str.find(|c: char| c != '0').unwrap_or(possible_str.len());
            for (char_idx, ch) in possible_str.chars().rev().enumerate() {
                let is_dim = if *total == 0 { char_idx > 0 } else {
                    let original_index = digits_to_fmt - 1 - char_idx;
                    original_index < first_nonzero_possible
                };
                let color = if is_dim { gray } else { white };
                let x_pos = cursor_x - (char_idx as f32 * digit_width);
                children.push(act!(text:
                    font("wendy_screenevaluation"): settext(ch.to_string()):
                    align(1.0, 0.5): xy(x_pos, item_y):
                    zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3])
                ));
            }
            cursor_x -= possible_str.len() as f32 * digit_width;

            // 2. Draw slash
            children.push(act!(text: font("wendy_screenevaluation"): settext("/"): align(1.0, 0.5): xy(cursor_x, item_y): zoom(value_zoom): diffuse(gray[0], gray[1], gray[2], gray[3])));
            cursor_x -= slash_width;

            // 3. Draw "achieved" number
            let achieved_block_right_x = cursor_x;
            let first_nonzero_achieved = achieved_str.find(|c: char| c != '0').unwrap_or(achieved_str.len());
            for (char_idx, ch) in achieved_str.chars().rev().enumerate() {
                let is_dim = if *achieved == 0 { char_idx > 0 } else {
                    let original_index = digits_to_fmt - 1 - char_idx;
                    original_index < first_nonzero_achieved
                };
                let color = if is_dim { gray } else { white };
                let x_pos = achieved_block_right_x - (char_idx as f32 * digit_width);
                children.push(act!(text:
                    font("wendy_screenevaluation"): settext(ch.to_string()):
                    align(1.0, 0.5): xy(x_pos, item_y):
                    zoom(value_zoom): diffuse(color[0], color[1], color[2], color[3])
                ));
            }

            // --- Position Label using HARDCODED width assumption ---
            let total_value_width_for_label = (achieved_str.len() + 1 + possible_str.len()) as f32 * fixed_char_width_scaled_for_label;
            let label_x = right_anchor_x - total_value_width_for_label - (10.0 * frame_zoom);

            children.push(act!(text:
                font("miso"): settext(*label_text): align(1.0, 0.5): xy(label_x, item_y):
                zoom(label_zoom): horizalign(right): diffuse(white[0], white[1], white[2], white[3])
            ));
        }
    }));

    actors.push(Actor::Frame {
        align: [0.5, 0.5],
        offset: [frame_cx, frame_cy],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z: 70,
    });
    actors
}

fn build_side_pane(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    if !is_wide() {
        return vec![];
    }
    let mut actors = Vec::new();

    let sidepane_center_x = screen_width() * 0.75;
    let sidepane_center_y = screen_center_y() + 80.0;
    let logical_screen_width = screen_width();
    let clamped_width = logical_screen_width.clamp(640.0, 854.0);
    let nf_center_x = screen_center_x() - (clamped_width * 0.25);
    let note_field_is_centered = (nf_center_x - screen_center_x()).abs() < 1.0;
    let is_ultrawide = screen_width() / screen_height() > (21.0 / 9.0);
    let banner_data_zoom = if note_field_is_centered && is_wide() && !is_ultrawide {
        let ar = screen_width() / screen_height();
        let t = ((ar - (16.0 / 10.0)) / ((16.0 / 9.0) - (16.0 / 10.0))).clamp(0.0, 1.0);
        0.825 + (0.925 - 0.825) * t
    } else {
        1.0
    };

    let judgments_local_x = -widescale(152.0, 204.0);
    let final_judgments_center_x = sidepane_center_x + (judgments_local_x * banner_data_zoom);
    let final_judgments_center_y = sidepane_center_y;
    let parent_local_zoom = 0.8;
    let final_text_base_zoom = banner_data_zoom * parent_local_zoom;

    let total_tapnotes = state.chart.stats.total_steps as f32;
    let digits = if total_tapnotes > 0.0 {
        (total_tapnotes.log10().floor() as usize + 1).max(4)
    } else {
        4
    };
    let extra_digits = digits.saturating_sub(4) as f32;
    let base_label_local_x_offset = 80.0;
    const LABEL_DIGIT_STEP: f32 = 16.0;
    const NUMBER_TO_LABEL_GAP: f32 = 8.0;
    let base_numbers_local_x_offset = base_label_local_x_offset - NUMBER_TO_LABEL_GAP;
    let row_height = 35.0;
    let y_base = -280.0;

    asset_manager.with_fonts(|all_fonts| asset_manager.with_font("wendy_screenevaluation", |f| {
        let numbers_zoom = final_text_base_zoom * 0.5;
        let max_digit_w = (font::measure_line_width_logical(f, "0", all_fonts) as f32) * numbers_zoom;
        if max_digit_w <= 0.0 { return; }

        let digit_local_width = max_digit_w / final_text_base_zoom;
        let label_local_x_offset = base_label_local_x_offset + (extra_digits * LABEL_DIGIT_STEP);
        let label_world_x = final_judgments_center_x + (label_local_x_offset * final_text_base_zoom);
        let numbers_local_x_offset = base_numbers_local_x_offset + (extra_digits * digit_local_width);
        let numbers_cx = final_judgments_center_x + (numbers_local_x_offset * final_text_base_zoom);

        for (index, grade) in JUDGMENT_ORDER.iter().enumerate() {
            let info = JUDGMENT_INFO.get(grade).unwrap();
            let count = *state.judgment_counts.get(grade).unwrap_or(&0);

            let local_y = y_base + (index as f32 * row_height);
            let world_y = final_judgments_center_y + (local_y * final_text_base_zoom);

            let bright = info.color;
            let dim = color::rgba_hex(color::JUDGMENT_DIM_HEX[index]);
            let full_number_str = format!("{:0width$}", count, width = digits);

            for (i, ch) in full_number_str.chars().enumerate() {
                let is_dim = if count == 0 { i < digits - 1 } else {
                    let first_nonzero = full_number_str.find(|c: char| c != '0').unwrap_or(full_number_str.len());
                    i < first_nonzero
                };
                let color = if is_dim { dim } else { bright };
                let index_from_right = digits - 1 - i;
                let cell_right_x = numbers_cx - (index_from_right as f32 * max_digit_w);

                actors.push(act!(text:
                    font("wendy_screenevaluation"): settext(ch.to_string()):
                    align(1.0, 0.5): xy(cell_right_x, world_y): zoom(numbers_zoom):
                    diffuse(color[0], color[1], color[2], color[3]): z(71)
                ));
            }

            let label_world_y = world_y + (1.0 * final_text_base_zoom);
            let label_zoom = final_text_base_zoom * 0.833;

            actors.push(act!(text:
                font("miso"): settext(info.label): align(0.0, 0.5):
                xy(label_world_x, label_world_y): zoom(label_zoom):
                maxwidth(72.0 * final_text_base_zoom): horizalign(left):
                diffuse(bright[0], bright[1], bright[2], bright[3]):
                z(71)
            ));
        }

        // --- Time Display (Remaining / Total) ---
        {
            let local_y = -40.0 * banner_data_zoom;

            let total_seconds = state.song.total_length_seconds.max(0) as f32;
            let total_time_str = format_game_time(total_seconds, total_seconds);

            let remaining_seconds = if let Some(fail_time) = state.fail_time {
                (total_seconds - fail_time.max(0.0)).max(0.0)
            } else {
                (total_seconds - state.current_music_time.max(0.0)).max(0.0)
            };
            let remaining_time_str = format_game_time(remaining_seconds, total_seconds);

            let font_name = "miso";
            let text_zoom = banner_data_zoom * 0.833;

            let numbers_block_width = (digits as f32) * max_digit_w;
            let numbers_left_x = numbers_cx - numbers_block_width + 2.0;

            let red_color = color::rgba_hex("#ff3030");
            let white_color = [1.0, 1.0, 1.0, 1.0];
            let remaining_color = if state.is_failing { red_color } else { white_color };

            // --- Total Time Row ---
            let y_pos_total = sidepane_center_y + local_y + 13.0;
            let label_offset = 29.0;

            actors.push(act!(text: font(font_name): settext(total_time_str):
                align(0.0, 0.5): horizalign(left):
                xy(numbers_left_x, y_pos_total):
                z(71):
                diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
            ));
            actors.push(act!(text: font(font_name): settext(" song"):
                align(0.0, 0.5): horizalign(left):
                xy(numbers_left_x + label_offset, y_pos_total + 1.0):
                zoom(text_zoom): z(71):
                diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
            ));

            // --- Remaining Time Row ---
            let y_pos_remaining = sidepane_center_y + local_y - 7.0;

            actors.push(act!(text: font(font_name): settext(remaining_time_str):
                align(0.0, 0.5): horizalign(left):
                xy(numbers_left_x, y_pos_remaining):
                z(71):
                diffuse(remaining_color[0], remaining_color[1], remaining_color[2], remaining_color[3])
            ));
            actors.push(act!(text: font(font_name): settext(" remaining"):
                align(0.0, 0.5): horizalign(left):
                xy(numbers_left_x + label_offset, y_pos_remaining + 1.0):
                zoom(text_zoom): z(71):
                diffuse(white_color[0], white_color[1], white_color[2], white_color[3])
            ));
        }
    }));

    // --- Peak NPS Display (as seen in Simply Love's Step Statistics) ---
    if is_wide() {
        let scaled_peak = (state.chart.max_nps as f32 * state.music_rate).max(0.0) as f32;
        let peak_nps_text = format!("Peak NPS: {:.2}", scaled_peak);

        // Positioned based on visual parity with Simply Love's Step Statistics pane
        // for Player 1, which is on the right side of the screen.
        let peak_nps_x = screen_width() - 59.0;
        let peak_nps_y = screen_center_y() + 126.0;

        actors.push(act!(text:
            font("miso"):
            settext(peak_nps_text):
            // Pivot point is the text's right-center
            align(1.0, 0.5):
            xy(peak_nps_x, peak_nps_y):
            zoom(0.9):
            diffuse(1.0, 1.0, 1.0, 1.0):
            // Align the text content itself to the right
            horizalign(right):
            z(200)
        ));
    }

    actors
}
