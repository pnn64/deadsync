use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub enum TweenType {
    Linear,
    Accelerate,
    Decelerate,
}

impl TweenType {
    #[must_use]
    pub fn ease(self, progress: f32) -> f32 {
        let t = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Accelerate => t * t,
            Self::Decelerate => (1.0 - t).mul_add(-(1.0 - t), 1.0),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ModelVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub tex_matrix_scale: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct ModelMesh {
    pub vertices: Arc<[ModelVertex]>,
    pub bounds: [f32; 6], // min_x, min_y, min_z, max_x, max_y, max_z
}

impl ModelMesh {
    #[inline(always)]
    #[must_use]
    pub fn size(&self) -> [f32; 2] {
        [
            (self.bounds[3] - self.bounds[0]).max(0.0),
            (self.bounds[4] - self.bounds[1]).max(0.0),
        ]
    }
}

#[inline(always)]
#[must_use]
pub fn model_texture_uv_params(
    uv_rect: [f32; 4],
    src: [i32; 2],
    atlas_tex_dims: Option<(u32, u32)>,
) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let uv_scale = [uv_rect[2] - uv_rect[0], uv_rect[3] - uv_rect[1]];
    let uv_offset = [uv_rect[0], uv_rect[1]];
    let uv_tex_shift = if let Some((tw, th)) = atlas_tex_dims {
        let tw = tw.max(1) as f32;
        let th = th.max(1) as f32;
        let base_u0 = src[0] as f32 / tw;
        let base_v0 = src[1] as f32 / th;
        [uv_offset[0] - base_u0, uv_offset[1] - base_v0]
    } else {
        [0.0, 0.0]
    };
    (uv_scale, uv_offset, uv_tex_shift)
}

/// Computes model UV parameters from a cached normalized atlas origin.
#[inline(always)]
#[must_use]
pub fn model_texture_uv_params_cached(
    uv_rect: [f32; 4],
    atlas_origin: Option<[f32; 2]>,
) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let uv_scale = [uv_rect[2] - uv_rect[0], uv_rect[3] - uv_rect[1]];
    let uv_offset = [uv_rect[0], uv_rect[1]];
    let uv_tex_shift = atlas_origin.map_or([0.0, 0.0], |origin| {
        [uv_offset[0] - origin[0], uv_offset[1] - origin[1]]
    });
    (uv_scale, uv_offset, uv_tex_shift)
}

#[derive(Debug, Clone, Copy)]
pub struct ModelDrawState {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub zoom: [f32; 3],
    pub tint: [f32; 4],
    pub glow: [f32; 4],
    /// Edge fades in left, right, top, bottom order.
    pub fade: [f32; 4],
    pub vert_align: f32,
    pub blend_add: bool,
    pub visible: bool,
}

impl Default for ModelDrawState {
    fn default() -> Self {
        Self {
            pos: [0.0, 0.0, 0.0],
            rot: [0.0, 0.0, 0.0],
            zoom: [1.0, 1.0, 1.0],
            tint: [1.0, 1.0, 1.0, 1.0],
            glow: [1.0, 1.0, 1.0, 0.0],
            fade: [0.0; 4],
            vert_align: 0.5,
            blend_add: false,
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ModelTweenSegment {
    pub start: f32,
    pub duration: f32,
    pub tween: TweenType,
    pub from: ModelDrawState,
    pub to: ModelDrawState,
}

/// Monotonic cursor for a compiled model tween timeline.
///
/// Backward time movement resets it to the first segment, preserving seek
/// correctness without allocating or rebuilding timeline state.
#[derive(Debug, Clone, Copy, Default)]
pub struct ModelTweenCursor {
    next_segment: usize,
    last_time: f32,
    initialized: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ModelAutoRotKey {
    pub frame: f32,
    pub z_deg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelEffectClock {
    Time,
    Beat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelEffectMode {
    None,
    DiffuseRamp,
    DiffuseShift,
    GlowShift,
    Pulse,
    Bob,
    Bounce,
    Wag,
    Spin,
}

#[derive(Clone, Copy, Debug)]
pub struct ModelEffectState {
    pub clock: ModelEffectClock,
    pub mode: ModelEffectMode,
    pub color1: [f32; 4],
    pub color2: [f32; 4],
    pub period: f32,
    pub offset: f32,
    // ITGmania Actor::SetEffectTiming():
    // ramp_to_half, hold_at_half, ramp_to_full, hold_at_full, hold_at_zero.
    pub timing: [f32; 5],
    pub magnitude: [f32; 3],
}

impl Default for ModelEffectState {
    fn default() -> Self {
        Self {
            clock: ModelEffectClock::Time,
            mode: ModelEffectMode::None,
            color1: [1.0, 1.0, 1.0, 1.0],
            color2: [1.0, 1.0, 1.0, 1.0],
            period: 1.0,
            offset: 0.0,
            timing: [0.5, 0.0, 0.5, 0.0, 0.0],
            magnitude: [1.0, 1.0, 1.0],
        }
    }
}

#[inline(always)]
#[must_use]
pub const fn model_effect_clock_units(effect: ModelEffectState, time: f32, beat: f32) -> f32 {
    match effect.clock {
        ModelEffectClock::Time => time,
        ModelEffectClock::Beat => beat,
    }
}

#[inline(always)]
#[must_use]
pub fn model_effect_mix(effect: ModelEffectState, time: f32, beat: f32) -> Option<f32> {
    model_effect_mix_impl(effect, time, beat, true)
}

#[inline(always)]
fn model_effect_mix_impl(
    effect: ModelEffectState,
    time: f32,
    beat: f32,
    canonical_fast_path: bool,
) -> Option<f32> {
    if !matches!(
        effect.mode,
        ModelEffectMode::DiffuseRamp
            | ModelEffectMode::DiffuseShift
            | ModelEffectMode::GlowShift
            | ModelEffectMode::Pulse
            | ModelEffectMode::Bob
            | ModelEffectMode::Bounce
            | ModelEffectMode::Wag
    ) {
        return None;
    }
    // ITG's default curve consists of equal half-cycle ramps with no holds.
    const CANONICAL_EFFECT_TIMING: [f32; 5] = [0.5, 0.0, 0.5, 0.0, 0.0];
    let units = model_effect_clock_units(effect, time, beat) + effect.offset;
    if canonical_fast_path && effect.timing == CANONICAL_EFFECT_TIMING {
        let through = units.rem_euclid(1.0);
        return Some(if through.is_finite() { through } else { 0.0 });
    }
    let t = effect.timing;
    let total = (t[0] + t[1] + t[2] + t[3] + t[4]).max(1e-6);
    let x = units.rem_euclid(total);

    // ITGmania Actor::PreDraw() fPercentThroughEffect semantics.
    let rup_plus_ath = t[0] + t[1];
    let rupath_plus_rdown = rup_plus_ath + t[2];
    let rupathrdown_plus_atf = rupath_plus_rdown + t[3];
    let p = if x < t[0] {
        if t[0] > f32::EPSILON {
            x / t[0] * 0.5
        } else {
            0.5
        }
    } else if x < rup_plus_ath {
        0.5
    } else if x < rupath_plus_rdown {
        if t[2] > f32::EPSILON {
            ((x - rup_plus_ath) / t[2]).mul_add(0.5, 0.5)
        } else {
            1.0
        }
    } else if x < rupathrdown_plus_atf {
        1.0
    } else {
        0.0
    };
    Some(p.clamp(0.0, 1.0))
}

#[cfg(any(test, feature = "bench-support"))]
fn model_effect_mix_legacy(effect: ModelEffectState, time: f32, beat: f32) -> Option<f32> {
    model_effect_mix_impl(effect, time, beat, false)
}

#[inline(always)]
#[must_use]
pub fn glowshift_mix(through: f32) -> f32 {
    ((through + 0.25) * 2.0 * std::f32::consts::PI)
        .sin()
        .mul_add(0.5, 0.5)
        .clamp(0.0, 1.0)
}

#[inline(always)]
#[must_use]
pub fn model_auto_rot_z_at(total_frames: f32, keys: &[ModelAutoRotKey], time: f32) -> Option<f32> {
    model_auto_rot_z_at_impl(total_frames, keys, time, true)
}

#[inline(always)]
fn model_auto_rot_z_at_impl(
    total_frames: f32,
    keys: &[ModelAutoRotKey],
    time: f32,
    small_key_fast_paths: bool,
) -> Option<f32> {
    if total_frames <= f32::EPSILON {
        return None;
    }
    let first = *keys.first()?;
    if small_key_fast_paths && keys.len() == 1 {
        return Some(first.z_deg);
    }
    let frame = (time * 30.0).rem_euclid(total_frames);
    if !frame.is_finite() {
        return Some(first.z_deg);
    }
    if frame <= first.frame {
        return Some(first.z_deg);
    }
    if small_key_fast_paths && keys.len() == 2 && first.frame <= keys[1].frame {
        let next = keys[1];
        if frame > next.frame {
            return Some(next.z_deg);
        }
        let span = (next.frame - first.frame).max(1e-6);
        let t = ((frame - first.frame) / span).clamp(0.0, 1.0);
        return Some((next.z_deg - first.z_deg).mul_add(t, first.z_deg));
    }
    let next_idx = keys.partition_point(|key| key.frame < frame);
    if next_idx >= keys.len() {
        return Some(keys[keys.len() - 1].z_deg);
    }
    let prev = keys[next_idx - 1];
    let next = keys[next_idx];
    let span = (next.frame - prev.frame).max(1e-6);
    let t = ((frame - prev.frame) / span).clamp(0.0, 1.0);
    Some((next.z_deg - prev.z_deg).mul_add(t, prev.z_deg))
}

#[cfg(any(test, feature = "bench-support"))]
fn model_auto_rot_z_at_legacy(
    total_frames: f32,
    keys: &[ModelAutoRotKey],
    time: f32,
) -> Option<f32> {
    model_auto_rot_z_at_impl(total_frames, keys, time, false)
}

#[must_use]
pub fn model_draw_at(
    base_draw: ModelDrawState,
    timeline: &[ModelTweenSegment],
    effect: ModelEffectState,
    auto_rot_total_frames: f32,
    auto_rot_z_keys: &[ModelAutoRotKey],
    time: f32,
    beat: f32,
) -> ModelDrawState {
    model_draw_at_impl(
        base_draw,
        timeline,
        effect,
        auto_rot_total_frames,
        auto_rot_z_keys,
        time,
        beat,
        true,
    )
}

fn model_draw_at_impl(
    base_draw: ModelDrawState,
    timeline: &[ModelTweenSegment],
    effect: ModelEffectState,
    auto_rot_total_frames: f32,
    auto_rot_z_keys: &[ModelAutoRotKey],
    time: f32,
    beat: f32,
    static_fast_path: bool,
) -> ModelDrawState {
    #[inline(always)]
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        (b - a).mul_add(t, a)
    }

    if static_fast_path
        && timeline.is_empty()
        && (auto_rot_total_frames <= f32::EPSILON || auto_rot_z_keys.is_empty())
        && matches!(
            effect.mode,
            ModelEffectMode::None
                | ModelEffectMode::GlowShift
                | ModelEffectMode::Bob
                | ModelEffectMode::Bounce
                | ModelEffectMode::Wag
        )
    {
        return sanitize_model_draw(base_draw);
    }

    let mut out = base_draw;
    let local = time.max(0.0);

    for seg in timeline {
        let start = seg.start.max(0.0);
        let duration = seg.duration.max(0.0);
        if local < start {
            break;
        }
        if duration <= f32::EPSILON {
            out = seg.to;
            continue;
        }
        let elapsed = local - start;
        if elapsed >= duration {
            out = seg.to;
            continue;
        }
        let p = seg.tween.ease(elapsed / duration);
        let mut s = seg.from;
        for i in 0..3 {
            s.pos[i] = lerp(seg.from.pos[i], seg.to.pos[i], p);
            s.rot[i] = lerp(seg.from.rot[i], seg.to.rot[i], p);
            s.zoom[i] = lerp(seg.from.zoom[i], seg.to.zoom[i], p);
        }
        for i in 0..4 {
            s.tint[i] = lerp(seg.from.tint[i], seg.to.tint[i], p);
            s.glow[i] = lerp(seg.from.glow[i], seg.to.glow[i], p);
            s.fade[i] = lerp(seg.from.fade[i], seg.to.fade[i], p);
        }
        s.vert_align = lerp(seg.from.vert_align, seg.to.vert_align, p);
        s.blend_add = if p >= 1.0 {
            seg.to.blend_add
        } else {
            seg.from.blend_add
        };
        s.visible = if p >= 1.0 {
            seg.to.visible
        } else {
            seg.from.visible
        };
        out = s;
        break;
    }

    if let Some(rot_z) = model_auto_rot_z_at(auto_rot_total_frames, auto_rot_z_keys, time) {
        out.rot[2] = (out.rot[2] + rot_z).rem_euclid(360.0);
    }

    if matches!(effect.mode, ModelEffectMode::Spin) {
        let clock = model_effect_clock_units(effect, time, beat);
        out.rot[0] = effect.magnitude[0]
            .mul_add(clock, out.rot[0])
            .rem_euclid(360.0);
        out.rot[1] = effect.magnitude[1]
            .mul_add(clock, out.rot[1])
            .rem_euclid(360.0);
        out.rot[2] = effect.magnitude[2]
            .mul_add(clock, out.rot[2])
            .rem_euclid(360.0);
    }
    if let Some(percent) = model_effect_mix(effect, time, beat) {
        match effect.mode {
            ModelEffectMode::DiffuseRamp => {
                let mut c = [0.0; 4];
                for (i, out) in c.iter_mut().enumerate() {
                    *out = lerp(effect.color2[i], effect.color1[i], percent).clamp(0.0, 1.0);
                }
                out.tint[0] *= c[0];
                out.tint[1] *= c[1];
                out.tint[2] *= c[2];
                out.tint[3] *= c[3];
            }
            ModelEffectMode::DiffuseShift => {
                let between = glowshift_mix(percent);
                let mut c = [0.0; 4];
                for (i, out) in c.iter_mut().enumerate() {
                    *out = lerp(effect.color2[i], effect.color1[i], between).clamp(0.0, 1.0);
                }
                out.tint[0] *= c[0];
                out.tint[1] *= c[1];
                out.tint[2] *= c[2];
                out.tint[3] *= c[3];
            }
            ModelEffectMode::Pulse => {
                let offset = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom = lerp(effect.magnitude[0], effect.magnitude[1], offset).max(0.0);
                let sx = lerp(effect.color2[0], effect.color1[0], offset).max(0.0);
                let sy = lerp(effect.color2[1], effect.color1[1], offset).max(0.0);
                let sz = lerp(effect.color2[2], effect.color1[2], offset).max(0.0);
                out.zoom[0] *= zoom * sx;
                out.zoom[1] *= zoom * sy;
                out.zoom[2] *= zoom * sz;
            }
            // ITG applies glowshift to the separate glow channel.
            ModelEffectMode::GlowShift => {}
            ModelEffectMode::Bob => {}
            ModelEffectMode::Bounce => {}
            ModelEffectMode::Wag => {}
            ModelEffectMode::Spin => {}
            ModelEffectMode::None => {}
        }
    }

    sanitize_model_draw(out)
}

#[cfg(any(test, feature = "bench-support"))]
fn model_draw_at_legacy(
    base_draw: ModelDrawState,
    timeline: &[ModelTweenSegment],
    effect: ModelEffectState,
    auto_rot_total_frames: f32,
    auto_rot_z_keys: &[ModelAutoRotKey],
    time: f32,
    beat: f32,
) -> ModelDrawState {
    model_draw_at_impl(
        base_draw,
        timeline,
        effect,
        auto_rot_total_frames,
        auto_rot_z_keys,
        time,
        beat,
        false,
    )
}

/// Evaluates a compiled model tween timeline without revisiting segments that
/// completed at an earlier monotonic timestamp.
pub fn model_draw_at_cursor(
    base_draw: ModelDrawState,
    timeline: &[ModelTweenSegment],
    effect: ModelEffectState,
    auto_rot_total_frames: f32,
    auto_rot_z_keys: &[ModelAutoRotKey],
    clock: [f32; 2],
    cursor: &mut ModelTweenCursor,
) -> ModelDrawState {
    let [time, beat] = clock;
    let local = time.max(0.0);
    if !cursor.initialized || local < cursor.last_time || cursor.next_segment > timeline.len() {
        cursor.next_segment = 0;
    }
    while let Some(seg) = timeline.get(cursor.next_segment) {
        let start = seg.start.max(0.0);
        let duration = seg.duration.max(0.0);
        if local < start || (duration > f32::EPSILON && local - start < duration) {
            break;
        }
        cursor.next_segment += 1;
    }
    cursor.last_time = local;
    cursor.initialized = true;
    let base_draw = cursor
        .next_segment
        .checked_sub(1)
        .map_or(base_draw, |index| timeline[index].to);
    model_draw_at(
        base_draw,
        &timeline[cursor.next_segment..],
        effect,
        auto_rot_total_frames,
        auto_rot_z_keys,
        time,
        beat,
    )
}

#[inline(always)]
#[must_use]
pub fn model_glow_with_draw(
    draw: ModelDrawState,
    effect: ModelEffectState,
    time: f32,
    beat: f32,
    diffuse_alpha: f32,
) -> Option<[f32; 4]> {
    model_glow_with_draw_impl(draw, effect, time, beat, diffuse_alpha, true)
}

#[inline(always)]
fn model_glow_with_draw_impl(
    draw: ModelDrawState,
    effect: ModelEffectState,
    time: f32,
    beat: f32,
    diffuse_alpha: f32,
    transparent_fast_path: bool,
) -> Option<[f32; 4]> {
    let mut glow = draw.glow;
    let glow_shifts = matches!(effect.mode, ModelEffectMode::GlowShift);
    if transparent_fast_path
        && !glow_shifts
        && glow[3].partial_cmp(&f32::EPSILON) != Some(std::cmp::Ordering::Greater)
    {
        return None;
    }
    if glow_shifts {
        let through = model_effect_mix(effect, time, beat)?;
        let mix = glowshift_mix(through);
        for (i, out) in glow.iter_mut().enumerate() {
            *out = (effect.color1[i] - effect.color2[i]).mul_add(mix, effect.color2[i]);
        }
        glow[3] *= diffuse_alpha;
    }
    glow[0] = glow[0].clamp(0.0, 1.0);
    glow[1] = glow[1].clamp(0.0, 1.0);
    glow[2] = glow[2].clamp(0.0, 1.0);
    glow[3] = glow[3].clamp(0.0, 1.0);
    (glow[3] > f32::EPSILON).then_some(glow)
}

#[cfg(any(test, feature = "bench-support"))]
fn model_glow_with_draw_legacy(
    draw: ModelDrawState,
    effect: ModelEffectState,
    time: f32,
    beat: f32,
    diffuse_alpha: f32,
) -> Option<[f32; 4]> {
    model_glow_with_draw_impl(draw, effect, time, beat, diffuse_alpha, false)
}

#[inline(always)]
#[must_use]
pub fn model_glow_at(
    base_draw: ModelDrawState,
    timeline: &[ModelTweenSegment],
    effect: ModelEffectState,
    auto_rot_total_frames: f32,
    auto_rot_z_keys: &[ModelAutoRotKey],
    time: f32,
    beat: f32,
    diffuse_alpha: f32,
) -> Option<[f32; 4]> {
    model_glow_with_draw(
        model_draw_at(
            base_draw,
            timeline,
            effect,
            auto_rot_total_frames,
            auto_rot_z_keys,
            time,
            beat,
        ),
        effect,
        time,
        beat,
        diffuse_alpha,
    )
}

fn sanitize_model_draw(mut out: ModelDrawState) -> ModelDrawState {
    out.zoom[0] = out.zoom[0].max(0.0);
    out.zoom[1] = out.zoom[1].max(0.0);
    out.zoom[2] = out.zoom[2].max(0.0);
    out.tint[0] = out.tint[0].clamp(0.0, 1.0);
    out.tint[1] = out.tint[1].clamp(0.0, 1.0);
    out.tint[2] = out.tint[2].clamp(0.0, 1.0);
    out.tint[3] = out.tint[3].clamp(0.0, 1.0);
    out.glow[0] = out.glow[0].clamp(0.0, 1.0);
    out.glow[1] = out.glow[1].clamp(0.0, 1.0);
    out.glow[2] = out.glow[2].clamp(0.0, 1.0);
    out.glow[3] = out.glow[3].clamp(0.0, 1.0);
    for fade in &mut out.fade {
        *fade = fade.clamp(0.0, 1.0);
    }
    out
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub mod model_draw_bench_support {
    use std::hint::black_box;

    use super::*;

    #[inline(always)]
    fn draw_checksum(draw: ModelDrawState, checksum: u64) -> u64 {
        let draw = black_box(draw);
        checksum
            .wrapping_add(u64::from(draw.pos[0].to_bits()))
            .wrapping_add(u64::from(draw.tint[0].to_bits()))
            .wrapping_add(u64::from(draw.glow[3].to_bits()))
            .rotate_left(7)
    }

    #[inline(always)]
    fn normalized_uv_checksum(values: [f32; 6], checksum: u64) -> u64 {
        values.into_iter().fold(checksum, |checksum, value| {
            checksum
                .wrapping_add((value * 65_536.0).round() as i64 as u64)
                .rotate_left(7)
        })
    }

    #[inline(always)]
    fn optional_float_checksum(value: Option<f32>, checksum: u64) -> u64 {
        checksum
            .wrapping_add(value.map_or(u64::MAX, |value| u64::from(value.to_bits())))
            .rotate_left(7)
    }

    fn small_auto_rot(evaluations: usize, key_count: usize, legacy: bool) -> u64 {
        let keys = black_box([
            ModelAutoRotKey {
                frame: 10.0,
                z_deg: -35.0,
            },
            ModelAutoRotKey {
                frame: 80.0,
                z_deg: 145.0,
            },
        ]);
        let keys = &keys[..key_count];
        let total_frames = black_box(120.0);
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let time = black_box((index & 4_095) as f32 * (1.0 / 1_024.0));
            let rotation = if legacy {
                model_auto_rot_z_at_legacy(total_frames, keys, time)
            } else {
                model_auto_rot_z_at(total_frames, keys, time)
            };
            checksum = optional_float_checksum(black_box(rotation), checksum);
        }
        checksum
    }

    #[must_use]
    pub fn single_key_auto_rot_old(evaluations: usize) -> u64 {
        small_auto_rot(evaluations, 1, true)
    }

    #[must_use]
    pub fn single_key_auto_rot_new(evaluations: usize) -> u64 {
        small_auto_rot(evaluations, 1, false)
    }

    #[must_use]
    pub fn two_key_auto_rot_old(evaluations: usize) -> u64 {
        small_auto_rot(evaluations, 2, true)
    }

    #[must_use]
    pub fn two_key_auto_rot_new(evaluations: usize) -> u64 {
        small_auto_rot(evaluations, 2, false)
    }

    fn transparent_static_glow(evaluations: usize, legacy: bool) -> u64 {
        let base = black_box(ModelDrawState {
            glow: [1.5, -0.5, 0.5, 0.0],
            ..ModelDrawState::default()
        });
        let effect = black_box(ModelEffectState::default());
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let mut draw = base;
            draw.glow[3] = black_box(-((index & 1) as f32));
            let glow = if legacy {
                model_glow_with_draw_legacy(draw, effect, 0.0, 0.0, 1.0)
            } else {
                model_glow_with_draw(draw, effect, 0.0, 0.0, 1.0)
            };
            checksum = black_box(glow).map_or_else(
                || checksum.wrapping_add(index as u64).rotate_left(7),
                |glow| {
                    normalized_uv_checksum([glow[0], glow[1], glow[2], glow[3], 0.0, 0.0], checksum)
                },
            );
        }
        checksum
    }

    #[must_use]
    pub fn transparent_static_glow_old(evaluations: usize) -> u64 {
        transparent_static_glow(evaluations, true)
    }

    #[must_use]
    pub fn transparent_static_glow_new(evaluations: usize) -> u64 {
        transparent_static_glow(evaluations, false)
    }

    fn static_model_draw(evaluations: usize, legacy: bool) -> u64 {
        let base = black_box(ModelDrawState {
            pos: [3.0, -5.0, 7.0],
            zoom: [-2.0, 0.5, 3.0],
            tint: [-1.0, 0.25, 2.0, 0.75],
            glow: [1.5, -0.5, 0.5, 2.0],
            fade: [-1.0, 0.25, 1.5, 0.75],
            ..ModelDrawState::default()
        });
        let effect = black_box(ModelEffectState {
            mode: ModelEffectMode::GlowShift,
            ..ModelEffectState::default()
        });
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let time = black_box((index & 65_535) as f32 * 0.003_906_25);
            let draw = if legacy {
                model_draw_at_legacy(base, &[], effect, 0.0, &[], time, time * 4.0)
            } else {
                model_draw_at(base, &[], effect, 0.0, &[], time, time * 4.0)
            };
            checksum = draw_checksum(draw, checksum);
        }
        checksum
    }

    #[must_use]
    pub fn static_model_draw_old(evaluations: usize) -> u64 {
        static_model_draw(evaluations, true)
    }

    #[must_use]
    pub fn static_model_draw_new(evaluations: usize) -> u64 {
        static_model_draw(evaluations, false)
    }

    fn canonical_effect_mix(evaluations: usize, legacy: bool) -> u64 {
        let effect = black_box(ModelEffectState {
            mode: ModelEffectMode::Pulse,
            offset: 0.125,
            ..ModelEffectState::default()
        });
        let mut checksum = 0_u64;
        for index in 0..evaluations {
            let time = black_box((index & 65_535) as f32 * 0.003_906_25);
            let through = if legacy {
                model_effect_mix_legacy(effect, time, 0.0)
            } else {
                model_effect_mix(effect, time, 0.0)
            }
            .unwrap_or_default();
            checksum = checksum
                .wrapping_add(u64::from(through.to_bits()))
                .rotate_left(7);
        }
        checksum
    }

    #[must_use]
    pub fn canonical_effect_mix_old(evaluations: usize) -> u64 {
        canonical_effect_mix(evaluations, true)
    }

    #[must_use]
    pub fn canonical_effect_mix_new(evaluations: usize) -> u64 {
        canonical_effect_mix(evaluations, false)
    }

    fn cached_model_uv(evaluations: usize, cached: bool) -> u64 {
        let src = black_box([64, 96]);
        let tex_dims = black_box((257, 509));
        let origin = black_box([
            src[0] as f32 * (1.0 / tex_dims.0 as f32),
            src[1] as f32 * (1.0 / tex_dims.1 as f32),
        ]);
        let mut total = [0.0_f32; 6];
        for index in 0..evaluations {
            let shift = black_box((index & 1) as f32 * 0.125);
            let uv_rect = black_box([0.25 + shift, 0.5 - shift, 0.75 + shift, 1.0 - shift]);
            let (scale, offset, tex_shift) = if cached {
                model_texture_uv_params_cached(uv_rect, Some(origin))
            } else {
                model_texture_uv_params(uv_rect, src, Some(tex_dims))
            };
            let values = [
                scale[0],
                scale[1],
                offset[0],
                offset[1],
                tex_shift[0],
                tex_shift[1],
            ];
            for (total, value) in total.iter_mut().zip(values) {
                *total += value;
            }
        }
        normalized_uv_checksum(total.map(|value| value / evaluations as f32), 0)
    }

    #[must_use]
    pub fn cached_model_uv_old(evaluations: usize) -> u64 {
        cached_model_uv(evaluations, false)
    }

    #[must_use]
    pub fn cached_model_uv_new(evaluations: usize) -> u64 {
        cached_model_uv(evaluations, true)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ModelAutoRotKey, ModelDrawState, ModelEffectClock, ModelEffectMode, ModelEffectState,
        ModelTweenCursor, ModelTweenSegment, TweenType, glowshift_mix, model_auto_rot_z_at,
        model_auto_rot_z_at_legacy, model_draw_at, model_draw_at_cursor, model_draw_at_legacy,
        model_effect_clock_units, model_effect_mix, model_effect_mix_legacy, model_glow_with_draw,
        model_glow_with_draw_legacy, model_texture_uv_params, model_texture_uv_params_cached,
    };

    fn assert_draw_bits_eq(actual: ModelDrawState, expected: ModelDrawState) {
        for (actual, expected) in actual.pos.into_iter().zip(expected.pos) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        for (actual, expected) in actual.rot.into_iter().zip(expected.rot) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        for (actual, expected) in actual.zoom.into_iter().zip(expected.zoom) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        for (actual, expected) in actual.tint.into_iter().zip(expected.tint) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        for (actual, expected) in actual.glow.into_iter().zip(expected.glow) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        for (actual, expected) in actual.fade.into_iter().zip(expected.fade) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        assert_eq!(actual.vert_align.to_bits(), expected.vert_align.to_bits());
        assert_eq!(actual.blend_add, expected.blend_add);
        assert_eq!(actual.visible, expected.visible);
    }

    fn assert_uv_params_close(
        actual: ([f32; 2], [f32; 2], [f32; 2]),
        expected: ([f32; 2], [f32; 2], [f32; 2]),
    ) {
        for (actual, expected) in actual
            .0
            .into_iter()
            .chain(actual.1)
            .chain(actual.2)
            .zip(expected.0.into_iter().chain(expected.1).chain(expected.2))
        {
            let tolerance = expected.abs().max(1.0) * f32::EPSILON * 2.0;
            assert!(
                (actual - expected).abs() <= tolerance,
                "optimized {actual:?} and legacy {expected:?} UV parameters differ"
            );
        }
    }

    fn assert_optional_float_bits_eq(actual: Option<f32>, expected: Option<f32>) {
        assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
    }

    fn assert_optional_color_bits_eq(actual: Option<[f32; 4]>, expected: Option<[f32; 4]>) {
        assert_eq!(
            actual.map(|color| color.map(f32::to_bits)),
            expected.map(|color| color.map(f32::to_bits))
        );
    }

    #[test]
    fn model_effect_clock_units_select_time_or_beat() {
        let mut effect = ModelEffectState::default();
        assert_eq!(model_effect_clock_units(effect, 2.0, 8.0), 2.0);
        effect.clock = ModelEffectClock::Beat;
        assert_eq!(model_effect_clock_units(effect, 2.0, 8.0), 8.0);
    }

    #[test]
    fn model_effect_mix_samples_itg_timing_curve() {
        let effect = ModelEffectState {
            mode: ModelEffectMode::Pulse,
            timing: [0.25, 0.25, 0.25, 0.25, 0.0],
            ..ModelEffectState::default()
        };

        assert_eq!(model_effect_mix(effect, 0.125, 0.0), Some(0.25));
        assert_eq!(model_effect_mix(effect, 0.375, 0.0), Some(0.5));
        assert_eq!(model_effect_mix(effect, 0.625, 0.0), Some(0.75));
        assert_eq!(model_effect_mix(effect, 0.875, 0.0), Some(1.0));
    }

    #[test]
    fn model_effect_mix_ignores_non_mixing_modes() {
        let effect = ModelEffectState {
            mode: ModelEffectMode::Spin,
            ..ModelEffectState::default()
        };

        assert_eq!(model_effect_mix(effect, 0.5, 0.0), None);
    }

    #[test]
    fn canonical_effect_timing_matches_legacy_curve() {
        for mode in [
            ModelEffectMode::DiffuseRamp,
            ModelEffectMode::DiffuseShift,
            ModelEffectMode::GlowShift,
            ModelEffectMode::Pulse,
            ModelEffectMode::Bob,
            ModelEffectMode::Bounce,
            ModelEffectMode::Wag,
        ] {
            for clock in [ModelEffectClock::Time, ModelEffectClock::Beat] {
                let effect = ModelEffectState {
                    mode,
                    clock,
                    offset: 0.125,
                    ..ModelEffectState::default()
                };
                for tick in -16_384..=16_384 {
                    let time = tick as f32 / 1_024.0;
                    let beat = tick as f32 / 4_096.0;
                    assert_eq!(
                        model_effect_mix(effect, time, beat).map(f32::to_bits),
                        model_effect_mix_legacy(effect, time, beat).map(f32::to_bits),
                    );
                }
                for value in [
                    -f32::MAX,
                    -0.0,
                    0.0,
                    f32::MAX,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                    f32::NAN,
                ] {
                    assert_eq!(
                        model_effect_mix(effect, value, value).map(f32::to_bits),
                        model_effect_mix_legacy(effect, value, value).map(f32::to_bits),
                    );
                }
            }
        }
    }

    #[test]
    fn glowshift_mix_uses_sine_phase() {
        assert!((glowshift_mix(0.0) - 1.0).abs() <= f32::EPSILON);
        assert!((glowshift_mix(0.5) - 0.0).abs() <= 1e-6);
    }

    #[test]
    fn model_auto_rot_interpolates_and_wraps() {
        let keys = [
            ModelAutoRotKey {
                frame: 10.0,
                z_deg: 20.0,
            },
            ModelAutoRotKey {
                frame: 40.0,
                z_deg: 80.0,
            },
        ];

        assert_eq!(model_auto_rot_z_at(80.0, &keys, 0.0), Some(20.0));
        assert!((model_auto_rot_z_at(80.0, &keys, 25.0 / 30.0).unwrap() - 50.0).abs() <= 1e-6);
        assert_eq!(model_auto_rot_z_at(80.0, &keys, 40.0 / 30.0), Some(80.0));
        assert_eq!(model_auto_rot_z_at(80.0, &keys, 80.0 / 30.0), Some(20.0));
    }

    #[test]
    fn small_auto_rot_key_sets_match_legacy_search() {
        let key_sets = [
            &[
                ModelAutoRotKey {
                    frame: 10.0,
                    z_deg: -35.0,
                },
                ModelAutoRotKey {
                    frame: 80.0,
                    z_deg: 145.0,
                },
            ][..1],
            &[
                ModelAutoRotKey {
                    frame: 10.0,
                    z_deg: -35.0,
                },
                ModelAutoRotKey {
                    frame: 80.0,
                    z_deg: 145.0,
                },
            ][..],
            &[
                ModelAutoRotKey {
                    frame: 80.0,
                    z_deg: 145.0,
                },
                ModelAutoRotKey {
                    frame: 10.0,
                    z_deg: -35.0,
                },
            ][..],
        ];
        for keys in key_sets {
            for total_frames in [
                -f32::INFINITY,
                -1.0,
                0.0,
                f32::EPSILON,
                120.0,
                f32::INFINITY,
                f32::NAN,
            ] {
                for time in [
                    -f32::MAX,
                    -1.0,
                    -0.0,
                    0.0,
                    10.0 / 30.0,
                    45.0 / 30.0,
                    80.0 / 30.0,
                    f32::MAX,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                    f32::NAN,
                ] {
                    assert_optional_float_bits_eq(
                        model_auto_rot_z_at(total_frames, keys, time),
                        model_auto_rot_z_at_legacy(total_frames, keys, time),
                    );
                }
            }
        }
    }

    #[test]
    fn model_texture_uv_params_preserve_atlas_shift_only() {
        let uv_rect = [0.25, 0.5, 0.75, 1.0];

        assert_eq!(
            model_texture_uv_params(uv_rect, [64, 32], Some((256, 64))),
            ([0.5, 0.5], [0.25, 0.5], [0.0, 0.0])
        );
        assert_eq!(
            model_texture_uv_params(uv_rect, [64, 32], None),
            ([0.5, 0.5], [0.25, 0.5], [0.0, 0.0])
        );
        assert_eq!(
            model_texture_uv_params([0.5, 0.25, 0.75, 0.75], [64, 32], Some((256, 64))),
            ([0.25, 0.5], [0.5, 0.25], [0.25, -0.25])
        );
    }

    #[test]
    fn cached_model_uv_origin_matches_legacy_normalization() {
        for tex_dims in [(1, 1), (257, 509), (4_096, 2_047)] {
            for src in [[0, 0], [17, 31], [-19, -7]] {
                let origin = [
                    src[0] as f32 * (1.0 / tex_dims.0 as f32),
                    src[1] as f32 * (1.0 / tex_dims.1 as f32),
                ];
                for uv_rect in [
                    [0.0, 0.0, 1.0, 1.0],
                    [0.125, 0.25, 0.75, 0.875],
                    [-0.5, 1.25, 2.0, -1.0],
                ] {
                    assert_uv_params_close(
                        model_texture_uv_params_cached(uv_rect, Some(origin)),
                        model_texture_uv_params(uv_rect, src, Some(tex_dims)),
                    );
                    assert_eq!(
                        model_texture_uv_params_cached(uv_rect, None),
                        model_texture_uv_params(uv_rect, src, None),
                    );
                }
            }
        }
    }

    #[test]
    fn static_model_draw_fast_path_matches_legacy_evaluator() {
        let base = ModelDrawState {
            pos: [3.0, -5.0, 7.0],
            rot: [-90.0, 450.0, 720.0],
            zoom: [-2.0, 0.5, 3.0],
            tint: [-1.0, 0.25, 2.0, 0.75],
            glow: [1.5, -0.5, 0.5, 2.0],
            fade: [-1.0, 0.25, 1.5, 0.75],
            ..ModelDrawState::default()
        };
        let key = ModelAutoRotKey {
            frame: 0.0,
            z_deg: 90.0,
        };
        for mode in [
            ModelEffectMode::None,
            ModelEffectMode::GlowShift,
            ModelEffectMode::Bob,
            ModelEffectMode::Bounce,
            ModelEffectMode::Wag,
        ] {
            let effect = ModelEffectState {
                mode,
                ..ModelEffectState::default()
            };
            for (total_frames, keys) in [(80.0, &[][..]), (0.0, &[key][..])] {
                for time in [
                    -f32::MAX,
                    -1.0,
                    -0.0,
                    0.0,
                    1.25,
                    f32::MAX,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                    f32::NAN,
                ] {
                    assert_draw_bits_eq(
                        model_draw_at(base, &[], effect, total_frames, keys, time, time * 4.0),
                        model_draw_at_legacy(
                            base,
                            &[],
                            effect,
                            total_frames,
                            keys,
                            time,
                            time * 4.0,
                        ),
                    );
                }
            }
        }
    }

    #[test]
    fn model_draw_at_applies_timeline_spin_and_clamps() {
        let from = ModelDrawState {
            tint: [2.0, -1.0, 0.5, 1.0],
            ..ModelDrawState::default()
        };
        let to = ModelDrawState {
            pos: [10.0, 0.0, 0.0],
            rot: [0.0, 0.0, 90.0],
            zoom: [2.0, 2.0, 2.0],
            tint: [0.5, 0.5, 0.5, 0.5],
            ..ModelDrawState::default()
        };
        let timeline = [ModelTweenSegment {
            start: 0.0,
            duration: 2.0,
            tween: TweenType::Linear,
            from,
            to,
        }];
        let effect = ModelEffectState {
            mode: ModelEffectMode::Spin,
            magnitude: [0.0, 0.0, 30.0],
            ..ModelEffectState::default()
        };

        let draw = model_draw_at(
            ModelDrawState::default(),
            &timeline,
            effect,
            0.0,
            &[],
            1.0,
            0.0,
        );

        assert_eq!(draw.pos[0], 5.0);
        assert_eq!(draw.rot[2], 75.0);
        assert_eq!(draw.zoom[0], 1.5);
        assert_eq!(draw.tint, [1.0, 0.0, 0.5, 0.75]);
    }

    #[test]
    fn cursor_draw_is_bit_exact_across_frames_and_backward_seeks() {
        let base = ModelDrawState::default();
        let first = ModelDrawState {
            pos: [8.0, -2.0, 1.0],
            rot: [5.0, 10.0, 20.0],
            tint: [0.8, 0.7, 0.6, 0.9],
            ..base
        };
        let second = ModelDrawState {
            pos: [12.0, 4.0, -3.0],
            rot: [15.0, 30.0, 60.0],
            zoom: [1.25, 0.75, 2.0],
            glow: [0.1, 0.2, 0.3, 0.4],
            ..first
        };
        let third = ModelDrawState {
            visible: false,
            blend_add: true,
            ..second
        };
        let timeline = [
            ModelTweenSegment {
                start: 0.0,
                duration: 1.0,
                tween: TweenType::Accelerate,
                from: base,
                to: first,
            },
            ModelTweenSegment {
                start: 1.0,
                duration: 2.0,
                tween: TweenType::Decelerate,
                from: first,
                to: second,
            },
            ModelTweenSegment {
                start: 3.0,
                duration: 0.0,
                tween: TweenType::Linear,
                from: second,
                to: third,
            },
        ];
        let effect = ModelEffectState {
            mode: ModelEffectMode::Spin,
            magnitude: [2.0, 3.0, 4.0],
            ..ModelEffectState::default()
        };
        let auto_rot = [ModelAutoRotKey {
            frame: 0.0,
            z_deg: 11.0,
        }];
        let mut cursor = ModelTweenCursor::default();

        for time in [-1.0, 0.0, 0.25, 1.0, 1.75, 3.0, 5.0, 1.25, 4.0] {
            let expected =
                model_draw_at(base, &timeline, effect, 30.0, &auto_rot, time, time * 4.0);
            let actual = model_draw_at_cursor(
                base,
                &timeline,
                effect,
                30.0,
                &auto_rot,
                [time, time * 4.0],
                &mut cursor,
            );
            assert_draw_bits_eq(actual, expected);
        }
    }

    #[test]
    fn model_glow_with_draw_samples_glowshift_channel() {
        let draw = ModelDrawState {
            glow: [1.0, 1.0, 1.0, 1.0],
            ..ModelDrawState::default()
        };
        let effect = ModelEffectState {
            mode: ModelEffectMode::GlowShift,
            color1: [1.0, 0.0, 0.0, 1.0],
            color2: [0.0, 0.0, 1.0, 0.5],
            ..ModelEffectState::default()
        };

        let glow = model_glow_with_draw(draw, effect, 0.0, 0.0, 0.25).unwrap();

        assert_eq!(glow, [1.0, 0.0, 0.0, 0.25]);
    }

    #[test]
    fn transparent_static_glow_matches_legacy_clamping() {
        for mode in [
            ModelEffectMode::None,
            ModelEffectMode::DiffuseRamp,
            ModelEffectMode::DiffuseShift,
            ModelEffectMode::Pulse,
            ModelEffectMode::Bob,
            ModelEffectMode::Bounce,
            ModelEffectMode::Wag,
            ModelEffectMode::Spin,
        ] {
            let effect = ModelEffectState {
                mode,
                ..ModelEffectState::default()
            };
            for alpha in [
                f32::NEG_INFINITY,
                -1.0,
                -0.0,
                0.0,
                f32::EPSILON,
                f32::EPSILON * 2.0,
                1.0,
                f32::INFINITY,
                f32::NAN,
            ] {
                let draw = ModelDrawState {
                    glow: [f32::NAN, f32::NEG_INFINITY, f32::INFINITY, alpha],
                    ..ModelDrawState::default()
                };
                assert_optional_color_bits_eq(
                    model_glow_with_draw(draw, effect, 1.25, 5.0, 0.75),
                    model_glow_with_draw_legacy(draw, effect, 1.25, 5.0, 0.75),
                );
            }
        }
    }
}
