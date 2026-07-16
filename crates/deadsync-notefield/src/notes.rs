use crate::holds::{song_time_ns_delta_seconds, translated_uv_rect};
use crate::measure_lines::{beat_scroll_travel, edit_beat_scroll_travel};
use crate::transforms::{
    AccelYParams, apply_accel_y, apply_accel_y_with_peak, move_col_extra, tipsy_y_extra,
};
use crate::{
    ModelMeshCache, actor_with_world_z, itg_actor_glow_alpha,
    noteskin_model_actor_from_draw_cached, song_lua_note_model_draw,
};
use deadlib_present::actors::{Actor, SpriteSource};
use deadlib_present::dsl::SpriteBuilder;
use deadlib_render::BlendMode;
use deadsync_core::song_time::SongTimeNs;
use deadsync_core::timing::beat_to_note_row;
use deadsync_noteskin::{ModelDrawState, NoteskinSlot};
use deadsync_rules::note::{MineResult, Note, NoteCountStat};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::TimingData;

/// Internal inputs for the white ITG Actor glow pass of a note layer.
struct NoteGlowRequest<'a, S> {
    slot: &'a S,
    draw: ModelDrawState,
    model_center: [f32; 2],
    sprite_center: [f32; 2],
    size: [f32; 2],
    uv: [f32; 4],
    rotation_y_deg: f32,
    model_rotation_z_deg: f32,
    sprite_rotation_z_deg: f32,
    alpha: f32,
    blend: BlendMode,
    z: i16,
    world_z: f32,
    prefer_sprite: bool,
}

/// Canonical inputs for one complete noteskin layer, including its diffuse and
/// white ITG Actor glow passes.
pub(crate) struct NoteLayerRequest<'a, S> {
    pub slot: &'a S,
    pub draw: ModelDrawState,
    pub model_center: [f32; 2],
    pub sprite_center: [f32; 2],
    pub size: [f32; 2],
    pub uv: [f32; 4],
    pub rotation_y_deg: f32,
    pub model_rotation_z_deg: f32,
    pub sprite_rotation_z_deg: f32,
    pub tint: [f32; 4],
    pub glow_alpha: f32,
    pub blend: BlendMode,
    pub z: i16,
    pub world_z: f32,
    pub prefer_sprite: bool,
}

/// Renderer-neutral inputs for one mine's fill-gradient/core/frame sequence.
/// Slot lookup and size calculation remain owned by the concrete theme adapter.
pub(crate) struct MineLayerRequest<'a, S> {
    pub fill_slot: Option<&'a S>,
    pub gradient_slot: Option<&'a S>,
    pub frame_slot: Option<&'a S>,
    pub gradient_size: [f32; 2],
    pub center: [f32; 2],
    pub mine_uv_phase: f32,
    pub mine_fill_phase: f32,
    pub elapsed_s: f32,
    pub display_time_s: f32,
    pub current_beat: f32,
    pub uv_translation: [f32; 2],
    pub rotation_y_deg: f32,
    pub note_rotation_z_deg: f32,
    pub alpha: f32,
    pub glow_alpha: f32,
    pub note_z: i16,
    pub world_z: f32,
    pub prefer_sprite: bool,
}

struct MineSlotPass<'a, S> {
    slot: &'a S,
    alpha_scale: f32,
    z: i16,
}

/// Appends a mine's gradient-or-fill actors followed by its frame actors.
pub(crate) fn compose_mine_layers<S, F, Z>(
    actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    request: MineLayerRequest<'_, S>,
    size_for_slot: &Z,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
    Z: Fn(&S) -> [f32; 2],
{
    let use_gradient = request.frame_slot.is_some()
        && request
            .fill_slot
            .is_some_and(|slot| slot.model().is_none() && slot.frame_count() <= 1)
        && request.gradient_slot.is_some();
    if use_gradient {
        compose_mine_gradient(
            actors,
            model_cache,
            request
                .gradient_slot
                .expect("gradient presence checked above"),
            &request,
            sprite_source,
        );
    } else if let Some(slot) = request.fill_slot {
        compose_mine_slot(
            actors,
            model_cache,
            MineSlotPass {
                slot,
                alpha_scale: 0.9,
                z: request.note_z.saturating_sub(1),
            },
            &request,
            size_for_slot,
            sprite_source,
        );
    }
    if let Some(slot) = request.frame_slot {
        compose_mine_slot(
            actors,
            model_cache,
            MineSlotPass {
                slot,
                alpha_scale: 1.0,
                z: request.note_z,
            },
            &request,
            size_for_slot,
            sprite_source,
        );
    }
}

fn compose_mine_gradient<S, F>(
    actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    slot: &S,
    request: &MineLayerRequest<'_, S>,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    if !(request.gradient_size[0] > 0.0 && request.gradient_size[1] > 0.0) {
        return;
    }
    let frame = slot.frame_index_from_phase(request.mine_fill_phase);
    let uv = slot.uv_for_frame_at(frame, request.elapsed_s);
    compose_note_layer(
        actors,
        model_cache,
        NoteLayerRequest {
            slot,
            draw: ModelDrawState::default(),
            model_center: request.center,
            sprite_center: request.center,
            size: request.gradient_size,
            uv,
            rotation_y_deg: 0.0,
            model_rotation_z_deg: 0.0,
            sprite_rotation_z_deg: 0.0,
            tint: [1.0, 1.0, 1.0, request.alpha],
            glow_alpha: request.glow_alpha,
            blend: BlendMode::Alpha,
            z: request.note_z.saturating_sub(2),
            world_z: request.world_z,
            prefer_sprite: true,
        },
        sprite_source,
    );
}

fn compose_mine_slot<S, F, Z>(
    actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    pass: MineSlotPass<'_, S>,
    request: &MineLayerRequest<'_, S>,
    size_for_slot: &Z,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
    Z: Fn(&S) -> [f32; 2],
{
    let slot = pass.slot;
    let draw = song_lua_note_model_draw(
        slot.model_draw_at(request.display_time_s, request.current_beat),
        request.rotation_y_deg,
    );
    if !draw.visible {
        return;
    }
    let frame = slot.frame_index_from_phase(request.mine_uv_phase);
    let uv_elapsed = if slot.model().is_some() {
        request.mine_uv_phase
    } else {
        request.elapsed_s
    };
    let uv = translated_uv_rect(
        slot.uv_for_frame_at(frame, uv_elapsed),
        request.uv_translation,
    );
    let base_rotation = -slot.sprite_def().rotation_deg as f32;
    compose_note_layer(
        actors,
        model_cache,
        NoteLayerRequest {
            slot,
            draw,
            model_center: request.center,
            sprite_center: request.center,
            size: size_for_slot(slot),
            uv,
            rotation_y_deg: request.rotation_y_deg,
            model_rotation_z_deg: base_rotation + request.note_rotation_z_deg,
            sprite_rotation_z_deg: base_rotation + draw.rot[2] + request.note_rotation_z_deg,
            tint: [1.0, 1.0, 1.0, pass.alpha_scale * request.alpha],
            glow_alpha: request.glow_alpha,
            blend: BlendMode::Alpha,
            z: pass.z,
            world_z: request.world_z,
            prefer_sprite: request.prefer_sprite,
        },
        sprite_source,
    );
}

/// Appends one note layer's diffuse pass followed by its optional glow pass.
/// Concrete asset owners inject the sprite source so cached texture handles
/// remain outside the canonical notefield crate.
pub(crate) fn compose_note_layer<S, F>(
    actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    request: NoteLayerRequest<'_, S>,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    if !request.prefer_sprite
        && let Some(actor) = noteskin_model_actor_from_draw_cached(
            request.slot,
            request.draw,
            request.model_center,
            request.size,
            request.uv,
            request.model_rotation_z_deg,
            request.tint,
            request.blend,
            request.z,
            model_cache,
        )
    {
        actors.push(actor_with_world_z(actor, request.world_z));
    } else {
        let mut actor = SpriteBuilder::with_source(sprite_source(request.slot));
        actor.align(0.5, 0.5);
        actor.xy(request.sprite_center[0], request.sprite_center[1]);
        actor.size(request.size[0], request.size[1]);
        actor.rotationy(request.rotation_y_deg);
        actor.rotationz(request.sprite_rotation_z_deg);
        actor.customtexturerect(request.uv);
        actor.diffuse(request.tint);
        actor.blend(request.blend);
        actor.z(request.z);
        actors.push(actor_with_world_z(actor.build(0), request.world_z));
    }

    compose_note_glow(
        actors,
        model_cache,
        NoteGlowRequest {
            slot: request.slot,
            draw: request.draw,
            model_center: request.model_center,
            sprite_center: request.sprite_center,
            size: request.size,
            uv: request.uv,
            rotation_y_deg: request.rotation_y_deg,
            model_rotation_z_deg: request.model_rotation_z_deg,
            sprite_rotation_z_deg: request.sprite_rotation_z_deg,
            alpha: request.glow_alpha,
            blend: request.blend,
            z: request.z,
            world_z: request.world_z,
            prefer_sprite: request.prefer_sprite,
        },
        sprite_source,
    );
}

/// Appends one note layer's glow pass, preserving model fallback and actor order.
fn compose_note_glow<S, F>(
    actors: &mut Vec<Actor>,
    model_cache: &mut ModelMeshCache,
    request: NoteGlowRequest<'_, S>,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    let glow_alpha = itg_actor_glow_alpha(request.alpha);
    if glow_alpha <= f32::EPSILON {
        return;
    }
    if !request.prefer_sprite
        && let Some(mut actor) = noteskin_model_actor_from_draw_cached(
            request.slot,
            request.draw,
            request.model_center,
            request.size,
            request.uv,
            request.model_rotation_z_deg,
            [1.0, 1.0, 1.0, 0.0],
            request.blend,
            request.z,
            model_cache,
        )
    {
        if let Actor::TexturedMesh { glow, .. } = &mut actor {
            *glow = [1.0, 1.0, 1.0, glow_alpha];
        }
        actors.push(actor_with_world_z(actor, request.world_z));
        return;
    }

    let mut actor = SpriteBuilder::with_source(sprite_source(request.slot));
    actor.align(0.5, 0.5);
    actor.xy(request.sprite_center[0], request.sprite_center[1]);
    actor.size(request.size[0], request.size[1]);
    actor.rotationy(request.rotation_y_deg);
    actor.rotationz(request.sprite_rotation_z_deg);
    actor.customtexturerect(request.uv);
    actor.diffuse([1.0, 1.0, 1.0, 0.0]);
    actor.glow([1.0, 1.0, 1.0, glow_alpha]);
    actor.blend(if request.draw.blend_add {
        BlendMode::Add
    } else {
        BlendMode::Alpha
    });
    actor.z(request.z);
    actors.push(actor_with_world_z(actor.build(0), request.world_z));
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollTravelRequest<'a> {
    pub timing: &'a TimingData,
    pub accel: AccelYParams,
    pub scroll_speed: ScrollSpeedSetting,
    pub current_time_ns: SongTimeNs,
    pub visible_beat: f32,
    pub search_beat: f32,
    pub scroll_reference_bpm: f32,
    pub music_rate: f32,
    pub edit_beat_spacing: bool,
    pub draw_distance_after_targets: f32,
    pub draw_distance_before_targets: f32,
    pub field_zoom: f32,
    pub elapsed_screen_s: f32,
    pub effect_height: f32,
    pub screen_height: f32,
    pub note_count_stats: &'a [NoteCountStat],
    pub arrow_effect_time_s: f32,
    pub lane_tipsy: f32,
    pub lane_move_y: &'a [f32],
}

#[derive(Clone, Copy, Debug)]
enum RawTravel {
    Edit {
        current_beat: f32,
    },
    Constant {
        current_time_ns: SongTimeNs,
        rate: f32,
        beats_per_second: f32,
    },
    Beat {
        current_displayed_beat: f32,
        displayed_speed_percent: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct ScrollTravel<'a> {
    request: ScrollTravelRequest<'a>,
    raw: RawTravel,
    displayed_speed_percent: f32,
    post_accel_scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScrollRangeKey {
    timing: *const TimingData,
    stats: *const NoteCountStat,
    stats_len: usize,
    accel: [u32; 5],
    scroll: [u32; 2],
    geometry: [u32; 7],
    edit_beat_spacing: bool,
    displayed_speed_percent: u32,
    expand_time: u32,
}

pub(crate) fn scroll_travel<'a>(request: ScrollTravelRequest<'a>) -> ScrollTravel<'a> {
    let displayed_speed_percent = request
        .timing
        .get_speed_multiplier_ns(request.visible_beat, request.current_time_ns);
    let (raw, post_accel_scale) = if request.edit_beat_spacing {
        let player_multiplier = request
            .scroll_speed
            .beat_multiplier(request.scroll_reference_bpm, request.music_rate);
        (
            RawTravel::Edit {
                current_beat: request.visible_beat,
            },
            request.field_zoom * player_multiplier,
        )
    } else {
        match request.scroll_speed {
            ScrollSpeedSetting::CMod(c_bpm) => {
                let rate = if request.music_rate.is_finite() && request.music_rate > 0.0 {
                    request.music_rate
                } else {
                    1.0
                };
                (
                    RawTravel::Constant {
                        current_time_ns: request.current_time_ns,
                        rate,
                        beats_per_second: c_bpm / 60.0,
                    },
                    request.field_zoom,
                )
            }
            ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                let player_multiplier = request
                    .scroll_speed
                    .beat_multiplier(request.scroll_reference_bpm, request.music_rate);
                (
                    RawTravel::Beat {
                        current_displayed_beat: request
                            .timing
                            .get_displayed_beat(request.visible_beat),
                        displayed_speed_percent,
                    },
                    request.field_zoom * player_multiplier,
                )
            }
        }
    };
    ScrollTravel {
        request,
        raw,
        displayed_speed_percent,
        post_accel_scale,
    }
}

impl ScrollTravel<'_> {
    pub fn raw_beat(&self, beat: f32) -> f32 {
        match self.raw {
            RawTravel::Edit { current_beat } => edit_beat_scroll_travel(beat, current_beat),
            RawTravel::Constant {
                current_time_ns,
                rate,
                beats_per_second,
            } => {
                let note_time_ns = self.request.timing.get_time_for_beat_ns(beat);
                let real_seconds = song_time_ns_delta_seconds(note_time_ns, current_time_ns) / rate;
                real_seconds * beats_per_second * ScrollSpeedSetting::ARROW_SPACING
            }
            RawTravel::Beat {
                current_displayed_beat,
                displayed_speed_percent,
            } => beat_scroll_travel(
                self.request.timing.get_displayed_beat(beat),
                current_displayed_beat,
                displayed_speed_percent,
            ),
        }
    }

    pub fn raw_note(&self, note: &Note, use_hold_end: bool) -> f32 {
        let beat = if use_hold_end {
            note.hold.as_ref().map_or(note.beat, |hold| hold.end_beat)
        } else {
            note.beat
        };
        self.raw_beat(beat)
    }

    pub fn adjusted_with_peak(&self, raw_travel: f32) -> (f32, bool) {
        let (travel, before_peak) = apply_accel_y_with_peak(
            raw_travel,
            self.request.elapsed_screen_s,
            self.request.effect_height,
            self.request.screen_height,
            self.request.accel,
        );
        (travel * self.post_accel_scale, before_peak)
    }

    pub fn adjusted(&self, raw_travel: f32) -> f32 {
        apply_accel_y(
            raw_travel,
            self.request.elapsed_screen_s,
            self.request.effect_height,
            self.request.screen_height,
            self.request.accel,
        ) * self.post_accel_scale
    }

    pub fn lane_offset(&self, local_col: usize) -> f32 {
        tipsy_y_extra(
            local_col,
            self.request.arrow_effect_time_s,
            self.request.lane_tipsy,
        ) + move_col_extra(self.request.lane_move_y, local_col)
    }

    pub fn lane_y(
        &self,
        local_col: usize,
        receptor_y: f32,
        direction: f32,
        raw_travel: f32,
    ) -> f32 {
        receptor_y + direction * self.adjusted(raw_travel) + self.lane_offset(local_col)
    }

    pub fn lane_y_for_beat(
        &self,
        local_col: usize,
        beat: f32,
        receptor_y: f32,
        direction: f32,
    ) -> f32 {
        self.lane_y(local_col, receptor_y, direction, self.raw_beat(beat))
    }

    pub fn adjusted_from_screen_y(
        &self,
        local_col: usize,
        receptor_y: f32,
        direction: f32,
        screen_y: f32,
    ) -> f32 {
        let direction = if direction.abs() <= 0.000_1 {
            if direction < 0.0 { -0.000_1 } else { 0.000_1 }
        } else {
            direction
        };
        (screen_y - receptor_y - self.lane_offset(local_col)) / direction
    }

    pub fn visible_row_range(&self) -> Option<(i32, i32)> {
        let first = find_first_displayed_beat(
            self.request.search_beat,
            self.request.draw_distance_after_targets,
            self.request.note_count_stats,
            |beat| self.adjusted(self.raw_beat(beat)),
        );
        let last = find_last_displayed_beat(
            self.request.search_beat,
            self.request.draw_distance_before_targets,
            self.displayed_speed_percent,
            self.request.accel.boomerang > f32::EPSILON,
            |beat| self.adjusted_with_peak(self.raw_beat(beat)),
        );
        first.zip(last).map(|(first, last)| {
            let first_row = beat_to_note_row(first);
            let last_row = beat_to_note_row(last.max(first)).max(first_row);
            (first_row, last_row)
        })
    }

    pub fn arrow_effect_time_s(&self) -> f32 {
        self.request.arrow_effect_time_s
    }

    pub(crate) fn range_key(&self) -> ScrollRangeKey {
        let scroll = match self.request.scroll_speed {
            ScrollSpeedSetting::CMod(value) => [0, value.to_bits()],
            ScrollSpeedSetting::XMod(value) => [1, value.to_bits()],
            ScrollSpeedSetting::MMod(value) => [2, value.to_bits()],
        };
        ScrollRangeKey {
            timing: self.request.timing,
            stats: self.request.note_count_stats.as_ptr(),
            stats_len: self.request.note_count_stats.len(),
            accel: [
                self.request.accel.boost.to_bits(),
                self.request.accel.brake.to_bits(),
                self.request.accel.wave.to_bits(),
                self.request.accel.expand.to_bits(),
                self.request.accel.boomerang.to_bits(),
            ],
            scroll,
            geometry: [
                self.request.scroll_reference_bpm.to_bits(),
                self.request.music_rate.to_bits(),
                self.request.draw_distance_after_targets.to_bits(),
                self.request.draw_distance_before_targets.to_bits(),
                self.request.field_zoom.to_bits(),
                self.request.effect_height.to_bits(),
                self.request.screen_height.to_bits(),
            ],
            edit_beat_spacing: self.request.edit_beat_spacing,
            displayed_speed_percent: self.displayed_speed_percent.to_bits(),
            expand_time: if self.request.accel.expand.abs() > f32::EPSILON {
                self.request.elapsed_screen_s.to_bits()
            } else {
                0
            },
        }
    }

    pub(crate) fn search_beat(&self) -> f32 {
        self.request.search_beat
    }

    pub(crate) fn current_time_ns(&self) -> SongTimeNs {
        self.request.current_time_ns
    }
}

pub(crate) fn note_itg_row(note: &Note) -> i32 {
    beat_to_note_row(note.beat)
}

pub(crate) fn lane_window_bounds_by_note_row(
    notes: &[Note],
    indices: &[usize],
    range: Option<(i32, i32)>,
) -> Option<(usize, usize)> {
    let (low, high) = range?;
    if high < 0 {
        return Some((0, 0));
    }
    let low = low.max(0);
    Some((
        indices.partition_point(|&note_index| note_itg_row(&notes[note_index]) < low),
        indices.partition_point(|&note_index| note_itg_row(&notes[note_index]) <= high),
    ))
}

pub(crate) fn lane_hold_window_bounds_by_note_row(
    notes: &[Note],
    indices: &[usize],
    range: Option<(i32, i32)>,
) -> Option<(usize, usize)> {
    let (low, _) = range?;
    let (mut start, end) = lane_window_bounds_by_note_row(notes, indices, range)?;
    let low = low.max(0);
    while start > 0 {
        let prev_note_index = indices[start - 1];
        let prev_end_row = notes[prev_note_index]
            .hold
            .as_ref()
            .map_or(note_itg_row(&notes[prev_note_index]), |hold| {
                beat_to_note_row(hold.end_beat)
            });
        if prev_end_row < low {
            break;
        }
        start -= 1;
    }
    Some((start, end))
}

pub(crate) fn for_each_visible_note_index<F: FnMut(usize)>(
    indices: &[usize],
    notes: &[Note],
    range: Option<(i32, i32)>,
    mut f: F,
) {
    let Some((low, high)) = range else {
        for &i in indices {
            f(i);
        }
        return;
    };
    let Some((start, end)) = lane_window_bounds_by_note_row(notes, indices, Some((low, high)))
    else {
        return;
    };
    for &i in &indices[start..end] {
        f(i);
    }
}

pub(crate) fn for_each_visible_hold_index<F: FnMut(usize)>(
    indices: &[usize],
    notes: &[Note],
    range: Option<(i32, i32)>,
    mut f: F,
) {
    let Some((low, high)) = range else {
        for &i in indices {
            f(i);
        }
        return;
    };
    let Some((start, end)) = lane_hold_window_bounds_by_note_row(notes, indices, Some((low, high)))
    else {
        return;
    };
    for &i in &indices[start..end] {
        f(i);
    }
}

pub(crate) fn hold_overlaps_visible_window(
    note_index: usize,
    notes: &[Note],
    range: Option<(i32, i32)>,
) -> bool {
    let Some(note) = notes.get(note_index) else {
        return false;
    };
    let Some((low, high)) = range else {
        return true;
    };
    let start = note_itg_row(note);
    let end = note
        .hold
        .as_ref()
        .map(|h| beat_to_note_row(h.end_beat))
        .unwrap_or(start);
    high >= 0 && end >= low.max(0) && start <= high
}

fn note_count_at(stats: &[NoteCountStat], beat: f32) -> NoteCountStat {
    let ix = stats
        .partition_point(|stat| stat.beat <= beat)
        .saturating_sub(1);
    stats.get(ix).copied().unwrap_or(NoteCountStat {
        beat: 0.0,
        notes_lower: 0,
        notes_upper: 0,
    })
}

fn note_count_range(stats: &[NoteCountStat], low: f32, high: f32) -> usize {
    let low = note_count_at(stats, low);
    let high = note_count_at(stats, high);
    high.notes_upper.saturating_sub(low.notes_lower)
}

pub(crate) fn find_first_displayed_beat<F: FnMut(f32) -> f32>(
    current_beat: f32,
    draw_distance: f32,
    stats: &[NoteCountStat],
    mut y_for_beat: F,
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance.is_finite() {
        return None;
    }
    let mut high = current_beat.max(0.0);
    let has_cache = !stats.is_empty();
    let mut low = if has_cache { 0.0 } else { high - 4.0 };
    let mut first = low;
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        if y_for_beat(mid) < -draw_distance
            || (has_cache && note_count_range(stats, mid, current_beat) > MAX_NOTES_AFTER)
        {
            first = mid;
            low = mid;
        } else {
            high = mid;
        }
    }
    Some(first)
}

pub(crate) fn find_last_displayed_beat<F: FnMut(f32) -> (f32, bool)>(
    current_beat: f32,
    draw_distance: f32,
    displayed_speed_percent: f32,
    boomerang: bool,
    mut y_for_beat: F,
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance.is_finite() {
        return None;
    }
    let mut search_distance = 10.0;
    let mut last = current_beat + search_distance;
    for _ in 0..20 {
        let (y_offset, before_peak) = y_for_beat(last);
        if boomerang && !before_peak {
            last += search_distance;
        } else if y_offset > draw_distance {
            last -= search_distance;
        } else {
            last += search_distance;
        }
        search_distance *= 0.5;
    }
    if displayed_speed_percent < 0.75 {
        last = last.min(current_beat + 16.0);
    }
    Some(last)
}

pub(crate) const fn mine_hides_after_resolution(mine_result: Option<MineResult>) -> bool {
    mine_result.is_some()
}

use crate::style::MAX_NOTES_AFTER;

#[cfg(test)]
mod tests {
    use super::{
        MineLayerRequest, NoteGlowRequest, NoteLayerRequest, ScrollTravelRequest,
        compose_mine_layers, compose_note_glow, compose_note_layer, for_each_visible_hold_index,
        for_each_visible_note_index, hold_overlaps_visible_window, scroll_travel,
    };
    use crate::{
        AccelYParams, ModelMeshCache, ModelMeshCacheStats, apply_accel_y, move_col_extra,
        tipsy_y_extra,
    };
    use deadlib_present::actors::{Actor, SizeSpec, SpriteSource};
    use deadlib_render::BlendMode;
    use deadsync_core::note::NoteType;
    use deadsync_core::timing::beat_to_note_row;
    use deadsync_noteskin::{
        ModelDrawState, ModelMesh, ModelVertex, NoteskinSlot, SpriteDefinition,
    };
    use deadsync_rules::note::{HoldData, Note};
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::timing::{
        ScrollSegment, SpeedSegment, SpeedUnit, TimingData, TimingSegments,
    };
    use std::cell::Cell;
    use std::sync::Arc;

    struct GlowSlot {
        def: SpriteDefinition,
        model: Option<ModelMesh>,
        texture: Arc<str>,
        draw: ModelDrawState,
    }

    impl GlowSlot {
        fn sprite() -> Self {
            Self {
                def: SpriteDefinition {
                    size: [64, 64],
                    ..SpriteDefinition::default()
                },
                model: None,
                texture: Arc::from("glow-slot"),
                draw: ModelDrawState::default(),
            }
        }

        fn model() -> Self {
            Self {
                model: Some(ModelMesh {
                    vertices: Arc::from([ModelVertex {
                        pos: [0.0, 0.0, 0.0],
                        uv: [0.0, 0.0],
                        tex_matrix_scale: [1.0, 1.0],
                    }]),
                    bounds: [0.0, 0.0, 0.0, 64.0, 64.0, 0.0],
                }),
                ..Self::sprite()
            }
        }
    }

    impl NoteskinSlot for GlowSlot {
        fn sprite_def(&self) -> &SpriteDefinition {
            &self.def
        }

        fn source_size(&self) -> [i32; 2] {
            [64, 64]
        }

        fn texture_key_shared(&self) -> Arc<str> {
            self.texture.clone()
        }

        fn model(&self) -> Option<&ModelMesh> {
            self.model.as_ref()
        }

        fn base_rot_sin_cos(&self) -> [f32; 2] {
            [0.0, 1.0]
        }

        fn frame_index(&self, _time: f32, _beat: f32) -> usize {
            0
        }

        fn frame_index_from_phase(&self, _phase: f32) -> usize {
            0
        }

        fn uv_for_frame_at(&self, _frame_index: usize, _elapsed: f32) -> [f32; 4] {
            [0.0, 0.0, 1.0, 1.0]
        }

        fn model_draw_at(&self, _time: f32, _beat: f32) -> ModelDrawState {
            self.draw
        }

        fn model_glow_with_draw(
            &self,
            _draw: ModelDrawState,
            _time: f32,
            _beat: f32,
            _diffuse_alpha: f32,
        ) -> Option<[f32; 4]> {
            None
        }

        fn model_uv_params(&self, uv: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
            ([uv[2] - uv[0], uv[3] - uv[1]], [uv[0], uv[1]], [0.0, 0.0])
        }
    }

    fn glow_request(slot: &GlowSlot) -> NoteGlowRequest<'_, GlowSlot> {
        NoteGlowRequest {
            slot,
            draw: ModelDrawState::default(),
            model_center: [10.0, 20.0],
            sprite_center: [30.0, 40.0],
            size: [48.0, 56.0],
            uv: [0.1, 0.2, 0.7, 0.8],
            rotation_y_deg: 12.0,
            model_rotation_z_deg: 23.0,
            sprite_rotation_z_deg: 34.0,
            alpha: 0.75,
            blend: BlendMode::Add,
            z: 140,
            world_z: 9.0,
            prefer_sprite: false,
        }
    }

    fn layer_request(slot: &GlowSlot) -> NoteLayerRequest<'_, GlowSlot> {
        NoteLayerRequest {
            slot,
            draw: ModelDrawState::default(),
            model_center: [10.0, 20.0],
            sprite_center: [30.0, 40.0],
            size: [48.0, 56.0],
            uv: [0.1, 0.2, 0.7, 0.8],
            rotation_y_deg: 12.0,
            model_rotation_z_deg: 23.0,
            sprite_rotation_z_deg: 34.0,
            tint: [0.2, 0.3, 0.4, 0.5],
            glow_alpha: 0.75,
            blend: BlendMode::Alpha,
            z: 140,
            world_z: 9.0,
            prefer_sprite: false,
        }
    }

    fn timing() -> TimingData {
        TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                ..TimingSegments::default()
            },
            &[],
        )
    }

    fn request<'a>(
        timing: &'a TimingData,
        scroll_speed: ScrollSpeedSetting,
        visible_beat: f32,
    ) -> ScrollTravelRequest<'a> {
        ScrollTravelRequest {
            timing,
            accel: AccelYParams::default(),
            scroll_speed,
            current_time_ns: timing.get_time_for_beat_ns(visible_beat),
            visible_beat,
            search_beat: visible_beat,
            scroll_reference_bpm: 120.0,
            music_rate: 1.0,
            edit_beat_spacing: false,
            draw_distance_after_targets: 64.0,
            draw_distance_before_targets: 64.0,
            field_zoom: 1.0,
            elapsed_screen_s: 0.0,
            effect_height: 640.0,
            screen_height: 720.0,
            note_count_stats: &[],
            arrow_effect_time_s: 0.0,
            lane_tipsy: 0.0,
            lane_move_y: &[],
        }
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.001,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn note_glow_skips_zero_alpha_before_resolving_sprite_source() {
        let slot = GlowSlot::sprite();
        let mut request = glow_request(&slot);
        request.alpha = f32::NAN;
        let mut actors = Vec::new();
        let mut cache = ModelMeshCache::default();
        let source_calls = Cell::new(0);

        compose_note_glow(&mut actors, &mut cache, request, &|_| {
            source_calls.set(source_calls.get() + 1);
            SpriteSource::static_texture("unused")
        });

        assert!(actors.is_empty());
        assert_eq!(source_calls.get(), 0);
        assert_eq!(cache.stats(), ModelMeshCacheStats::default());
    }

    #[test]
    fn note_glow_uses_cached_model_actor_and_preserves_layer_fields() {
        let slot = GlowSlot::model();
        let mut actors = Vec::new();
        let mut cache = ModelMeshCache::with_capacity(1);

        for _ in 0..2 {
            compose_note_glow(&mut actors, &mut cache, glow_request(&slot), &|_| {
                panic!("model glow must not resolve a sprite source")
            });
        }

        assert_eq!(actors.len(), 2);
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 1,
                misses: 1,
                saturated_misses: 0,
            }
        );
        let Actor::TexturedMesh {
            offset,
            world_z,
            tint,
            glow,
            blend,
            z,
            ..
        } = &actors[0]
        else {
            panic!("model-backed glow should emit a textured mesh");
        };
        assert_eq!(*offset, [10.0, 20.0]);
        assert_eq!(*world_z, 9.0);
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.75]);
        assert_eq!(*blend, BlendMode::Add);
        assert_eq!(*z, 140);
    }

    #[test]
    fn note_glow_prefer_sprite_uses_supplied_source_and_sprite_transform() {
        let slot = GlowSlot::model();
        let mut request = glow_request(&slot);
        request.prefer_sprite = true;
        let mut actors = Vec::new();
        let mut cache = ModelMeshCache::default();
        let source_calls = Cell::new(0);

        compose_note_glow(&mut actors, &mut cache, request, &|_| {
            source_calls.set(source_calls.get() + 1);
            SpriteSource::static_texture("fast-path")
        });

        assert_eq!(source_calls.get(), 1);
        assert_eq!(cache.stats(), ModelMeshCacheStats::default());
        let Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            tint,
            glow,
            z,
            uv_rect,
            blend,
            rot_y_deg,
            rot_z_deg,
            ..
        } = &actors[0]
        else {
            panic!("preferred sprite glow should emit a sprite");
        };
        assert_eq!(*align, [0.5, 0.5]);
        assert_eq!(*offset, [30.0, 40.0]);
        assert_eq!(*world_z, 9.0);
        assert!(matches!(size, [SizeSpec::Px(48.0), SizeSpec::Px(56.0)]));
        assert_eq!(source.texture_key(), Some("fast-path"));
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.75]);
        assert_eq!(*z, 140);
        assert_eq!(*uv_rect, Some([0.1, 0.2, 0.7, 0.8]));
        assert_eq!(*blend, BlendMode::Alpha);
        assert_eq!(*rot_y_deg, 12.0);
        assert_eq!(*rot_z_deg, 34.0);
    }

    #[test]
    fn note_glow_sprite_uses_authored_additive_blend() {
        let slot = GlowSlot::sprite();
        let mut request = glow_request(&slot);
        request.draw.blend_add = true;
        let mut actors = Vec::new();
        let mut cache = ModelMeshCache::default();

        compose_note_glow(&mut actors, &mut cache, request, &|_| {
            SpriteSource::static_texture("additive")
        });

        assert!(matches!(
            actors.as_slice(),
            [Actor::Sprite {
                blend: BlendMode::Add,
                ..
            }]
        ));
    }

    #[test]
    fn note_layer_sprite_preserves_diffuse_then_glow_and_additive_blend() {
        let slot = GlowSlot::sprite();
        let mut request = layer_request(&slot);
        request.draw.blend_add = true;
        request.blend = BlendMode::Add;
        let mut actors = Vec::new();
        let mut cache = ModelMeshCache::default();

        compose_note_layer(&mut actors, &mut cache, request, &|_| {
            SpriteSource::static_texture("additive-layer")
        });

        assert_eq!(actors.len(), 2);
        let Actor::Sprite {
            source,
            tint,
            glow,
            blend,
            world_z,
            rot_y_deg,
            rot_z_deg,
            ..
        } = &actors[0]
        else {
            panic!("sprite-backed diffuse pass should emit a sprite");
        };
        assert_eq!(source.texture_key(), Some("additive-layer"));
        assert_eq!(*tint, [0.2, 0.3, 0.4, 0.5]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*blend, BlendMode::Add);
        assert_eq!(*world_z, 9.0);
        assert_eq!(*rot_y_deg, 12.0);
        assert_eq!(*rot_z_deg, 34.0);

        let Actor::Sprite {
            tint,
            glow,
            blend,
            world_z,
            ..
        } = &actors[1]
        else {
            panic!("sprite-backed glow pass should follow the diffuse pass");
        };
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.75]);
        assert_eq!(*blend, BlendMode::Add);
        assert_eq!(*world_z, 9.0);
    }

    #[test]
    fn note_layer_model_reuses_cached_geometry_for_diffuse_and_glow() {
        let slot = GlowSlot::model();
        let mut actors = Vec::new();
        let mut cache = ModelMeshCache::with_capacity(1);

        compose_note_layer(&mut actors, &mut cache, layer_request(&slot), &|_| {
            panic!("model-backed note layer must not resolve a sprite source")
        });
        compose_note_layer(&mut actors, &mut cache, layer_request(&slot), &|_| {
            panic!("cached model-backed note layer must not resolve a sprite source")
        });

        assert_eq!(actors.len(), 4);
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 3,
                misses: 1,
                saturated_misses: 0,
            }
        );
        let Actor::TexturedMesh {
            offset,
            tint,
            glow,
            blend,
            world_z,
            z,
            ..
        } = &actors[0]
        else {
            panic!("model-backed diffuse pass should emit a textured mesh");
        };
        assert_eq!(*offset, [10.0, 20.0]);
        assert_eq!(*tint, [0.2, 0.3, 0.4, 0.5]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*blend, BlendMode::Alpha);
        assert_eq!(*world_z, 9.0);
        assert_eq!(*z, 140);

        let Actor::TexturedMesh { tint, glow, .. } = &actors[1] else {
            panic!("model-backed glow pass should follow the diffuse pass");
        };
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.75]);
    }

    fn named_slot(mut slot: GlowSlot, key: &'static str) -> GlowSlot {
        slot.texture = Arc::from(key);
        slot
    }

    fn mine_request<'a>(
        fill_slot: Option<&'a GlowSlot>,
        gradient_slot: Option<&'a GlowSlot>,
        frame_slot: Option<&'a GlowSlot>,
    ) -> MineLayerRequest<'a, GlowSlot> {
        MineLayerRequest {
            fill_slot,
            gradient_slot,
            frame_slot,
            gradient_size: [18.0, 20.0],
            center: [30.0, 40.0],
            mine_uv_phase: 0.25,
            mine_fill_phase: 0.5,
            elapsed_s: 2.0,
            display_time_s: 3.0,
            current_beat: 4.0,
            uv_translation: [0.1, 0.2],
            rotation_y_deg: 12.0,
            note_rotation_z_deg: 5.0,
            alpha: 0.8,
            glow_alpha: 0.6,
            note_z: 140,
            world_z: 9.0,
            prefer_sprite: false,
        }
    }

    #[test]
    fn mine_layers_order_gradient_before_frame_sprite_passes() {
        let fill = named_slot(GlowSlot::sprite(), "mine-fill");
        let gradient = named_slot(GlowSlot::sprite(), "mine-gradient");
        let mut frame = named_slot(GlowSlot::sprite(), "mine-frame");
        frame.def.rotation_deg = 10;
        frame.draw.rot[2] = 3.0;
        let mut actors = Vec::new();
        let mut cache = ModelMeshCache::default();
        let size_calls = Cell::new(0);

        compose_mine_layers(
            &mut actors,
            &mut cache,
            mine_request(Some(&fill), Some(&gradient), Some(&frame)),
            &|slot| {
                size_calls.set(size_calls.get() + 1);
                assert_eq!(slot.texture.as_ref(), "mine-frame");
                [70.0, 72.0]
            },
            &|slot| SpriteSource::Texture(slot.texture.clone()),
        );

        assert_eq!(actors.len(), 4);
        assert_eq!(size_calls.get(), 1);
        assert_eq!(cache.stats(), ModelMeshCacheStats::default());
        let keys = actors
            .iter()
            .map(|actor| match actor {
                Actor::Sprite { source, .. } => source.texture_key().unwrap_or_default(),
                actor => panic!("mine sprite path emitted {actor:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            ["mine-gradient", "mine-gradient", "mine-frame", "mine-frame"]
        );

        let Actor::Sprite {
            size,
            tint,
            glow,
            z,
            world_z,
            rot_y_deg,
            rot_z_deg,
            ..
        } = &actors[0]
        else {
            unreachable!();
        };
        assert!(matches!(size, [SizeSpec::Px(18.0), SizeSpec::Px(20.0)]));
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.8]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*z, 138);
        assert_eq!(*world_z, 9.0);
        assert_eq!(*rot_y_deg, 0.0);
        assert_eq!(*rot_z_deg, 0.0);

        let Actor::Sprite {
            size,
            tint,
            glow,
            z,
            world_z,
            rot_y_deg,
            rot_z_deg,
            ..
        } = &actors[2]
        else {
            unreachable!();
        };
        assert!(matches!(size, [SizeSpec::Px(70.0), SizeSpec::Px(72.0)]));
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.8]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*z, 140);
        assert_eq!(*world_z, 9.0);
        assert_eq!(*rot_y_deg, 12.0);
        assert_eq!(*rot_z_deg, -2.0);
        let Actor::Sprite { tint, glow, .. } = &actors[3] else {
            unreachable!();
        };
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.6]);
    }

    #[test]
    fn mine_layers_model_fill_reuses_geometry_for_glow() {
        let mut fill = named_slot(GlowSlot::model(), "mine-model-fill");
        fill.def.rotation_deg = 10;
        fill.draw.rot[2] = 3.0;
        let mut actors = Vec::new();
        let mut cache = ModelMeshCache::with_capacity(1);
        let source_calls = Cell::new(0);

        compose_mine_layers(
            &mut actors,
            &mut cache,
            mine_request(Some(&fill), None, None),
            &|_| [64.0, 66.0],
            &|_| {
                source_calls.set(source_calls.get() + 1);
                SpriteSource::static_texture("unused")
            },
        );

        assert_eq!(actors.len(), 2);
        assert_eq!(source_calls.get(), 0);
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 1,
                misses: 1,
                saturated_misses: 0,
            }
        );
        let Actor::TexturedMesh {
            tint,
            glow,
            z,
            world_z,
            ..
        } = &actors[0]
        else {
            panic!("model-backed mine fill should emit a textured mesh");
        };
        assert_eq!(tint[..3], [1.0, 1.0, 1.0]);
        assert_near(tint[3], 0.72);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*z, 139);
        assert_eq!(*world_z, 9.0);
        let Actor::TexturedMesh { tint, glow, .. } = &actors[1] else {
            unreachable!();
        };
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.6]);
    }

    fn note(beat: f32) -> Note {
        Note {
            beat,
            quantization_idx: 0,
            column: 0,
            note_type: NoteType::Tap,
            row_index: beat_to_note_row(beat).max(0) as usize,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    fn hold(beat: f32, end_beat: f32) -> Note {
        let mut note = note(beat);
        note.note_type = NoteType::Hold;
        note.hold = Some(HoldData {
            end_row_index: beat_to_note_row(end_beat).max(0) as usize,
            end_beat,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 1.0,
            last_held_row_index: note.row_index,
            last_held_beat: beat,
        });
        note
    }

    #[test]
    fn projects_cmod_xmod_mmod_and_edit_spacing() {
        let timing = timing();

        let mut cmod_request = request(&timing, ScrollSpeedSetting::CMod(600.0), 4.0);
        cmod_request.music_rate = 2.0;
        let cmod = scroll_travel(cmod_request);
        assert_near(cmod.raw_beat(5.0), 160.0);
        assert_near(cmod.adjusted(cmod.raw_beat(5.0)), 160.0);

        let xmod = scroll_travel(request(&timing, ScrollSpeedSetting::XMod(2.0), 4.0));
        assert_near(xmod.raw_beat(5.0), 64.0);
        assert_near(xmod.adjusted(xmod.raw_beat(5.0)), 128.0);

        let mmod = scroll_travel(request(&timing, ScrollSpeedSetting::MMod(600.0), 4.0));
        assert_near(mmod.raw_beat(5.0), 64.0);
        assert_near(mmod.adjusted(mmod.raw_beat(5.0)), 320.0);

        let scrolled_timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                scrolls: vec![ScrollSegment {
                    beat: 0.0,
                    ratio: 0.25,
                }],
                ..TimingSegments::default()
            },
            &[],
        );
        let displayed = scroll_travel(request(
            &scrolled_timing,
            ScrollSpeedSetting::XMod(2.0),
            4.0,
        ));
        let mut edit_request = request(&scrolled_timing, ScrollSpeedSetting::XMod(2.0), 4.0);
        edit_request.edit_beat_spacing = true;
        let edit = scroll_travel(edit_request);
        assert_near(displayed.raw_beat(5.0), 16.0);
        assert_near(edit.raw_beat(5.0), 64.0);
        assert_near(edit.adjusted(edit.raw_beat(5.0)), 128.0);
    }

    #[test]
    fn invalid_rate_and_reference_bpm_keep_existing_fallbacks() {
        let timing = timing();
        let mut cmod_request = request(&timing, ScrollSpeedSetting::CMod(600.0), 4.0);
        cmod_request.music_rate = f32::NAN;
        assert_near(scroll_travel(cmod_request).raw_beat(5.0), 320.0);

        let mut mmod_request = request(&timing, ScrollSpeedSetting::MMod(600.0), 4.0);
        mmod_request.music_rate = 0.0;
        mmod_request.scroll_reference_bpm = f32::NAN;
        let mmod = scroll_travel(mmod_request);
        assert_near(mmod.raw_beat(5.0), 64.0);
        assert_near(mmod.adjusted(mmod.raw_beat(5.0)), 64.0);
    }

    #[test]
    fn applies_brake_and_boomerang_before_post_scroll_scale() {
        let timing = timing();
        let mut brake_request = request(&timing, ScrollSpeedSetting::XMod(2.0), 0.0);
        brake_request.accel.brake = 1.0;
        let brake = scroll_travel(brake_request);
        let raw = brake.raw_beat(1.0);
        let expected = apply_accel_y(
            raw,
            0.0,
            brake_request.effect_height,
            brake_request.screen_height,
            brake_request.accel,
        ) * 2.0;
        assert_near(brake.adjusted(raw), expected);
        assert_ne!(
            brake.adjusted(raw),
            apply_accel_y(
                raw * 2.0,
                0.0,
                brake_request.effect_height,
                brake_request.screen_height,
                brake_request.accel,
            )
        );

        let mut boomerang_request = request(&timing, ScrollSpeedSetting::XMod(2.0), 0.0);
        boomerang_request.accel.boomerang = 1.0;
        let boomerang = scroll_travel(boomerang_request);
        let raw = boomerang.raw_beat(10.0);
        let (adjusted, before_peak) = boomerang.adjusted_with_peak(raw);
        let (expected, expected_before_peak) = crate::apply_accel_y_with_peak(
            raw,
            0.0,
            boomerang_request.effect_height,
            boomerang_request.screen_height,
            boomerang_request.accel,
        );
        assert_eq!(before_peak, expected_before_peak);
        assert!(!before_peak);
        assert_near(adjusted, expected * 2.0);
    }

    #[test]
    fn zero_scroll_lead_in_preserves_visible_future_rows() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                speeds: vec![SpeedSegment {
                    beat: 0.0,
                    ratio: 0.1,
                    delay: 0.0,
                    unit: SpeedUnit::Beats,
                }],
                scrolls: vec![
                    ScrollSegment {
                        beat: 0.0,
                        ratio: 0.0,
                    },
                    ScrollSegment {
                        beat: 4.0,
                        ratio: 1.0,
                    },
                ],
                ..TimingSegments::default()
            },
            &[],
        );
        let mut request = request(&timing, ScrollSpeedSetting::XMod(1.0), -12.0);
        request.draw_distance_before_targets = 120.0;
        let range = scroll_travel(request)
            .visible_row_range()
            .expect("finite lead-in range");
        assert!(range.1 >= beat_to_note_row(4.0), "range={range:?}");
    }

    #[test]
    fn planned_rows_bound_notes_and_keep_overlapping_holds() {
        let timing = timing();
        let travel = scroll_travel(request(&timing, ScrollSpeedSetting::XMod(1.0), 4.0));
        let range = travel.visible_row_range().expect("finite row range");
        let notes = vec![hold(2.0, 4.0), note(4.0), note(10.0)];

        let mut taps = Vec::new();
        for_each_visible_note_index(&[0, 1, 2], &notes, Some(range), |i| taps.push(i));
        assert_eq!(taps, vec![1]);

        let mut holds = Vec::new();
        for_each_visible_hold_index(&[0], &notes, Some(range), |i| holds.push(i));
        assert_eq!(holds, vec![0]);
        assert!(hold_overlaps_visible_window(0, &notes, Some(range)));
        assert_near(travel.raw_note(&notes[0], true), 0.0);
    }

    #[test]
    fn lane_projection_uses_supplied_arrow_effect_time() {
        let timing = timing();
        let move_y = [0.0, 5.0];
        let mut request = request(&timing, ScrollSpeedSetting::XMod(1.0), 4.0);
        request.arrow_effect_time_s = 2.25;
        request.lane_tipsy = 0.75;
        request.lane_move_y = &move_y;
        let travel = scroll_travel(request);
        let expected_offset = tipsy_y_extra(1, 2.25, 0.75) + move_col_extra(&move_y, 1);
        assert_near(travel.lane_offset(1), expected_offset);

        let raw = travel.raw_beat(5.0);
        let y = travel.lane_y(1, 100.0, -1.0, raw);
        assert_near(y, 100.0 - travel.adjusted(raw) + expected_offset);
        assert_near(
            travel.adjusted_from_screen_y(1, 100.0, -1.0, y),
            travel.adjusted(raw),
        );
        assert_near(travel.arrow_effect_time_s(), 2.25);
    }
}
