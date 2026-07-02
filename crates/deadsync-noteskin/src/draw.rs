use std::sync::Arc;

use deadlib_render::TexturedMeshVertex;

#[derive(Debug, Clone, Copy)]
pub enum TweenType {
    Linear,
    Accelerate,
    Decelerate,
}

impl TweenType {
    pub fn ease(self, progress: f32) -> f32 {
        let t = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Accelerate => t * t,
            Self::Decelerate => 1.0 - (1.0 - t) * (1.0 - t),
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
    pub fn size(&self) -> [f32; 2] {
        [
            (self.bounds[3] - self.bounds[0]).max(0.0),
            (self.bounds[4] - self.bounds[1]).max(0.0),
        ]
    }
}

#[inline(always)]
pub fn build_textured_model_geometry(
    model: &ModelMesh,
    mirror_h: bool,
    mirror_v: bool,
) -> Arc<[TexturedMeshVertex]> {
    let mut vertices = Vec::with_capacity(model.vertices.len());
    for v in model.vertices.iter() {
        let mut pos = v.pos;
        if mirror_h {
            pos[0] = -pos[0];
        }
        if mirror_v {
            pos[1] = -pos[1];
        }
        let u = if mirror_h { 1.0 - v.uv[0] } else { v.uv[0] };
        let v_tex = if mirror_v { 1.0 - v.uv[1] } else { v.uv[1] };
        vertices.push(TexturedMeshVertex {
            pos,
            uv: [u, v_tex],
            tex_matrix_scale: v.tex_matrix_scale,
            color: [1.0; 4],
        });
    }
    Arc::from(vertices)
}

#[inline(always)]
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

#[derive(Debug, Clone, Copy)]
pub struct ModelDrawState {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub zoom: [f32; 3],
    pub tint: [f32; 4],
    pub glow: [f32; 4],
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
pub fn model_effect_clock_units(effect: ModelEffectState, time: f32, beat: f32) -> f32 {
    match effect.clock {
        ModelEffectClock::Time => time,
        ModelEffectClock::Beat => beat,
    }
}

#[inline(always)]
pub fn model_effect_mix(effect: ModelEffectState, time: f32, beat: f32) -> Option<f32> {
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
    let t = effect.timing;
    let total = (t[0] + t[1] + t[2] + t[3] + t[4]).max(1e-6);
    let units = model_effect_clock_units(effect, time, beat) + effect.offset;
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
            0.5 + ((x - rup_plus_ath) / t[2]) * 0.5
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

#[inline(always)]
pub fn glowshift_mix(through: f32) -> f32 {
    (((through + 0.25) * 2.0 * std::f32::consts::PI).sin() * 0.5 + 0.5).clamp(0.0, 1.0)
}

#[inline(always)]
pub fn model_auto_rot_z_at(total_frames: f32, keys: &[ModelAutoRotKey], time: f32) -> Option<f32> {
    if total_frames <= f32::EPSILON {
        return None;
    }
    let first = *keys.first()?;
    let frame = (time * 30.0).rem_euclid(total_frames);
    if !frame.is_finite() {
        return Some(first.z_deg);
    }
    if frame <= first.frame {
        return Some(first.z_deg);
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

pub fn model_draw_at(
    base_draw: ModelDrawState,
    timeline: &[ModelTweenSegment],
    effect: ModelEffectState,
    auto_rot_total_frames: f32,
    auto_rot_z_keys: &[ModelAutoRotKey],
    time: f32,
    beat: f32,
) -> ModelDrawState {
    #[inline(always)]
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        (b - a).mul_add(t, a)
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
        out.rot[0] = (out.rot[0] + effect.magnitude[0] * clock).rem_euclid(360.0);
        out.rot[1] = (out.rot[1] + effect.magnitude[1] * clock).rem_euclid(360.0);
        out.rot[2] = (out.rot[2] + effect.magnitude[2] * clock).rem_euclid(360.0);
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

#[inline(always)]
pub fn model_glow_with_draw(
    draw: ModelDrawState,
    effect: ModelEffectState,
    time: f32,
    beat: f32,
    diffuse_alpha: f32,
) -> Option<[f32; 4]> {
    let mut glow = draw.glow;
    if matches!(effect.mode, ModelEffectMode::GlowShift) {
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

#[inline(always)]
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
    out
}

#[cfg(test)]
mod tests {
    use super::{
        ModelAutoRotKey, ModelDrawState, ModelEffectClock, ModelEffectMode, ModelEffectState,
        ModelTweenSegment, TweenType, glowshift_mix, model_auto_rot_z_at, model_draw_at,
        model_effect_clock_units, model_effect_mix, model_glow_with_draw, model_texture_uv_params,
    };

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
}
