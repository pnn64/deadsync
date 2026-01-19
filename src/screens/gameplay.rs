use crate::act;
use crate::assets::{self, AssetManager};
use crate::core::space::widescale;
use crate::core::space::*;
use crate::game::judgment;
use crate::game::judgment::{JudgeGrade, TimingWindow};
use crate::game::note::{HoldResult, NoteType};
use crate::game::parsing::noteskin::NUM_QUANTIZATIONS;
use crate::game::{profile, scroll::ScrollSpeedSetting};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::components::gameplay_stats;
use crate::ui::components::screen_bar::{self, ScreenBarParams};
use std::array::from_fn;

use crate::game::gameplay::active_hold_is_engaged;
use crate::game::gameplay::{
    COMBO_HUNDRED_MILESTONE_DURATION, COMBO_THOUSAND_MILESTONE_DURATION, ComboMilestoneKind,
    HOLD_JUDGMENT_TOTAL_DURATION, MINE_EXPLOSION_DURATION, RECEPTOR_GLOW_DURATION,
    RECEPTOR_Y_OFFSET_FROM_CENTER, RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE, TRANSITION_IN_DURATION,
    TRANSITION_OUT_DURATION,
};
pub use crate::game::gameplay::{State, init, update};

// --- CONSTANTS ---

// Gameplay Layout & Feel
const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0; // Match Simply Love's on-screen arrow height
const TARGET_EXPLOSION_PIXEL_SIZE: f32 = 125.0; // Simply Love tap explosions top out around 125px tall
const HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER: f32 = -90.0; // Mirrors Simply Love metrics for hold judgments
const HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR: f32 =
    HOLD_JUDGMENT_Y_OFFSET_FROM_CENTER - RECEPTOR_Y_OFFSET_FROM_CENTER;
const TAP_JUDGMENT_OFFSET_FROM_CENTER: f32 = 30.0; // From _fallback JudgmentTransformCommand
const COMBO_OFFSET_FROM_CENTER: f32 = 30.0; // From _fallback ComboTransformCommand (non-centered)
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

#[derive(Clone, Debug)]
struct MineFillState {
    layers: [[f32; 4]; MINE_FILL_LAYERS],
}

fn mine_fill_state(colors: &[[f32; 4]], beat: f32) -> Option<MineFillState> {
    if colors.is_empty() {
        return None;
    }

    let phase = beat.rem_euclid(1.0);
    let len = colors.len();

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

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::new();
    let profile = profile::get();
    let hold_judgment_texture: Option<&str> = match profile.hold_judgment_graphic {
        profile::HoldJudgmentGraphic::Love => Some("hold_judgements/Love 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::Mute => Some("hold_judgements/mute 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::ITG2 => Some("hold_judgements/ITG2 1x2 (doubleres).png"),
        profile::HoldJudgmentGraphic::None => None,
    };
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

    // Global offset adjustment overlay (centered text with subtle shadow).
    if let Some(msg) = &state.sync_overlay_message {
        let zoom = widescale(0.8, 1.0);
        let y = screen_center_y() + 120.0;

        // Main text
        actors.push(act!(text:
            font("miso"):
            settext(msg.clone()):
            align(0.5, 0.5):
            xy(screen_center_x(), y):
            zoom(zoom):
            shadowlength(2.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(901)
        ));
    }
    // --- Playfield Positioning (1:1 with Simply Love) ---
    // Use the cached field_zoom from gameplay state so visual layout and
    // scroll math share the exact same scaling as gameplay.
    let field_zoom = state.field_zoom;
    // NoteFieldOffsetX is stored as a non-negative magnitude; for a single P1-style field,
    // positive values move the field left, mirroring Simply Love's use of a sign flip.
    let notefield_offset_x = -(profile.note_field_offset_x.clamp(0, 50) as f32);
    let notefield_offset_y = profile.note_field_offset_y.clamp(-50, 50) as f32;
    let logical_screen_width = screen_width();
    let clamped_width = logical_screen_width.clamp(640.0, 854.0);
    let playfield_center_x = screen_center_x() - (clamped_width * 0.25) + notefield_offset_x;
    let receptor_y_normal = screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER + notefield_offset_y;
    let receptor_y_reverse =
        screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE + notefield_offset_y;

    let is_centered = profile
        .scroll_option
        .contains(profile::ScrollOption::Centered);
    let receptor_y_centered = screen_center_y() + notefield_offset_y;
    let column_dirs = state.column_scroll_dirs;
    let column_receptor_ys: [f32; 4] = from_fn(|i| {
        if is_centered {
            receptor_y_centered
        } else if column_dirs[i] >= 0.0 {
            receptor_y_normal
        } else {
            receptor_y_reverse
        }
    });

    if let Some(ns) = &state.noteskin {
        let target_arrow_px = TARGET_ARROW_PIXEL_SIZE * field_zoom;
        let target_explosion_px = TARGET_EXPLOSION_PIXEL_SIZE * field_zoom;
        let scale_sprite = |size: [i32; 2]| -> [f32; 2] {
            let width = size[0].max(0) as f32;
            let height = size[1].max(0) as f32;
            if height <= 0.0 || target_arrow_px <= 0.0 {
                [width, height]
            } else {
                let scale = target_arrow_px / height;
                [width * scale, target_arrow_px]
            }
        };
        let scale_explosion = |size: [i32; 2]| -> [f32; 2] {
            let width = size[0].max(0) as f32;
            let height = size[1].max(0) as f32;
            if height <= 0.0 || target_explosion_px <= 0.0 {
                [width, height]
            } else {
                let scale = target_explosion_px / height;
                [width * scale, target_explosion_px]
            }
        };
        let current_time = state.current_music_time;
        // Precompute per-frame values used for converting beat/time to Y positions
        let (rate, cmod_pps_opt, curr_disp_beat, beatmod_multiplier) = match state.scroll_speed {
            ScrollSpeedSetting::CMod(c_bpm) => {
                let pps = (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING * field_zoom;
                let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
                    state.music_rate
                } else {
                    1.0
                };
                (rate, Some(pps), 0.0, 0.0)
            }
            ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                let curr_disp = state.timing.get_displayed_beat(state.current_beat);
                let speed_multiplier = state
                    .timing
                    .get_speed_multiplier(state.current_beat, state.current_music_time);
                let player_multiplier = state
                    .scroll_speed
                    .beat_multiplier(state.scroll_reference_bpm, state.music_rate);
                let final_multiplier = player_multiplier * speed_multiplier;
                (1.0, None, curr_disp, final_multiplier)
            }
        };
        // For dynamic values (e.g., last_held_beat while letting go), fall back to timing for that beat.
        // Direction and receptor row are per-lane: upwards lanes anchor to the normal receptor row,
        // downwards lanes anchor to the reverse row.
        let compute_lane_y_dynamic = |beat: f32, receptor_y_lane: f32, dir: f32| -> f32 {
            let dir = if dir >= 0.0 { 1.0 } else { -1.0 };
            match state.scroll_speed {
                ScrollSpeedSetting::CMod(_) => {
                    let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                    let note_time_chart = state.timing.get_time_for_beat(beat);
                    let time_diff_real = (note_time_chart - current_time) / rate;
                    receptor_y_lane + dir * time_diff_real * pps_chart
                }
                ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                    let note_disp_beat = state.timing.get_displayed_beat(beat);
                    let beat_diff_disp = note_disp_beat - curr_disp_beat;
                    receptor_y_lane
                        + dir
                            * (beat_diff_disp
                                * ScrollSpeedSetting::ARROW_SPACING
                                * field_zoom
                                * beatmod_multiplier)
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
            let col_x_offset = ns.column_xs[i] as f32 * field_zoom;
            let receptor_y_lane = column_receptor_ys[i];
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
                xy(playfield_center_x + col_x_offset as f32, receptor_y_lane):
                zoomto(receptor_size[0], receptor_size[1]):
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
                    visuals.explosion.as_ref().or(ns.hold.explosion.as_ref())
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
                    xy(playfield_center_x + col_x_offset as f32, receptor_y_lane):
                    zoomto(hold_size[0], hold_size[1]):
                    rotationz(-final_rotation):
                    customtexturerect(hold_uv[0], hold_uv[1], hold_uv[2], hold_uv[3]):
                    blend(normal):
                    z(Z_HOLD_EXPLOSION)
                ));
            }
            let glow_timer = state.receptor_glow_timers[i];
            if glow_timer > 0.0
                && let Some(glow_slot) = ns.receptor_glow.get(i).and_then(|slot| slot.as_ref())
            {
                let glow_frame =
                    glow_slot.frame_index(state.total_elapsed_in_screen, state.current_beat);
                let glow_uv = glow_slot.uv_for_frame(glow_frame);
                let glow_size = glow_slot.size();
                let alpha = (glow_timer / RECEPTOR_GLOW_DURATION).powf(0.75);
                actors.push(act!(sprite(glow_slot.texture_key().to_string()):
                    align(0.5, 0.5):
                    xy(playfield_center_x + col_x_offset as f32, receptor_y_lane):
                    zoomto(glow_size[0] as f32, glow_size[1] as f32):
                    rotationz(-glow_slot.def.rotation_deg as f32):
                    customtexturerect(glow_uv[0], glow_uv[1], glow_uv[2], glow_uv[3]):
                    diffuse(1.0, 1.0, 1.0, alpha):
                    blend(add):
                    z(Z_HOLD_GLOW)
                ));
            }
        }
        // Tap explosions
        for i in 0..4 {
            if let Some(active) = state.tap_explosions[i].as_ref()
                && let Some(explosion) = ns.tap_explosions.get(&active.window)
            {
                let col_x_offset = ns.column_xs[i] as f32 * field_zoom;
                let receptor_y_lane = column_receptor_ys[i];
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
                    xy(playfield_center_x + col_x_offset as f32, receptor_y_lane):
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
                let glow_strength = glow[0].abs() + glow[1].abs() + glow[2].abs() + glow[3].abs();
                if glow_strength > f32::EPSILON {
                    actors.push(act!(sprite(slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(playfield_center_x + col_x_offset as f32, receptor_y_lane):
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
                let col_x_offset = ns.column_xs[i] as f32 * field_zoom;
                let receptor_y_lane = column_receptor_ys[i];
                let base_rotation = ns
                    .receptor_off
                    .get(i)
                    .map(|slot| slot.def.rotation_deg as f32)
                    .unwrap_or(0.0);
                let final_rotation = base_rotation + rotation_progress;
                actors.push(act!(sprite("hit_mine_explosion.png"):
                    align(0.5, 0.5):
                    xy(playfield_center_x + col_x_offset as f32, receptor_y_lane):
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
        let notes_len = state.notes.len();
        let extra_hold_indices = state
            .active_holds
            .iter()
            .filter_map(|a| a.as_ref().map(|h| h.note_index))
            .chain(state.decaying_hold_indices.iter().copied())
            .filter(|&idx| idx < notes_len && (idx < min_visible_index || idx >= max_visible_index));

        // Render holds in the visible window, plus any active/decaying holds outside it.
        // This avoids per-frame allocations and hashing for deduping.
        for note_index in (min_visible_index..max_visible_index).chain(extra_hold_indices) {
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

            // Prepare static/dynamic Y positions for the hold body
            // Head Y: dynamic if actively held or let go, otherwise static cache
            let mut head_beat = note.beat;
            let is_head_dynamic =
                hold.let_go_started_at.is_some() || hold.result == Some(HoldResult::LetGo);

            if is_head_dynamic {
                head_beat = hold.last_held_beat.clamp(note.beat, hold.end_beat);
            }

            let col_dir = column_dirs[note.column];
            let dir = col_dir;
            let lane_receptor_y = column_receptor_ys[note.column];

            // Compute Y positions: O(1) via cache for static parts, dynamic for moving head
            let head_y = if is_head_dynamic {
                compute_lane_y_dynamic(head_beat, lane_receptor_y, dir)
            } else {
                match state.scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                        let note_time_chart = state.note_time_cache[note_index];
                        let time_diff_real = (note_time_chart - current_time) / rate;
                        lane_receptor_y + dir * time_diff_real * pps_chart
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = state.note_display_beat_cache[note_index];
                        let beat_diff_disp = note_disp_beat - curr_disp_beat;
                        lane_receptor_y
                            + dir
                                * (beat_diff_disp
                                    * ScrollSpeedSetting::ARROW_SPACING
                                    * field_zoom
                                    * beatmod_multiplier)
                    }
                }
            };

            let tail_y = match state.scroll_speed {
                ScrollSpeedSetting::CMod(_) => {
                    let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                    // Use cached end time for O(1) lookup
                    let note_end_time_chart = state.hold_end_time_cache[note_index].unwrap_or(0.0);
                    let time_diff_real = (note_end_time_chart - current_time) / rate;
                    lane_receptor_y + dir * time_diff_real * pps_chart
                }
                ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                    // Use cached end display beat for O(1) lookup
                    let note_end_disp_beat =
                        state.hold_end_display_beat_cache[note_index].unwrap_or(0.0);
                    let beat_diff_disp = note_end_disp_beat - curr_disp_beat;
                    lane_receptor_y
                        + dir
                            * (beat_diff_disp
                                * ScrollSpeedSetting::ARROW_SPACING
                                * field_zoom
                                * beatmod_multiplier)
                }
            };

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
            let col_x_offset = ns.column_xs[note.column] as f32 * field_zoom;

            let active_state = state.active_holds[note.column]
                .as_ref()
                .filter(|h| h.note_index == note_index);
            let engaged = active_state.map(active_hold_is_engaged).unwrap_or(false);
            let use_active = active_state
                .map(|h| h.is_pressed && !h.let_go)
                .unwrap_or(false);
            let let_go_gray = ns.hold_let_go_gray_percent.clamp(0.0, 1.0);
            let hold_life = hold.life.clamp(0.0, 1.0);
            let hold_color_scale = let_go_gray + (1.0 - let_go_gray) * hold_life;
            let hold_diffuse = [hold_color_scale, hold_color_scale, hold_color_scale, 1.0];
            if engaged {
                if head_is_top {
                    top = top.max(lane_receptor_y);
                } else {
                    bottom = bottom.min(lane_receptor_y);
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
                    .or(visuals.bottomcap_inactive.as_ref())
            } else {
                visuals
                    .bottomcap_inactive
                    .as_ref()
                    .or(visuals.bottomcap_active.as_ref())
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
            if body_bottom > body_top
                && let Some(body_slot) = if use_active {
                    visuals
                        .body_active
                        .as_ref()
                        .or(visuals.body_inactive.as_ref())
                } else {
                    visuals
                        .body_inactive
                        .as_ref()
                        .or(visuals.body_active.as_ref())
                }
            {
                let texture_size = body_slot.size();
                let texture_width = texture_size[0].max(1) as f32;
                let texture_height = texture_size[1].max(1) as f32;
                if texture_width > std::f32::EPSILON && texture_height > std::f32::EPSILON {
                    let body_width = TARGET_ARROW_PIXEL_SIZE * field_zoom;
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
                    const SEGMENT_PHASE_EPS: f32 = 1e-4;
                    let max_segments = 2048;
                    let lane_reverse = col_dir < 0.0;
                    let receptor = lane_receptor_y;

                    // Unified segmentation path for both normal and reverse scroll.
                    // For reverse scroll, we work in "forward space" by mirroring coordinates,
                    // run the same segmentation logic, then mirror back to screen space.

                    // Transform to "forward space" if reverse scroll (mirror around receptor)
                    let (eff_head_y, eff_tail_y, eff_body_top, eff_body_bottom) = if lane_reverse {
                        (
                            2.0 * receptor - head_y,
                            2.0 * receptor - tail_y,
                            2.0 * receptor - body_bottom,
                            2.0 * receptor - body_top,
                        )
                    } else {
                        (head_y, tail_y, body_top, body_bottom)
                    };

                    let eff_head_is_top = eff_head_y <= eff_tail_y;
                    let eff_natural_top = if eff_head_is_top {
                        eff_head_y
                    } else {
                        eff_tail_y
                    };
                    let eff_natural_bottom = if eff_head_is_top {
                        eff_tail_y
                    } else {
                        eff_head_y
                    };

                    // Skip if hold has no effective length
                    if hold_length > std::f32::EPSILON {
                        // Calculate visible distances in forward space
                        let visible_top_distance = if eff_head_is_top {
                            (eff_body_top - eff_natural_top).clamp(0.0, hold_length)
                        } else {
                            (eff_natural_bottom - eff_body_top).clamp(0.0, hold_length)
                        };
                        let visible_bottom_distance = if eff_head_is_top {
                            (eff_body_bottom - eff_natural_top).clamp(0.0, hold_length)
                        } else {
                            (eff_natural_bottom - eff_body_bottom).clamp(0.0, hold_length)
                        };

                        // Phase offset: shifts fractional remainder to first segment so the
                        // final segment aligns with the tail cap. Only applies when head is on top.
                        let phase_offset = if eff_head_is_top {
                            let total_phase = hold_length / segment_height;
                            if total_phase >= 1.0 + SEGMENT_PHASE_EPS {
                                let fractional = total_phase.fract();
                                if fractional > SEGMENT_PHASE_EPS
                                    && (1.0 - fractional) > SEGMENT_PHASE_EPS
                                {
                                    1.0 - fractional
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            }
                        } else {
                            0.0
                        };

                        let mut phase = visible_top_distance / segment_height + phase_offset;
                        let phase_end_adjusted =
                            visible_bottom_distance / segment_height + phase_offset;
                        let mut emitted = 0;

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
                            let y_start = eff_natural_top + distance_start;
                            let y_end = eff_natural_top + distance_end;
                            let segment_top_eff = y_start.max(eff_body_top);
                            let segment_bottom_eff = y_end.min(eff_body_bottom);

                            if segment_bottom_eff - segment_top_eff <= std::f32::EPSILON {
                                phase = next_phase;
                                continue;
                            }

                            // UV calculations
                            let base_floor = phase.floor();
                            let start_fraction = (phase - base_floor).clamp(0.0, 1.0);
                            let end_fraction = (next_phase - base_floor).clamp(0.0, 1.0);
                            let mut v0 = v_top + v_range * start_fraction;
                            let mut v1 = v_top + v_range * end_fraction;

                            let segment_size_eff = segment_bottom_eff - segment_top_eff;
                            let portion = (segment_size_eff / segment_height).clamp(0.0, 1.0);

                            let is_last_segment = (eff_body_bottom - segment_bottom_eff).abs()
                                <= 0.5
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

                            // Transform back to screen space if reverse scroll
                            let (
                                segment_center_screen,
                                segment_size_screen,
                                seg_top_screen,
                                seg_bottom_screen,
                            ) = if lane_reverse {
                                let top_scr = 2.0 * receptor - segment_bottom_eff;
                                let bottom_scr = 2.0 * receptor - segment_top_eff;
                                (
                                    (top_scr + bottom_scr) * 0.5,
                                    bottom_scr - top_scr,
                                    top_scr,
                                    bottom_scr,
                                )
                            } else {
                                (
                                    (segment_top_eff + segment_bottom_eff) * 0.5,
                                    segment_size_eff,
                                    segment_top_eff,
                                    segment_bottom_eff,
                                )
                            };

                            let rotation = if lane_reverse { 180.0 } else { 0.0 };

                            // Track rendered bounds in screen space
                            rendered_body_top = Some(match rendered_body_top {
                                None => seg_top_screen,
                                Some(v) => v.min(seg_top_screen),
                            });
                            rendered_body_bottom = Some(match rendered_body_bottom {
                                None => seg_bottom_screen,
                                Some(v) => v.max(seg_bottom_screen),
                            });

                            actors.push(act!(sprite(body_slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(playfield_center_x + col_x_offset as f32, segment_center_screen):
                                zoomto(body_width, segment_size_screen):
                                rotationz(rotation):
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
                        _ => {
                            continue;
                        }
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
                    if !cap_adjacent_ok {
                        continue;
                    }
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
                            rotationz(if col_dir < 0.0 { 180.0 } else { 0.0 }):
                            z(Z_HOLD_CAP)
                        ));
                    }
                }
            }
            if (hold.let_go_started_at.is_some() || hold.result == Some(HoldResult::LetGo))
                && head_y >= lane_receptor_y - state.draw_distance_after_targets
                && head_y <= lane_receptor_y + state.draw_distance_before_targets
            {
                let note_idx =
                    (note.column % 4) * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                if let Some(note_slot) = ns.notes.get(note_idx) {
                    let frame =
                        note_slot.frame_index(state.total_elapsed_in_screen, state.current_beat);
                    let uv = note_slot.uv_for_frame(frame);
                    let size = scale_sprite(note_slot.size());
                    actors.push(act!(sprite(note_slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(playfield_center_x + col_x_offset as f32, head_y):
                        zoomto(size[0], size[1]):
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
        // Active arrows
        for (col_idx, column_arrows) in state.arrows.iter().enumerate() {
            let dir = column_dirs[col_idx];
            let receptor_y_lane = column_receptor_ys[col_idx];
            for arrow in column_arrows {
                // Use cached per-note timing to avoid per-frame timing queries
                let y_pos = match state.scroll_speed {
                    ScrollSpeedSetting::CMod(_) => {
                        let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                        let note_time_chart = state.note_time_cache[arrow.note_index];
                        let time_diff_real = (current_time - note_time_chart) / rate;
                        receptor_y_lane - dir * time_diff_real * pps_chart
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = state.note_display_beat_cache[arrow.note_index];
                        let beat_diff_disp = note_disp_beat - curr_disp_beat;
                        receptor_y_lane
                            + dir
                                * beat_diff_disp
                                * ScrollSpeedSetting::ARROW_SPACING
                                * field_zoom
                                * beatmod_multiplier
                    }
                };
                let delta = (y_pos - receptor_y_lane) * dir;
                if delta < -state.draw_distance_after_targets
                    || delta > state.draw_distance_before_targets
                {
                    continue;
                }
                let col_x_offset = ns.column_xs[arrow.column] as f32 * field_zoom;
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
                        .unwrap_or([
                            TARGET_ARROW_PIXEL_SIZE * field_zoom,
                            TARGET_ARROW_PIXEL_SIZE * field_zoom,
                        ]);
                    if let Some(slot) = fill_slot {
                        let fill_gradient = ns
                            .mine_fill_gradients
                            .get(arrow.column)
                            .and_then(|colors| colors.as_deref());
                        if let Some(fill_state) = fill_gradient
                            .and_then(|colors| mine_fill_state(colors, state.current_beat))
                        {
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
                let note = &state.notes[arrow.note_index];
                let note_idx =
                    (arrow.column % 4) * NUM_QUANTIZATIONS + note.quantization_idx as usize;
                if let Some(note_slot) = ns.notes.get(note_idx) {
                    let note_frame =
                        note_slot.frame_index(state.total_elapsed_in_screen, state.current_beat);
                    let note_uv = note_slot.uv_for_frame(note_frame);
                    let note_size = scale_sprite(note_slot.size());
                    actors.push(act!(sprite(note_slot.texture_key().to_string()):
                        align(0.5, 0.5):
                        xy(playfield_center_x + col_x_offset as f32, y_pos):
                        zoomto(note_size[0], note_size[1]):
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
        let combo_center_y = if state.reverse_scroll {
            screen_center_y() - COMBO_OFFSET_FROM_CENTER
        } else {
            screen_center_y() + COMBO_OFFSET_FROM_CENTER
        } + notefield_offset_y;
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
        let combo_y = if is_centered {
            receptor_y_centered + 155.0
        } else if state.reverse_scroll {
            screen_center_y() - COMBO_OFFSET_FROM_CENTER + notefield_offset_y
        } else {
            screen_center_y() + COMBO_OFFSET_FROM_CENTER + notefield_offset_y
        };
        let miss_combo_font_name = match profile.combo_font {
            crate::game::profile::ComboFont::Wendy => Some("wendy_combo"),
            crate::game::profile::ComboFont::ArialRounded => Some("combo_arial_rounded"),
            crate::game::profile::ComboFont::Asap => Some("combo_asap"),
            crate::game::profile::ComboFont::BebasNeue => Some("combo_bebas_neue"),
            crate::game::profile::ComboFont::SourceCode => Some("combo_source_code"),
            crate::game::profile::ComboFont::Work => Some("combo_work"),
            crate::game::profile::ComboFont::WendyCursed => Some("combo_wendy_cursed"),
            crate::game::profile::ComboFont::None => None,
        };
        if let Some(font_name) = miss_combo_font_name {
            actors.push(act!(text:
                font(font_name): settext(state.miss_combo.to_string()):
                align(0.5, 0.5): xy(playfield_center_x, combo_y):
                zoom(0.75): horizalign(center): shadowlength(1.0):
                diffuse(1.0, 0.0, 0.0, 1.0):
                z(90)
            ));
        }
    } else if state.combo >= SHOW_COMBO_AT {
        let combo_y = if is_centered {
            receptor_y_centered + 155.0
        } else if state.reverse_scroll {
            screen_center_y() - COMBO_OFFSET_FROM_CENTER + notefield_offset_y
        } else {
            screen_center_y() + COMBO_OFFSET_FROM_CENTER + notefield_offset_y
        };
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
        let combo_font_name = match profile.combo_font {
            crate::game::profile::ComboFont::Wendy => Some("wendy_combo"),
            crate::game::profile::ComboFont::ArialRounded => Some("combo_arial_rounded"),
            crate::game::profile::ComboFont::Asap => Some("combo_asap"),
            crate::game::profile::ComboFont::BebasNeue => Some("combo_bebas_neue"),
            crate::game::profile::ComboFont::SourceCode => Some("combo_source_code"),
            crate::game::profile::ComboFont::Work => Some("combo_work"),
            crate::game::profile::ComboFont::WendyCursed => Some("combo_wendy_cursed"),
            crate::game::profile::ComboFont::None => None,
        };
        if let Some(font_name) = combo_font_name {
            actors.push(act!(text:
                font(font_name): settext(state.combo.to_string()):
                align(0.5, 0.5): xy(playfield_center_x, combo_y):
                zoom(0.75): horizalign(center): shadowlength(1.0):
                diffuse(final_color[0], final_color[1], final_color[2], final_color[3]):
                z(90)
            ));
        }
    }
    // Judgment Sprite (tap judgments)
    if let Some(render_info) = &state.last_judgment {
        if matches!(profile.judgment_graphic, profile::JudgmentGraphic::None) {
            // Player chose to hide tap judgment graphics.
            // Still keep life/score effects; only suppress the visual sprite.
        } else {
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
                let use_fa_plus_window = profile.show_fa_plus_window;
                // Map JudgeGrade + TimingWindow to a row index in the 7-row sheet:
                //  row 0: FA+ Fantastic (W0)
                //  row 1: regular Fantastic (W1)
                //  row 2..6: Excellent..Miss, matching our existing layout.
                let frame_row = match judgment.grade {
                    JudgeGrade::Fantastic => {
                        if use_fa_plus_window {
                            match judgment.window {
                                Some(TimingWindow::W0) => 0,
                                _ => 1,
                            }
                        } else {
                            0
                        }
                    }
                    JudgeGrade::Excellent => 2,
                    JudgeGrade::Great => 3,
                    JudgeGrade::Decent => 4,
                    JudgeGrade::WayOff => 5,
                    JudgeGrade::Miss => 6,
                };
                let frame_offset = if offset_sec < 0.0 { 0 } else { 1 };
                let columns = match profile.judgment_graphic {
                    profile::JudgmentGraphic::Censored => 1,
                    _ => 2,
                };
                let col_index = if columns > 1 { frame_offset } else { 0 };
                let linear_index = (frame_row * columns + col_index) as u32;
                let judgment_texture = match profile.judgment_graphic {
                    profile::JudgmentGraphic::Bebas => "judgements/Bebas 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Censored => "judgements/Censored 1x7 (doubleres).png",
                    profile::JudgmentGraphic::Chromatic => {
                        "judgements/Chromatic 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Code => "judgements/Code 2x7 (doubleres).png",
                    profile::JudgmentGraphic::ComicSans => {
                        "judgements/Comic Sans 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Emoticon => "judgements/Emoticon 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Focus => "judgements/Focus 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Grammar => "judgements/Grammar 2x7 (doubleres).png",
                    profile::JudgmentGraphic::GrooveNights => {
                        "judgements/GrooveNights 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::ITG2 => "judgements/ITG2 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Love => "judgements/Love 2x7 (doubleres).png",
                    profile::JudgmentGraphic::LoveChroma => {
                        "judgements/Love Chroma 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Miso => "judgements/Miso 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Papyrus => "judgements/Papyrus 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Rainbowmatic => {
                        "judgements/Rainbowmatic 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::Roboto => "judgements/Roboto 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Shift => "judgements/Shift 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Tactics => "judgements/Tactics 2x7 (doubleres).png",
                    profile::JudgmentGraphic::Wendy => "judgements/Wendy 2x7 (doubleres).png",
                    profile::JudgmentGraphic::WendyChroma => {
                        "judgements/Wendy Chroma 2x7 (doubleres).png"
                    }
                    profile::JudgmentGraphic::None => {
                        unreachable!("JudgmentGraphic::None is filtered above")
                    }
                };
                let judgment_y = if is_centered {
                    receptor_y_centered + 95.0
                } else if state.reverse_scroll {
                    screen_center_y() + TAP_JUDGMENT_OFFSET_FROM_CENTER + notefield_offset_y
                } else {
                    screen_center_y() - TAP_JUDGMENT_OFFSET_FROM_CENTER + notefield_offset_y
                };
                actors.push(act!(sprite(judgment_texture):
                    align(0.5, 0.5): xy(playfield_center_x, judgment_y):
                    z(200): zoomtoheight(76.0): setstate(linear_index): zoom(zoom)
                ));
            }
        }
    }
    for (column, render_info) in state.hold_judgments.iter().enumerate() {
        let Some(render_info) = render_info else {
            continue;
        };
        let elapsed = render_info.triggered_at.elapsed().as_secs_f32();
        if elapsed >= HOLD_JUDGMENT_TOTAL_DURATION {
            continue;
        }
        // Hold judgments scale with Mini/Tiny in ITGmania as pow(0.5, mini+tiny), clamped to 1.0.
        let mini_for_holds = ((profile.mini_percent as f32).clamp(-100.0, 150.0) / 100.0).max(0.0);
        let hold_judgment_zoom_mod = 0.5_f32.powf(mini_for_holds).min(1.0);
        let zoom = if elapsed < 0.3 {
            let progress = (elapsed / 0.3).clamp(0.0, 1.0);
            (HOLD_JUDGMENT_INITIAL_ZOOM
                + progress * (HOLD_JUDGMENT_FINAL_ZOOM - HOLD_JUDGMENT_INITIAL_ZOOM))
                * hold_judgment_zoom_mod
        } else {
            HOLD_JUDGMENT_FINAL_ZOOM * hold_judgment_zoom_mod
        };
        let frame_index = match render_info.result {
            HoldResult::Held => 0,
            HoldResult::LetGo => 1,
        } as u32;
        if let Some(texture) = hold_judgment_texture {
            let dir = column_dirs[column];
            let receptor_y_lane = column_receptor_ys[column];
            let hold_judgment_y = if dir >= 0.0 {
                // Non-reverse lane: match Simply Love's baseline offset below receptors.
                receptor_y_lane + HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR
            } else {
                // Reverse lane: mirror around the receptor so the hold judgment
                // appears just above the receptors instead of near screen center.
                receptor_y_lane - HOLD_JUDGMENT_OFFSET_FROM_RECEPTOR
            };
            let column_offset = state
                .noteskin
                .as_ref()
                .and_then(|ns| ns.column_xs.get(column))
                .map(|&x| x as f32)
                .unwrap_or_else(|| ((column as f32) - 1.5) * TARGET_ARROW_PIXEL_SIZE * field_zoom);
            actors.push(act!(sprite(texture):
                align(0.5, 0.5):
                xy(playfield_center_x + column_offset, hold_judgment_y):
                z(195):
                setstate(frame_index):
                zoom(zoom):
                diffusealpha(1.0)
            ));
        }
    }
    // Difficulty Box
    let x = screen_center_x() - widescale(292.5, 342.5);
    let y = 56.0;
    let difficulty_color = color::difficulty_rgba(&state.chart.difficulty, state.active_color_index);
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
    let (score_text, score_color) = if profile.show_ex_score {
        // FA+ EX score display (Simply Love EX scoring semantics), with
        // failure-aware gating so score stops changing after life reaches 0.
        let mines_disabled = false; // NoMines handling not wired yet.
        let ex_percent = judgment::calculate_ex_score_from_notes(
            &state.notes,
            &state.note_time_cache,
            &state.hold_end_time_cache,
            state.chart.stats.total_steps, // <- use this
            state.holds_total,
            state.rolls_total,
            state.mines_total,
            state.fail_time,
            mines_disabled,
        );
        let text = format!("{:.2}", ex_percent.max(0.0));
        let color = color::rgba_hex(color::JUDGMENT_HEX[0]); // Fantastic blue (#21CCE8)
        (text, color)
    } else {
        let score_percent = (judgment::calculate_itg_score_percent(
            &state.scoring_counts,
            state.holds_held_for_score,
            state.rolls_held_for_score,
            state.mines_hit_for_score,
            state.possible_grade_points,
        ) * 100.0) as f32;
        let text = format!("{:.2}", score_percent);
        let color = [1.0, 1.0, 1.0, 1.0];
        (text, color)
    };
    actors.push(act!(text:
        font("wendy_monospace_numbers"): settext(score_text):
        align(1.0, 1.0): xy(score_x, score_y):
        zoom(0.5): horizalign(right):
        diffuse(score_color[0], score_color[1], score_color[2], score_color[3]):
        z(90)
    ));
    // Current BPM Display (1:1 with Simply Love)
    {
        let base_bpm = state.timing.get_bpm_for_beat(state.current_beat);
        let display_bpm = if base_bpm.is_finite() {
            (base_bpm
                * if state.music_rate.is_finite() {
                    state.music_rate
                } else {
                    1.0
                })
            .round() as i32
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
        let rate = if state.music_rate.is_finite() {
            state.music_rate
        } else {
            1.0
        };
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
            // Logic Parity:
            // velocity = -(songposition:GetCurBPS() * 0.5)
            // if songposition:GetFreeze() or songposition:GetDelay() then velocity = 0 end
            let bps = state.timing.get_bpm_for_beat(state.current_beat) / 60.0;
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
                // Apply the calculated velocity
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
    actors.extend(gameplay_stats::build(
        state,
        asset_manager,
        playfield_center_x,
    ));
    actors
}
