use crate::holds::{song_time_ns_delta_seconds, translated_uv_rect};
use crate::measure_lines::{beat_scroll_travel, edit_beat_scroll_travel};
use crate::transforms::{
    AccelYCache, AccelYParams, accel_y_cache, accel_y_is_identity, apply_accel_y_cached,
    apply_accel_y_with_peak_cached, move_col_extra, tipsy_y_extra,
};
use crate::{
    ModelMeshCache, itg_actor_glow_alpha, noteskin_model_flat_draw_cached, song_lua_note_model_draw,
};
use deadlib_present::actors::{FlatDraw, FlatSprite, SpriteSource};
use deadlib_render_core::BlendMode;
use deadsync_core::input::MAX_COLS;
use deadsync_core::song_time::SongTimeNs;
use deadsync_core::timing::beat_to_note_row;
use deadsync_gameplay::ChartNoteIndex;
use deadsync_noteskin::{
    ModelDrawState, NoteColorType, NotePartAnimation, NotePartTextureTranslate, NoteskinSlot,
};
use deadsync_rules::note::{MineResult, Note, NoteCountStat};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::TimingData;
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

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NotePartPhaseCache {
    base_phase: f32,
    vivid_interval: f32,
    single_bucket_vivid: bool,
    vivid: bool,
}

#[inline(always)]
pub(crate) fn note_part_phase_cache(
    song_seconds: f32,
    song_beat: f32,
    animation: NotePartAnimation,
    beat_based: bool,
) -> NotePartPhaseCache {
    let length = animation.length.max(1e-6);
    let clock = if beat_based { song_beat } else { song_seconds };
    let vivid_interval = if animation.vivid { 1.0 / length } else { 0.0 };
    NotePartPhaseCache {
        base_phase: clock.rem_euclid(length) / length,
        vivid_interval,
        single_bucket_vivid: animation.vivid && length <= 1.0,
        vivid: animation.vivid,
    }
}

#[inline(always)]
fn note_beat_fraction(note_beat: f32) -> f32 {
    note_beat - note_beat.floor()
}

#[inline(always)]
fn vivid_bucket_offset(note_fraction: f32, interval: f32) -> f32 {
    (note_fraction / interval).floor() * interval
}

#[inline(always)]
fn wrap_vivid_phase(phase: f32) -> f32 {
    if phase < 1.0 {
        phase
    } else if phase < 2.0 {
        phase - 1.0
    } else {
        phase.rem_euclid(1.0)
    }
}

#[inline(always)]
pub(crate) fn note_part_phase_cached(note_beat: f32, cache: NotePartPhaseCache) -> f32 {
    if !cache.vivid {
        return cache.base_phase;
    }
    if cache.single_bucket_vivid && note_beat.is_finite() {
        return cache.base_phase;
    }
    let note_fraction = note_beat_fraction(note_beat);
    let vivid_offset = vivid_bucket_offset(note_fraction, cache.vivid_interval);
    wrap_vivid_phase(cache.base_phase + vivid_offset)
}

#[inline(always)]
pub(crate) fn note_part_uv_translation_for_quantization(
    note_beat: f32,
    quantization_idx: u8,
    metrics: NotePartTextureTranslate,
    is_addition: bool,
) -> [f32; 2] {
    let count = metrics.note_color_count.max(1);
    let countf = count as f32;
    let color = match metrics.note_color_type {
        NoteColorType::Denominator => i32::from(quantization_idx).min(count - 1) as f32,
        NoteColorType::Progress => integral_color_remainder((note_beat * countf).ceil(), count),
        NoteColorType::ProgressAlternate => {
            let mut scaled = note_beat * countf;
            let in_fast_integer_range =
                scaled.is_finite() && scaled.abs() < 9_223_372_036_854_775_808.0;
            let is_integer = if in_fast_integer_range {
                scaled == scaled.trunc()
            } else {
                scaled - (scaled as i64 as f32) == 0.0
            };
            if is_integer {
                scaled += countf - 1.0;
            }
            integral_color_remainder(scaled.ceil(), count)
        }
    };
    let add = if is_addition {
        metrics.addition_offset
    } else {
        [0.0, 0.0]
    };
    [
        metrics.note_color_spacing[0].mul_add(color, add[0]),
        metrics.note_color_spacing[1].mul_add(color, add[1]),
    ]
}

#[inline(always)]
fn integral_color_remainder(value: f32, count: i32) -> f32 {
    if value == 0.0 {
        return value;
    }
    // Every integer through 2^24 is exact in f32; above that, `% count as f32`
    // can intentionally use a rounded divisor and must remain on the float path.
    if count <= 16_777_216 && value >= i32::MIN as f32 && value < 2_147_483_648.0 {
        let remainder = (value as i32) % count;
        if remainder == 0 && value.is_sign_negative() {
            -0.0
        } else {
            remainder as f32
        }
    } else {
        value % count as f32
    }
}

#[cfg(test)]
mod note_metadata_cache_tests {
    use super::*;

    #[test]
    fn part_phase_cache_handles_clocks_and_vivid_buckets() {
        let plain = note_part_phase_cache(
            5.5,
            3.25,
            NotePartAnimation {
                length: 2.0,
                vivid: false,
            },
            false,
        );
        assert_eq!(note_part_phase_cached(0.25, plain), 0.75);
        assert_eq!(note_part_phase_cached(f32::NAN, plain), 0.75);

        let vivid = note_part_phase_cache(
            5.5,
            3.25,
            NotePartAnimation {
                length: 2.0,
                vivid: true,
            },
            true,
        );
        assert_eq!(note_part_phase_cached(0.25, vivid), 0.625);
        assert_eq!(note_part_phase_cached(0.75, vivid), 0.125);

        let single_bucket = note_part_phase_cache(
            5.5,
            3.25,
            NotePartAnimation {
                length: 0.5,
                vivid: true,
            },
            true,
        );
        assert_eq!(note_part_phase_cached(0.75, single_bucket), 0.5);
        assert!(note_part_phase_cached(f32::NAN, single_bucket).is_nan());
    }

    #[test]
    fn preclassified_uv_translation_obeys_color_modes() {
        let metrics = |note_color_type| NotePartTextureTranslate {
            addition_offset: [0.125, -0.25],
            note_color_spacing: [0.25, 0.5],
            note_color_count: 4,
            note_color_type,
        };

        let denominator = metrics(NoteColorType::Denominator);
        assert_eq!(
            note_part_uv_translation_for_quantization(0.0, 2, denominator, false),
            [0.5, 1.0]
        );
        assert_eq!(
            note_part_uv_translation_for_quantization(0.0, 2, denominator, true),
            [0.625, 0.75]
        );
        assert_eq!(
            note_part_uv_translation_for_quantization(0.0, u8::MAX, denominator, false),
            [0.75, 1.5]
        );

        let progress = metrics(NoteColorType::Progress);
        assert_eq!(
            note_part_uv_translation_for_quantization(0.3, 0, progress, false),
            [0.5, 1.0]
        );
        assert_eq!(
            note_part_uv_translation_for_quantization(-0.25, 0, progress, false),
            [-0.25, -0.5]
        );

        let alternate = metrics(NoteColorType::ProgressAlternate);
        assert_eq!(
            note_part_uv_translation_for_quantization(0.5, 0, alternate, false),
            [0.25, 0.5]
        );
        assert_eq!(
            note_part_uv_translation_for_quantization(0.25, 0, alternate, false),
            [0.0, 0.0]
        );
    }

    #[test]
    fn floor_fraction_matches_euclidean_note_fraction() {
        for note_beat in [
            -4096.9375,
            -1024.5,
            -4.0,
            -0.75,
            -0.0,
            0.0,
            0.125,
            12.75,
            1_024.937_5,
            f32::MAX,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NAN,
        ] {
            let expected = note_beat.rem_euclid(1.0);
            let actual = note_beat_fraction(note_beat);
            if expected.is_nan() {
                assert!(actual.is_nan(), "note_beat={note_beat}");
            } else {
                assert_eq!(actual, expected, "note_beat={note_beat}");
            }
        }
    }

    #[test]
    fn bounded_vivid_phase_wrap_matches_euclidean_wrap() {
        for phase in [
            -0.0_f32,
            0.0,
            0.125,
            0.999_999,
            1.0,
            1.125,
            1.999_999,
            2.0,
            2.75,
            f32::INFINITY,
            f32::NAN,
        ] {
            let expected = phase.rem_euclid(1.0);
            let actual = wrap_vivid_phase(phase);
            if expected.is_nan() {
                assert!(actual.is_nan(), "phase={phase}");
            } else {
                assert_eq!(actual.to_bits(), expected.to_bits(), "phase={phase}");
            }
        }
    }
}

struct MineSlotPass<'a, S> {
    slot: &'a S,
    alpha_scale: f32,
    z: i16,
}

#[inline(always)]
fn flat_sprite<S, F>(
    slot: &S,
    request: &NoteLayerRequest<'_, S>,
    tint: [f32; 4],
    glow: [f32; 4],
    blend: BlendMode,
    sprite_source: &F,
) -> FlatDraw
where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    FlatDraw::Sprite(FlatSprite {
        center: request.sprite_center,
        world_z: request.world_z,
        size: request.size,
        source: sprite_source(slot),
        tint,
        glow,
        uv_rect: request.uv,
        flip_x: slot.sprite_def().mirror_h,
        flip_y: slot.sprite_def().mirror_v,
        fade: request.draw.fade,
        blend,
        rot_y_deg: request.rotation_y_deg,
        rot_z_deg: request.sprite_rotation_z_deg,
        z: request.z,
    })
}

/// Writes one pre-resolved tap, mine, or hold-head layer into the narrow flat
/// presentation stream.
pub(crate) fn compose_flat_note_layer<S, F>(
    draws: &mut Vec<FlatDraw>,
    model_cache: &mut ModelMeshCache,
    request: NoteLayerRequest<'_, S>,
    sprite_source: &F,
) where
    S: NoteskinSlot,
    F: Fn(&S) -> SpriteSource,
{
    if !request.prefer_sprite
        && let Some(mut mesh) = noteskin_model_flat_draw_cached(
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
        mesh.world_z = request.world_z;
        draws.push(FlatDraw::TexturedMesh(mesh));
    } else {
        draws.push(flat_sprite(
            request.slot,
            &request,
            request.tint,
            [1.0, 1.0, 1.0, 0.0],
            request.blend,
            sprite_source,
        ));
    }

    let glow_alpha = itg_actor_glow_alpha(request.glow_alpha);
    if glow_alpha <= f32::EPSILON {
        return;
    }
    let glow = [1.0, 1.0, 1.0, glow_alpha];
    let glow_blend = if request.draw.blend_add {
        BlendMode::Add
    } else {
        BlendMode::Alpha
    };
    if !request.prefer_sprite
        && let Some(mut mesh) = noteskin_model_flat_draw_cached(
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
        mesh.world_z = request.world_z;
        mesh.glow = glow;
        draws.push(FlatDraw::TexturedMesh(mesh));
    } else {
        draws.push(flat_sprite(
            request.slot,
            &request,
            [1.0, 1.0, 1.0, 0.0],
            glow,
            glow_blend,
            sprite_source,
        ));
    }
}

/// Appends a mine's gradient-or-fill draws followed by its frame draws.
pub(crate) fn compose_flat_mine_layers<S, F, Z>(
    draws: &mut Vec<FlatDraw>,
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
        compose_flat_mine_gradient(
            draws,
            model_cache,
            request
                .gradient_slot
                .expect("gradient presence checked above"),
            &request,
            sprite_source,
        );
    } else if let Some(slot) = request.fill_slot {
        compose_flat_mine_slot(
            draws,
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
        compose_flat_mine_slot(
            draws,
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

fn compose_flat_mine_gradient<S, F>(
    draws: &mut Vec<FlatDraw>,
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
    compose_flat_note_layer(
        draws,
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

fn compose_flat_mine_slot<S, F, Z>(
    draws: &mut Vec<FlatDraw>,
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
        model_cache.draw_at(slot, request.display_time_s, request.current_beat),
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
    compose_flat_note_layer(
        draws,
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollTravelRequest<'a> {
    pub timing: &'a TimingData,
    pub accel: AccelYParams,
    pub random_speed: f32,
    pub stage_seed: u32,
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
    ConstantOneRate {
        current_time_ns: SongTimeNs,
        beats_per_second: f32,
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
    accel_is_identity: bool,
    accel_cache: AccelYCache,
    random_speed_lane_seeds: [u32; MAX_COLS],
}

pub(crate) fn scroll_travel(request: ScrollTravelRequest<'_>) -> ScrollTravel<'_> {
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
                let raw = if rate == 1.0 {
                    RawTravel::ConstantOneRate {
                        current_time_ns: request.current_time_ns,
                        beats_per_second: c_bpm / 60.0,
                    }
                } else {
                    RawTravel::Constant {
                        current_time_ns: request.current_time_ns,
                        rate,
                        beats_per_second: c_bpm / 60.0,
                    }
                };
                (raw, request.field_zoom)
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
    let random_speed_lane_seeds = if request.random_speed <= 0.0 {
        [0; MAX_COLS]
    } else {
        std::array::from_fn(|local_col| random_speed_lane_seed(request.stage_seed, local_col))
    };
    ScrollTravel {
        request,
        raw,
        displayed_speed_percent,
        post_accel_scale,
        accel_is_identity: accel_y_is_identity(request.accel),
        accel_cache: accel_y_cache(
            request.elapsed_screen_s,
            request.effect_height,
            request.accel,
        ),
        random_speed_lane_seeds,
    }
}

impl ScrollTravel<'_> {
    #[inline(always)]
    pub(crate) const fn supports_sparse_measure_line_candidates(
        &self,
        displayed_beat_monotonic: bool,
    ) -> bool {
        self.accel_is_identity
            && match self.raw {
                RawTravel::Beat { .. } => displayed_beat_monotonic,
                RawTravel::Edit { .. }
                | RawTravel::ConstantOneRate { .. }
                | RawTravel::Constant { .. } => true,
            }
    }

    #[must_use]
    pub fn raw_beat(&self, beat: f32) -> f32 {
        match self.raw {
            RawTravel::Edit { current_beat } => edit_beat_scroll_travel(beat, current_beat),
            RawTravel::ConstantOneRate {
                current_time_ns,
                beats_per_second,
            } => {
                let note_time_ns = self.request.timing.get_time_for_beat_ns(beat);
                song_time_ns_delta_seconds(note_time_ns, current_time_ns)
                    * beats_per_second
                    * ScrollSpeedSetting::ARROW_SPACING
            }
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

    pub(crate) fn raw_note(
        &self,
        note: &Note,
        use_hold_end: bool,
        cached_time_ns: Option<SongTimeNs>,
        cached_displayed_beat: Option<f32>,
    ) -> f32 {
        if let (
            RawTravel::ConstantOneRate {
                current_time_ns,
                beats_per_second,
            },
            Some(note_time_ns),
        ) = (self.raw, cached_time_ns)
        {
            return song_time_ns_delta_seconds(note_time_ns, current_time_ns)
                * beats_per_second
                * ScrollSpeedSetting::ARROW_SPACING;
        }
        if let (
            RawTravel::Constant {
                current_time_ns,
                rate,
                beats_per_second,
            },
            Some(note_time_ns),
        ) = (self.raw, cached_time_ns)
        {
            let real_seconds = song_time_ns_delta_seconds(note_time_ns, current_time_ns) / rate;
            return real_seconds * beats_per_second * ScrollSpeedSetting::ARROW_SPACING;
        }
        if let (
            RawTravel::Beat {
                current_displayed_beat,
                displayed_speed_percent,
            },
            Some(displayed_beat),
        ) = (self.raw, cached_displayed_beat)
        {
            return beat_scroll_travel(
                displayed_beat,
                current_displayed_beat,
                displayed_speed_percent,
            );
        }
        let beat = if use_hold_end {
            note.hold.as_ref().map_or(note.beat, |hold| hold.end_beat)
        } else {
            note.beat
        };
        self.raw_beat(beat)
    }

    #[inline(always)]
    #[must_use]
    pub fn adjusted_with_peak(&self, raw_travel: f32) -> (f32, bool) {
        if self.accel_is_identity {
            return (raw_travel * self.post_accel_scale, true);
        }
        let (travel, before_peak) = apply_accel_y_with_peak_cached(
            raw_travel,
            self.request.effect_height,
            self.request.screen_height,
            self.request.accel,
            self.accel_cache,
        );
        (travel * self.post_accel_scale, before_peak)
    }

    #[inline(always)]
    #[must_use]
    pub fn adjusted(&self, raw_travel: f32) -> f32 {
        if self.accel_is_identity {
            return raw_travel * self.post_accel_scale;
        }
        apply_accel_y_cached(
            raw_travel,
            self.request.effect_height,
            self.request.screen_height,
            self.request.accel,
            self.accel_cache,
        ) * self.post_accel_scale
    }

    #[inline(always)]
    pub(crate) fn adjusted_note(&self, raw_travel: f32, beat: f32, local_col: usize) -> f32 {
        self.adjusted_note_for_row(raw_travel, beat_to_note_row(beat), local_col)
    }

    #[inline(always)]
    pub(crate) fn adjusted_note_for_row(
        &self,
        raw_travel: f32,
        note_row: i32,
        local_col: usize,
    ) -> f32 {
        let adjusted = self.adjusted(raw_travel);
        if raw_travel < 0.0 || self.request.random_speed <= 0.0 {
            return adjusted;
        }
        debug_assert!(local_col < MAX_COLS);
        adjusted
            * random_speed_mult_from_lane_seed(
                self.random_speed_lane_seeds[local_col],
                note_row,
                self.request.random_speed,
            )
    }

    #[inline(always)]
    pub(crate) fn adjusted_hold_anchor(
        &self,
        raw_travel: f32,
        head_raw_travel: f32,
        head_adjusted_travel: f32,
        tail_raw_travel: f32,
        tail_adjusted_travel: f32,
    ) -> f32 {
        if raw_travel.to_bits() == head_raw_travel.to_bits() {
            head_adjusted_travel
        } else if raw_travel.to_bits() == tail_raw_travel.to_bits() {
            tail_adjusted_travel
        } else {
            self.adjusted(raw_travel)
        }
    }

    #[must_use]
    pub fn lane_offset(&self, local_col: usize) -> f32 {
        tipsy_y_extra(
            local_col,
            self.request.arrow_effect_time_s,
            self.request.lane_tipsy,
        ) + move_col_extra(self.request.lane_move_y, local_col)
    }

    #[must_use]
    pub fn lane_y(
        &self,
        local_col: usize,
        receptor_y: f32,
        direction: f32,
        raw_travel: f32,
    ) -> f32 {
        direction.mul_add(self.adjusted(raw_travel), receptor_y) + self.lane_offset(local_col)
    }

    #[must_use]
    pub fn lane_y_for_beat(
        &self,
        local_col: usize,
        beat: f32,
        receptor_y: f32,
        direction: f32,
    ) -> f32 {
        self.lane_y(local_col, receptor_y, direction, self.raw_beat(beat))
    }

    #[must_use]
    pub fn adjusted_from_screen_y(
        &self,
        local_col: usize,
        receptor_y: f32,
        direction: f32,
        screen_y: f32,
    ) -> f32 {
        self.adjusted_from_screen_y_with_lane_offset(
            receptor_y,
            direction,
            screen_y,
            self.lane_offset(local_col),
        )
    }

    pub(crate) fn adjusted_from_screen_y_with_lane_offset(
        &self,
        receptor_y: f32,
        direction: f32,
        screen_y: f32,
        lane_offset: f32,
    ) -> f32 {
        let direction = if direction.abs() <= 0.000_1 {
            if direction < 0.0 { -0.000_1 } else { 0.000_1 }
        } else {
            direction
        };
        (screen_y - receptor_y - lane_offset) / direction
    }

    #[must_use]
    pub fn visible_row_range(&self) -> Option<(i32, i32)> {
        self.visible_row_range_for_distances(
            self.request.draw_distance_after_targets,
            self.request.draw_distance_before_targets,
        )
    }

    pub(crate) fn visible_row_range_with_extra(&self, extra_distance: f32) -> Option<(i32, i32)> {
        let extra_distance = if extra_distance.is_finite() {
            extra_distance.max(0.0)
        } else {
            0.0
        };
        self.visible_row_range_for_distances(
            self.request.draw_distance_after_targets + extra_distance,
            self.request.draw_distance_before_targets + extra_distance,
        )
    }

    fn visible_row_range_for_distances(
        &self,
        draw_distance_after_targets: f32,
        draw_distance_before_targets: f32,
    ) -> Option<(i32, i32)> {
        let stop_at_row_precision = matches!(
            self.raw,
            RawTravel::ConstantOneRate { .. } | RawTravel::Constant { .. }
        );
        let first_row = find_first_displayed_beat_inner(
            self.request.search_beat,
            draw_distance_after_targets,
            self.request.note_count_stats,
            |beat| self.adjusted(self.raw_beat(beat)),
            stop_at_row_precision,
        )
        .map(beat_to_note_row);
        let last_row = find_last_displayed_beat_inner(
            self.request.search_beat,
            draw_distance_before_targets,
            self.displayed_speed_percent,
            self.request.accel.boomerang > f32::EPSILON,
            |beat| self.adjusted_with_peak(self.raw_beat(beat)),
            stop_at_row_precision,
        )
        .map(beat_to_note_row);
        first_row
            .zip(last_row)
            .map(|(first, last)| (first, last.max(first)))
    }

    #[must_use]
    pub const fn arrow_effect_time_s(&self) -> f32 {
        self.request.arrow_effect_time_s
    }
}

const RANDOM_SPEED_LCG_MULTIPLIER: u32 = 1_664_525;
const RANDOM_SPEED_LCG_INCREMENT: u32 = 1_013_904_223;
const RANDOM_SPEED_LCG_MULTIPLIER_3: u32 = RANDOM_SPEED_LCG_MULTIPLIER
    .wrapping_mul(RANDOM_SPEED_LCG_MULTIPLIER)
    .wrapping_mul(RANDOM_SPEED_LCG_MULTIPLIER);
const RANDOM_SPEED_LCG_INCREMENT_3: u32 = RANDOM_SPEED_LCG_INCREMENT.wrapping_mul(
    RANDOM_SPEED_LCG_MULTIPLIER
        .wrapping_mul(RANDOM_SPEED_LCG_MULTIPLIER)
        .wrapping_add(RANDOM_SPEED_LCG_MULTIPLIER)
        .wrapping_add(1),
);

// Three LCG rounds are one affine transform modulo 2^32. Keeping the composed
// coefficients removes two dependent multiply-add pairs from every random-speed note.
#[inline(always)]
const fn random_speed_seed(seed: u32) -> u32 {
    seed.wrapping_mul(RANDOM_SPEED_LCG_MULTIPLIER_3)
        .wrapping_add(RANDOM_SPEED_LCG_INCREMENT_3)
}

#[inline(always)]
fn random_speed_lane_seed(stage_seed: u32, local_col: usize) -> u32 {
    random_speed_seed(stage_seed.wrapping_add((local_col as u32).wrapping_mul(100)))
}

// The affine transform distributes over the row contribution, so each lane's
// stage/column component can be prepared once when the frame travel is built.
#[inline(always)]
fn random_speed_seed_from_lane_seed(lane_seed: u32, note_row: i32) -> u32 {
    lane_seed.wrapping_add(
        (note_row as u32)
            .wrapping_shl(8)
            .wrapping_mul(RANDOM_SPEED_LCG_MULTIPLIER_3),
    )
}

#[inline(always)]
fn random_speed_mult_from_lane_seed(lane_seed: u32, note_row: i32, amount: f32) -> f32 {
    let seed = random_speed_seed_from_lane_seed(lane_seed, note_row);
    (seed as f32 / 4_294_967_296.0).mul_add(amount, 1.0)
}

pub(crate) fn note_itg_row(note: &Note) -> i32 {
    beat_to_note_row(note.beat)
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LaneWindowCursor {
    pub start: usize,
    pub end: usize,
}

pub(crate) fn lane_window_bounds_by_note_row_from_cursor<I: Copy + Into<usize>>(
    note_itg_rows: &[i32],
    indices: &[I],
    range: Option<(i32, i32)>,
    cursor: &mut LaneWindowCursor,
) -> Option<(usize, usize)> {
    let (low, high) = range?;
    if high < 0 {
        *cursor = LaneWindowCursor::default();
        return Some((0, 0));
    }
    let low = low.max(0);
    cursor.start =
        deadsync_gameplay::partition_point_from_hint(indices, cursor.start, |&note_index| {
            note_itg_rows[note_index.into()] < low
        });
    cursor.end = deadsync_gameplay::partition_point_from_hint(indices, cursor.end, |&note_index| {
        note_itg_rows[note_index.into()] <= high
    });
    Some((cursor.start, cursor.end))
}
pub(crate) fn lane_hold_window_bounds_by_note_row_from_cursor<I: Copy + Into<usize>>(
    notes: &[Note],
    note_itg_rows: &[i32],
    indices: &[I],
    range: Option<(i32, i32)>,
    cursor: &mut LaneWindowCursor,
) -> Option<(usize, usize)> {
    let (low, _) = range?;
    let (mut start, end) =
        lane_window_bounds_by_note_row_from_cursor(note_itg_rows, indices, range, cursor)?;
    let low = low.max(0);
    while start > 0 {
        let prev_note_index = indices[start - 1].into();
        let prev_end_row = notes[prev_note_index].hold.as_ref().map_or_else(
            || note_itg_row(&notes[prev_note_index]),
            |hold| beat_to_note_row(hold.end_beat),
        );
        if prev_end_row < low {
            break;
        }
        start -= 1;
    }
    Some((start, end))
}

#[inline(always)]
pub(crate) fn for_each_lane_index<F: FnMut(usize)>(
    indices: &[ChartNoteIndex],
    bounds: (usize, usize),
    mut f: F,
) {
    let start = bounds.0.min(indices.len());
    let end = bounds.1.min(indices.len()).max(start);
    for &index in &indices[start..end] {
        f(index.get());
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

fn note_count_cutoff_beat(stats: &[NoteCountStat], high: NoteCountStat) -> Option<f32> {
    if high.notes_upper <= MAX_NOTES_AFTER {
        return None;
    }
    let min_notes_lower = high.notes_upper - MAX_NOTES_AFTER;
    let index = stats.partition_point(|stat| stat.notes_lower < min_notes_lower);
    match index {
        0 => None,
        index if index == stats.len() => Some(f32::INFINITY),
        index => Some(stats[index].beat),
    }
}

#[cfg(test)]
pub(crate) fn find_first_displayed_beat<F: FnMut(f32) -> f32>(
    current_beat: f32,
    draw_distance: f32,
    stats: &[NoteCountStat],
    y_for_beat: F,
) -> Option<f32> {
    find_first_displayed_beat_inner(current_beat, draw_distance, stats, y_for_beat, false)
}

fn find_first_displayed_beat_inner<F: FnMut(f32) -> f32>(
    current_beat: f32,
    draw_distance: f32,
    stats: &[NoteCountStat],
    mut y_for_beat: F,
    stop_at_row_precision: bool,
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance.is_finite() {
        return None;
    }
    let mut high = current_beat.max(0.0);
    let note_count_cutoff = (!stats.is_empty())
        .then(|| note_count_at(stats, current_beat))
        .and_then(|count| note_count_cutoff_beat(stats, count));
    let mut low = if stats.is_empty() { high - 4.0 } else { 0.0 };
    let mut first = low;
    for _ in 0..24 {
        let mid = f32::midpoint(low, high);
        if y_for_beat(mid) < -draw_distance || note_count_cutoff.is_some_and(|cutoff| mid < cutoff)
        {
            first = mid;
            low = mid;
        } else {
            high = mid;
        }
        if stop_at_row_precision && beat_to_note_row(low) == beat_to_note_row(high) {
            break;
        }
    }
    Some(first)
}

#[cfg(test)]
pub(crate) fn find_last_displayed_beat<F: FnMut(f32) -> (f32, bool)>(
    current_beat: f32,
    draw_distance: f32,
    displayed_speed_percent: f32,
    boomerang: bool,
    y_for_beat: F,
) -> Option<f32> {
    find_last_displayed_beat_inner(
        current_beat,
        draw_distance,
        displayed_speed_percent,
        boomerang,
        y_for_beat,
        false,
    )
}

fn find_last_displayed_beat_inner<F: FnMut(f32) -> (f32, bool)>(
    current_beat: f32,
    draw_distance: f32,
    displayed_speed_percent: f32,
    boomerang: bool,
    mut y_for_beat: F,
    stop_at_row_precision: bool,
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
        if stop_at_row_precision {
            let rounding_guard = f32::EPSILON * last.abs() * 32.0;
            let remaining = search_distance.mul_add(2.0, rounding_guard);
            let cap = if displayed_speed_percent < 0.75 {
                current_beat + 16.0
            } else {
                f32::INFINITY
            };
            let low_row = beat_to_note_row((last - remaining).min(cap));
            let high_row = beat_to_note_row((last + remaining).min(cap));
            if low_row == high_row {
                break;
            }
        }
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
        LaneWindowCursor, MineLayerRequest, NoteLayerRequest, ScrollTravelRequest,
        compose_flat_mine_layers, compose_flat_note_layer, hold_overlaps_visible_window,
        lane_hold_window_bounds_by_note_row_from_cursor,
        lane_window_bounds_by_note_row_from_cursor, random_speed_lane_seed, random_speed_seed,
        random_speed_seed_from_lane_seed, scroll_travel, song_time_ns_delta_seconds,
    };
    use crate::{AccelYParams, ModelMeshCache, ModelMeshCacheStats, move_col_extra, tipsy_y_extra};
    use deadlib_present::actors::{FlatDraw, SpriteSource};
    use deadlib_render_core::BlendMode;
    use deadsync_core::input::MAX_COLS;
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

    fn request(
        timing: &TimingData,
        scroll_speed: ScrollSpeedSetting,
        visible_beat: f32,
    ) -> ScrollTravelRequest<'_> {
        ScrollTravelRequest {
            timing,
            accel: AccelYParams::default(),
            random_speed: 0.0,
            stage_seed: 0,
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
    fn flat_note_layer_emits_diffuse_then_glow() {
        let mut slot = GlowSlot::sprite();
        slot.def.mirror_h = true;
        slot.def.mirror_v = true;
        let mut request = layer_request(&slot);
        request.draw.blend_add = true;
        request.draw.fade = [0.1, 0.2, 0.3, 0.4];
        request.blend = BlendMode::Add;
        let source = |_: &GlowSlot| SpriteSource::static_texture("flat-layer");
        let mut draws = Vec::new();

        compose_flat_note_layer(&mut draws, &mut ModelMeshCache::default(), request, &source);

        let [FlatDraw::Sprite(diffuse), FlatDraw::Sprite(glow)] = draws.as_slice() else {
            panic!("sprite note layer should emit diffuse and glow sprites");
        };
        assert_eq!(diffuse.center, [30.0, 40.0]);
        assert_eq!(diffuse.world_z, 9.0);
        assert_eq!(diffuse.size, [48.0, 56.0]);
        assert_eq!(diffuse.source.texture_key(), Some("flat-layer"));
        assert_eq!(diffuse.tint, [0.2, 0.3, 0.4, 0.5]);
        assert_eq!(diffuse.glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(diffuse.uv_rect, [0.1, 0.2, 0.7, 0.8]);
        assert!(diffuse.flip_x);
        assert!(diffuse.flip_y);
        assert_eq!(diffuse.fade, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(diffuse.blend, BlendMode::Add);
        assert_eq!(diffuse.rot_y_deg, 12.0);
        assert_eq!(diffuse.rot_z_deg, 34.0);
        assert_eq!(diffuse.z, 140);
        assert_eq!(glow.tint, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(glow.glow, [1.0, 1.0, 1.0, 0.75]);
        assert_eq!(glow.blend, BlendMode::Add);
    }

    #[test]
    fn note_layer_model_reuses_cached_geometry_for_diffuse_and_glow() {
        let slot = GlowSlot::model();
        let mut draws = Vec::new();
        let mut cache = ModelMeshCache::with_capacity(1);
        cache.begin_hit_stats(true);

        for _ in 0..2 {
            compose_flat_note_layer(&mut draws, &mut cache, layer_request(&slot), &|_| {
                panic!("model-backed note layer must not resolve a sprite source")
            });
        }

        assert_eq!(draws.len(), 4);
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 3,
                misses: 1,
                saturated_misses: 0,
            }
        );
        let FlatDraw::TexturedMesh(mesh) = &draws[0] else {
            panic!("model-backed diffuse pass should emit a textured mesh");
        };
        assert_eq!(mesh.offset, [10.0, 20.0]);
        assert_eq!(mesh.tint, [0.2, 0.3, 0.4, 0.5]);
        assert_eq!(mesh.glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(mesh.blend, BlendMode::Alpha);
        assert_eq!(mesh.world_z, 9.0);
        assert_eq!(mesh.z, 140);

        let FlatDraw::TexturedMesh(glow) = &draws[1] else {
            panic!("model-backed glow pass should follow the diffuse pass");
        };
        assert_eq!(glow.tint, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(glow.glow, [1.0, 1.0, 1.0, 0.75]);
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
        let mut draws = Vec::new();
        let mut cache = ModelMeshCache::default();
        let size_calls = Cell::new(0);

        compose_flat_mine_layers(
            &mut draws,
            &mut cache,
            mine_request(Some(&fill), Some(&gradient), Some(&frame)),
            &|slot| {
                size_calls.set(size_calls.get() + 1);
                assert_eq!(slot.texture.as_ref(), "mine-frame");
                [70.0, 72.0]
            },
            &|slot| SpriteSource::Texture(slot.texture.clone()),
        );

        assert_eq!(draws.len(), 4);
        assert_eq!(size_calls.get(), 1);
        assert_eq!(cache.stats(), ModelMeshCacheStats::default());
        let keys = draws
            .iter()
            .map(|draw| match draw {
                FlatDraw::Sprite(sprite) => sprite.source.texture_key().unwrap_or_default(),
                draw => panic!("mine sprite path emitted {draw:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            ["mine-gradient", "mine-gradient", "mine-frame", "mine-frame"]
        );

        let FlatDraw::Sprite(sprite) = &draws[0] else {
            unreachable!();
        };
        let deadlib_present::actors::FlatSprite {
            size,
            tint,
            glow,
            z,
            world_z,
            rot_y_deg,
            rot_z_deg,
            ..
        } = sprite;
        assert_eq!(*size, [18.0, 20.0]);
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.8]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*z, 138);
        assert_eq!(*world_z, 9.0);
        assert_eq!(*rot_y_deg, 0.0);
        assert_eq!(*rot_z_deg, 0.0);

        let FlatDraw::Sprite(sprite) = &draws[2] else {
            unreachable!();
        };
        let deadlib_present::actors::FlatSprite {
            size,
            tint,
            glow,
            z,
            world_z,
            rot_y_deg,
            rot_z_deg,
            ..
        } = sprite;
        assert_eq!(*size, [70.0, 72.0]);
        assert_eq!(*tint, [1.0, 1.0, 1.0, 0.8]);
        assert_eq!(*glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(*z, 140);
        assert_eq!(*world_z, 9.0);
        assert_eq!(*rot_y_deg, 12.0);
        assert_eq!(*rot_z_deg, -2.0);
        let FlatDraw::Sprite(sprite) = &draws[3] else {
            unreachable!();
        };
        assert_eq!(sprite.tint, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(sprite.glow, [1.0, 1.0, 1.0, 0.6]);
    }

    #[test]
    fn mine_layers_model_fill_reuses_geometry_for_glow() {
        let mut fill = named_slot(GlowSlot::model(), "mine-model-fill");
        fill.def.rotation_deg = 10;
        fill.draw.rot[2] = 3.0;
        let mut draws = Vec::new();
        let mut cache = ModelMeshCache::with_capacity(1);
        cache.begin_hit_stats(true);
        let source_calls = Cell::new(0);

        compose_flat_mine_layers(
            &mut draws,
            &mut cache,
            mine_request(Some(&fill), None, None),
            &|_| [64.0, 66.0],
            &|_| {
                source_calls.set(source_calls.get() + 1);
                SpriteSource::static_texture("unused")
            },
        );

        assert_eq!(draws.len(), 2);
        assert_eq!(source_calls.get(), 0);
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 2,
                misses: 1,
                saturated_misses: 0,
            }
        );
        let FlatDraw::TexturedMesh(mesh) = &draws[0] else {
            panic!("model-backed mine fill should emit a textured mesh");
        };
        assert_eq!(mesh.tint[..3], [1.0, 1.0, 1.0]);
        assert_near(mesh.tint[3], 0.72);
        assert_eq!(mesh.glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(mesh.z, 139);
        assert_eq!(mesh.world_z, 9.0);
        let FlatDraw::TexturedMesh(mesh) = &draws[1] else {
            unreachable!();
        };
        assert_eq!(mesh.tint, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(mesh.glow, [1.0, 1.0, 1.0, 0.6]);
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
    fn collapsed_and_lane_cached_random_speed_keep_stable_seeds() {
        for (input, expected) in [
            (0, 3_519_870_697),
            (1, 2_165_703_038),
            (0x1234_5678, 635_173_569),
            (u32::MAX, 579_071_060),
        ] {
            assert_eq!(random_speed_seed(input), expected);
        }

        for stage_seed in [0, 1, 0x1234_5678, u32::MAX] {
            for note_row in [i32::MIN, -192, -1, 0, 1, 192, i32::MAX] {
                for local_col in 0..MAX_COLS {
                    let input = stage_seed
                        .wrapping_add((note_row as u32).wrapping_shl(8))
                        .wrapping_add((local_col as u32).wrapping_mul(100));
                    let collapsed = random_speed_seed(input);
                    assert_eq!(
                        random_speed_seed_from_lane_seed(
                            random_speed_lane_seed(stage_seed, local_col),
                            note_row,
                        ),
                        collapsed,
                    );
                }
            }
        }
    }

    #[test]
    fn random_speed_matches_itg_per_note_scroll_scaling() {
        let timing = timing();
        let mut travel_request = request(&timing, ScrollSpeedSetting::XMod(1.0), 4.0);
        travel_request.random_speed = 0.5;
        travel_request.stage_seed = 0x1234_5678;
        let travel = scroll_travel(travel_request);
        let raw = travel.raw_beat(5.0);
        let base = travel.adjusted(raw);
        let first = travel.adjusted_note(raw, 5.0, 0);
        let second = travel.adjusted_note(raw, 5.0, 1);

        assert_eq!(
            travel
                .adjusted_note_for_row(raw, beat_to_note_row(5.0), 0)
                .to_bits(),
            first.to_bits(),
        );

        assert!((base..=base * 1.5).contains(&first));
        assert!((base..=base * 1.5).contains(&second));
        assert_ne!(first.to_bits(), second.to_bits());
        assert_eq!(
            travel.adjusted_note(-raw, 3.0, 0).to_bits(),
            travel.adjusted(-raw).to_bits()
        );
    }

    #[test]
    fn constant_scroll_rate_variants_are_bit_exact() {
        let timing = timing();
        for rate in [0.5_f32, 0.75, 1.0, 1.25, 2.0] {
            let mut request = request(&timing, ScrollSpeedSetting::CMod(725.0), 4.0);
            request.music_rate = rate;
            let travel = scroll_travel(request);
            let note_beat = 7.375_f32;
            let note_time_ns = timing.get_time_for_beat_ns(note_beat);
            let expected = (song_time_ns_delta_seconds(note_time_ns, request.current_time_ns)
                / rate)
                * (725.0 / 60.0)
                * ScrollSpeedSetting::ARROW_SPACING;
            assert_eq!(
                travel.raw_beat(note_beat).to_bits(),
                expected.to_bits(),
                "rate {rate}",
            );
        }
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
        let expected = raw * (raw / brake_request.effect_height) * 2.0;
        let pre_scaled = raw * 2.0 * (raw * 2.0 / brake_request.effect_height);
        assert_near(brake.adjusted(raw), expected);
        assert_ne!(brake.adjusted(raw), pre_scaled);

        let mut boomerang_request = request(&timing, ScrollSpeedSetting::XMod(2.0), 0.0);
        boomerang_request.accel.boomerang = 1.0;
        let boomerang = scroll_travel(boomerang_request);
        let raw = boomerang.raw_beat(10.0);
        let (adjusted, before_peak) = boomerang.adjusted_with_peak(raw);
        let expected = 1.5f32.mul_add(raw, -raw * raw / boomerang_request.screen_height);
        assert!(!before_peak);
        assert_near(adjusted, expected * 2.0);
    }

    #[test]
    fn hold_anchor_reuses_only_bit_identical_adjusted_endpoints() {
        let timing = timing();
        let mut travel_request = request(&timing, ScrollSpeedSetting::XMod(2.0), 4.0);
        travel_request.accel = AccelYParams {
            boost: 0.35,
            brake: 0.45,
            wave: 0.8,
            expand: 0.6,
            boomerang: 0.2,
        };
        let travel = scroll_travel(travel_request);
        let head_raw = 96.0;
        let tail_raw = 384.0;
        let head_adjusted = travel.adjusted(head_raw);
        let tail_adjusted = travel.adjusted(tail_raw);
        for anchor_raw in [head_raw, tail_raw, 0.0, 240.0] {
            assert_eq!(
                travel
                    .adjusted_hold_anchor(
                        anchor_raw,
                        head_raw,
                        head_adjusted,
                        tail_raw,
                        tail_adjusted,
                    )
                    .to_bits(),
                travel.adjusted(anchor_raw).to_bits(),
            );
        }

        let identity = scroll_travel(request(&timing, ScrollSpeedSetting::XMod(2.0), 4.0));
        let negative_zero = -0.0_f32;
        let positive_zero = 0.0_f32;
        assert_eq!(
            identity
                .adjusted_hold_anchor(
                    positive_zero,
                    negative_zero,
                    identity.adjusted(negative_zero),
                    tail_raw,
                    identity.adjusted(tail_raw),
                )
                .to_bits(),
            identity.adjusted(positive_zero).to_bits(),
        );
    }

    #[test]
    fn inactive_acceleration_options_select_identity_path() {
        let timing = timing();
        let mut travel_request = request(&timing, ScrollSpeedSetting::XMod(2.0), 4.0);
        travel_request.field_zoom = 0.75;
        travel_request.accel = AccelYParams {
            boost: f32::NAN,
            brake: -1.0,
            wave: f32::EPSILON,
            expand: f32::NEG_INFINITY,
            boomerang: -0.0,
        };
        let travel = scroll_travel(travel_request);
        assert!(travel.accel_is_identity);
        for raw in [-128.0, -0.0, 0.0, 160.0, 640.0] {
            let expected = raw * travel.post_accel_scale;
            assert_eq!(travel.adjusted(raw).to_bits(), expected.to_bits());
            let (actual, actual_before_peak) = travel.adjusted_with_peak(raw);
            assert_eq!(actual.to_bits(), expected.to_bits());
            assert!(actual_before_peak);
        }

        travel_request.accel.wave = f32::EPSILON * 2.0;
        assert!(!scroll_travel(travel_request).accel_is_identity);
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
        let note_itg_rows = notes
            .iter()
            .map(|note| beat_to_note_row(note.beat))
            .collect::<Vec<_>>();

        let note_indices = [0usize, 1, 2];
        let note_bounds = lane_window_bounds_by_note_row_from_cursor(
            &note_itg_rows,
            &note_indices,
            Some(range),
            &mut LaneWindowCursor::default(),
        )
        .expect("finite row range");
        assert_eq!(&note_indices[note_bounds.0..note_bounds.1], &[1]);

        let hold_indices = [0usize];
        let hold_bounds = lane_hold_window_bounds_by_note_row_from_cursor(
            &notes,
            &note_itg_rows,
            &hold_indices,
            Some(range),
            &mut LaneWindowCursor::default(),
        )
        .expect("finite row range");
        assert_eq!(&hold_indices[hold_bounds.0..hold_bounds.1], &[0]);
        assert!(hold_overlaps_visible_window(0, &notes, Some(range)));
        let hold_end = notes[0].hold.as_ref().expect("hold fixture").end_beat;
        assert_near(travel.raw_beat(hold_end), 0.0);
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
