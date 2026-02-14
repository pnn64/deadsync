use super::noteskin_itg;
use crate::assets;
use image::image_dimensions;
use log::{info, warn};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

pub const NUM_QUANTIZATIONS: usize = 9;
const MINE_GRADIENT_SAMPLES: usize = 64;
const ITG_ARG0_TOKEN: &str = "__ITG_ARG0__";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum Quantization {
    Q4th = 0,
    Q8th,
    Q12th,
    Q16th,
    Q24th,
    Q32nd,
    Q48th,
    Q64th,
    Q192nd,
}

#[derive(Debug, Clone, Default)]
pub struct SpriteDefinition {
    pub src: [i32; 2],
    pub size: [i32; 2],
    pub rotation_deg: i32,
    pub mirror_h: bool,
    pub mirror_v: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum AnimationRate {
    FramesPerSecond(f32),
    FramesPerBeat(f32),
}

#[derive(Debug)]
pub enum SpriteSource {
    Atlas {
        texture_key: String,
        tex_dims: (u32, u32),
    },
    Animated {
        texture_key: String,
        tex_dims: (u32, u32),
        frame_size: [i32; 2],
        grid: (usize, usize),
        frame_count: usize,
        rate: AnimationRate,
        frame_durations: Option<Arc<[f32]>>,
    },
}

impl SpriteSource {
    pub fn texture_key(&self) -> &str {
        match self {
            Self::Atlas { texture_key, .. } => texture_key,
            Self::Animated { texture_key, .. } => texture_key,
        }
    }

    pub fn frame_count(&self) -> usize {
        match self {
            Self::Atlas { .. } => 1,
            Self::Animated { frame_count, .. } => (*frame_count).max(1),
        }
    }

    pub const fn frame_size(&self) -> Option<[i32; 2]> {
        match self {
            Self::Atlas { .. } => None,
            Self::Animated { frame_size, .. } => Some(*frame_size),
        }
    }

    pub const fn is_beat_based(&self) -> bool {
        matches!(
            self,
            Self::Animated {
                rate: AnimationRate::FramesPerBeat(_),
                ..
            }
        )
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

#[derive(Debug, Clone, Copy)]
pub enum ModelEffectClock {
    Time,
    Beat,
}

#[derive(Debug, Clone, Copy)]
pub enum ModelEffectMode {
    None,
    DiffuseRamp,
    GlowShift,
    Pulse,
    Spin,
}

#[derive(Debug, Clone, Copy)]
pub struct ModelEffectState {
    pub clock: ModelEffectClock,
    pub mode: ModelEffectMode,
    pub color1: [f32; 4],
    pub color2: [f32; 4],
    pub period: f32,
    pub offset: f32,
    pub timing: [f32; 4], // ramp_up, hold_high, ramp_down, hold_low
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
            timing: [0.5, 0.0, 0.5, 0.0],
            magnitude: [1.0, 1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ModelAutoRotKey {
    pub frame: f32,
    pub z_deg: f32,
}

#[derive(Debug, Clone)]
pub struct SpriteSlot {
    pub def: SpriteDefinition,
    pub source: Arc<SpriteSource>,
    pub uv_velocity: [f32; 2],
    pub uv_offset: [f32; 2],
    pub note_color_translate: bool,
    pub model: Option<Arc<ModelMesh>>,
    pub model_draw: ModelDrawState,
    pub model_timeline: Arc<[ModelTweenSegment]>,
    pub model_effect: ModelEffectState,
    pub model_auto_rot_total_frames: f32,
    pub model_auto_rot_z_keys: Arc<[ModelAutoRotKey]>,
}

impl SpriteSlot {
    #[inline(always)]
    fn model_effect_mix(effect: ModelEffectState, time: f32, beat: f32) -> Option<f32> {
        if !matches!(
            effect.mode,
            ModelEffectMode::DiffuseRamp | ModelEffectMode::GlowShift | ModelEffectMode::Pulse
        ) {
            return None;
        }
        let period = effect.period.max(1e-6);
        let phase_input = match effect.clock {
            ModelEffectClock::Time => time,
            ModelEffectClock::Beat => beat,
        };
        let phase = (phase_input + effect.offset).rem_euclid(period) / period;
        let t = effect.timing;
        let total = (t[0] + t[1] + t[2] + t[3]).max(1e-6);
        let mut x = phase * total;
        let mix = if x < t[0] && t[0] > f32::EPSILON {
            x / t[0]
        } else {
            x -= t[0];
            if x < t[1] {
                1.0
            } else {
                x -= t[1];
                if x < t[2] && t[2] > f32::EPSILON {
                    1.0 - (x / t[2])
                } else {
                    0.0
                }
            }
        }
        .clamp(0.0, 1.0);
        Some(mix)
    }

    #[inline(always)]
    fn model_auto_rot_z_at(&self, time: f32) -> Option<f32> {
        if self.model_auto_rot_total_frames <= f32::EPSILON {
            return None;
        }
        let keys = self.model_auto_rot_z_keys.as_ref();
        if keys.is_empty() {
            return None;
        }
        let frame = (time * 30.0).rem_euclid(self.model_auto_rot_total_frames);
        if !frame.is_finite() {
            return Some(keys[0].z_deg);
        }
        let first = keys[0];
        if frame <= first.frame {
            return Some(first.z_deg);
        }
        let mut prev = first;
        for key in keys.iter().copied().skip(1) {
            if frame <= key.frame {
                let span = (key.frame - prev.frame).max(1e-6);
                let t = ((frame - prev.frame) / span).clamp(0.0, 1.0);
                return Some((key.z_deg - prev.z_deg).mul_add(t, prev.z_deg));
            }
            prev = key;
        }
        Some(prev.z_deg)
    }

    pub fn texture_key(&self) -> &str {
        self.source.texture_key()
    }

    pub const fn size(&self) -> [i32; 2] {
        self.def.size
    }

    #[inline(always)]
    pub fn logical_size(&self) -> [f32; 2] {
        let mut width = self.def.size[0].max(0) as f32;
        let mut height = self.def.size[1].max(0) as f32;
        if crate::assets::parse_texture_hints(self.texture_key()).doubleres {
            width *= 0.5;
            height *= 0.5;
        }
        [width, height]
    }

    pub fn frame_index(&self, time: f32, beat: f32) -> usize {
        let frames = self.source.frame_count();
        if frames <= 1 {
            return 0;
        }
        match self.source.as_ref() {
            SpriteSource::Atlas { .. } => 0,
            SpriteSource::Animated {
                rate,
                frame_durations,
                ..
            } => {
                if let Some(durations) = frame_durations.as_ref() {
                    let clock = match rate {
                        AnimationRate::FramesPerSecond(_) => time,
                        AnimationRate::FramesPerBeat(_) => beat,
                    };
                    let mut total = 0.0f32;
                    for duration in durations.iter().take(frames) {
                        if *duration > f32::EPSILON {
                            total += *duration;
                        }
                    }
                    if total > f32::EPSILON && total.is_finite() {
                        let mut phase = clock.rem_euclid(total);
                        for (idx, duration) in durations.iter().take(frames).enumerate() {
                            let d = (*duration).max(0.0);
                            if d <= f32::EPSILON {
                                continue;
                            }
                            if phase < d {
                                return idx;
                            }
                            phase -= d;
                        }
                        if let Some(last_idx) = durations
                            .iter()
                            .take(frames)
                            .enumerate()
                            .rfind(|(_, duration)| **duration > f32::EPSILON)
                            .map(|(idx, _)| idx)
                        {
                            return last_idx;
                        }
                    }
                }
                let frame = match rate {
                    AnimationRate::FramesPerSecond(fps) if *fps > 0.0 => {
                        (time * fps).floor() as isize
                    }
                    AnimationRate::FramesPerBeat(frames_per_beat) if *frames_per_beat > 0.0 => {
                        (beat * frames_per_beat).floor() as isize
                    }
                    _ => return 0,
                };
                ((frame % frames as isize) + frames as isize) as usize % frames
            }
        }
    }

    pub fn frame_index_from_phase(&self, phase: f32) -> usize {
        let frames = self.source.frame_count();
        if frames <= 1 {
            return 0;
        }
        let p = phase.rem_euclid(1.0);
        match self.source.as_ref() {
            SpriteSource::Atlas { .. } => 0,
            SpriteSource::Animated {
                frame_durations, ..
            } => {
                if let Some(durations) = frame_durations.as_ref() {
                    let mut total = 0.0f32;
                    for duration in durations.iter().take(frames) {
                        if *duration > f32::EPSILON {
                            total += *duration;
                        }
                    }
                    if total > f32::EPSILON && total.is_finite() {
                        let mut target = p * total;
                        for (idx, duration) in durations.iter().take(frames).enumerate() {
                            let d = (*duration).max(0.0);
                            if d <= f32::EPSILON {
                                continue;
                            }
                            if target < d {
                                return idx;
                            }
                            target -= d;
                        }
                        if let Some(last_idx) = durations
                            .iter()
                            .take(frames)
                            .enumerate()
                            .rfind(|(_, duration)| **duration > f32::EPSILON)
                            .map(|(idx, _)| idx)
                        {
                            return last_idx;
                        }
                    }
                }
                ((p * frames as f32).floor() as usize).min(frames - 1)
            }
        }
    }

    pub fn model_draw_at(&self, time: f32, beat: f32) -> ModelDrawState {
        #[inline(always)]
        fn lerp(a: f32, b: f32, t: f32) -> f32 {
            (b - a).mul_add(t, a)
        }

        let mut out = self.model_draw;
        let local = time.max(0.0);

        for seg in self.model_timeline.iter() {
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

        if let Some(rot_z) = self.model_auto_rot_z_at(time) {
            out.rot[2] = (out.rot[2] + rot_z).rem_euclid(360.0);
        }

        let effect = self.model_effect;
        if matches!(effect.mode, ModelEffectMode::Spin) {
            let clock = match effect.clock {
                ModelEffectClock::Time => time,
                ModelEffectClock::Beat => beat,
            };
            let spin_units = clock - effect.offset;
            out.rot[0] = (out.rot[0] + effect.magnitude[0] * spin_units).rem_euclid(360.0);
            out.rot[1] = (out.rot[1] + effect.magnitude[1] * spin_units).rem_euclid(360.0);
            out.rot[2] = (out.rot[2] + effect.magnitude[2] * spin_units).rem_euclid(360.0);
        }
        if let Some(mix) = Self::model_effect_mix(effect, time, beat) {
            match effect.mode {
                ModelEffectMode::DiffuseRamp => {
                    let mut c = [0.0; 4];
                    for (i, out) in c.iter_mut().enumerate() {
                        *out = lerp(effect.color1[i], effect.color2[i], mix).clamp(0.0, 1.0);
                    }
                    out.tint[0] *= c[0];
                    out.tint[1] *= c[1];
                    out.tint[2] *= c[2];
                    out.tint[3] *= c[3];
                }
                ModelEffectMode::Pulse => {
                    out.zoom[0] *= lerp(1.0, effect.magnitude[0], mix).max(0.0);
                    out.zoom[1] *= lerp(1.0, effect.magnitude[1], mix).max(0.0);
                    out.zoom[2] *= lerp(1.0, effect.magnitude[2], mix).max(0.0);
                }
                // ITG applies glowshift to the separate glow channel, not diffuse.
                // The renderer samples this via `model_glow_at()`.
                ModelEffectMode::GlowShift => {}
                ModelEffectMode::Spin => {}
                ModelEffectMode::None => {}
            }
        }

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

    pub fn model_glow_at(&self, time: f32, beat: f32, diffuse_alpha: f32) -> Option<[f32; 4]> {
        let mut glow = self.model_draw_at(time, beat).glow;
        let effect = self.model_effect;
        if matches!(effect.mode, ModelEffectMode::GlowShift) {
            let through = Self::model_effect_mix(effect, time, beat)?;
            // Match ITG's glow_shift blending: convert effect-phase percentage to the
            // sinusoidal color mix used by Actor::PreDraw.
            let mix =
                (((through + 0.25) * 2.0 * std::f32::consts::PI).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
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

    pub fn uv_for_frame_at(&self, frame_index: usize, elapsed: f32) -> [f32; 4] {
        let mut uv = match self.source.as_ref() {
            SpriteSource::Atlas { tex_dims, .. } => {
                let tw = tex_dims.0.max(1) as f32;
                let th = tex_dims.1.max(1) as f32;
                let src = self.def.src;
                let size = self.def.size;

                let mut u0 = src[0] as f32;
                let mut v0 = src[1] as f32;
                let mut u1 = (src[0] + size[0]) as f32;
                let mut v1 = (src[1] + size[1]) as f32;

                if self.model.is_none() {
                    if size[0] > 0 {
                        u0 += 0.5;
                        u1 -= 0.5;
                    }
                    if size[1] > 0 {
                        v0 += 0.5;
                        v1 -= 0.5;
                    }
                }

                [u0 / tw, v0 / th, u1 / tw, v1 / th]
            }
            SpriteSource::Animated {
                tex_dims,
                frame_size,
                grid,
                frame_count,
                ..
            } => {
                let frames = (*frame_count).max(1);
                let idx = if frames > 0 { frame_index % frames } else { 0 };
                let cols = grid.0.max(1);
                let row = idx / cols;
                let col = idx % cols;
                let src_x = self.def.src[0] + (col as i32 * frame_size[0]);
                let src_y = self.def.src[1] + (row as i32 * frame_size[1]);
                let tw = tex_dims.0.max(1) as f32;
                let th = tex_dims.1.max(1) as f32;

                let mut u0 = src_x as f32;
                let mut v0 = src_y as f32;
                let mut u1 = (src_x + frame_size[0]) as f32;
                let mut v1 = (src_y + frame_size[1]) as f32;

                if self.model.is_none() {
                    if frame_size[0] > 0 {
                        u0 += 0.5;
                        u1 -= 0.5;
                    }
                    if frame_size[1] > 0 {
                        v0 += 0.5;
                        v1 -= 0.5;
                    }
                }

                [u0 / tw, v0 / th, u1 / tw, v1 / th]
            }
        };

        // ITG model textures can scroll via AnimatedTexture TexVelocity/TexOffset.
        // Model UVs often rely on a full [0..1] span, so preserve span width when
        // offsetting and avoid per-endpoint wrapping here.
        if self.uv_velocity != [0.0, 0.0] || self.uv_offset != [0.0, 0.0] {
            let w = (uv[2] - uv[0]).abs();
            let h = (uv[3] - uv[1]).abs();
            let shift_u = self.uv_offset[0] + self.uv_velocity[0] * elapsed;
            let shift_v = self.uv_offset[1] + self.uv_velocity[1] * elapsed;
            if self.model.is_some() {
                uv[0] += shift_u;
                uv[2] += shift_u;
                uv[1] += shift_v;
                uv[3] += shift_v;
            } else {
                let u_span = (1.0 - w).max(0.0);
                let v_span = (1.0 - h).max(0.0);
                let u_shift = if u_span > f32::EPSILON {
                    shift_u.rem_euclid(u_span)
                } else {
                    0.0
                };
                let v_shift = if v_span > f32::EPSILON {
                    shift_v.rem_euclid(v_span)
                } else {
                    0.0
                };
                uv[0] += u_shift;
                uv[2] += u_shift;
                uv[1] += v_shift;
                uv[3] += v_shift;
            }
        }
        uv
    }
}

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
pub struct ExplosionState {
    pub zoom: f32,
    pub color: [f32; 4],
    pub visible: bool,
}

impl Default for ExplosionState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            color: [1.0, 1.0, 1.0, 1.0],
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExplosionSegment {
    pub duration: f32,
    pub tween: TweenType,
    pub start: ExplosionState,
    pub end_zoom: Option<f32>,
    pub end_color: Option<[f32; 4]>,
    pub end_visible: Option<bool>,
}

#[derive(Debug, Clone, Copy)]
pub struct GlowEffect {
    pub period: f32,
    pub color1: [f32; 4],
    pub color2: [f32; 4],
}

impl GlowEffect {
    fn color_at(&self, time: f32, base_alpha: f32) -> [f32; 4] {
        if self.period <= f32::EPSILON || base_alpha <= f32::EPSILON {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let phase = (time / self.period).rem_euclid(1.0);
        if !phase.is_finite() {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let percent_between = ((phase + 0.25) * std::f32::consts::TAU)
            .sin()
            .mul_add(0.5, 0.5);

        let mut color = [0.0; 4];
        for i in 0..4 {
            color[i] =
                self.color1[i].mul_add(percent_between, self.color2[i] * (1.0 - percent_between));
        }
        color[3] *= base_alpha;
        color
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExplosionVisualState {
    pub zoom: f32,
    pub diffuse: [f32; 4],
    pub glow: [f32; 4],
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct ExplosionAnimation {
    pub initial: ExplosionState,
    pub segments: Vec<ExplosionSegment>,
    pub glow: Option<GlowEffect>,
}

impl Default for ExplosionAnimation {
    fn default() -> Self {
        Self {
            initial: ExplosionState {
                zoom: 1.0,
                color: [1.0, 1.0, 1.0, 1.0],
                visible: true,
            },
            segments: vec![ExplosionSegment {
                duration: 0.3,
                tween: TweenType::Linear,
                start: ExplosionState {
                    zoom: 1.0,
                    color: [1.0, 1.0, 1.0, 1.0],
                    visible: true,
                },
                end_zoom: Some(1.0),
                end_color: Some([1.0, 1.0, 1.0, 0.0]),
                end_visible: None,
            }],
            glow: None,
        }
    }
}

impl ExplosionAnimation {
    pub fn duration(&self) -> f32 {
        self.segments
            .iter()
            .map(|segment| segment.duration.max(0.0))
            .sum::<f32>()
            .max(0.0)
    }

    pub fn state_at(&self, time: f32) -> ExplosionVisualState {
        let mut elapsed = time;
        let mut current = self.initial;

        for segment in &self.segments {
            let duration = segment.duration.max(0.0);
            if duration <= 0.0 {
                if let Some(zoom) = segment.end_zoom {
                    current.zoom = zoom;
                }
                if let Some(color) = segment.end_color {
                    current.color = color;
                }
                if let Some(visible) = segment.end_visible {
                    current.visible = visible;
                }
                continue;
            }

            if elapsed > duration {
                if let Some(zoom) = segment.end_zoom {
                    current.zoom = zoom;
                }
                if let Some(color) = segment.end_color {
                    current.color = color;
                }
                if let Some(visible) = segment.end_visible {
                    current.visible = visible;
                }
                elapsed -= duration;
                continue;
            }

            let progress = (elapsed / duration).clamp(0.0, 1.0);
            let eased = segment.tween.ease(progress);

            let mut zoom = current.zoom;
            if let Some(target_zoom) = segment.end_zoom {
                zoom = (target_zoom - segment.start.zoom).mul_add(eased, segment.start.zoom);
            }

            let mut color = current.color;
            if let Some(target_color) = segment.end_color {
                let mut interpolated = current.color;
                for i in 0..4 {
                    interpolated[i] = (target_color[i] - segment.start.color[i])
                        .mul_add(eased, segment.start.color[i]);
                }
                color = interpolated;
            }

            let diffuse = color;
            let glow = self
                .glow
                .map_or([0.0, 0.0, 0.0, 0.0], |g| g.color_at(time, diffuse[3]));
            let visible = segment.end_visible.unwrap_or(current.visible);

            return ExplosionVisualState {
                zoom,
                diffuse,
                glow,
                visible,
            };
        }

        let diffuse = current.color;
        let glow = self
            .glow
            .map_or([0.0, 0.0, 0.0, 0.0], |g| g.color_at(time, diffuse[3]));

        ExplosionVisualState {
            zoom: current.zoom,
            diffuse,
            glow,
            visible: current.visible,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TapExplosion {
    pub slot: SpriteSlot,
    pub animation: ExplosionAnimation,
}

#[derive(Debug, Clone, Copy)]
pub struct ReceptorGlowBehavior {
    pub duration: f32,
    pub alpha_start: f32,
    pub alpha_end: f32,
    pub zoom_start: f32,
    pub zoom_end: f32,
    pub tween: TweenType,
    pub blend_add: bool,
}

impl ReceptorGlowBehavior {
    pub fn sample(self, timer_remaining: f32) -> (f32, f32) {
        let duration = self.duration.max(0.0);
        if duration <= f32::EPSILON {
            return (self.alpha_end.clamp(0.0, 1.0), self.zoom_end.max(0.0));
        }
        let elapsed = (duration - timer_remaining.clamp(0.0, duration)).clamp(0.0, duration);
        let progress = elapsed / duration;
        let eased = self.tween.ease(progress);
        let alpha = (self.alpha_end - self.alpha_start).mul_add(eased, self.alpha_start);
        let zoom = (self.zoom_end - self.zoom_start).mul_add(eased, self.zoom_start);
        (alpha.clamp(0.0, 1.0), zoom.max(0.0))
    }
}

impl Default for ReceptorGlowBehavior {
    fn default() -> Self {
        Self {
            duration: 0.2,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 1.0,
            zoom_end: 1.0,
            tween: TweenType::Decelerate,
            blend_add: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HoldVisuals {
    pub head_inactive: Option<SpriteSlot>,
    pub head_active: Option<SpriteSlot>,
    pub body_inactive: Option<SpriteSlot>,
    pub body_active: Option<SpriteSlot>,
    pub bottomcap_inactive: Option<SpriteSlot>,
    pub bottomcap_active: Option<SpriteSlot>,
    pub explosion: Option<SpriteSlot>,
}

#[derive(Debug, Clone, Copy)]
pub struct NoteDisplayMetrics {
    pub draw_hold_head_for_taps_on_same_row: bool,
    pub draw_roll_head_for_taps_on_same_row: bool,
    pub tap_hold_roll_on_row_means_hold: bool,
    pub hold_head_is_above_wavy_parts: bool,
    pub hold_tail_is_above_wavy_parts: bool,
    pub start_drawing_hold_body_offset_from_head: f32,
    pub stop_drawing_hold_body_offset_from_tail: f32,
    pub hold_let_go_gray_percent: f32,
    pub flip_head_and_tail_when_reverse: bool,
    pub flip_hold_body_when_reverse: bool,
    pub top_hold_anchor_when_reverse: bool,
    pub hold_active_is_add_layer: bool,
    pub part_animation: [NotePartAnimation; NOTE_ANIM_PART_COUNT],
    pub part_texture_translate: [NotePartTextureTranslate; NOTE_ANIM_PART_COUNT],
}

const NOTE_ANIM_PART_COUNT: usize = 14;

#[derive(Debug, Clone, Copy)]
#[repr(usize)]
pub enum NoteAnimPart {
    Tap = 0,
    Mine,
    Lift,
    Fake,
    HoldHead,
    HoldTopCap,
    HoldBody,
    HoldBottomCap,
    HoldTail,
    RollHead,
    RollTopCap,
    RollBody,
    RollBottomCap,
    RollTail,
}

impl NoteAnimPart {
    const ALL: [Self; NOTE_ANIM_PART_COUNT] = [
        Self::Tap,
        Self::Mine,
        Self::Lift,
        Self::Fake,
        Self::HoldHead,
        Self::HoldTopCap,
        Self::HoldBody,
        Self::HoldBottomCap,
        Self::HoldTail,
        Self::RollHead,
        Self::RollTopCap,
        Self::RollBody,
        Self::RollBottomCap,
        Self::RollTail,
    ];

    const fn metric_prefix(self) -> &'static str {
        match self {
            Self::Tap => "TapNote",
            Self::Mine => "TapMine",
            Self::Lift => "TapLift",
            Self::Fake => "TapFake",
            Self::HoldHead => "HoldHead",
            Self::HoldTopCap => "HoldTopCap",
            Self::HoldBody => "HoldBody",
            Self::HoldBottomCap => "HoldBottomCap",
            Self::HoldTail => "HoldTail",
            Self::RollHead => "RollHead",
            Self::RollTopCap => "RollTopCap",
            Self::RollBody => "RollBody",
            Self::RollBottomCap => "RollBottomCap",
            Self::RollTail => "RollTail",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NotePartAnimation {
    pub length: f32,
    pub vivid: bool,
}

impl Default for NotePartAnimation {
    fn default() -> Self {
        Self {
            length: 1.0,
            vivid: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteColorType {
    Denominator,
    Progress,
    ProgressAlternate,
}

impl NoteColorType {
    fn from_metric(value: &str) -> Option<Self> {
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if value.eq_ignore_ascii_case("Denominator") {
            Some(Self::Denominator)
        } else if value.eq_ignore_ascii_case("Progress") {
            Some(Self::Progress)
        } else if value.eq_ignore_ascii_case("ProgressAlternate") {
            Some(Self::ProgressAlternate)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NotePartTextureTranslate {
    pub addition_offset: [f32; 2],
    pub note_color_spacing: [f32; 2],
    pub note_color_count: i32,
    pub note_color_type: NoteColorType,
}

impl Default for NotePartTextureTranslate {
    fn default() -> Self {
        Self {
            addition_offset: [0.0, 0.0],
            note_color_spacing: [0.0, 0.0],
            note_color_count: 8,
            note_color_type: NoteColorType::Denominator,
        }
    }
}

impl Default for NoteDisplayMetrics {
    fn default() -> Self {
        Self {
            draw_hold_head_for_taps_on_same_row: true,
            draw_roll_head_for_taps_on_same_row: true,
            tap_hold_roll_on_row_means_hold: true,
            hold_head_is_above_wavy_parts: true,
            hold_tail_is_above_wavy_parts: true,
            start_drawing_hold_body_offset_from_head: 0.0,
            stop_drawing_hold_body_offset_from_tail: 0.0,
            hold_let_go_gray_percent: 0.25,
            flip_head_and_tail_when_reverse: false,
            flip_hold_body_when_reverse: false,
            top_hold_anchor_when_reverse: false,
            hold_active_is_add_layer: false,
            part_animation: [NotePartAnimation::default(); NOTE_ANIM_PART_COUNT],
            part_texture_translate: [NotePartTextureTranslate::default(); NOTE_ANIM_PART_COUNT],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub num_cols: usize,
    #[allow(dead_code)]
    pub num_players: usize,
}

#[derive(Debug)]
pub struct Noteskin {
    pub notes: Vec<SpriteSlot>,
    pub note_layers: Vec<Arc<[SpriteSlot]>>,
    pub receptor_off: Vec<SpriteSlot>,
    pub receptor_glow: Vec<Option<SpriteSlot>>,
    pub mines: Vec<Option<SpriteSlot>>,
    pub mine_fill_gradients: Vec<Option<Vec<[f32; 4]>>>,
    pub mine_frames: Vec<Option<SpriteSlot>>,
    pub column_xs: Vec<i32>,
    pub tap_explosions: HashMap<String, TapExplosion>,
    pub receptor_glow_behavior: ReceptorGlowBehavior,
    pub receptor_pulse: ReceptorPulse,
    pub hold_let_go_gray_percent: f32,
    pub hold_columns: Vec<HoldVisuals>,
    pub roll_columns: Vec<HoldVisuals>,
    pub hold: HoldVisuals,
    pub roll: HoldVisuals,
    pub animation_is_beat_based: bool,
    pub note_display_metrics: NoteDisplayMetrics,
}

impl Noteskin {
    #[inline(always)]
    pub fn part_uv_phase(
        &self,
        part: NoteAnimPart,
        song_seconds: f32,
        song_beat: f32,
        note_beat: f32,
    ) -> f32 {
        let anim = self.note_display_metrics.part_animation[part as usize];
        Self::part_uv_phase_inner(
            song_seconds,
            song_beat,
            note_beat,
            anim.length,
            anim.vivid,
            self.animation_is_beat_based,
        )
    }

    #[inline(always)]
    fn part_uv_phase_inner(
        song_seconds: f32,
        song_beat: f32,
        note_beat: f32,
        length: f32,
        vivid: bool,
        beat_based: bool,
    ) -> f32 {
        let length = length.max(1e-6);
        let clock = if beat_based { song_beat } else { song_seconds };
        let mut phase = clock.rem_euclid(length) / length;
        if vivid {
            let note_fraction = note_beat.rem_euclid(1.0);
            let vivid_interval = 1.0 / length;
            let vivid_offset = (note_fraction / vivid_interval).floor() * vivid_interval;
            phase = (phase + vivid_offset).rem_euclid(1.0);
        }
        phase
    }

    #[inline(always)]
    pub fn tap_note_uv_phase(&self, song_seconds: f32, song_beat: f32, note_beat: f32) -> f32 {
        self.part_uv_phase(NoteAnimPart::Tap, song_seconds, song_beat, note_beat)
    }

    #[inline(always)]
    pub fn tap_mine_uv_phase(&self, song_seconds: f32, song_beat: f32, note_beat: f32) -> f32 {
        self.part_uv_phase(NoteAnimPart::Mine, song_seconds, song_beat, note_beat)
    }

    #[inline(always)]
    pub fn part_uv_translation(
        &self,
        part: NoteAnimPart,
        note_beat: f32,
        is_addition: bool,
    ) -> [f32; 2] {
        let metrics = self.note_display_metrics.part_texture_translate[part as usize];
        Self::part_uv_translation_inner(note_beat, metrics, is_addition)
    }

    #[inline(always)]
    fn part_uv_translation_inner(
        note_beat: f32,
        metrics: NotePartTextureTranslate,
        is_addition: bool,
    ) -> [f32; 2] {
        let count = metrics.note_color_count.max(1);
        let countf = count as f32;
        let color = match metrics.note_color_type {
            NoteColorType::Denominator => {
                let note_type = Self::beat_to_note_type_index(note_beat) as f32;
                note_type.clamp(0.0, (count - 1) as f32)
            }
            NoteColorType::Progress => (note_beat * countf).ceil() % countf,
            NoteColorType::ProgressAlternate => {
                let mut scaled = note_beat * countf;
                if scaled - (scaled as i64 as f32) == 0.0 {
                    scaled += countf - 1.0;
                }
                scaled.ceil() % countf
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
    fn beat_to_note_type_index(beat: f32) -> i32 {
        let row = (beat * 48.0).round() as i32;
        if row.rem_euclid(48) == 0 {
            0
        } else if row.rem_euclid(24) == 0 {
            1
        } else if row.rem_euclid(16) == 0 {
            2
        } else if row.rem_euclid(12) == 0 {
            3
        } else if row.rem_euclid(8) == 0 {
            4
        } else if row.rem_euclid(6) == 0 {
            5
        } else if row.rem_euclid(4) == 0 {
            6
        } else if row.rem_euclid(3) == 0 {
            7
        } else {
            8
        }
    }

    #[inline(always)]
    pub fn hold_visuals_for_col(&self, col: usize, is_roll: bool) -> &HoldVisuals {
        if is_roll {
            self.roll_columns
                .get(col)
                .or_else(|| self.roll_columns.first())
                .unwrap_or(&self.roll)
        } else {
            self.hold_columns
                .get(col)
                .or_else(|| self.hold_columns.first())
                .unwrap_or(&self.hold)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReceptorPulse {
    pub effect_color1: [f32; 4],
    pub effect_color2: [f32; 4],
    pub effect_period: f32,
    pub ramp_to_half: f32,
    pub hold_at_half: f32,
    pub ramp_to_full: f32,
    pub hold_at_full: f32,
    pub hold_at_zero: f32,
    pub effect_offset: f32,
}

impl ReceptorPulse {
    fn total_period(&self) -> f32 {
        let mut total = 0.0;
        total += self.ramp_to_half.max(0.0);
        total += self.hold_at_half.max(0.0);
        total += self.ramp_to_full.max(0.0);
        total += self.hold_at_full.max(0.0);
        total += self.hold_at_zero.max(0.0);
        total
    }

    pub fn color_for_beat(&self, beat: f32) -> [f32; 4] {
        let cycle = self.total_period();
        if cycle <= f32::EPSILON {
            return self.effect_color2;
        }
        let period = self.effect_period.max(f32::EPSILON);
        let phase = (beat + self.effect_offset).rem_euclid(period) / period * cycle;

        let ramp_to_half = self.ramp_to_half.max(0.0);
        let hold_at_half = self.hold_at_half.max(0.0);
        let ramp_to_full = self.ramp_to_full.max(0.0);
        let hold_at_full = self.hold_at_full.max(0.0);

        let ramp_and_hold_half = ramp_to_half + hold_at_half;
        let through_ramp_full = ramp_and_hold_half + ramp_to_full;
        let through_hold_full = through_ramp_full + hold_at_full;

        let percent = if ramp_to_half > 0.0 && phase < ramp_to_half {
            (phase / ramp_to_half) * 0.5
        } else if phase < ramp_and_hold_half {
            0.5
        } else if ramp_to_full > 0.0 && phase < through_ramp_full {
            ((phase - ramp_and_hold_half) / ramp_to_full).mul_add(0.5, 0.5)
        } else if phase < through_hold_full {
            1.0
        } else {
            0.0
        };

        let mut color = [0.0; 4];
        for i in 0..4 {
            color[i] =
                self.effect_color1[i].mul_add(percent, self.effect_color2[i] * (1.0 - percent));
        }
        color
    }
}

impl Default for ReceptorPulse {
    fn default() -> Self {
        Self {
            effect_color1: [1.0, 1.0, 1.0, 1.0],
            effect_color2: [1.0, 1.0, 1.0, 1.0],
            effect_period: 1.0,
            ramp_to_half: 0.25,
            hold_at_half: 0.5,
            ramp_to_full: 0.0,
            hold_at_full: 0.0,
            hold_at_zero: 0.25,
            effect_offset: -0.25,
        }
    }
}

fn mine_fill_gradients(mines: &[Option<SpriteSlot>]) -> Vec<Option<Vec<[f32; 4]>>> {
    mines
        .iter()
        .map(|slot| slot.as_ref().and_then(load_mine_gradient_colors))
        .collect()
}

fn load_mine_gradient_colors(slot: &SpriteSlot) -> Option<Vec<[f32; 4]>> {
    let texture_key = slot.texture_key();
    let path = Path::new("assets").join(texture_key);
    let image = assets::open_image_fallback(&path).ok()?.to_rgba8();

    let mut width = slot.def.size[0];
    let mut height = slot.def.size[1];
    if (width <= 0 || height <= 0)
        && let Some(frame) = slot.source.frame_size()
    {
        width = frame[0];
        height = frame[1];
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
        warn!("Mine fill region ({src_x}, {src_y}) is outside of texture {texture_key}");
        return None;
    }

    if src_x + sample_width > image.width() {
        sample_width = image.width().saturating_sub(src_x);
    }
    if src_y + sample_height > image.height() {
        sample_height = image.height().saturating_sub(src_y);
    }

    if sample_width == 0 || sample_height == 0 {
        warn!("Mine fill region has zero sample size for texture {texture_key}");
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
            let a = f32::from(pixel[3]) / 255.0;
            if a <= f32::EPSILON {
                continue;
            }
            r += f32::from(pixel[0]) * a;
            g += f32::from(pixel[1]) * a;
            b += f32::from(pixel[2]) * a;
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
            (c1[0] - c0[0]).mul_add(frac, c0[0]),
            (c1[1] - c0[1]).mul_add(frac, c0[1]),
            (c1[2] - c0[2]).mul_add(frac, c0[2]),
            1.0,
        ];

        sampled[0] = sampled[0].clamp(0.0, 1.0);
        sampled[1] = sampled[1].clamp(0.0, 1.0);
        sampled[2] = sampled[2].clamp(0.0, 1.0);

        samples.push(sampled);
    }

    Some(samples)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ItgSkinCacheKey {
    num_cols: usize,
    num_players: usize,
    skin: String,
}

static ITG_SKIN_CACHE: OnceLock<Mutex<HashMap<ItgSkinCacheKey, Arc<Noteskin>>>> = OnceLock::new();
static CEL_ROLL_RESOLVE_LOGGED: AtomicBool = AtomicBool::new(false);
static DEFAULT_EXPLOSION_DEBUG_LOGGED: AtomicBool = AtomicBool::new(false);

#[inline(always)]
fn itg_skin_cache_key(style: &Style, skin: &str) -> ItgSkinCacheKey {
    let trimmed = skin.trim();
    let normalized = if trimmed.is_empty() {
        "default"
    } else {
        trimmed
    };
    ItgSkinCacheKey {
        num_cols: style.num_cols,
        num_players: style.num_players,
        skin: normalized.to_ascii_lowercase(),
    }
}

pub fn load_itg_skin_cached(style: &Style, skin: &str) -> Result<Arc<Noteskin>, String> {
    let key = itg_skin_cache_key(style, skin);
    let cache = ITG_SKIN_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key)
        .cloned()
    {
        return Ok(cached);
    }

    let loaded = Arc::new(load_itg_skin(style, skin)?);
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = guard.entry(key).or_insert_with(|| loaded.clone());
    Ok(entry.clone())
}

pub fn prewarm_itg_preview_cache() {
    let skins = discover_itg_skins("dance");
    let styles = [
        Style {
            num_cols: 4,
            num_players: 1,
        },
        Style {
            num_cols: 8,
            num_players: 1,
        },
    ];

    for style in styles {
        for skin in &skins {
            if let Err(err) = load_itg_skin_cached(&style, skin) {
                warn!(
                    "noteskin prewarm failed for '{}' ({} columns): {}",
                    skin, style.num_cols, err
                );
            }
        }
    }
}

pub fn load_itg_default(style: &Style) -> Result<Noteskin, String> {
    let root = Path::new("assets/noteskins");
    load_itg(root, "dance", "default", style).or_else(|default_err| {
        warn!("ITG default noteskin load failed ({default_err}); trying dance/cel fallback");
        load_itg(root, "dance", "cel", style).map_err(|cel_err| {
            format!(
                "failed to load ITG default noteskin ({default_err}) and dance/cel fallback ({cel_err})"
            )
        })
    })
}

pub fn discover_itg_skins(game: &str) -> Vec<String> {
    let root = Path::new("assets/noteskins");
    let mut found = Vec::new();
    let game_dir = root.join(game);
    let Ok(entries) = fs::read_dir(&game_dir) else {
        return vec!["default".to_string(), "cel".to_string()];
    };
    let mut seen = HashSet::new();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.is_empty() || name == "common" || name.starts_with('.') {
            continue;
        }
        let dir = entry.path();
        let has_itg_files = dir.join("NoteSkin.lua").is_file()
            || dir.join("metrics.ini").is_file()
            || fs::read_dir(&dir)
                .ok()
                .and_then(|mut it| it.next())
                .is_some();
        if has_itg_files && seen.insert(name.clone()) {
            found.push(name);
        }
    }
    found.sort();
    let mut ordered = Vec::with_capacity(found.len().max(2));
    for preferred in ["default", "cel"] {
        if let Some(pos) = found.iter().position(|s| s == preferred) {
            ordered.push(found.remove(pos));
        }
    }
    ordered.extend(found);
    if ordered.is_empty() {
        vec!["default".to_string(), "cel".to_string()]
    } else {
        ordered
    }
}

pub fn load_itg_skin(style: &Style, skin: &str) -> Result<Noteskin, String> {
    let requested = skin.trim();
    if requested.is_empty() || requested.eq_ignore_ascii_case("default") {
        return load_itg_default(style);
    }

    let root = Path::new("assets/noteskins");
    load_itg(root, "dance", requested, style).or_else(|requested_err| {
        warn!(
            "ITG noteskin '{}' load failed ({}); falling back to dance/default",
            requested, requested_err
        );
        load_itg_default(style).map_err(|fallback_err| {
            format!(
                "failed to load ITG noteskin '{}' ({requested_err}) and fallback dance/default ({fallback_err})",
                requested
            )
        })
    })
}

pub fn load_itg(root: &Path, game: &str, skin: &str, style: &Style) -> Result<Noteskin, String> {
    let data = noteskin_itg::load_noteskin_data(root, game, skin)?;
    load_itg_sprite_noteskin(&data, style)
}

fn load_itg_sprite_noteskin(
    data: &noteskin_itg::NoteskinData,
    style: &Style,
) -> Result<Noteskin, String> {
    let behavior = itg_load_lua_behavior(data);
    let note_display_metrics = itg_note_display_metrics(&data.metrics);
    let animation_is_beat_based = itg_animation_is_beat_based(data);

    let mut notes = Vec::with_capacity(style.num_cols * NUM_QUANTIZATIONS);
    let mut note_layers = Vec::with_capacity(style.num_cols * NUM_QUANTIZATIONS);
    let mut receptor_off = Vec::with_capacity(style.num_cols);
    let mut receptor_glow = Vec::with_capacity(style.num_cols);
    let mut mines = Vec::with_capacity(style.num_cols);
    let mut mine_frames = Vec::with_capacity(style.num_cols);
    let mut hold_columns = Vec::with_capacity(style.num_cols);
    let mut roll_columns = Vec::with_capacity(style.num_cols);
    let resolve_single_slot = |button: &str, element: &str| {
        itg_resolve_actor_sprites(data, &behavior, button, element)
            .into_iter()
            .next()
            .map(|s| s.slot)
            .or_else(|| {
                data.resolve_path(button, element)
                    .and_then(|p| itg_slot_from_path(&p))
            })
    };

    for col in 0..style.num_cols {
        let button = itg_button_for_col(col);
        let mut note_sprites = itg_resolve_actor_sprites(data, &behavior, button, "Tap Note")
            .into_iter()
            .map(|mut s| {
                let (draw, timeline, effect) = itg_model_draw_program(&s.commands);
                s.slot.model_draw = draw;
                s.slot.model_timeline = timeline;
                s.slot.model_effect = effect;
                s.slot
            })
            .collect::<Vec<_>>();
        if note_sprites.len() > 1 {
            note_sprites.sort_by_key(|slot| {
                if slot.model.is_some() {
                    if slot.uv_velocity[0].abs() > f32::EPSILON
                        || slot.uv_velocity[1].abs() > f32::EPSILON
                    {
                        0u8
                    } else {
                        1u8
                    }
                } else {
                    2u8
                }
            });
        }
        if note_sprites.is_empty()
            && let Some(fallback) =
                itg_find_texture_with_prefix(data, "_arrow").and_then(|p| itg_slot_from_path(&p))
        {
            note_sprites.push(fallback);
        }
        let model_color_texture = note_sprites
            .iter()
            .find(|slot| {
                slot.model.is_some()
                    && (slot.uv_velocity[0].abs() > f32::EPSILON
                        || slot.uv_velocity[1].abs() > f32::EPSILON)
            })
            .map(|slot| slot.texture_key().to_string())
            .or_else(|| {
                note_sprites
                    .iter()
                    .find(|slot| slot.model.is_some())
                    .map(|slot| slot.texture_key().to_string())
            });
        let note_base = model_color_texture
            .as_ref()
            .and_then(|key| note_sprites.iter().find(|slot| slot.texture_key() == key))
            .cloned()
            .or_else(|| note_sprites.first().cloned())
            .ok_or_else(|| format!("failed to resolve Tap Note for button '{button}'"))?;
        for _ in 0..NUM_QUANTIZATIONS {
            let layers = note_sprites.clone();
            let primary = layers.first().cloned().unwrap_or_else(|| note_base.clone());
            notes.push(primary);
            note_layers.push(Arc::from(layers));
        }

        let receptor_sprites = itg_resolve_actor_sprites(data, &behavior, button, "Receptor");
        let receptor_slot = receptor_sprites
            .first()
            .map(|s| s.slot.clone())
            .or_else(|| {
                itg_find_texture_with_prefix(data, "_receptor").and_then(|p| itg_slot_from_path(&p))
            })
            .ok_or_else(|| format!("failed to resolve Receptor for button '{button}'"))?;
        let glow_slot = receptor_sprites
            .get(1)
            .map(|s| s.slot.clone())
            .or_else(|| {
                if receptor_sprites.is_empty() {
                    itg_find_texture_with_prefix(data, "_rflash")
                        .and_then(|p| itg_slot_from_path(&p))
                } else {
                    None
                }
            })
            .or_else(|| {
                if receptor_sprites.is_empty() {
                    itg_find_texture_with_prefix(data, "_glow").and_then(|p| itg_slot_from_path(&p))
                } else {
                    None
                }
            });
        receptor_off.push(receptor_slot);
        receptor_glow.push(glow_slot);

        let mut mine_sprites = itg_resolve_actor_sprites(data, &behavior, button, "Tap Mine")
            .into_iter()
            .map(|mut s| {
                let (draw, timeline, effect) = itg_model_draw_program(&s.commands);
                s.slot.model_draw = draw;
                s.slot.model_timeline = timeline;
                s.slot.model_effect = effect;
                s.slot
            })
            .collect::<Vec<_>>();
        let mine_fallback =
            itg_find_texture_with_prefix(data, "_mine").and_then(|p| itg_slot_from_path(&p));
        let mine_fill = mine_sprites
            .first()
            .cloned()
            .or_else(|| mine_sprites.get(1).cloned())
            .or_else(|| mine_fallback.clone());
        let mine_frame = if mine_sprites.len() > 1 {
            mine_sprites.get(1).cloned().or_else(|| mine_sprites.pop())
        } else {
            None
        };
        mines.push(mine_fill);
        mine_frames.push(mine_frame);

        let hold_head_inactive = if behavior.remap_head_to_tap {
            None
        } else {
            resolve_single_slot(button, "Hold Head Inactive")
        };
        let hold_head_active = if behavior.remap_head_to_tap {
            None
        } else {
            resolve_single_slot(button, "Hold Head Active")
        };
        let hold_body_inactive = resolve_single_slot(button, "Hold Body Inactive");
        let hold_body_active = resolve_single_slot(button, "Hold Body Active");
        let hold_bottomcap_inactive = resolve_single_slot(button, "Hold BottomCap Inactive");
        let hold_bottomcap_active = resolve_single_slot(button, "Hold BottomCap Active");

        let hold_visual = HoldVisuals {
            head_inactive: hold_head_inactive.clone(),
            head_active: hold_head_active.or(hold_head_inactive.clone()),
            body_inactive: hold_body_inactive.clone(),
            body_active: hold_body_active.or(hold_body_inactive.clone()),
            bottomcap_inactive: hold_bottomcap_inactive.clone(),
            bottomcap_active: hold_bottomcap_active.or(hold_bottomcap_inactive.clone()),
            explosion: None,
        };

        let roll_head_inactive = if behavior.remap_head_to_tap {
            None
        } else {
            resolve_single_slot(button, "Roll Head Inactive")
        };
        let roll_head_active = if behavior.remap_head_to_tap {
            None
        } else {
            resolve_single_slot(button, "Roll Head Active")
        };
        let roll_body_inactive = resolve_single_slot(button, "Roll Body Inactive");
        let roll_body_active = resolve_single_slot(button, "Roll Body Active");
        let roll_bottomcap_inactive = resolve_single_slot(button, "Roll BottomCap Inactive");
        let roll_bottomcap_active = resolve_single_slot(button, "Roll BottomCap Active");

        let roll_visual = HoldVisuals {
            head_inactive: roll_head_inactive
                .clone()
                .or(hold_visual.head_inactive.clone()),
            head_active: roll_head_active
                .or(roll_head_inactive)
                .or(hold_visual.head_active.clone())
                .or(hold_visual.head_inactive.clone()),
            body_inactive: roll_body_inactive
                .clone()
                .or(hold_visual.body_inactive.clone()),
            body_active: roll_body_active
                .or(roll_body_inactive)
                .or(hold_visual.body_active.clone())
                .or(hold_visual.body_inactive.clone()),
            bottomcap_inactive: roll_bottomcap_inactive
                .clone()
                .or(hold_visual.bottomcap_inactive.clone()),
            bottomcap_active: roll_bottomcap_active
                .or(roll_bottomcap_inactive)
                .or(hold_visual.bottomcap_active.clone())
                .or(hold_visual.bottomcap_inactive.clone()),
            explosion: None,
        };

        hold_columns.push(hold_visual);
        roll_columns.push(roll_visual);
    }
    if data.name.starts_with("ddr-") {
        info!(
            "ddr noteskin '{}': behavior remap_head_to_tap={} remap_tap_fake_to_tap={} keep_hold_non_head_button={}",
            data.name,
            behavior.remap_head_to_tap,
            behavior.remap_tap_fake_to_tap,
            behavior.keep_hold_non_head_button
        );
        for col in 0..style.num_cols {
            let hold = hold_columns.get(col);
            let roll = roll_columns.get(col);
            info!(
                "ddr noteskin '{}': col={} button={} hold_head_inactive={} hold_head_active={} roll_head_inactive={} roll_head_active={}",
                data.name,
                col,
                itg_button_for_col(col),
                hold.and_then(|v| v.head_inactive.as_ref())
                    .map(|slot| slot.texture_key())
                    .unwrap_or("<none>"),
                hold.and_then(|v| v.head_active.as_ref())
                    .map(|slot| slot.texture_key())
                    .unwrap_or("<none>"),
                roll.and_then(|v| v.head_inactive.as_ref())
                    .map(|slot| slot.texture_key())
                    .unwrap_or("<none>"),
                roll.and_then(|v| v.head_active.as_ref())
                    .map(|slot| slot.texture_key())
                    .unwrap_or("<none>"),
            );
        }
    }

    let down_col = (0..style.num_cols)
        .find(|&col| itg_button_for_col(col).eq_ignore_ascii_case("Down"))
        .unwrap_or(0);
    let mut hold = hold_columns
        .get(down_col)
        .cloned()
        .or_else(|| hold_columns.first().cloned())
        .unwrap_or_default();
    let mut roll = roll_columns
        .get(down_col)
        .cloned()
        .or_else(|| roll_columns.first().cloned())
        .unwrap_or_else(|| HoldVisuals {
            head_inactive: hold.head_inactive.clone(),
            head_active: hold.head_active.clone(),
            body_inactive: hold.body_inactive.clone(),
            body_active: hold.body_active.clone(),
            bottomcap_inactive: hold.bottomcap_inactive.clone(),
            bottomcap_active: hold.bottomcap_active.clone(),
            explosion: None,
        });

    let explosion_sprites = itg_resolve_actor_sprites(data, &behavior, "Down", "Explosion");
    let dim_sprites = explosion_sprites
        .iter()
        .filter(|s| {
            s.element
                .to_ascii_lowercase()
                .starts_with("tap explosion dim")
        })
        .collect::<Vec<_>>();
    let bright_sprites = explosion_sprites
        .iter()
        .filter(|s| {
            s.element
                .to_ascii_lowercase()
                .starts_with("tap explosion bright")
        })
        .collect::<Vec<_>>();
    let slot_with_active_cmd =
        |slot: &SpriteSlot, commands: &HashMap<String, String>, active_key: &str| {
            let mut with_fx = slot.clone();
            let mut scripted = HashMap::new();
            if let Some(v) = commands.get("initcommand") {
                scripted.insert("initcommand".to_string(), v.clone());
            }
            if let Some(v) = commands.get(active_key) {
                // Reuse the existing actor program parser by mapping the active hold command
                // to the steady-state command slot.
                scripted.insert("nonecommand".to_string(), v.clone());
            }
            let (draw, timeline, effect) = itg_model_draw_program(&scripted);
            with_fx.model_draw = draw;
            with_fx.model_timeline = timeline;
            with_fx.model_effect = effect;
            with_fx
        };

    let find_explosion_wrapper = |active_key: &str, element_hint: &str| {
        explosion_sprites
            .iter()
            .find(|sprite| sprite.commands.contains_key(active_key))
            .or_else(|| {
                explosion_sprites
                    .iter()
                    .find(|sprite| sprite.element.to_ascii_lowercase().contains(element_hint))
            })
    };
    let hold_wrapper = find_explosion_wrapper("holdingoncommand", "hold explosion");
    let roll_wrapper = find_explosion_wrapper("rolloncommand", "roll explosion");

    let hold_explosion_blank = behavior.blank.contains("hold explosion");
    let roll_explosion_blank = behavior.blank.contains("roll explosion");
    let hold_explosion_sprites =
        itg_resolve_actor_sprites(data, &behavior, "Down", "Hold Explosion");
    let hold_source = hold_explosion_sprites
        .iter()
        .find(|sprite| sprite.commands.contains_key("holdingoncommand"))
        .or_else(|| hold_explosion_sprites.first());
    let hold_wrapper_source =
        hold_wrapper.filter(|sprite| sprite.commands.contains_key("holdingoncommand"));
    hold.explosion = if hold_explosion_blank {
        None
    } else {
        hold_wrapper_source
            .map(|sprite| slot_with_active_cmd(&sprite.slot, &sprite.commands, "holdingoncommand"))
            .or_else(|| {
                hold_source.map(|sprite| {
                    let cmd = hold_wrapper.map_or(&sprite.commands, |wrapped| &wrapped.commands);
                    slot_with_active_cmd(&sprite.slot, cmd, "holdingoncommand")
                })
            })
            .or_else(|| {
                hold_wrapper.map(|s| slot_with_active_cmd(&s.slot, &s.commands, "holdingoncommand"))
            })
            .or_else(|| {
                data.resolve_path("Down", "Hold Explosion")
                    .and_then(|p| itg_slot_from_path(&p))
            })
            .or_else(|| {
                data.resolve_path("Down", "Hold Explosion")
                    .and_then(|p| itg_slot_from_actor_path_first_sprite(data, &p))
                    .map(|slot| {
                        hold_wrapper.map_or(slot.clone(), |wrapped| {
                            slot_with_active_cmd(&slot, &wrapped.commands, "holdingoncommand")
                        })
                    })
            })
            .or_else(|| {
                itg_find_texture_with_prefix(data, "_down hold explosion")
                    .and_then(|p| {
                        itg_slot_from_path_all_frames(
                            &p,
                            Some(0.01),
                            itg_animation_is_beat_based(data),
                        )
                    })
                    .map(|slot| {
                        hold_wrapper.map_or(slot.clone(), |wrapped| {
                            slot_with_active_cmd(&slot, &wrapped.commands, "holdingoncommand")
                        })
                    })
            })
    };
    let roll_explosion_sprites =
        itg_resolve_actor_sprites(data, &behavior, "Down", "Roll Explosion");
    let roll_source = roll_explosion_sprites
        .iter()
        .find(|sprite| sprite.commands.contains_key("rolloncommand"))
        .or_else(|| roll_explosion_sprites.first());
    let roll_wrapper_source =
        roll_wrapper.filter(|sprite| sprite.commands.contains_key("rolloncommand"));
    let roll_explosion = if roll_explosion_blank {
        None
    } else {
        roll_wrapper_source
            .map(|sprite| slot_with_active_cmd(&sprite.slot, &sprite.commands, "rolloncommand"))
            .or_else(|| {
                roll_source.map(|sprite| {
                    let cmd = roll_wrapper.map_or(&sprite.commands, |wrapped| &wrapped.commands);
                    slot_with_active_cmd(&sprite.slot, cmd, "rolloncommand")
                })
            })
            .or_else(|| {
                roll_wrapper.map(|s| slot_with_active_cmd(&s.slot, &s.commands, "rolloncommand"))
            })
            .or_else(|| {
                data.resolve_path("Down", "Roll Explosion")
                    .and_then(|p| itg_slot_from_path(&p))
            })
            .or_else(|| {
                data.resolve_path("Down", "Roll Explosion")
                    .and_then(|p| itg_slot_from_actor_path_first_sprite(data, &p))
                    .map(|slot| {
                        roll_wrapper.map_or(slot.clone(), |wrapped| {
                            slot_with_active_cmd(&slot, &wrapped.commands, "rolloncommand")
                        })
                    })
            })
            .or_else(|| {
                itg_find_texture_with_prefix(data, "_down hold explosion")
                    .and_then(|p| {
                        itg_slot_from_path_all_frames(
                            &p,
                            Some(0.01),
                            itg_animation_is_beat_based(data),
                        )
                    })
                    .map(|slot| {
                        roll_wrapper.map_or(slot.clone(), |wrapped| {
                            slot_with_active_cmd(&slot, &wrapped.commands, "rolloncommand")
                        })
                    })
            })
    };
    roll.explosion = if roll_explosion_blank {
        None
    } else {
        roll_explosion.or(hold.explosion.clone())
    };
    if let (Some(roll_slot), Some(hold_slot)) = (roll.explosion.clone(), hold.explosion.clone()) {
        let roll_key = roll_slot.texture_key().to_ascii_lowercase();
        let hold_key = hold_slot.texture_key().to_ascii_lowercase();
        let roll_is_common_fallback_hold =
            roll_key.contains("noteskins/common/common/fallback hold explosion");
        let hold_is_skin_specific = !hold_key.contains("noteskins/common/common/");
        if roll_is_common_fallback_hold && hold_is_skin_specific {
            let roll_commands = roll_wrapper
                .filter(|sprite| sprite.commands.contains_key("rolloncommand"))
                .map(|sprite| sprite.commands.clone())
                .or_else(|| {
                    let mut metrics_commands = HashMap::new();
                    if let Some(v) = data.metrics.get("HoldGhostArrow", "RollOnCommand") {
                        metrics_commands.insert("rolloncommand".to_string(), v.to_string());
                    }
                    if let Some(v) = data.metrics.get("HoldGhostArrow", "RollOffCommand") {
                        metrics_commands.insert("rolloffcommand".to_string(), v.to_string());
                    }
                    if metrics_commands.is_empty() {
                        None
                    } else {
                        Some(metrics_commands)
                    }
                });
            if let Some(commands) = roll_commands {
                roll.explosion = Some(slot_with_active_cmd(&hold_slot, &commands, "rolloncommand"));
            } else {
                roll.explosion = Some(hold_slot);
            }
        }
    }
    for visuals in &mut hold_columns {
        visuals.explosion = hold.explosion.clone();
    }
    for visuals in &mut roll_columns {
        visuals.explosion = roll.explosion.clone();
    }
    if data.name.eq_ignore_ascii_case("cel")
        && !CEL_ROLL_RESOLVE_LOGGED.swap(true, Ordering::Relaxed)
    {
        let hold_direct = data
            .resolve_path("Down", "Hold Explosion")
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let roll_direct = data
            .resolve_path("Down", "Roll Explosion")
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let search_dirs = data
            .search_dirs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>();
        let wrappers = explosion_sprites
            .iter()
            .filter(|sprite| {
                sprite.commands.contains_key("holdingoncommand")
                    || sprite.commands.contains_key("rolloncommand")
            })
            .map(|sprite| {
                format!(
                    "{}=>{} keys={}",
                    sprite.element,
                    sprite.slot.texture_key(),
                    sprite
                        .commands
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .collect::<Vec<_>>();
        info!(
            "cel roll resolve debug: search_dirs={:?} down_hold_path={} down_roll_path={} hold_wrapper_tex={} roll_wrapper_tex={} hold_final_tex={} roll_final_tex={} wrappers={:?}",
            search_dirs,
            hold_direct,
            roll_direct,
            hold_wrapper
                .map(|s| s.slot.texture_key())
                .unwrap_or("<none>"),
            roll_wrapper
                .map(|s| s.slot.texture_key())
                .unwrap_or("<none>"),
            hold.explosion
                .as_ref()
                .map(|s| s.texture_key())
                .unwrap_or("<none>"),
            roll.explosion
                .as_ref()
                .map(|s| s.texture_key())
                .unwrap_or("<none>"),
            wrappers,
        );
    }
    let explosion_slot = dim_sprites
        .first()
        .map(|s| s.slot.clone())
        .or_else(|| bright_sprites.first().map(|s| s.slot.clone()))
        .or_else(|| {
            data.resolve_path("Down", "Tap Explosion Dim")
                .and_then(|p| itg_slot_from_path(&p))
        })
        .or_else(|| {
            data.resolve_path("Down", "Tap Explosion Bright")
                .and_then(|p| itg_slot_from_path(&p))
        });

    let mut tap_explosions = HashMap::new();
    if let Some(slot) = explosion_slot {
        let select_tap_explosion_source = |window: &str| {
            let key = format!("{}command", window.to_ascii_lowercase());
            dim_sprites
                .iter()
                .find(|sprite| {
                    let sprite = **sprite;
                    sprite.commands.contains_key(&key)
                })
                .copied()
                .or_else(|| {
                    bright_sprites
                        .iter()
                        .find(|sprite| {
                            let sprite = **sprite;
                            sprite.commands.contains_key(&key)
                        })
                        .copied()
                })
                .or_else(|| dim_sprites.first().copied())
                .or_else(|| bright_sprites.first().copied())
        };

        for window in ["W1", "W2", "W3", "W4", "W5"] {
            let key = format!("{}command", window.to_ascii_lowercase());
            let source = select_tap_explosion_source(window);
            let command = source
                .and_then(|s| s.commands.get(&key))
                .cloned()
                .or_else(|| {
                    let metric_key = format!("{window}Command");
                    data.metrics
                        .get("GhostArrowDim", &metric_key)
                        .or_else(|| data.metrics.get("GhostArrowBright", &metric_key))
                        .map(str::to_string)
                });
            let command_with_init = command.and_then(|cmd| {
                if cmd.trim().is_empty() {
                    return None;
                }
                let mut sequence = Vec::with_capacity(4);
                let mut push_command = |raw: Option<&String>| {
                    if let Some(value) = raw {
                        let trimmed = value.trim();
                        if !trimmed.is_empty() {
                            sequence.push(trimmed.to_string());
                        }
                    }
                };
                push_command(source.and_then(|s| s.commands.get("initcommand")));
                push_command(source.and_then(|s| s.commands.get("judgmentcommand")));
                let mode_command = source.and_then(|s| {
                    if dim_sprites.iter().any(|d| std::ptr::eq(*d, s)) {
                        s.commands.get("dimcommand")
                    } else {
                        s.commands.get("brightcommand")
                    }
                });
                push_command(mode_command);
                sequence.push(cmd);
                if sequence.is_empty() {
                    None
                } else {
                    Some(sequence.join(";"))
                }
            });
            let animation = command_with_init
                .as_deref()
                .filter(|v| !v.trim().is_empty())
                .map(parse_explosion_animation)
                .unwrap_or_default();
            let slot_for_window = source
                .map(|s| s.slot.clone())
                .unwrap_or_else(|| slot.clone());
            tap_explosions.insert(
                window.to_string(),
                TapExplosion {
                    slot: slot_for_window,
                    animation,
                },
            );
        }
    }

    let hold_let_go_gray_percent = note_display_metrics
        .hold_let_go_gray_percent
        .clamp(0.0, 1.0);

    let receptor_glow_behavior = itg_receptor_glow_behavior(data, &behavior);
    let receptor_pulse = itg_receptor_pulse(&data.metrics);
    let mine_fill_gradients = mine_fill_gradients(&mines);
    let column_xs = if style.num_cols == 0 {
        Vec::new()
    } else {
        (0..style.num_cols)
            .map(|i| (i as i32 * 68) - ((style.num_cols - 1) as i32 * 34))
            .collect()
    };
    if data.name.eq_ignore_ascii_case("default")
        && !DEFAULT_EXPLOSION_DEBUG_LOGGED.swap(true, Ordering::Relaxed)
    {
        let bright_path = data
            .resolve_path("Down", "Tap Explosion Bright")
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let dim_path = data
            .resolve_path("Down", "Tap Explosion Dim")
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let hold_path = data
            .resolve_path("Down", "Hold Explosion")
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let roll_path = data
            .resolve_path("Down", "Roll Explosion")
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let hold_tex = hold
            .explosion
            .as_ref()
            .map(|slot| {
                format!(
                    "{} frames={} size={:?} rot={} effect={:?}",
                    slot.texture_key(),
                    slot.source.frame_count(),
                    slot.logical_size(),
                    slot.def.rotation_deg,
                    slot.model_effect.mode
                )
            })
            .unwrap_or_else(|| "<none>".to_string());
        let roll_tex = roll
            .explosion
            .as_ref()
            .map(|slot| {
                format!(
                    "{} frames={} size={:?} rot={} effect={:?}",
                    slot.texture_key(),
                    slot.source.frame_count(),
                    slot.logical_size(),
                    slot.def.rotation_deg,
                    slot.model_effect.mode
                )
            })
            .unwrap_or_else(|| "<none>".to_string());
        let windows = ["W1", "W2", "W3", "W4", "W5"];
        let tap_windows = windows
            .iter()
            .map(|window| {
                tap_explosions.get(*window).map_or_else(
                    || format!("{window}:<none>"),
                    |explosion| {
                        format!(
                            "{window}: tex={} frames={} size={:?} init_visible={} init_alpha={:.3} has_glow={}",
                            explosion.slot.texture_key(),
                            explosion.slot.source.frame_count(),
                            explosion.slot.logical_size(),
                            explosion.animation.initial.visible,
                            explosion.animation.initial.color[3],
                            explosion.animation.glow.is_some()
                        )
                    },
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        info!(
            "default explosion parse debug: down_bright_path={} down_dim_path={} down_hold_path={} down_roll_path={} hold_explosion={} roll_explosion={} tap_windows={}",
            bright_path, dim_path, hold_path, roll_path, hold_tex, roll_tex, tap_windows
        );
    }

    Ok(Noteskin {
        notes,
        note_layers,
        receptor_off,
        receptor_glow,
        mines,
        mine_fill_gradients,
        mine_frames,
        column_xs,
        tap_explosions,
        receptor_glow_behavior,
        receptor_pulse,
        hold_let_go_gray_percent,
        hold_columns,
        roll_columns,
        hold,
        roll,
        animation_is_beat_based,
        note_display_metrics,
    })
}

fn itg_note_display_metrics(metrics: &noteskin_itg::IniData) -> NoteDisplayMetrics {
    let mut out = NoteDisplayMetrics::default();
    let read_bool = |key: &str, default: bool| {
        metrics
            .get("NoteDisplay", key)
            .and_then(itg_parse_ini_int)
            .map_or(default, |v| v != 0)
    };
    let read_float = |key: &str, default: f32| {
        metrics
            .get("NoteDisplay", key)
            .and_then(itg_parse_ini_float)
            .unwrap_or(default)
    };
    let read_int = |key: &str, default: i32| {
        metrics
            .get("NoteDisplay", key)
            .and_then(itg_parse_ini_int)
            .unwrap_or(default)
    };

    out.draw_hold_head_for_taps_on_same_row = read_bool(
        "DrawHoldHeadForTapsOnSameRow",
        out.draw_hold_head_for_taps_on_same_row,
    );
    out.draw_roll_head_for_taps_on_same_row = read_bool(
        "DrawRollHeadForTapsOnSameRow",
        out.draw_roll_head_for_taps_on_same_row,
    );
    out.tap_hold_roll_on_row_means_hold = read_bool(
        "TapHoldRollOnRowMeansHold",
        out.tap_hold_roll_on_row_means_hold,
    );
    out.hold_head_is_above_wavy_parts = read_bool(
        "HoldHeadIsAboveWavyParts",
        out.hold_head_is_above_wavy_parts,
    );
    out.hold_tail_is_above_wavy_parts = read_bool(
        "HoldTailIsAboveWavyParts",
        out.hold_tail_is_above_wavy_parts,
    );
    out.start_drawing_hold_body_offset_from_head = read_float(
        "StartDrawingHoldBodyOffsetFromHead",
        out.start_drawing_hold_body_offset_from_head,
    );
    out.stop_drawing_hold_body_offset_from_tail = read_float(
        "StopDrawingHoldBodyOffsetFromTail",
        out.stop_drawing_hold_body_offset_from_tail,
    );
    out.hold_let_go_gray_percent = read_float("HoldLetGoGrayPercent", out.hold_let_go_gray_percent);
    out.flip_head_and_tail_when_reverse = read_bool(
        "FlipHeadAndTailWhenReverse",
        out.flip_head_and_tail_when_reverse,
    );
    out.flip_hold_body_when_reverse =
        read_bool("FlipHoldBodyWhenReverse", out.flip_hold_body_when_reverse);
    out.top_hold_anchor_when_reverse =
        read_bool("TopHoldAnchorWhenReverse", out.top_hold_anchor_when_reverse);
    out.hold_active_is_add_layer = read_bool("HoldActiveIsAddLayer", out.hold_active_is_add_layer);
    for part in NoteAnimPart::ALL {
        let prefix = part.metric_prefix();
        let length_key = format!("{prefix}AnimationLength");
        let vivid_key = format!("{prefix}AnimationIsVivid");
        let add_x_key = format!("{prefix}AdditionTextureCoordOffsetX");
        let add_y_key = format!("{prefix}AdditionTextureCoordOffsetY");
        let spacing_x_key = format!("{prefix}NoteColorTextureCoordSpacingX");
        let spacing_y_key = format!("{prefix}NoteColorTextureCoordSpacingY");
        let count_key = format!("{prefix}NoteColorCount");
        let color_type_key = format!("{prefix}NoteColorType");
        let default_anim = out.part_animation[part as usize];
        let length = read_float(&length_key, default_anim.length).abs().max(1e-6);
        let vivid = read_bool(&vivid_key, default_anim.vivid);
        out.part_animation[part as usize] = NotePartAnimation { length, vivid };
        let default_translate = out.part_texture_translate[part as usize];
        let addition_offset = [
            read_float(&add_x_key, default_translate.addition_offset[0]),
            read_float(&add_y_key, default_translate.addition_offset[1]),
        ];
        let note_color_spacing = [
            read_float(&spacing_x_key, default_translate.note_color_spacing[0]),
            read_float(&spacing_y_key, default_translate.note_color_spacing[1]),
        ];
        let note_color_count = read_int(&count_key, default_translate.note_color_count);
        let note_color_type = metrics
            .get("NoteDisplay", &color_type_key)
            .and_then(NoteColorType::from_metric)
            .unwrap_or(default_translate.note_color_type);
        out.part_texture_translate[part as usize] = NotePartTextureTranslate {
            addition_offset,
            note_color_spacing,
            note_color_count,
            note_color_type,
        };
    }
    out
}

fn itg_receptor_pulse(metrics: &noteskin_itg::IniData) -> ReceptorPulse {
    let mut pulse = ReceptorPulse::default();
    let Some(command) = metrics.get("ReceptorArrow", "InitCommand") else {
        return pulse;
    };

    for raw_token in command.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((cmd, raw_args)) = token.split_once(',') else {
            continue;
        };
        match cmd.trim().to_ascii_lowercase().as_str() {
            "effectcolor1" => {
                if let Some(color) = itg_parse_color(raw_args) {
                    pulse.effect_color1 = color;
                }
            }
            "effectcolor2" => {
                if let Some(color) = itg_parse_color(raw_args) {
                    pulse.effect_color2 = color;
                }
            }
            "effecttiming" => {
                let values = itg_parse_f32_list(raw_args);
                if values.len() >= 4 {
                    pulse.ramp_to_half = values[0].max(0.0);
                    pulse.hold_at_half = values[1].max(0.0);
                    pulse.ramp_to_full = values[2].max(0.0);
                    pulse.hold_at_full = values[3].max(0.0);
                    pulse.hold_at_zero = values.get(4).copied().unwrap_or(0.0).max(0.0);
                }
            }
            "effectperiod" => {
                if let Ok(v) = raw_args
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .parse::<f32>()
                {
                    pulse.effect_period = v.max(f32::EPSILON);
                }
            }
            "effectoffset" => {
                if let Ok(v) = raw_args
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .parse::<f32>()
                {
                    pulse.effect_offset = v;
                }
            }
            _ => {}
        }
    }

    pulse
}

#[derive(Debug, Clone, Copy)]
struct ItgCommandEffect {
    start_alpha: Option<f32>,
    target_alpha: Option<f32>,
    start_zoom: Option<f32>,
    target_zoom: Option<f32>,
    duration: f32,
    tween: TweenType,
    blend_add: Option<bool>,
}

impl Default for ItgCommandEffect {
    fn default() -> Self {
        Self {
            start_alpha: None,
            target_alpha: None,
            start_zoom: None,
            target_zoom: None,
            duration: 0.0,
            tween: TweenType::Linear,
            blend_add: None,
        }
    }
}

fn itg_receptor_glow_behavior(
    data: &noteskin_itg::NoteskinData,
    behavior: &ItgLuaBehavior,
) -> ReceptorGlowBehavior {
    let mut out = ReceptorGlowBehavior::default();
    let receptor = itg_resolve_actor_sprites(data, behavior, "Down", "Receptor");
    let overlay = receptor.get(1);
    let init_cmd = overlay
        .and_then(|s| s.commands.get("initcommand"))
        .cloned()
        .or_else(|| {
            data.metrics
                .get("ReceptorOverlay", "InitCommand")
                .map(str::to_string)
        })
        .unwrap_or_default();
    let press_cmd = overlay
        .and_then(|s| s.commands.get("presscommand"))
        .cloned()
        .or_else(|| {
            data.metrics
                .get("ReceptorOverlay", "PressCommand")
                .map(str::to_string)
        })
        .unwrap_or_default();
    let lift_cmd = overlay
        .and_then(|s| s.commands.get("liftcommand"))
        .cloned()
        .or_else(|| {
            data.metrics
                .get("ReceptorOverlay", "LiftCommand")
                .map(str::to_string)
        })
        .unwrap_or_default();
    let none_cmd = overlay
        .and_then(|s| s.commands.get("nonecommand"))
        .cloned()
        .or_else(|| {
            data.metrics
                .get("ReceptorOverlay", "NoneCommand")
                .map(str::to_string)
        })
        .unwrap_or_default();

    let init = itg_parse_command_effect(&init_cmd);
    let press = itg_parse_command_effect(&press_cmd);
    let lift = itg_parse_command_effect(&lift_cmd);
    let none = itg_parse_command_effect(&none_cmd);

    out.duration = if lift.duration > f32::EPSILON {
        lift.duration
    } else if none.duration > f32::EPSILON {
        none.duration
    } else if press.duration > f32::EPSILON {
        press.duration
    } else {
        out.duration
    };
    out.alpha_start = press
        .start_alpha
        .or(press.target_alpha)
        .or(init.target_alpha)
        .unwrap_or(out.alpha_start);
    out.alpha_end = lift
        .target_alpha
        .or(none.target_alpha)
        .or(init.target_alpha)
        .unwrap_or(0.0);
    out.zoom_start = press
        .start_zoom
        .or(press.target_zoom)
        .or(init.target_zoom)
        .unwrap_or(out.zoom_start);
    out.zoom_end = lift
        .target_zoom
        .or(none.target_zoom)
        .or(init.target_zoom)
        .unwrap_or(out.zoom_end);
    out.tween = if lift.duration > f32::EPSILON {
        lift.tween
    } else if none.duration > f32::EPSILON {
        none.tween
    } else if press.duration > f32::EPSILON {
        press.tween
    } else {
        out.tween
    };
    out.blend_add = press
        .blend_add
        .or(lift.blend_add)
        .or(init.blend_add)
        .unwrap_or(out.blend_add);
    out.alpha_start = out.alpha_start.clamp(0.0, 1.0);
    out.alpha_end = out.alpha_end.clamp(0.0, 1.0);
    out.zoom_start = out.zoom_start.max(0.0);
    out.zoom_end = out.zoom_end.max(0.0);
    out.duration = out.duration.max(0.0);
    out
}

fn itg_parse_command_effect(script: &str) -> ItgCommandEffect {
    let mut out = ItgCommandEffect::default();
    let mut pending_duration = 0.0f32;
    let mut pending_tween = TweenType::Linear;
    for raw in script.split(';') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let Some((cmd, args)) = itg_split_command_token(token) else {
            continue;
        };
        match cmd.as_str() {
            "linear" | "accelerate" | "decelerate" => {
                if let Some(arg) = args.first()
                    && let Some(duration) = itg_parse_numeric_token(arg)
                {
                    pending_duration = duration.max(0.0);
                    pending_tween = match cmd.as_str() {
                        "accelerate" => TweenType::Accelerate,
                        "decelerate" => TweenType::Decelerate,
                        _ => TweenType::Linear,
                    };
                }
            }
            "sleep" => {
                if let Some(arg) = args.first()
                    && let Some(duration) = itg_parse_numeric_token(arg)
                {
                    pending_duration = duration.max(0.0);
                    pending_tween = TweenType::Linear;
                }
            }
            "diffusealpha" => {
                if let Some(arg) = args.first()
                    && let Some(alpha) = itg_parse_numeric_token(arg)
                {
                    if pending_duration > f32::EPSILON {
                        out.target_alpha = Some(alpha);
                        out.duration = pending_duration;
                        out.tween = pending_tween;
                        pending_duration = 0.0;
                    } else {
                        out.start_alpha = Some(alpha);
                        out.target_alpha = Some(alpha);
                    }
                }
            }
            "zoom" => {
                if let Some(arg) = args.first()
                    && let Some(zoom) = itg_parse_numeric_token(arg)
                {
                    if pending_duration > f32::EPSILON {
                        out.target_zoom = Some(zoom);
                        out.duration = pending_duration;
                        out.tween = pending_tween;
                        pending_duration = 0.0;
                    } else {
                        out.start_zoom = Some(zoom);
                        out.target_zoom = Some(zoom);
                    }
                }
            }
            "blend" => {
                if args
                    .iter()
                    .any(|a| a.to_ascii_lowercase().contains("blendmode_add"))
                {
                    out.blend_add = Some(true);
                } else if !args.is_empty() {
                    out.blend_add = Some(false);
                }
            }
            _ => {}
        }
    }
    out
}

#[derive(Debug, Clone, Copy)]
enum ItgActorMod {
    X(f32),
    Y(f32),
    Z(f32),
    AddX(f32),
    AddY(f32),
    AddZ(f32),
    RotationX(f32),
    RotationY(f32),
    RotationZ(f32),
    AddRotationX(f32),
    AddRotationY(f32),
    AddRotationZ(f32),
    Zoom(f32),
    ZoomX(f32),
    ZoomY(f32),
    ZoomZ(f32),
    Diffuse([f32; 4]),
    DiffuseAlpha(f32),
    Glow([f32; 4]),
    VertAlign(f32),
    BlendAdd(bool),
    Visible(bool),
}

fn itg_parse_vertalign_token(token: &str) -> Option<f32> {
    let value = token.trim().trim_matches('"').trim_matches('\'');
    if let Ok(v) = value.parse::<f32>() {
        return Some(v);
    }
    match value.to_ascii_lowercase().as_str() {
        "top" => Some(0.0),
        "middle" | "center" => Some(0.5),
        "bottom" => Some(1.0),
        _ => None,
    }
}

fn itg_parse_actor_mod_token(cmd: &str, args: &[&str]) -> Option<ItgActorMod> {
    let first = args.first().and_then(|v| {
        v.trim()
            .trim_matches('"')
            .trim_matches('\'')
            .parse::<f32>()
            .ok()
    });
    let bool_first = args.first().map(|v| {
        let t = v.trim().trim_matches('"').trim_matches('\'');
        t.eq_ignore_ascii_case("true") || t == "1"
    });

    match cmd {
        "x" => first.map(ItgActorMod::X),
        "y" => first.map(ItgActorMod::Y),
        "z" => first.map(ItgActorMod::Z),
        "addx" => first.map(ItgActorMod::AddX),
        "addy" => first.map(ItgActorMod::AddY),
        "addz" => first.map(ItgActorMod::AddZ),
        "rotationx" => first.map(ItgActorMod::RotationX),
        "rotationy" => first.map(ItgActorMod::RotationY),
        "rotationz" => first.map(ItgActorMod::RotationZ),
        "addrotationx" => first.map(ItgActorMod::AddRotationX),
        "addrotationy" => first.map(ItgActorMod::AddRotationY),
        "addrotationz" => first.map(ItgActorMod::AddRotationZ),
        "zoom" => first.map(ItgActorMod::Zoom),
        "zoomx" => first.map(ItgActorMod::ZoomX),
        "zoomy" => first.map(ItgActorMod::ZoomY),
        "zoomz" => first.map(ItgActorMod::ZoomZ),
        "diffuse" => {
            if args.len() >= 4 {
                let parsed = args
                    .iter()
                    .take(4)
                    .map(|v| {
                        v.trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .parse::<f32>()
                            .ok()
                    })
                    .collect::<Option<Vec<f32>>>();
                if let Some(vals) = parsed {
                    return Some(ItgActorMod::Diffuse([vals[0], vals[1], vals[2], vals[3]]));
                }
            }
            args.first()
                .and_then(|v| itg_parse_color(v))
                .map(ItgActorMod::Diffuse)
        }
        "diffusealpha" => first.map(ItgActorMod::DiffuseAlpha),
        "glow" => {
            if args.len() >= 4 {
                let parsed = args
                    .iter()
                    .take(4)
                    .map(|v| {
                        v.trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .parse::<f32>()
                            .ok()
                    })
                    .collect::<Option<Vec<f32>>>();
                if let Some(vals) = parsed {
                    return Some(ItgActorMod::Glow([vals[0], vals[1], vals[2], vals[3]]));
                }
            }
            args.first()
                .and_then(|v| itg_parse_color(v))
                .map(ItgActorMod::Glow)
        }
        "vertalign" | "valign" => args
            .first()
            .and_then(|v| itg_parse_vertalign_token(v))
            .map(ItgActorMod::VertAlign),
        "blend" => {
            if args
                .iter()
                .any(|a| a.to_ascii_lowercase().contains("blendmode_add"))
            {
                Some(ItgActorMod::BlendAdd(true))
            } else if !args.is_empty() {
                Some(ItgActorMod::BlendAdd(false))
            } else {
                None
            }
        }
        "visible" => bool_first.map(ItgActorMod::Visible),
        _ => None,
    }
}

fn itg_apply_actor_mods(state: &mut ModelDrawState, mods: &[ItgActorMod]) {
    for m in mods {
        match *m {
            ItgActorMod::X(v) => state.pos[0] = v,
            ItgActorMod::Y(v) => state.pos[1] = v,
            ItgActorMod::Z(v) => state.pos[2] = v,
            ItgActorMod::AddX(v) => state.pos[0] += v,
            ItgActorMod::AddY(v) => state.pos[1] += v,
            ItgActorMod::AddZ(v) => state.pos[2] += v,
            ItgActorMod::RotationX(v) => state.rot[0] = v,
            ItgActorMod::RotationY(v) => state.rot[1] = v,
            ItgActorMod::RotationZ(v) => state.rot[2] = v,
            ItgActorMod::AddRotationX(v) => state.rot[0] += v,
            ItgActorMod::AddRotationY(v) => state.rot[1] += v,
            ItgActorMod::AddRotationZ(v) => state.rot[2] += v,
            ItgActorMod::Zoom(v) => state.zoom = [v, v, v],
            ItgActorMod::ZoomX(v) => state.zoom[0] = v,
            ItgActorMod::ZoomY(v) => state.zoom[1] = v,
            ItgActorMod::ZoomZ(v) => state.zoom[2] = v,
            ItgActorMod::Diffuse(v) => state.tint = v,
            ItgActorMod::DiffuseAlpha(v) => state.tint[3] = v,
            ItgActorMod::Glow(v) => state.glow = v,
            ItgActorMod::VertAlign(v) => state.vert_align = v,
            ItgActorMod::BlendAdd(v) => state.blend_add = v,
            ItgActorMod::Visible(v) => state.visible = v,
        }
    }
}

fn itg_model_draw_program(
    commands: &HashMap<String, String>,
) -> (ModelDrawState, Arc<[ModelTweenSegment]>, ModelEffectState) {
    let mut state = ModelDrawState::default();
    let mut effect = ModelEffectState::default();
    let mut timeline: Vec<ModelTweenSegment> = Vec::new();
    let mut cursor_time = 0.0f32;
    let mut pending_tween: Option<(f32, TweenType)> = None;
    let mut grouped_mods: Vec<ItgActorMod> = Vec::new();

    let flush_group = |state: &mut ModelDrawState,
                       timeline: &mut Vec<ModelTweenSegment>,
                       cursor_time: &mut f32,
                       pending_tween: &mut Option<(f32, TweenType)>,
                       grouped_mods: &mut Vec<ItgActorMod>| {
        if grouped_mods.is_empty() {
            return;
        }
        if let Some((duration, tween)) = pending_tween.take() {
            if duration > f32::EPSILON {
                let from = *state;
                let mut to = from;
                itg_apply_actor_mods(&mut to, grouped_mods);
                timeline.push(ModelTweenSegment {
                    start: *cursor_time,
                    duration,
                    tween,
                    from,
                    to,
                });
                *state = to;
                *cursor_time += duration;
                grouped_mods.clear();
                return;
            }
        }
        itg_apply_actor_mods(state, grouped_mods);
        grouped_mods.clear();
    };

    for key in ["initcommand", "nonecommand"] {
        let Some(script) = commands.get(key) else {
            continue;
        };
        for raw in script.split(';') {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            let Some((cmd, args)) = itg_split_command_token(token) else {
                continue;
            };

            match cmd.as_str() {
                "linear" | "accelerate" | "decelerate" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    let duration = args
                        .first()
                        .and_then(|v| {
                            v.trim()
                                .trim_matches('"')
                                .trim_matches('\'')
                                .parse::<f32>()
                                .ok()
                        })
                        .unwrap_or(0.0)
                        .max(0.0);
                    let tween = match cmd.as_str() {
                        "accelerate" => TweenType::Accelerate,
                        "decelerate" => TweenType::Decelerate,
                        _ => TweenType::Linear,
                    };
                    pending_tween = Some((duration, tween));
                }
                "sleep" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    let duration = args
                        .first()
                        .and_then(|v| {
                            v.trim()
                                .trim_matches('"')
                                .trim_matches('\'')
                                .parse::<f32>()
                                .ok()
                        })
                        .unwrap_or(0.0)
                        .max(0.0);
                    cursor_time += duration;
                }
                "effectclock" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    let raw_clock = args
                        .first()
                        .map(|v| {
                            v.trim()
                                .trim_matches('"')
                                .trim_matches('\'')
                                .to_ascii_lowercase()
                        })
                        .unwrap_or_else(|| "time".to_string());
                    effect.clock = if raw_clock.contains("beat") {
                        ModelEffectClock::Beat
                    } else if raw_clock.contains("time")
                        || raw_clock.contains("music")
                        || raw_clock.contains("seconds")
                    {
                        ModelEffectClock::Time
                    } else {
                        warn!("unsupported effectclock '{raw_clock}' in model DSL path");
                        ModelEffectClock::Time
                    };
                }
                "diffuseramp" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    effect.mode = ModelEffectMode::DiffuseRamp;
                }
                "glowshift" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    effect.mode = ModelEffectMode::GlowShift;
                }
                "pulse" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    effect.mode = ModelEffectMode::Pulse;
                }
                "spin" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    effect.mode = ModelEffectMode::Spin;
                    effect.magnitude = [0.0, 0.0, 180.0];
                }
                "effectcolor1" | "effectcolor2" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    let parsed = if args.len() >= 4 {
                        let vals = args
                            .iter()
                            .take(4)
                            .filter_map(|v| {
                                v.trim()
                                    .trim_matches('"')
                                    .trim_matches('\'')
                                    .parse::<f32>()
                                    .ok()
                            })
                            .collect::<Vec<_>>();
                        if vals.len() == 4 {
                            Some([vals[0], vals[1], vals[2], vals[3]])
                        } else {
                            None
                        }
                    } else {
                        args.first().and_then(|v| itg_parse_color(v))
                    };
                    if let Some(c) = parsed {
                        if cmd == "effectcolor1" {
                            effect.color1 = c;
                        } else {
                            effect.color2 = c;
                        }
                    }
                }
                "effectperiod" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    if let Some(v) = args.first().and_then(|v| {
                        v.trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .parse::<f32>()
                            .ok()
                    }) {
                        effect.period = v.max(1e-6);
                    }
                }
                "effectoffset" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    if let Some(v) = args.first().and_then(|v| {
                        v.trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .parse::<f32>()
                            .ok()
                    }) {
                        effect.offset = v;
                    }
                }
                "effecttiming" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    let vals = args
                        .iter()
                        .take(4)
                        .filter_map(|v| {
                            v.trim()
                                .trim_matches('"')
                                .trim_matches('\'')
                                .parse::<f32>()
                                .ok()
                        })
                        .collect::<Vec<_>>();
                    if vals.len() == 4 {
                        effect.timing = [
                            vals[0].max(0.0),
                            vals[1].max(0.0),
                            vals[2].max(0.0),
                            vals[3].max(0.0),
                        ];
                    }
                }
                "effectmagnitude" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                    let vals = args
                        .iter()
                        .take(3)
                        .filter_map(|v| {
                            v.trim()
                                .trim_matches('"')
                                .trim_matches('\'')
                                .parse::<f32>()
                                .ok()
                        })
                        .collect::<Vec<_>>();
                    if vals.len() == 3 {
                        effect.magnitude = [vals[0], vals[1], vals[2]];
                    }
                }
                // known ITG actor commands that don't map to model draw state here
                "stoptweening" | "finishtweening" | "playcommand" | "animate" => {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                }
                _ => {
                    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
                    if let Some(mod_cmd) = itg_parse_actor_mod_token(cmd.as_str(), &arg_refs) {
                        grouped_mods.push(mod_cmd);
                    } else {
                        warn!("unsupported noteskin actor command in model DSL path: '{cmd}'");
                    }
                }
            }
        }
    }

    flush_group(
        &mut state,
        &mut timeline,
        &mut cursor_time,
        &mut pending_tween,
        &mut grouped_mods,
    );

    state.zoom[0] = state.zoom[0].max(0.0);
    state.zoom[1] = state.zoom[1].max(0.0);
    state.zoom[2] = state.zoom[2].max(0.0);
    state.tint[0] = state.tint[0].clamp(0.0, 1.0);
    state.tint[1] = state.tint[1].clamp(0.0, 1.0);
    state.tint[2] = state.tint[2].clamp(0.0, 1.0);
    state.tint[3] = state.tint[3].clamp(0.0, 1.0);
    state.glow[0] = state.glow[0].clamp(0.0, 1.0);
    state.glow[1] = state.glow[1].clamp(0.0, 1.0);
    state.glow[2] = state.glow[2].clamp(0.0, 1.0);
    state.glow[3] = state.glow[3].clamp(0.0, 1.0);

    (state, Arc::from(timeline), effect)
}

fn itg_parse_f32_list(raw: &str) -> Vec<f32> {
    raw.split(',')
        .filter_map(|part| {
            part.trim()
                .trim_matches('"')
                .trim_matches('\'')
                .parse::<f32>()
                .ok()
        })
        .collect()
}

fn itg_parse_color(raw: &str) -> Option<[f32; 4]> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    let value = if lower.starts_with("color(") && trimmed.ends_with(')') {
        let inner = &trimmed[6..trimmed.len().saturating_sub(1)];
        inner.trim().trim_matches('"').trim_matches('\'')
    } else {
        trimmed.trim_matches('"').trim_matches('\'')
    };
    let values = itg_parse_f32_list(value);
    if values.len() < 4 {
        return None;
    }
    Some([values[0], values[1], values[2], values[3]])
}

fn itg_find_texture_with_prefix(
    data: &noteskin_itg::NoteskinData,
    prefix: &str,
) -> Option<PathBuf> {
    let want = prefix.to_ascii_lowercase();
    for dir in &data.search_dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut matches = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .filter(|path| {
                path.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|name| {
                        name.to_ascii_lowercase().starts_with(&want)
                            && name.to_ascii_lowercase().ends_with(".png")
                    })
            })
            .collect::<Vec<_>>();
        if matches.is_empty() {
            continue;
        }
        matches.sort_by(|a, b| {
            let a_name = a
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let b_name = b
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            a_name.cmp(&b_name)
        });
        return matches.into_iter().next();
    }
    None
}

fn itg_texture_key(path: &Path) -> Option<String> {
    let rel = path.strip_prefix("assets").ok()?;
    let mut key = rel.to_string_lossy().replace('\\', "/");
    while key.starts_with('/') {
        key.remove(0);
    }
    Some(key)
}

fn itg_slot_from_path(path: &Path) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let source = Arc::new(SpriteSource::Atlas {
        texture_key: key,
        tex_dims: dims,
    });
    Some(SpriteSlot {
        def: SpriteDefinition {
            src: [0, 0],
            size: [dims.0 as i32, dims.1 as i32],
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        },
        source,
        uv_velocity: [0.0, 0.0],
        uv_offset: [0.0, 0.0],
        note_color_translate: true,
        model: None,
        model_draw: ModelDrawState::default(),
        model_timeline: Arc::from(Vec::<ModelTweenSegment>::new()),
        model_effect: ModelEffectState::default(),
        model_auto_rot_total_frames: 0.0,
        model_auto_rot_z_keys: Arc::from(Vec::<ModelAutoRotKey>::new()),
    })
}

#[derive(Debug, Default, Clone)]
struct ItgLuaBehavior {
    redir_table: HashMap<String, String>,
    rotate: HashMap<String, i32>,
    parts_to_rotate: HashSet<String>,
    blank: HashSet<String>,
    remap_head_to_tap: bool,
    remap_tap_fake_to_tap: bool,
    keep_hold_non_head_button: bool,
}

#[derive(Debug, Default, Clone)]
struct ItgLuaSpriteDecl {
    texture_expr: String,
    frame0: usize,
    frame_count: usize,
    frame_delays: Option<Vec<f32>>,
    commands: HashMap<String, String>,
}

#[derive(Debug, Default, Clone)]
struct ItgLuaModelDecl {
    meshes_expr: Option<String>,
    materials_expr: Option<String>,
    texture_expr: Option<String>,
    frame0: usize,
    commands: HashMap<String, String>,
}

#[derive(Debug, Default, Clone)]
struct ItgLuaRefDecl {
    button_override: Option<String>,
    element: String,
    wrapper_expr: Option<String>,
    frame_override: Option<usize>,
    commands: HashMap<String, String>,
}

#[derive(Debug, Default, Clone)]
struct ItgLuaActorDecl {
    sprites: Vec<ItgLuaSpriteDecl>,
    models: Vec<ItgLuaModelDecl>,
    refs: Vec<ItgLuaRefDecl>,
    path_refs: Vec<ItgLuaPathRefDecl>,
}

#[derive(Debug, Default, Clone)]
struct ItgLuaPathRefDecl {
    path_expr: String,
    arg_expr: Option<String>,
    frame_override: Option<usize>,
    commands: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct ItgLuaResolvedSprite {
    element: String,
    slot: SpriteSlot,
    commands: HashMap<String, String>,
}

#[inline(always)]
fn itg_button_for_col(col: usize) -> &'static str {
    match col % 4 {
        0 => "Left",
        1 => "Down",
        2 => "Up",
        _ => "Right",
    }
}

fn itg_load_lua_behavior(data: &noteskin_itg::NoteskinData) -> ItgLuaBehavior {
    let mut behavior = ItgLuaBehavior::default();
    for dir in data.search_dirs.iter().rev() {
        let path = dir.join("NoteSkin.lua");
        if !path.is_file() {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        if itg_extract_lua_table(&content, "ret.RedirTable").is_some() {
            behavior.redir_table = itg_parse_lua_string_map(&content, "ret.RedirTable")
                .into_iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v))
                .collect();
        }
        if itg_extract_lua_table(&content, "ret.Rotate").is_some() {
            behavior.rotate = itg_parse_lua_int_map(&content, "ret.Rotate")
                .into_iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v))
                .collect();
        }
        if itg_extract_lua_table(&content, "ret.PartsToRotate").is_some() {
            behavior.parts_to_rotate = itg_parse_lua_bool_set(&content, "ret.PartsToRotate")
                .into_iter()
                .map(|name| name.to_ascii_lowercase())
                .collect();
        }
        if itg_extract_lua_table(&content, "ret.Blank").is_some() {
            behavior.blank = itg_parse_lua_bool_set(&content, "ret.Blank")
                .into_iter()
                .map(|name| name.to_ascii_lowercase())
                .collect();
        }
        let defines_redir =
            content.contains("ret.Redir = function") || content.contains("ret.Redir=function");
        if defines_redir {
            let assigns_tap_note = content.contains("sElement = \"Tap Note\"")
                || content.contains("sElement='Tap Note'")
                || content.contains("sElement = 'Tap Note'")
                || content.contains("sElement=\"Tap Note\"");
            let remap_head_to_tap = assigns_tap_note
                && (content.contains("Hold Head")
                    || content.contains("Roll Head")
                    || content.contains("string.find(sElement, \"Head\")")
                    || content.contains("string.find(sElement,'Head')"));
            let remap_tap_fake_to_tap = assigns_tap_note
                && (content.contains("Tap Fake")
                    || content.contains("sElement == \"Tap Fake\"")
                    || content.contains("sElement=='Tap Fake'"));
            let keep_hold_non_head_button = (content
                .contains("if not string.find(sElement, \"Head\")")
                || content.contains("if not string.find(sElement,'Head')"))
                && (content.contains("not string.find(sElement, \"Explosion\")")
                    || content.contains("not string.find(sElement,'Explosion')"))
                && (content.contains("string.find(sElement, \"Hold\")")
                    || content.contains("string.find(sElement,'Hold')"))
                && content.contains("return sButton, sElement");
            behavior.remap_head_to_tap = remap_head_to_tap;
            behavior.remap_tap_fake_to_tap = remap_tap_fake_to_tap;
            behavior.keep_hold_non_head_button = keep_hold_non_head_button;
        }
    }
    behavior
}

fn itg_parse_lua_string_map(content: &str, marker: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(table) = itg_extract_lua_table(content, marker) else {
        return out;
    };
    for raw in table.lines() {
        let line = raw.trim().trim_end_matches(',');
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        let Some((lhs, rhs)) = line.split_once('=') else {
            continue;
        };
        let Some(key) = itg_parse_lua_table_key(lhs.trim()) else {
            continue;
        };
        let Some(value) = itg_parse_lua_quoted(rhs.trim()) else {
            continue;
        };
        out.insert(key, value);
    }
    out
}

fn itg_parse_lua_int_map(content: &str, marker: &str) -> HashMap<String, i32> {
    let mut out = HashMap::new();
    let Some(table) = itg_extract_lua_table(content, marker) else {
        return out;
    };
    for raw in table.lines() {
        let line = raw.trim().trim_end_matches(',');
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        let Some((lhs, rhs)) = line.split_once('=') else {
            continue;
        };
        let Some(key) = itg_parse_lua_table_key(lhs.trim()) else {
            continue;
        };
        let Ok(value) = rhs.trim().parse::<i32>() else {
            continue;
        };
        out.insert(key, value);
    }
    out
}

fn itg_parse_lua_bool_set(content: &str, marker: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(table) = itg_extract_lua_table(content, marker) else {
        return out;
    };
    for raw in table.lines() {
        let line = raw.trim().trim_end_matches(',');
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        let Some((lhs, rhs)) = line.split_once('=') else {
            continue;
        };
        if !rhs.trim().eq_ignore_ascii_case("true") {
            continue;
        }
        if let Some(key) = itg_parse_lua_table_key(lhs.trim()) {
            out.insert(key);
        }
    }
    out
}

fn itg_extract_lua_table<'a>(content: &'a str, marker: &str) -> Option<&'a str> {
    let marker_idx = content.find(marker)?;
    let rest = &content[marker_idx..];
    let open_rel = rest.find('{')?;
    let open = marker_idx + open_rel;
    let close = itg_find_matching(content, open, '{', '}')?;
    content.get(open + 1..close)
}

fn itg_parse_lua_table_key(raw: &str) -> Option<String> {
    let key = raw.trim();
    if let Some(s) = itg_parse_lua_quoted(key) {
        return Some(s);
    }
    if key.starts_with('[') && key.ends_with(']') {
        return itg_parse_lua_quoted(&key[1..key.len() - 1]);
    }
    if key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Some(key.to_string());
    }
    None
}

fn itg_parse_lua_quoted(raw: &str) -> Option<String> {
    let trimmed = raw
        .trim()
        .trim_end_matches(',')
        .trim_end_matches(';')
        .trim();
    if trimmed.len() < 2 {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let quote = bytes[0];
    if (quote != b'"' && quote != b'\'') || bytes[trimmed.len() - 1] != quote {
        return None;
    }
    Some(trimmed[1..trimmed.len() - 1].to_string())
}

fn itg_find_matching(content: &str, open_idx: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in content.char_indices().skip_while(|(i, _)| *i < open_idx) {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

fn itg_skip_ws(content: &str, mut idx: usize) -> usize {
    let bytes = content.as_bytes();
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    idx
}

fn itg_parse_actor_decl(content: &str, metrics: &noteskin_itg::IniData) -> ItgLuaActorDecl {
    let mut decl = ItgLuaActorDecl::default();
    let arg0_aliases = itg_parse_arg0_aliases(content);

    let mut cursor = 0usize;
    while let Some(rel) = content[cursor..].find("Def.Sprite") {
        let start = cursor + rel;
        let Some(open_rel) = content[start..].find('{') else {
            break;
        };
        let open = start + open_rel;
        let Some(close) = itg_find_matching(content, open, '{', '}') else {
            break;
        };
        if let Some(sprite) = itg_parse_sprite_block(&content[open + 1..close], metrics) {
            decl.sprites.push(sprite);
        }
        cursor = close + 1;
    }

    cursor = 0usize;
    while let Some(rel) = content[cursor..].find("Def.Model") {
        let start = cursor + rel;
        let Some(open_rel) = content[start..].find('{') else {
            break;
        };
        let open = start + open_rel;
        let Some(close) = itg_find_matching(content, open, '{', '}') else {
            break;
        };
        if let Some(model) = itg_parse_model_block(&content[open + 1..close], metrics) {
            decl.models.push(model);
        }
        cursor = close + 1;
    }

    cursor = 0usize;
    while let Some(rel) = content[cursor..].find("LoadActor(") {
        let call_start = cursor + rel;
        if content
            .as_bytes()
            .get(call_start.saturating_sub(1))
            .is_some_and(|b| *b == b':')
        {
            cursor = call_start + 1;
            continue;
        }
        let open = call_start + "LoadActor".len();
        let Some(close) = itg_find_matching(content, open, '(', ')') else {
            break;
        };
        let args_raw = &content[open + 1..close];
        let args = itg_split_call_args(args_raw);
        let (commands, next_cursor) = itg_find_post_call_commands(content, close, metrics);
        let frame_override = itg_find_post_call_frame_override(content, close);
        if !args.is_empty() {
            let path_expr = itg_rewrite_arg0_expr(&args[0], &arg0_aliases);
            let arg_expr = args.get(1).map(|s| itg_rewrite_arg0_expr(s, &arg0_aliases));
            decl.path_refs.push(ItgLuaPathRefDecl {
                path_expr,
                arg_expr,
                frame_override,
                commands,
            });
        }
        cursor = next_cursor;
    }

    cursor = 0usize;
    while let Some(rel) = content[cursor..].find("NOTESKIN:LoadActor(") {
        let call_start = cursor + rel;
        let open = call_start + "NOTESKIN:LoadActor".len();
        let Some(close) = itg_find_matching(content, open, '(', ')') else {
            break;
        };
        let args = &content[open + 1..close];
        let Some((button_override, element)) = itg_parse_loadactor_args(args) else {
            cursor = close + 1;
            continue;
        };

        let mut wrapper_expr = None;
        let (mut commands, mut next_cursor) = itg_find_post_call_commands(content, close, metrics);
        let mut frame_override = itg_find_post_call_frame_override(content, close);
        if commands.is_empty()
            && let Some((outer_args, outer_close)) =
                itg_find_enclosing_loadactor_for_noteskin(content, call_start, close)
            && outer_args.len() >= 2
        {
            wrapper_expr = Some(outer_args[0].clone());
            let (outer_commands, outer_next_cursor) =
                itg_find_post_call_commands(content, outer_close, metrics);
            let outer_frame_override = itg_find_post_call_frame_override(content, outer_close);
            if !outer_commands.is_empty() {
                commands = outer_commands;
                next_cursor = outer_next_cursor;
                frame_override = outer_frame_override;
            } else {
                next_cursor = outer_close + 1;
            }
        }
        decl.refs.push(ItgLuaRefDecl {
            button_override,
            element,
            wrapper_expr,
            frame_override,
            commands,
        });
        cursor = next_cursor;
    }

    decl
}

fn itg_parse_arg0_aliases(content: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for raw in content.lines() {
        let line = raw.trim().trim_end_matches(';').trim();
        if !line.starts_with("local ") {
            continue;
        }
        let rest = line[6..].trim();
        let Some((lhs, rhs)) = rest.split_once('=') else {
            continue;
        };
        if rhs.trim() != "..." {
            continue;
        }
        let name = lhs.trim();
        if name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            out.insert(name.to_string());
        }
    }
    out
}

fn itg_rewrite_arg0_expr(expr: &str, arg0_aliases: &HashSet<String>) -> String {
    let trimmed = expr.trim();
    if trimmed == "..." || arg0_aliases.contains(trimmed) {
        ITG_ARG0_TOKEN.to_string()
    } else {
        trimmed.to_string()
    }
}

fn itg_split_call_args(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = 0u8;
    let bytes = raw.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let b = bytes[idx];
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
            idx += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => {
                quote = b;
            }
            b'(' | b'{' | b'[' => {
                depth += 1;
            }
            b')' | b'}' | b']' => {
                depth = depth.saturating_sub(1);
            }
            b',' if depth == 0 => {
                let part = raw[start..idx].trim();
                if !part.is_empty() {
                    out.push(part.to_string());
                }
                start = idx + 1;
            }
            _ => {}
        }
        idx += 1;
    }
    let tail = raw[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn itg_find_post_call_commands(
    content: &str,
    call_close: usize,
    metrics: &noteskin_itg::IniData,
) -> (HashMap<String, String>, usize) {
    let mut after = itg_skip_ws(content, call_close + 1);
    if !content
        .get(after..)
        .is_some_and(|tail| tail.starts_with(".."))
    {
        return (HashMap::new(), call_close + 1);
    }
    after += 2;
    after = itg_skip_ws(content, after);
    if !content.as_bytes().get(after).is_some_and(|ch| *ch == b'{') {
        return (HashMap::new(), call_close + 1);
    }
    let Some(end) = itg_find_matching(content, after, '{', '}') else {
        return (HashMap::new(), call_close + 1);
    };
    (
        itg_parse_commands_block(&content[after + 1..end], metrics),
        end + 1,
    )
}

fn itg_find_post_call_frame_override(content: &str, call_close: usize) -> Option<usize> {
    let mut after = itg_skip_ws(content, call_close + 1);
    if !content
        .get(after..)
        .is_some_and(|tail| tail.starts_with(".."))
    {
        return None;
    }
    after += 2;
    after = itg_skip_ws(content, after);
    if !content.as_bytes().get(after).is_some_and(|ch| *ch == b'{') {
        return None;
    }
    let end = itg_find_matching(content, after, '{', '}')?;
    itg_parse_frame_override_block(&content[after + 1..end])
}

fn itg_parse_frame_override_block(block: &str) -> Option<usize> {
    let marker = "Frames";
    let marker_idx = block.find(marker)?;
    let tail = &block[marker_idx + marker.len()..];
    let eq_idx = tail.find('=')?;
    let after_eq = marker_idx + marker.len() + eq_idx + 1;
    let bytes = block.as_bytes();
    let mut open = after_eq;
    while open < bytes.len() && bytes[open].is_ascii_whitespace() {
        open += 1;
    }
    if bytes.get(open).is_none_or(|b| *b != b'{') {
        return None;
    }
    let close = itg_find_matching(block, open, '{', '}')?;
    let frames = &block[open + 1..close];
    let frame_key_idx = frames.find("Frame")?;
    let frame_tail = &frames[frame_key_idx + "Frame".len()..];
    let frame_eq = frame_tail.find('=')?;
    let value_tail = frame_tail[frame_eq + 1..].trim();
    let digits: String = value_tail
        .chars()
        .skip_while(|ch| ch.is_ascii_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<usize>().ok()
}

fn itg_find_enclosing_loadactor_for_noteskin(
    content: &str,
    call_start: usize,
    call_close: usize,
) -> Option<(Vec<String>, usize)> {
    let mut search_end = call_start;
    while let Some(pos) = content[..search_end].rfind("LoadActor(") {
        if content
            .as_bytes()
            .get(pos.saturating_sub(1))
            .is_some_and(|b| *b == b':')
        {
            search_end = pos;
            continue;
        }
        let open = pos + "LoadActor".len();
        let Some(outer_close) = itg_find_matching(content, open, '(', ')') else {
            search_end = pos;
            continue;
        };
        if pos < call_start && outer_close >= call_close {
            let args_raw = &content[open + 1..outer_close];
            return Some((itg_split_call_args(args_raw), outer_close));
        }
        search_end = pos;
    }
    None
}

fn itg_parse_sprite_block(
    block: &str,
    metrics: &noteskin_itg::IniData,
) -> Option<ItgLuaSpriteDecl> {
    let mut texture_expr = None;
    let mut frame0 = 0usize;
    let mut frame_count = 1usize;
    let mut frame_max = 0usize;
    let mut frame_seen = false;
    let mut frame_delays = HashMap::<usize, f32>::new();
    let mut commands = HashMap::new();
    for raw in block.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        if let Some((prefix, _)) = line.split_once("--") {
            line = prefix.trim();
        }
        let line = line.trim_end_matches(',').trim_end_matches(';').trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let value = v.trim();
        if key.eq_ignore_ascii_case("Texture") {
            texture_expr = Some(value.to_string());
            continue;
        }
        if key.eq_ignore_ascii_case("Frames")
            && let Some((linear_count, linear_delays)) = itg_parse_linear_frames_expr(value)
        {
            frame_count = linear_count.max(1);
            frame_delays = linear_delays
                .into_iter()
                .enumerate()
                .map(|(idx, delay)| (idx, delay))
                .collect();
            continue;
        }
        let key_lower = key.to_ascii_lowercase();
        if key_lower.starts_with("frame") && key_lower[5..].chars().all(|ch| ch.is_ascii_digit()) {
            if let Ok(parsed) = value.parse::<usize>() {
                frame_seen = true;
                frame_max = frame_max.max(parsed);
                if key_lower == "frame0000" {
                    frame0 = parsed;
                }
            }
            continue;
        }
        if key_lower.starts_with("delay") && key_lower[5..].chars().all(|ch| ch.is_ascii_digit()) {
            if let Ok(idx) = key_lower[5..].parse::<usize>()
                && let Some(delay) = itg_parse_lua_float_token(value)
            {
                frame_delays.insert(idx, delay.max(0.0));
            }
            continue;
        }
        if key_lower.ends_with("command")
            && let Some(cmd) = itg_resolve_command_expr(value, metrics)
        {
            commands.insert(key_lower, cmd);
        }
    }
    for (k, v) in itg_parse_function_commands(block) {
        commands.insert(k, v);
    }
    if frame_seen {
        frame_count = frame_max.saturating_add(1).max(1);
    }
    let frame_delays = if frame_delays.is_empty() {
        None
    } else {
        let mut delays = vec![frame_delays.get(&0).copied().unwrap_or(1.0); frame_count];
        for (idx, delay) in frame_delays {
            if idx < delays.len() {
                delays[idx] = delay.max(0.0);
            }
        }
        Some(delays)
    };
    Some(ItgLuaSpriteDecl {
        texture_expr: texture_expr?,
        frame0,
        frame_count,
        frame_delays,
        commands,
    })
}

fn itg_parse_linear_frames_expr(raw: &str) -> Option<(usize, Vec<f32>)> {
    let value = raw.trim().trim_end_matches(';').trim();
    if !value.starts_with("Sprite.LinearFrames") {
        return None;
    }
    let open = value.find('(')?;
    let close = itg_find_matching(value, open, '(', ')')?;
    let args = itg_split_call_args(&value[open + 1..close]);
    if args.len() < 2 {
        return None;
    }
    let frame_count = args[0]
        .trim()
        .parse::<usize>()
        .ok()
        .or_else(|| itg_parse_lua_float_token(&args[0]).map(|v| v as usize))?
        .max(1);
    let seconds = itg_parse_lua_float_token(&args[1])?;
    let delay = (seconds / frame_count as f32).max(0.0);
    Some((frame_count, vec![delay; frame_count]))
}

fn itg_parse_model_block(block: &str, metrics: &noteskin_itg::IniData) -> Option<ItgLuaModelDecl> {
    let mut meshes_expr = None;
    let mut materials_expr = None;
    let mut texture_expr = None;
    let mut frame0 = 0usize;
    let mut commands = HashMap::new();
    for raw in block.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        if let Some((prefix, _)) = line.split_once("--") {
            line = prefix.trim();
        }
        let line = line.trim_end_matches(',').trim_end_matches(';').trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        let value = v.trim();
        if key.eq_ignore_ascii_case("Meshes") {
            meshes_expr = Some(value.to_string());
            continue;
        }
        if key.eq_ignore_ascii_case("Materials") {
            materials_expr = Some(value.to_string());
            continue;
        }
        if key.eq_ignore_ascii_case("Texture") {
            texture_expr = Some(value.to_string());
            continue;
        }
        let key_lower = key.to_ascii_lowercase();
        if key_lower.starts_with("frame")
            && key_lower[5..].chars().all(|ch| ch.is_ascii_digit())
            && let Ok(parsed) = value.parse::<usize>()
            && key_lower == "frame0000"
        {
            frame0 = parsed;
            continue;
        }
        if key_lower.ends_with("command")
            && let Some(cmd) = itg_resolve_command_expr(value, metrics)
        {
            commands.insert(key_lower, cmd);
        }
    }
    for (k, v) in itg_parse_function_commands(block) {
        commands.insert(k, v);
    }
    if meshes_expr.is_none() && materials_expr.is_none() && texture_expr.is_none() {
        return None;
    }
    Some(ItgLuaModelDecl {
        meshes_expr,
        materials_expr,
        texture_expr,
        frame0,
        commands,
    })
}

fn itg_parse_loadactor_args(args: &str) -> Option<(Option<String>, String)> {
    let quoted = itg_extract_quoted_strings(args);
    let element = quoted.last()?.to_string();
    let button_override = if args.contains("Var \"Button\"") || args.contains("Var 'Button'") {
        None
    } else if quoted.len() >= 2 {
        Some(quoted[0].clone())
    } else {
        None
    };
    Some((button_override, element))
}

fn itg_parse_commands_block(
    block: &str,
    metrics: &noteskin_itg::IniData,
) -> HashMap<String, String> {
    let mut commands = HashMap::new();
    for raw in block.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        if let Some((prefix, _)) = line.split_once("--") {
            line = prefix.trim();
        }
        let line = line.trim_end_matches(',').trim_end_matches(';').trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        if !key.ends_with("command") {
            continue;
        }
        if let Some(cmd) = itg_resolve_command_expr(v.trim(), metrics) {
            commands.insert(key, cmd);
        }
    }
    for (k, v) in itg_parse_function_commands(block) {
        commands.insert(k, v);
    }
    commands
}

fn itg_parse_function_commands(block: &str) -> HashMap<String, String> {
    let mut commands = HashMap::new();
    let bytes = block.as_bytes();
    let mut cursor = 0usize;
    while let Some(eq_rel) = block[cursor..].find('=') {
        let eq = cursor + eq_rel;
        let key_start = block[..eq]
            .rfind(['\n', '\r', '{', ';', ','])
            .map_or(0, |idx| idx + 1);
        let key = block[key_start..eq].trim();
        let key_lower = key.to_ascii_lowercase();
        if !key_lower.ends_with("command") {
            cursor = eq + 1;
            continue;
        }
        let mut rhs = itg_skip_ws(block, eq + 1);
        if !block.get(rhs..).is_some_and(|s| s.starts_with("function")) {
            cursor = eq + 1;
            continue;
        }
        rhs += "function".len();
        let Some(param_open_rel) = block[rhs..].find('(') else {
            cursor = eq + 1;
            continue;
        };
        let param_open = rhs + param_open_rel;
        let Some(param_close) = itg_find_matching(block, param_open, '(', ')') else {
            cursor = eq + 1;
            continue;
        };
        let body_start = param_close + 1;
        let Some(end_idx) = itg_find_function_end(block, body_start) else {
            cursor = eq + 1;
            continue;
        };
        let body = &block[body_start..end_idx];
        if let Some(cmd) = itg_parse_self_chain_commands(body) {
            commands.insert(key_lower, cmd);
        }
        cursor = end_idx + 3;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
    }
    commands
}

fn itg_find_function_end(content: &str, mut cursor: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut depth = 1usize;
    let mut quote = 0u8;
    while cursor < bytes.len() {
        let b = bytes[cursor];
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
            cursor += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            quote = b;
            cursor += 1;
            continue;
        }
        if content[cursor..].starts_with("function")
            && itg_token_boundary(bytes, cursor, "function".len())
        {
            depth += 1;
            cursor += "function".len();
            continue;
        }
        if content[cursor..].starts_with("end") && itg_token_boundary(bytes, cursor, "end".len()) {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(cursor);
            }
            cursor += "end".len();
            continue;
        }
        cursor += 1;
    }
    None
}

fn itg_token_boundary(bytes: &[u8], start: usize, len: usize) -> bool {
    let prev_ok = if start == 0 {
        true
    } else {
        !is_lua_ident(bytes[start - 1])
    };
    let end = start + len;
    let next_ok = if end >= bytes.len() {
        true
    } else {
        !is_lua_ident(bytes[end])
    };
    prev_ok && next_ok
}

fn is_lua_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn itg_parse_self_chain_commands(body: &str) -> Option<String> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut cursor = 0usize;
    while let Some(rel) = body[cursor..].find("self:") {
        let name_start = cursor + rel + 5;
        let mut name_end = name_start;
        while name_end < bytes.len() && is_lua_ident(bytes[name_end]) {
            name_end += 1;
        }
        if name_end == name_start {
            cursor = name_start;
            continue;
        }
        let name = body[name_start..name_end].trim();
        let mut open = itg_skip_ws(body, name_end);
        if bytes.get(open).is_none_or(|b| *b != b'(') {
            cursor = name_end;
            continue;
        }
        let Some(close) = itg_find_matching(body, open, '(', ')') else {
            cursor = name_end;
            continue;
        };
        let args = body[open + 1..close].trim();
        if args.is_empty() {
            out.push(name.to_string());
        } else {
            out.push(format!("{name},{args}"));
        }
        open = close + 1;
        cursor = open;
    }
    if out.is_empty() {
        None
    } else {
        Some(out.join(";"))
    }
}

fn itg_resolve_command_expr(raw: &str, metrics: &noteskin_itg::IniData) -> Option<String> {
    let value = raw
        .trim()
        .trim_end_matches(',')
        .trim_end_matches(';')
        .trim();
    if value.starts_with("NOTESKIN:GetMetricA(") {
        let args = itg_extract_quoted_strings(value);
        if args.len() >= 2 {
            return metrics.get(&args[0], &args[1]).map(str::to_string);
        }
    }
    if value.starts_with("cmd(") && value.ends_with(')') {
        return Some(value[4..value.len() - 1].trim().to_string());
    }
    if let Some(q) = itg_parse_lua_quoted(value) {
        return Some(q);
    }
    Some(value.to_string())
}

fn itg_extract_quoted_strings(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let quote = bytes[idx];
        if quote != b'"' && quote != b'\'' {
            idx += 1;
            continue;
        }
        idx += 1;
        let start = idx;
        while idx < bytes.len() && bytes[idx] != quote {
            idx += 1;
        }
        if idx <= bytes.len() {
            out.push(input[start..idx].to_string());
        }
        idx += 1;
    }
    out
}

fn itg_resolve_actor_sprites(
    data: &noteskin_itg::NoteskinData,
    behavior: &ItgLuaBehavior,
    button: &str,
    element: &str,
) -> Vec<ItgLuaResolvedSprite> {
    let mut visiting = HashSet::new();
    itg_resolve_actor_sprites_inner(data, behavior, button, element, 0, &mut visiting)
}

fn itg_resolve_actor_sprites_inner(
    data: &noteskin_itg::NoteskinData,
    behavior: &ItgLuaBehavior,
    button: &str,
    element: &str,
    depth: usize,
    visiting: &mut HashSet<String>,
) -> Vec<ItgLuaResolvedSprite> {
    if depth > 24 {
        warn!("noteskin lua actor recursion depth exceeded at '{button} {element}'");
        return Vec::new();
    }

    let visit_key = format!(
        "{}|{}",
        button.to_ascii_lowercase(),
        element.to_ascii_lowercase()
    );
    if !visiting.insert(visit_key.clone()) {
        warn!("noteskin lua actor recursion loop detected at '{button} {element}'");
        return Vec::new();
    }

    let element_lower = element.to_ascii_lowercase();
    if behavior.blank.contains(&element_lower) {
        visiting.remove(&visit_key);
        return Vec::new();
    }

    let mut resolved_element = element.to_string();
    if behavior.remap_head_to_tap
        && (resolved_element.contains("Head")
            || (behavior.remap_tap_fake_to_tap
                && resolved_element.eq_ignore_ascii_case("Tap Fake")))
    {
        resolved_element = "Tap Note".to_string();
    }
    let resolved_element_lower = resolved_element.to_ascii_lowercase();
    let keep_button = behavior.keep_hold_non_head_button
        && resolved_element_lower.contains("hold")
        && !resolved_element_lower.contains("head")
        && !resolved_element_lower.contains("explosion");
    let resolved_button = if keep_button {
        button.to_string()
    } else {
        behavior
            .redir_table
            .get(&button.to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| button.to_string())
    };
    let path = data.resolve_path(&resolved_button, &resolved_element);
    let Some(path) = path else {
        visiting.remove(&visit_key);
        return Vec::new();
    };

    let out = itg_resolve_actor_file(
        data,
        behavior,
        button,
        element,
        &element_lower,
        &path,
        depth,
        visiting,
        None,
    );

    visiting.remove(&visit_key);
    out
}

fn itg_resolve_actor_file(
    data: &noteskin_itg::NoteskinData,
    behavior: &ItgLuaBehavior,
    button: &str,
    element: &str,
    element_lower: &str,
    path: &Path,
    depth: usize,
    visiting: &mut HashSet<String>,
    arg0_path: Option<&Path>,
) -> Vec<ItgLuaResolvedSprite> {
    if depth > 48 {
        warn!(
            "noteskin lua file recursion depth exceeded at '{}' for '{button} {element}'",
            path.display()
        );
        return Vec::new();
    }

    let mut out = Vec::new();
    let is_lua = path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"));
    if !is_lua {
        if let Some(mut slot) =
            itg_slot_from_path_with_frame(&path, 0).or_else(|| itg_slot_from_path(&path))
        {
            if behavior.parts_to_rotate.contains(element_lower)
                && let Some(rot) = behavior.rotate.get(&button.to_ascii_lowercase())
            {
                slot.def.rotation_deg = *rot;
            }
            out.push(ItgLuaResolvedSprite {
                element: element.to_string(),
                slot,
                commands: HashMap::new(),
            });
        }
        return out;
    }

    let path_key = format!("file:{}", path.display().to_string().to_ascii_lowercase());
    if !visiting.insert(path_key.clone()) {
        warn!(
            "noteskin lua file recursion loop detected at '{}' for '{button} {element}'",
            path.display()
        );
        return Vec::new();
    }

    let Ok(content) = fs::read_to_string(&path) else {
        visiting.remove(&path_key);
        return Vec::new();
    };
    let decl = itg_parse_actor_decl(&content, &data.metrics);
    let default_anim_is_beat = itg_animation_is_beat_based(data);
    for sprite in decl.sprites {
        let texture_path = itg_resolve_texture_expr(data, &sprite.texture_expr, arg0_path);
        let Some(texture_path) = texture_path else {
            continue;
        };
        let anim_is_beat =
            itg_sprite_animation_is_beat_based(&sprite.commands, default_anim_is_beat);
        let mut slot = if sprite.frame_count > 1 {
            itg_slot_from_path_animated(
                &texture_path,
                sprite.frame0,
                sprite.frame_count,
                sprite.frame_delays.as_deref(),
                anim_is_beat,
            )
            .or_else(|| itg_slot_from_path_with_frame(&texture_path, sprite.frame0))
        } else {
            itg_slot_from_path_with_frame(&texture_path, sprite.frame0)
        }
        .or_else(|| itg_slot_from_path(&texture_path));
        let Some(mut slot) = slot.take() else {
            continue;
        };
        if behavior.parts_to_rotate.contains(element_lower)
            && let Some(rot) = behavior.rotate.get(&button.to_ascii_lowercase())
        {
            slot.def.rotation_deg = *rot;
        }
        out.push(ItgLuaResolvedSprite {
            element: element.to_string(),
            slot,
            commands: sprite.commands,
        });
    }
    for model in decl.models {
        let model_path = model
            .materials_expr
            .as_deref()
            .or(model.meshes_expr.as_deref())
            .or(model.texture_expr.as_deref())
            .and_then(|expr| itg_resolve_texture_expr(data, expr, arg0_path));
        let Some(model_path) = model_path else {
            continue;
        };
        let (draw, timeline, effect) = itg_model_draw_program(&model.commands);
        let model_auto_rot = itg_parse_milkshape_model_auto_rot(&model_path);
        if let Some(model_layers) = itg_parse_milkshape_model_layers(data, &model_path) {
            let mut pushed = false;
            for layer in model_layers {
                let mut slot =
                    itg_slot_from_path_with_frame(&layer.texture.texture_path, model.frame0)
                        .or_else(|| itg_slot_from_path(&layer.texture.texture_path));
                let Some(mut slot) = slot.take() else {
                    continue;
                };
                slot.model = Some(layer.mesh);
                slot.model_draw = draw;
                slot.model_timeline = Arc::clone(&timeline);
                slot.model_effect = effect;
                if let Some(auto_rot) = model_auto_rot.as_ref() {
                    slot.model_auto_rot_total_frames = auto_rot.total_frames;
                    slot.model_auto_rot_z_keys = Arc::clone(&auto_rot.z_keys);
                }
                slot.note_color_translate = !layer.flags.nomove;
                slot.uv_velocity = if layer.flags.nomove {
                    [0.0, 0.0]
                } else {
                    layer.texture.tex.uv_velocity
                };
                slot.uv_offset = layer.texture.tex.uv_offset;
                if behavior.parts_to_rotate.contains(element_lower)
                    && let Some(rot) = behavior.rotate.get(&button.to_ascii_lowercase())
                {
                    slot.def.rotation_deg = *rot;
                }
                out.push(ItgLuaResolvedSprite {
                    element: element.to_string(),
                    slot,
                    commands: model.commands.clone(),
                });
                pushed = true;
            }
            if pushed {
                continue;
            }
        }

        let Some(model_texture) = itg_resolve_model_texture_path(data, &model_path) else {
            warn!(
                "noteskin model '{}' for '{button} {element}' did not resolve a texture fallback",
                model_path.display()
            );
            continue;
        };
        let mut slot = itg_slot_from_path_with_frame(&model_texture.texture_path, model.frame0)
            .or_else(|| itg_slot_from_path(&model_texture.texture_path));
        let Some(mut slot) = slot.take() else {
            continue;
        };
        slot.model = itg_parse_milkshape_model(data, &model_path);
        slot.model_draw = draw;
        slot.model_timeline = timeline;
        slot.model_effect = effect;
        if let Some(auto_rot) = model_auto_rot.as_ref() {
            slot.model_auto_rot_total_frames = auto_rot.total_frames;
            slot.model_auto_rot_z_keys = Arc::clone(&auto_rot.z_keys);
        }
        slot.uv_velocity = model_texture.tex.uv_velocity;
        slot.uv_offset = model_texture.tex.uv_offset;
        if behavior.parts_to_rotate.contains(element_lower)
            && let Some(rot) = behavior.rotate.get(&button.to_ascii_lowercase())
        {
            slot.def.rotation_deg = *rot;
        }
        out.push(ItgLuaResolvedSprite {
            element: element.to_string(),
            slot,
            commands: model.commands,
        });
    }
    for path_ref in decl.path_refs {
        let Some(path) = itg_resolve_texture_expr(data, &path_ref.path_expr, arg0_path) else {
            continue;
        };
        let path_ref_arg = path_ref
            .arg_expr
            .as_deref()
            .and_then(|expr| itg_resolve_texture_expr(data, expr, arg0_path));
        let mut child = itg_resolve_actor_file(
            data,
            behavior,
            button,
            element,
            element_lower,
            &path,
            depth + 1,
            visiting,
            path_ref_arg.as_deref(),
        );
        for sprite in &mut child {
            if let Some(frame) = path_ref.frame_override {
                itg_apply_frame_override(&mut sprite.slot, frame);
            }
            for (k, v) in &path_ref.commands {
                sprite.commands.insert(k.clone(), v.clone());
            }
        }
        out.extend(child);
    }
    for reference in decl.refs {
        let child_button = reference.button_override.as_deref().unwrap_or(button);
        let wrapper_commands = reference
            .wrapper_expr
            .as_deref()
            .and_then(|expr| itg_resolve_texture_expr(data, expr, arg0_path))
            .and_then(|path| itg_parse_wrapper_commands_from_file(&path, &data.metrics))
            .unwrap_or_default();
        let mut child = itg_resolve_actor_sprites_inner(
            data,
            behavior,
            child_button,
            &reference.element,
            depth + 1,
            visiting,
        );
        for sprite in &mut child {
            if let Some(frame) = reference.frame_override {
                itg_apply_frame_override(&mut sprite.slot, frame);
            }
            for (k, v) in &wrapper_commands {
                sprite.commands.insert(k.clone(), v.clone());
            }
            for (k, v) in &reference.commands {
                sprite.commands.insert(k.clone(), v.clone());
            }
        }
        out.extend(child);
    }

    visiting.remove(&path_key);
    out
}

#[derive(Debug, Clone, Copy)]
struct ItgModelTexturePath {
    uv_velocity: [f32; 2],
    uv_offset: [f32; 2],
}

impl Default for ItgModelTexturePath {
    fn default() -> Self {
        Self {
            uv_velocity: [0.0, 0.0],
            uv_offset: [0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone)]
struct ItgResolvedModelTexture {
    texture_path: PathBuf,
    tex: ItgModelTexturePath,
}

impl ItgResolvedModelTexture {
    fn from_path(texture_path: PathBuf) -> Self {
        Self {
            texture_path,
            tex: ItgModelTexturePath::default(),
        }
    }
}

fn itg_resolve_model_texture_path(
    data: &noteskin_itg::NoteskinData,
    model_path: &Path,
) -> Option<ItgResolvedModelTexture> {
    if !model_path.is_file() {
        return None;
    }
    let ext = model_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase());
    if let Some(ref ext) = ext {
        if itg_is_texture_image_ext(ext) {
            return Some(ItgResolvedModelTexture::from_path(model_path.to_path_buf()));
        }
        if ext == "ini" {
            return itg_resolve_animated_texture_ini(model_path);
        }
    }
    let content = fs::read_to_string(model_path).ok()?;
    for candidate in itg_extract_quoted_strings(&content) {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(candidate_path) = itg_resolve_relative_or_noteskin_path(data, model_path, trimmed)
        else {
            continue;
        };
        let ext = candidate_path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase());
        let Some(ext) = ext else {
            continue;
        };
        if itg_is_texture_image_ext(&ext) {
            return Some(ItgResolvedModelTexture::from_path(candidate_path));
        }
        if ext == "ini"
            && let Some(resolved) = itg_resolve_animated_texture_ini(&candidate_path)
        {
            return Some(resolved);
        }
    }
    let stem = model_path.file_stem().and_then(|s| s.to_str())?;
    let stem_lower = stem.to_ascii_lowercase();
    let derived = if stem_lower.ends_with(" model") {
        format!("{} tex", &stem[..stem.len().saturating_sub(6)])
    } else if stem_lower.ends_with("model") {
        format!("{}tex", &stem[..stem.len().saturating_sub(5)])
    } else {
        format!("{stem} tex")
    };
    data.resolve_path("", &derived).and_then(|path| {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        if itg_is_texture_image_ext(&ext) {
            Some(ItgResolvedModelTexture::from_path(path))
        } else if ext == "ini" {
            itg_resolve_animated_texture_ini(&path)
        } else {
            None
        }
    })
}

fn itg_resolve_relative_or_noteskin_path(
    data: &noteskin_itg::NoteskinData,
    base_file: &Path,
    raw: &str,
) -> Option<PathBuf> {
    let rel = raw.trim().trim_matches('"').trim_matches('\'');
    if rel.is_empty() {
        return None;
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() && rel_path.is_file() {
        return Some(rel_path.to_path_buf());
    }
    if let Some(parent) = base_file.parent() {
        let direct = parent.join(rel_path);
        if direct.is_file() {
            return Some(direct);
        }
    }
    data.resolve_path("", rel)
}

fn itg_resolve_animated_texture_ini(path: &Path) -> Option<ItgResolvedModelTexture> {
    let ini = noteskin_itg::IniData::parse_file(path).ok()?;
    let frame = ini
        .get("AnimatedTexture", "Frame0000")
        .or_else(|| ini.get("AnimatedTexture", "Frame0001"))?;
    let rel = frame.trim().trim_matches('"').trim_matches('\'');
    if rel.is_empty() {
        return None;
    }
    let rel_path = Path::new(rel);
    let texture_path = if rel_path.is_absolute() && rel_path.is_file() {
        rel_path.to_path_buf()
    } else {
        let base = path.parent()?;
        let resolved = base.join(rel_path);
        if !resolved.is_file() {
            return None;
        }
        resolved
    };
    let tex_velocity_x = ini
        .get("AnimatedTexture", "TexVelocityX")
        .and_then(itg_parse_ini_float)
        .unwrap_or(0.0);
    let tex_velocity_y = ini
        .get("AnimatedTexture", "TexVelocityY")
        .and_then(itg_parse_ini_float)
        .unwrap_or(0.0);
    let tex_offset_x = ini
        .get("AnimatedTexture", "TexOffsetX")
        .and_then(itg_parse_ini_float)
        .unwrap_or(0.0);
    let tex_offset_y = ini
        .get("AnimatedTexture", "TexOffsetY")
        .and_then(itg_parse_ini_float)
        .unwrap_or(0.0);
    Some(ItgResolvedModelTexture {
        texture_path,
        tex: ItgModelTexturePath {
            uv_velocity: [tex_velocity_x, tex_velocity_y],
            uv_offset: [tex_offset_x, tex_offset_y],
        },
    })
}

fn itg_parse_ini_value(raw: &str) -> Option<&str> {
    let trimmed = raw.split_once("//").map_or(raw, |(prefix, _)| prefix);
    let trimmed = trimmed
        .split_once(';')
        .map_or(trimmed, |(prefix, _)| prefix);
    let value = trimmed.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return None;
    }
    Some(value)
}

fn itg_parse_ini_int(raw: &str) -> Option<i32> {
    let value = itg_parse_ini_value(raw)?;
    let bytes = value.as_bytes();
    let mut end = 0usize;
    if bytes.first().is_some_and(|b| *b == b'+' || *b == b'-') {
        end = 1;
    }
    let digit_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == digit_start {
        return None;
    }
    let parsed = value[..end].parse::<i64>().ok()?;
    Some(parsed.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
}

fn itg_parse_ini_float(raw: &str) -> Option<f32> {
    let value = itg_parse_ini_value(raw)?;
    value.parse::<f32>().ok()
}

fn itg_parse_lua_float_token(raw: &str) -> Option<f32> {
    let value = raw.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return None;
    }
    if let Ok(v) = value.parse::<f32>() {
        return Some(v);
    }
    if value.contains(',') && !value.contains('.') {
        let patched = value.replace(',', ".");
        return patched.parse::<f32>().ok();
    }
    None
}

#[derive(Debug, Clone)]
struct ItgResolvedModelLayer {
    mesh: Arc<ModelMesh>,
    texture: ItgResolvedModelTexture,
    flags: ItgModelMaterialFlags,
}

#[derive(Debug)]
struct ItgMilkshapeMeshLayer {
    material_index: i32,
    vertices: Vec<ModelVertex>,
    bounds: [f32; 6],
}

#[derive(Debug, Clone, Copy, Default)]
struct ItgModelMaterialFlags {
    nomove: bool,
}

#[derive(Debug, Clone)]
struct ItgModelAutoRot {
    total_frames: f32,
    z_keys: Arc<[ModelAutoRotKey]>,
}

fn itg_parse_model_material_flags(name: &str) -> ItgModelMaterialFlags {
    let lower = name.to_ascii_lowercase();
    ItgModelMaterialFlags {
        nomove: lower.contains("nomove"),
    }
}

fn itg_parse_milkshape_mesh_material_index(header: &str) -> i32 {
    let trimmed = header.trim();
    let rest = if let Some(end_quote) = trimmed.rfind('"') {
        &trimmed[end_quote + 1..]
    } else {
        trimmed
    };
    let mut parts = rest.split_whitespace();
    let _flags = parts.next();
    parts
        .next()
        .and_then(|raw| raw.parse::<i32>().ok())
        .unwrap_or(0)
}

fn itg_parse_milkshape_model_auto_rot(path: &Path) -> Option<ItgModelAutoRot> {
    let content = fs::read_to_string(path).ok()?;
    if !content.to_ascii_lowercase().contains("milkshape 3d ascii") {
        return None;
    }
    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"));
    while let Some(line) = lines.next() {
        let Some(raw_bones) = line.strip_prefix("Bones:") else {
            continue;
        };
        let bone_count = raw_bones.trim().parse::<usize>().ok()?;
        if bone_count == 0 {
            return None;
        }
        let mut total_frames = 0.0f32;
        let mut first_bone = Vec::new();
        for bone_idx in 0..bone_count {
            let _name = lines.next()?;
            let _parent = lines.next()?;
            let _bind = lines.next()?;
            let pos_count = lines.next()?.trim().parse::<usize>().ok()?;
            for _ in 0..pos_count {
                let frame = lines
                    .next()?
                    .split_whitespace()
                    .next()?
                    .parse::<f32>()
                    .ok()?;
                total_frames = total_frames.max(frame);
            }
            let rot_count = lines.next()?.trim().parse::<usize>().ok()?;
            for _ in 0..rot_count {
                let rot_line = lines.next()?;
                let mut parts = rot_line.split_whitespace();
                let frame = parts.next()?.parse::<f32>().ok()?;
                let _x = parts.next()?.parse::<f32>().ok()?;
                let _y = parts.next()?.parse::<f32>().ok()?;
                let z = parts.next()?.parse::<f32>().ok()?;
                total_frames = total_frames.max(frame);
                if bone_idx == 0 {
                    first_bone.push((frame, z.to_degrees()));
                }
            }
        }
        if first_bone.is_empty() || total_frames <= f32::EPSILON {
            return None;
        }
        first_bone.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut keys: Vec<ModelAutoRotKey> = Vec::with_capacity(first_bone.len());
        for (frame, mut z_deg) in first_bone {
            if let Some(prev) = keys.last().copied() {
                while z_deg - prev.z_deg > 180.0 {
                    z_deg -= 360.0;
                }
                while z_deg - prev.z_deg < -180.0 {
                    z_deg += 360.0;
                }
            }
            keys.push(ModelAutoRotKey { frame, z_deg });
        }
        return Some(ItgModelAutoRot {
            total_frames,
            z_keys: Arc::from(keys),
        });
    }
    None
}

fn itg_resolve_model_material_texture(
    data: &noteskin_itg::NoteskinData,
    model_path: &Path,
    raw_texture: &str,
) -> Option<ItgResolvedModelTexture> {
    let texture_ref = raw_texture.trim().trim_matches('"').trim_matches('\'');
    if texture_ref.is_empty() {
        return None;
    }
    let texture_path = itg_resolve_relative_or_noteskin_path(data, model_path, texture_ref)?;
    let ext = texture_path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if itg_is_texture_image_ext(&ext) {
        Some(ItgResolvedModelTexture::from_path(texture_path))
    } else if ext == "ini" {
        itg_resolve_animated_texture_ini(&texture_path)
    } else if texture_path.is_file() {
        itg_resolve_model_texture_path(data, &texture_path)
    } else {
        None
    }
}

fn itg_parse_milkshape_model_layers(
    data: &noteskin_itg::NoteskinData,
    path: &Path,
) -> Option<Vec<ItgResolvedModelLayer>> {
    let content = fs::read_to_string(path).ok()?;
    if !content.to_ascii_lowercase().contains("milkshape 3d ascii") {
        return None;
    }

    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"));

    let mesh_count = loop {
        let line = lines.next()?;
        if let Some(raw_count) = line.strip_prefix("Meshes:") {
            break raw_count.trim().parse::<usize>().ok()?;
        }
    };

    let mut meshes = Vec::with_capacity(mesh_count);
    let mut model_bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];

    for _ in 0..mesh_count {
        let mesh_header = lines.next()?;
        let material_index = itg_parse_milkshape_mesh_material_index(mesh_header);
        let vertex_count = lines.next()?.trim().parse::<usize>().ok()?;
        let mut mesh_vertices = Vec::with_capacity(vertex_count);
        for _ in 0..vertex_count {
            let line = lines.next()?;
            let mut parts = line.split_whitespace();
            let flags = parts.next()?.parse::<u32>().ok()?;
            let x = parts.next()?.parse::<f32>().ok()?;
            let y = parts.next()?.parse::<f32>().ok()?;
            let z = parts.next()?.parse::<f32>().ok()?;
            let mut u = parts.next()?.parse::<f32>().ok()?;
            let mut v = parts.next()?.parse::<f32>().ok()?;
            if flags & 4 != 0 {
                if u.abs() > f32::EPSILON {
                    u = x / u;
                }
                if v.abs() > f32::EPSILON {
                    v = y / v;
                }
            }
            mesh_vertices.push(ModelVertex {
                pos: [x, y, z],
                uv: [u, v],
                tex_matrix_scale: [
                    if flags & 1 != 0 { 0.0 } else { 1.0 },
                    if flags & 2 != 0 { 0.0 } else { 1.0 },
                ],
            });
        }

        let normal_count = lines.next()?.trim().parse::<usize>().ok()?;
        for _ in 0..normal_count {
            let _ = lines.next()?;
        }

        let triangle_count = lines.next()?.trim().parse::<usize>().ok()?;
        let mut tri_vertices: Vec<ModelVertex> = Vec::with_capacity(triangle_count * 3);
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for _ in 0..triangle_count {
            let line = lines.next()?;
            let mut parts = line.split_whitespace();
            let _flags = parts.next()?;
            let i0 = parts.next()?.parse::<usize>().ok()?;
            let i1 = parts.next()?.parse::<usize>().ok()?;
            let i2 = parts.next()?.parse::<usize>().ok()?;

            let Some(v0) = mesh_vertices.get(i0).copied() else {
                continue;
            };
            let Some(v1) = mesh_vertices.get(i1).copied() else {
                continue;
            };
            let Some(v2) = mesh_vertices.get(i2).copied() else {
                continue;
            };
            for vtx in [v0, v1, v2] {
                bounds[0] = bounds[0].min(vtx.pos[0]);
                bounds[1] = bounds[1].min(vtx.pos[1]);
                bounds[2] = bounds[2].min(vtx.pos[2]);
                bounds[3] = bounds[3].max(vtx.pos[0]);
                bounds[4] = bounds[4].max(vtx.pos[1]);
                bounds[5] = bounds[5].max(vtx.pos[2]);
                tri_vertices.push(vtx);
            }
        }

        if !tri_vertices.is_empty() {
            model_bounds[0] = model_bounds[0].min(bounds[0]);
            model_bounds[1] = model_bounds[1].min(bounds[1]);
            model_bounds[2] = model_bounds[2].min(bounds[2]);
            model_bounds[3] = model_bounds[3].max(bounds[3]);
            model_bounds[4] = model_bounds[4].max(bounds[4]);
            model_bounds[5] = model_bounds[5].max(bounds[5]);
            meshes.push(ItgMilkshapeMeshLayer {
                material_index,
                vertices: tri_vertices,
                bounds,
            });
        }
    }

    if meshes.is_empty() {
        return None;
    }

    let material_count = loop {
        let line = lines.next()?;
        if let Some(raw_count) = line.strip_prefix("Materials:") {
            break raw_count.trim().parse::<usize>().ok()?;
        }
    };
    let mut material_textures = Vec::with_capacity(material_count);
    for _ in 0..material_count {
        let name = lines.next()?.trim().to_string();
        let _ambient = lines.next()?;
        let _diffuse = lines.next()?;
        let _specular = lines.next()?;
        let _emissive = lines.next()?;
        let _shininess = lines.next()?;
        let _transparency = lines.next()?;
        let texture_line = lines.next()?.trim().to_string();
        let _alpha_map = lines.next()?;
        material_textures.push((texture_line, itg_parse_model_material_flags(&name)));
    }

    let fallback_texture = itg_resolve_model_texture_path(data, path);
    let shared_bounds = if model_bounds[0].is_finite()
        && model_bounds[1].is_finite()
        && model_bounds[2].is_finite()
        && model_bounds[3].is_finite()
        && model_bounds[4].is_finite()
        && model_bounds[5].is_finite()
    {
        model_bounds
    } else {
        [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]
    };
    let mut layers = Vec::with_capacity(meshes.len());
    for mesh in meshes {
        let texture_with_flags = if mesh.material_index >= 0 {
            material_textures
                .get(mesh.material_index as usize)
                .and_then(|(raw, flags)| {
                    itg_resolve_model_material_texture(data, path, raw)
                        .map(|resolved| (resolved, *flags))
                })
        } else {
            None
        }
        .or_else(|| {
            fallback_texture
                .clone()
                .map(|resolved| (resolved, ItgModelMaterialFlags::default()))
        });
        let Some((texture, flags)) = texture_with_flags else {
            continue;
        };
        let bounds = if shared_bounds[3] > shared_bounds[0] && shared_bounds[4] > shared_bounds[1] {
            shared_bounds
        } else {
            mesh.bounds
        };
        layers.push(ItgResolvedModelLayer {
            mesh: Arc::new(ModelMesh {
                vertices: mesh.vertices.into(),
                bounds,
            }),
            texture,
            flags,
        });
    }

    if layers.is_empty() {
        None
    } else {
        Some(layers)
    }
}

fn itg_parse_milkshape_model(
    data: &noteskin_itg::NoteskinData,
    path: &Path,
) -> Option<Arc<ModelMesh>> {
    itg_parse_milkshape_model_layers(data, path)
        .and_then(|layers| layers.into_iter().next().map(|layer| layer.mesh))
}

fn itg_is_texture_image_ext(ext: &str) -> bool {
    matches!(ext, "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp")
}

fn itg_resolve_texture_expr(
    data: &noteskin_itg::NoteskinData,
    expr: &str,
    arg0_path: Option<&Path>,
) -> Option<PathBuf> {
    let value = expr.trim();
    if value == ITG_ARG0_TOKEN {
        return arg0_path.map(Path::to_path_buf);
    }
    if value.starts_with("NOTESKIN:GetPath(") {
        let args = itg_extract_quoted_strings(value);
        if args.len() >= 2 {
            return data
                .resolve_path(&args[0], &args[1])
                .or_else(|| data.resolve_path("", &args[1]));
        }
        if args.len() == 1 {
            return data
                .resolve_path(&args[0], "")
                .or_else(|| data.resolve_path("", &args[0]));
        }
    }
    let name = itg_parse_lua_quoted(value).unwrap_or_else(|| value.to_string());
    data.resolve_path(&name, "")
        .or_else(|| data.resolve_path("", &name))
        .or_else(|| {
            if value == "..." {
                arg0_path.map(Path::to_path_buf)
            } else {
                None
            }
        })
}

fn itg_parse_wrapper_commands_from_file(
    path: &Path,
    metrics: &noteskin_itg::IniData,
) -> Option<HashMap<String, String>> {
    let is_lua = path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"));
    if !is_lua {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    let marker = ".. {";
    let marker_idx = content.find(marker)?;
    let open = marker_idx + marker.len() - 1;
    let close = itg_find_matching(&content, open, '{', '}')?;
    Some(itg_parse_commands_block(&content[open + 1..close], metrics))
}

fn itg_apply_frame_override(slot: &mut SpriteSlot, frame: usize) {
    let key = slot.texture_key().to_string();
    let Some((tex_w, tex_h)) = texture_dimensions(&key) else {
        return;
    };
    let (grid_x, grid_y) = assets::sprite_sheet_dims(&key);
    let cols = grid_x.max(1) as usize;
    let rows = grid_y.max(1) as usize;
    let count = (cols * rows).max(1);
    let idx = frame % count;
    let col = idx % cols;
    let row = idx / cols;
    let frame_w = (tex_w / cols as u32).max(1);
    let frame_h = (tex_h / rows as u32).max(1);
    slot.def.src = [col as i32 * frame_w as i32, row as i32 * frame_h as i32];
    slot.def.size = [frame_w as i32, frame_h as i32];
}

fn itg_animation_is_beat_based(data: &noteskin_itg::NoteskinData) -> bool {
    data.metrics
        .get("NoteDisplay", "AnimationIsBeatBased")
        .or_else(|| data.metrics.get("Global", "AnimationIsBeatBased"))
        .and_then(itg_parse_ini_float)
        .is_some_and(|v| v > 0.5)
}

fn itg_sprite_animation_is_beat_based(
    commands: &HashMap<String, String>,
    default_is_beat_based: bool,
) -> bool {
    let mut clock = None;
    let preferred = ["initcommand", "nonecommand", "oncommand", "offcommand"];
    for key in preferred {
        if let Some(script) = commands.get(key)
            && let Some(is_beat) = itg_parse_effectclock_from_commands(script)
        {
            clock = Some(is_beat);
        }
    }
    let mut extras = commands
        .iter()
        .filter(|(key, _)| !preferred.contains(&key.as_str()))
        .map(|(key, script)| (key.as_str(), script.as_str()))
        .collect::<Vec<_>>();
    extras.sort_unstable_by(|a, b| a.0.cmp(b.0));
    for (_, script) in extras {
        if let Some(is_beat) = itg_parse_effectclock_from_commands(script) {
            clock = Some(is_beat);
        }
    }
    clock.unwrap_or(default_is_beat_based)
}

fn itg_parse_effectclock_from_commands(script: &str) -> Option<bool> {
    let mut out = None;
    for raw in script.split(';') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let Some((cmd, args)) = itg_split_command_token(token) else {
            continue;
        };
        if cmd != "effectclock" {
            continue;
        }
        let clock = args
            .first()
            .map(|value| {
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_ascii_lowercase()
            })
            .unwrap_or_else(|| "time".to_string());
        if clock.contains("beat") {
            out = Some(true);
        } else if clock.contains("time") || clock.contains("music") || clock.contains("seconds") {
            out = Some(false);
        }
    }
    out
}

fn itg_slot_from_path_with_frame(path: &Path, frame: usize) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let (grid_x, grid_y) = assets::sprite_sheet_dims(&key);
    let cols = (grid_x.max(1)) as usize;
    let rows = (grid_y.max(1)) as usize;
    let frame_count = (cols * rows).max(1);
    let idx = frame % frame_count;
    let col = idx % cols;
    let row = idx / cols;
    let frame_w = (dims.0 / cols as u32).max(1);
    let frame_h = (dims.1 / rows as u32).max(1);
    let source = Arc::new(SpriteSource::Atlas {
        texture_key: key,
        tex_dims: dims,
    });
    Some(SpriteSlot {
        def: SpriteDefinition {
            src: [col as i32 * frame_w as i32, row as i32 * frame_h as i32],
            size: [frame_w as i32, frame_h as i32],
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        },
        source,
        uv_velocity: [0.0, 0.0],
        uv_offset: [0.0, 0.0],
        note_color_translate: true,
        model: None,
        model_draw: ModelDrawState::default(),
        model_timeline: Arc::from(Vec::<ModelTweenSegment>::new()),
        model_effect: ModelEffectState::default(),
        model_auto_rot_total_frames: 0.0,
        model_auto_rot_z_keys: Arc::from(Vec::<ModelAutoRotKey>::new()),
    })
}

fn itg_slot_from_path_animated(
    path: &Path,
    frame0: usize,
    frame_count: usize,
    frame_delays: Option<&[f32]>,
    beat_based: bool,
) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let (grid_x, grid_y) = assets::sprite_sheet_dims(&key);
    let cols = grid_x.max(1) as usize;
    let rows = grid_y.max(1) as usize;
    let available = (cols * rows).max(1);
    if available <= 1 || frame_count <= 1 {
        return itg_slot_from_path_with_frame(path, frame0);
    }
    let anim_frames = frame_count.min(available).max(1);
    let start = frame0 % available;
    let col = start % cols;
    let row = start / cols;
    let frame_w = (dims.0 / cols as u32).max(1);
    let frame_h = (dims.1 / rows as u32).max(1);
    let default_delay = frame_delays
        .and_then(|delays| delays.first().copied())
        .unwrap_or(1.0)
        .max(1e-6);
    let rate = if beat_based {
        AnimationRate::FramesPerBeat(1.0 / default_delay)
    } else {
        AnimationRate::FramesPerSecond(1.0 / default_delay)
    };
    let frame_durations = frame_delays
        .map(|delays| {
            let mut normalized = Vec::with_capacity(anim_frames);
            let fallback = delays.first().copied().unwrap_or(1.0).max(0.0);
            for idx in 0..anim_frames {
                normalized.push(delays.get(idx).copied().unwrap_or(fallback).max(0.0));
            }
            Arc::<[f32]>::from(normalized)
        })
        .filter(|durations| !durations.is_empty());
    let source = Arc::new(SpriteSource::Animated {
        texture_key: key,
        tex_dims: dims,
        frame_size: [frame_w as i32, frame_h as i32],
        grid: (cols, rows),
        frame_count: anim_frames,
        rate,
        frame_durations,
    });
    Some(SpriteSlot {
        def: SpriteDefinition {
            src: [col as i32 * frame_w as i32, row as i32 * frame_h as i32],
            size: [frame_w as i32, frame_h as i32],
            rotation_deg: 0,
            mirror_h: false,
            mirror_v: false,
        },
        source,
        uv_velocity: [0.0, 0.0],
        uv_offset: [0.0, 0.0],
        note_color_translate: true,
        model: None,
        model_draw: ModelDrawState::default(),
        model_timeline: Arc::from(Vec::<ModelTweenSegment>::new()),
        model_effect: ModelEffectState::default(),
        model_auto_rot_total_frames: 0.0,
        model_auto_rot_z_keys: Arc::from(Vec::<ModelAutoRotKey>::new()),
    })
}

fn itg_slot_from_actor_path_first_sprite(
    data: &noteskin_itg::NoteskinData,
    path: &Path,
) -> Option<SpriteSlot> {
    let is_lua = path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"));
    if !is_lua {
        return itg_slot_from_path(path);
    }

    let content = fs::read_to_string(path).ok()?;
    let decl = itg_parse_actor_decl(&content, &data.metrics);
    let default_anim_is_beat = itg_animation_is_beat_based(data);
    for sprite in decl.sprites {
        let texture_path = itg_resolve_texture_expr(data, &sprite.texture_expr, None)?;
        let anim_is_beat =
            itg_sprite_animation_is_beat_based(&sprite.commands, default_anim_is_beat);
        let slot = if sprite.frame_count > 1 {
            itg_slot_from_path_animated(
                &texture_path,
                sprite.frame0,
                sprite.frame_count,
                sprite.frame_delays.as_deref(),
                anim_is_beat,
            )
            .or_else(|| itg_slot_from_path_with_frame(&texture_path, sprite.frame0))
        } else {
            itg_slot_from_path_with_frame(&texture_path, sprite.frame0)
        }
        .or_else(|| itg_slot_from_path(&texture_path));
        if let Some(slot) = slot {
            return Some(slot);
        }
    }
    None
}

fn itg_slot_from_path_all_frames(
    path: &Path,
    frame_delay: Option<f32>,
    beat_based: bool,
) -> Option<SpriteSlot> {
    let key = itg_texture_key(path)?;
    let (cols, rows) = assets::sprite_sheet_dims(&key);
    let frame_count = (cols.max(1) as usize).saturating_mul(rows.max(1) as usize);
    if frame_count <= 1 {
        return itg_slot_from_path(path);
    }
    let delays = frame_delay.map(|delay| {
        let d = delay.max(1e-6);
        vec![d; frame_count]
    });
    itg_slot_from_path_animated(path, 0, frame_count, delays.as_deref(), beat_based)
        .or_else(|| itg_slot_from_path(path))
}

struct PendingSegment {
    tween: TweenType,
    duration: f32,
    start: ExplosionState,
    target_zoom: Option<f32>,
    target_color: Option<[f32; 4]>,
    target_visible: Option<bool>,
}

fn parse_explosion_animation(script: &str) -> ExplosionAnimation {
    let mut animation = ExplosionAnimation {
        initial: ExplosionState::default(),
        segments: Vec::new(),
        glow: None,
    };

    let mut current_state = ExplosionState::default();
    let mut initial_locked = false;
    let mut pending: Option<PendingSegment> = None;

    let finish_pending = |pending: &mut Option<PendingSegment>,
                          animation: &mut ExplosionAnimation,
                          current_state: &mut ExplosionState| {
        if let Some(segment) = pending.take() {
            let mut end_state = segment.start;
            if let Some(z) = segment.target_zoom {
                end_state.zoom = z;
            }
            if let Some(color) = segment.target_color {
                end_state.color = color;
            }
            if let Some(visible) = segment.target_visible {
                end_state.visible = visible;
            }

            animation.segments.push(ExplosionSegment {
                duration: segment.duration.max(0.0),
                tween: segment.tween,
                start: segment.start,
                end_zoom: segment.target_zoom,
                end_color: segment.target_color,
                end_visible: segment.target_visible,
            });

            *current_state = end_state;
        }
    };

    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }

        let Some((command, args)) = itg_split_command_token(token) else {
            continue;
        };

        match command.as_str() {
            "linear" | "accelerate" | "decelerate" => {
                finish_pending(&mut pending, &mut animation, &mut current_state);
                if let Some(arg) = args.first() {
                    if let Some(duration) = itg_parse_numeric_token(arg) {
                        pending = Some(PendingSegment {
                            tween: match command.as_str() {
                                "accelerate" => TweenType::Accelerate,
                                "decelerate" => TweenType::Decelerate,
                                _ => TweenType::Linear,
                            },
                            duration: duration.max(0.0),
                            start: current_state,
                            target_zoom: None,
                            target_color: None,
                            target_visible: None,
                        });
                        if !initial_locked {
                            animation.initial = current_state;
                            initial_locked = true;
                        }
                    } else {
                        warn!("Failed to parse duration '{arg}' for explosion command '{command}'");
                    }
                } else {
                    warn!("Explosion command '{command}' missing duration argument");
                }
            }
            "sleep" => {
                finish_pending(&mut pending, &mut animation, &mut current_state);
                if let Some(arg) = args.first() {
                    if let Some(duration) = itg_parse_numeric_token(arg) {
                        pending = Some(PendingSegment {
                            tween: TweenType::Linear,
                            duration: duration.max(0.0),
                            start: current_state,
                            target_zoom: None,
                            target_color: None,
                            target_visible: None,
                        });
                        if !initial_locked {
                            animation.initial = current_state;
                            initial_locked = true;
                        }
                    } else {
                        warn!("Failed to parse duration '{arg}' for explosion command '{command}'");
                    }
                } else {
                    warn!("Explosion command '{command}' missing duration argument");
                }
            }
            "stoptweening" | "finishtweening" | "playcommand" | "animate" | "blend" => {
                finish_pending(&mut pending, &mut animation, &mut current_state);
            }
            "diffusealpha" => {
                if let Some(arg) = args.first() {
                    if let Some(value) = itg_parse_numeric_token(arg) {
                        if let Some(segment) = pending.as_mut() {
                            let mut target_color =
                                segment.target_color.unwrap_or(segment.start.color);
                            target_color[3] = value;
                            segment.target_color = Some(target_color);
                        } else {
                            current_state.color[3] = value;
                            if !initial_locked {
                                animation.initial = current_state;
                            }
                        }
                    } else {
                        warn!("Failed to parse diffusealpha value '{arg}' in explosion commands");
                    }
                }
            }
            "zoom" => {
                if let Some(arg) = args.first() {
                    if let Some(value) = itg_parse_numeric_token(arg) {
                        if let Some(segment) = pending.as_mut() {
                            segment.target_zoom = Some(value);
                        } else {
                            current_state.zoom = value;
                            if !initial_locked {
                                animation.initial = current_state;
                            }
                        }
                    } else {
                        warn!("Failed to parse zoom value '{arg}' in explosion commands");
                    }
                }
            }
            "visible" => {
                if let Some(arg) = args.first() {
                    let t = arg.trim().trim_matches('"').trim_matches('\'');
                    let value = t.eq_ignore_ascii_case("true") || t == "1";
                    if let Some(segment) = pending.as_mut() {
                        segment.target_visible = Some(value);
                    } else {
                        current_state.visible = value;
                        if !initial_locked {
                            animation.initial = current_state;
                        }
                    }
                }
            }
            "diffuse" => {
                if let Some(parsed) = parse_color4(&args) {
                    if let Some(segment) = pending.as_mut() {
                        segment.target_color = Some(parsed);
                    } else {
                        current_state.color = parsed;
                        if !initial_locked {
                            animation.initial = current_state;
                        }
                    }
                } else if args.len() >= 3 {
                    let mut parsed = [0.0f32; 4];
                    let mut ok = true;
                    for i in 0..3 {
                        if let Some(v) = itg_parse_numeric_token(&args[i]) {
                            parsed[i] = v
                        } else {
                            warn!(
                                "Failed to parse diffuse component '{}' in explosion commands",
                                args[i]
                            );
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        parsed[3] = if args.len() >= 4 {
                            itg_parse_numeric_token(&args[3]).unwrap_or(current_state.color[3])
                        } else {
                            current_state.color[3]
                        };

                        if let Some(segment) = pending.as_mut() {
                            segment.target_color = Some(parsed);
                        } else {
                            current_state.color = parsed;
                            if !initial_locked {
                                animation.initial = current_state;
                            }
                        }
                    }
                }
            }
            "glowshift" => {
                animation.glow.get_or_insert(GlowEffect {
                    period: 0.0,
                    color1: [1.0, 1.0, 1.0, 0.0],
                    color2: [1.0, 1.0, 1.0, 0.0],
                });
            }
            "effectperiod" => {
                if let Some(arg) = args.first()
                    && let Some(period) = itg_parse_numeric_token(arg)
                {
                    if let Some(glow) = animation.glow.as_mut() {
                        glow.period = period.max(0.0);
                    } else {
                        animation.glow = Some(GlowEffect {
                            period: period.max(0.0),
                            color1: [1.0, 1.0, 1.0, 0.0],
                            color2: [1.0, 1.0, 1.0, 0.0],
                        });
                    }
                }
            }
            "effectcolor1" => {
                if let Some(color) = parse_color4(&args) {
                    if let Some(glow) = animation.glow.as_mut() {
                        glow.color1 = color;
                    } else {
                        animation.glow = Some(GlowEffect {
                            period: 0.0,
                            color1: color,
                            color2: color,
                        });
                    }
                }
            }
            "effectcolor2" => {
                if let Some(color) = parse_color4(&args) {
                    if let Some(glow) = animation.glow.as_mut() {
                        glow.color2 = color;
                    } else {
                        animation.glow = Some(GlowEffect {
                            period: 0.0,
                            color1: color,
                            color2: color,
                        });
                    }
                }
            }
            other => {
                if !other.is_empty() {
                    warn!("Unhandled explosion command '{other}'.");
                }
            }
        }
    }

    finish_pending(&mut pending, &mut animation, &mut current_state);

    if !initial_locked {
        animation.initial = current_state;
    }

    if animation.segments.is_empty() {
        animation.segments.push(ExplosionSegment {
            duration: 0.3,
            tween: TweenType::Linear,
            start: animation.initial,
            end_zoom: Some(animation.initial.zoom),
            end_color: Some([
                animation.initial.color[0],
                animation.initial.color[1],
                animation.initial.color[2],
                0.0,
            ]),
            end_visible: None,
        });
    }

    animation
}

fn itg_split_command_token(token: &str) -> Option<(String, Vec<String>)> {
    let parts = itg_split_call_args(token);
    if parts.is_empty() {
        return None;
    }
    let command = parts[0].trim().to_ascii_lowercase();
    if command.is_empty() {
        return None;
    }
    let args = parts
        .iter()
        .skip(1)
        .map(|part| part.trim().to_string())
        .collect::<Vec<_>>();
    Some((command, args))
}

fn itg_parse_numeric_token(raw: &str) -> Option<f32> {
    raw.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .parse::<f32>()
        .ok()
}

fn parse_color4<T: AsRef<str>>(args: &[T]) -> Option<[f32; 4]> {
    if args.len() == 1 {
        let raw = args[0].as_ref().trim();
        if let Some(color) = itg_parse_color(raw) {
            return Some(color);
        }
        let values = itg_parse_f32_list(raw);
        if values.len() >= 4 {
            return Some([values[0], values[1], values[2], values[3]]);
        }
    }

    if args.len() < 4 {
        return None;
    }
    let mut values = [0.0; 4];
    for (i, arg) in args.iter().enumerate().take(4) {
        values[i] = arg.as_ref().trim().parse().ok()?;
    }
    Some(values)
}

fn texture_dimensions(key: &str) -> Option<(u32, u32)> {
    if let Some(meta) = assets::texture_dims(key) {
        return Some((meta.w, meta.h));
    }
    let path = PathBuf::from("assets").join(key);
    image_dimensions(&path).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        AnimationRate, ModelEffectClock, ModelEffectMode, NUM_QUANTIZATIONS, NoteAnimPart,
        NoteColorType, Quantization, SpriteSource, Style, itg_load_lua_behavior,
        itg_model_draw_program, load_itg_skin, parse_explosion_animation,
    };
    use crate::game::parsing::noteskin_itg;
    use std::collections::{HashMap, HashSet};
    use std::path::Path;

    #[test]
    fn actor_mod_parser_supports_vertalign_and_glow() {
        let mut commands = HashMap::new();
        commands.insert(
            "initcommand".to_string(),
            "vertalign,bottom;glow,0.1,0.2,0.3,0.4".to_string(),
        );
        let (draw, timeline, effect) = itg_model_draw_program(&commands);
        assert!(timeline.is_empty(), "expected no tween timeline");
        assert!(
            (draw.vert_align - 1.0).abs() <= f32::EPSILON,
            "vertalign,bottom should map to 1.0"
        );
        assert!(
            (draw.glow[0] - 0.1).abs() <= 1e-6
                && (draw.glow[1] - 0.2).abs() <= 1e-6
                && (draw.glow[2] - 0.3).abs() <= 1e-6
                && (draw.glow[3] - 0.4).abs() <= 1e-6,
            "glow command should populate base glow color; got {:?}",
            draw.glow
        );
        assert!(
            matches!(effect.mode, ModelEffectMode::None),
            "plain actor mods should not set an effect mode"
        );
    }

    #[test]
    fn loads_default_and_cel_itg_noteskins() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        assert!(load_itg_skin(&style, "default").is_ok());
        assert!(load_itg_skin(&style, "cel").is_ok());
    }

    #[test]
    fn cel_exposes_model_and_uv_motion() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        assert!(!ns.notes.is_empty());
        assert!(ns.notes.iter().any(|slot| slot.model.is_some()));
        assert!(ns.notes.iter().any(|slot| {
            slot.uv_velocity[0].abs() > f32::EPSILON || slot.uv_velocity[1].abs() > f32::EPSILON
        }));
    }

    #[test]
    fn cel_model_tap_note_uses_multiple_material_textures() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let layers = ns
            .note_layers
            .first()
            .expect("cel should expose at least one tap note layer set");
        let textures = layers
            .iter()
            .filter(|slot| slot.model.is_some())
            .map(|slot| slot.texture_key().to_string())
            .collect::<HashSet<_>>();
        assert!(
            textures.len() >= 2,
            "expected cel model tap note to resolve frame + color textures; got {:?}",
            textures
        );
    }

    #[test]
    fn cel_model_tap_note_honors_nomove_material() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let layers = ns
            .note_layers
            .first()
            .expect("cel should expose at least one tap note layer set");
        let mut saw_model = false;
        let mut saw_moving = false;
        let mut saw_nomove = false;
        for layer in layers.iter().filter(|slot| slot.model.is_some()) {
            saw_model = true;
            let moving = layer.uv_velocity[0].abs() > f32::EPSILON
                || layer.uv_velocity[1].abs() > f32::EPSILON;
            if moving {
                saw_moving = true;
            } else {
                saw_nomove = true;
            }
        }
        assert!(
            saw_model,
            "expected at least one model-backed tap-note layer"
        );
        assert!(
            saw_moving,
            "expected at least one scrolling model material in cel tap note"
        );
        assert!(
            saw_nomove,
            "expected at least one nomove model material in cel tap note"
        );
    }

    #[test]
    fn default_exposes_multi_layer_tap_note() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        assert_eq!(ns.notes.len(), ns.note_layers.len());
        assert!(ns.note_layers.iter().any(|layers| layers.len() > 1));
        let q4_layers = ns
            .note_layers
            .first()
            .expect("default should expose 4th-note tap layers");
        assert_eq!(
            q4_layers.len(),
            5,
            "default tap note should have arrow + four circles"
        );
        let circle_layers = q4_layers
            .iter()
            .filter(|slot| slot.texture_key().to_ascii_lowercase().contains("_circle"))
            .count();
        assert_eq!(
            circle_layers, 4,
            "default tap note should keep four circle layers"
        );
    }

    #[test]
    fn default_and_cel_parse_notedisplay_flags() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let default_ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        assert!(
            default_ns
                .note_display_metrics
                .draw_hold_head_for_taps_on_same_row
        );
        assert!(
            default_ns
                .note_display_metrics
                .draw_roll_head_for_taps_on_same_row
        );
        assert!(
            default_ns
                .note_display_metrics
                .tap_hold_roll_on_row_means_hold
        );
        assert!(
            default_ns
                .note_display_metrics
                .flip_head_and_tail_when_reverse
        );
        assert!(default_ns.note_display_metrics.flip_hold_body_when_reverse);

        let cel_ns =
            load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        assert!(
            !cel_ns
                .note_display_metrics
                .draw_hold_head_for_taps_on_same_row
        );
        assert!(
            !cel_ns
                .note_display_metrics
                .draw_roll_head_for_taps_on_same_row
        );
        assert!(cel_ns.note_display_metrics.flip_head_and_tail_when_reverse);
        assert!(cel_ns.note_display_metrics.flip_hold_body_when_reverse);
        assert!(cel_ns.note_display_metrics.top_hold_anchor_when_reverse);
    }

    #[test]
    fn default_and_cel_parse_note_color_translation_metrics() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let default_ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let default_tap =
            default_ns.note_display_metrics.part_texture_translate[NoteAnimPart::Tap as usize];
        assert_eq!(default_tap.note_color_count, 8);
        assert_eq!(default_tap.note_color_type, NoteColorType::Denominator);
        assert!((default_tap.note_color_spacing[1] - 0.125).abs() <= 1e-6);
        let default_tap_8th = default_ns.part_uv_translation(NoteAnimPart::Tap, 0.5, false);
        assert!(default_tap_8th[0].abs() <= f32::EPSILON);
        assert!((default_tap_8th[1] - 0.125).abs() <= 1e-6);

        let cel_ns =
            load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let cel_roll_head =
            cel_ns.note_display_metrics.part_texture_translate[NoteAnimPart::RollHead as usize];
        assert_eq!(cel_roll_head.note_color_count, 8);
        assert_eq!(cel_roll_head.note_color_type, NoteColorType::Denominator);
        assert!((cel_roll_head.note_color_spacing[0] - 0.03125).abs() <= 1e-6);
        let cel_roll_head_8th = cel_ns.part_uv_translation(NoteAnimPart::RollHead, 0.5, false);
        assert!((cel_roll_head_8th[0] - 0.03125).abs() <= 1e-6);
        assert!(cel_roll_head_8th[1].abs() <= f32::EPSILON);
    }

    #[test]
    fn default_does_not_bake_quantization_uv_shift_into_slots() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let q4 = ns
            .note_layers
            .first()
            .and_then(|layers| layers.first())
            .expect("default should expose first 4th-note layer");
        let q8 = ns
            .note_layers
            .get(1)
            .and_then(|layers| layers.first())
            .expect("default should expose first 8th-note layer");
        assert_eq!(q4.def.src, q8.def.src);
        assert!(
            (q4.uv_offset[0] - q8.uv_offset[0]).abs() <= f32::EPSILON
                && (q4.uv_offset[1] - q8.uv_offset[1]).abs() <= f32::EPSILON
        );
    }

    #[test]
    fn ddr_vivid_parses_hold_body_offsets() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-vivid")
            .expect("dance/ddr-vivid should load from assets/noteskins");
        assert!(
            (ns.note_display_metrics
                .start_drawing_hold_body_offset_from_head
                - 0.0)
                .abs()
                <= f32::EPSILON
        );
        assert!(
            (ns.note_display_metrics
                .stop_drawing_hold_body_offset_from_tail
                + 32.0)
                .abs()
                <= 1e-6
        );
        assert!((ns.note_display_metrics.hold_let_go_gray_percent - 0.33).abs() <= 1e-6);
        assert!(
            (ns.note_display_metrics.part_animation[NoteAnimPart::HoldBody as usize].length - 4.0)
                .abs()
                <= 1e-6
        );
        assert!(
            (ns.note_display_metrics.part_animation[NoteAnimPart::RollBody as usize].length - 2.0)
                .abs()
                <= 1e-6
        );
        assert!(
            !ns.note_display_metrics.part_animation[NoteAnimPart::HoldBody as usize].vivid
                && !ns.note_display_metrics.part_animation[NoteAnimPart::RollBody as usize].vivid
        );
    }

    #[test]
    fn vivid_zero_spacing_keeps_model_uv_offsets_across_quants() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg_skin(&style, "vivid").expect("dance/vivid should load from assets/noteskins");
        let q4 = ns
            .note_layers
            .first()
            .and_then(|layers| layers.iter().find(|slot| slot.model.is_some()))
            .expect("vivid should expose model-backed tap note layer for 4th notes");
        let q8 = ns
            .note_layers
            .get(1)
            .and_then(|layers| layers.iter().find(|slot| slot.model.is_some()))
            .expect("vivid should expose model-backed tap note layer for 8th notes");
        assert!(
            (q4.uv_offset[0] - q8.uv_offset[0]).abs() <= f32::EPSILON,
            "vivid should not force note-color X offset when spacing metrics are zero"
        );
        assert!(
            (q4.uv_offset[1] - q8.uv_offset[1]).abs() <= f32::EPSILON,
            "vivid should not force note-color Y offset when spacing metrics are zero"
        );
    }

    #[test]
    fn vivid_tap_note_honors_vertex_tex_matrix_scale_flags() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg_skin(&style, "vivid").expect("dance/vivid should load from assets/noteskins");
        let layers = ns
            .note_layers
            .first()
            .expect("vivid should expose at least one tap note layer set");
        let mut saw_static_uv_vertex = false;
        let mut saw_scrolling_uv_vertex = false;
        for layer in layers.iter().filter_map(|slot| slot.model.as_ref()) {
            for vertex in layer.vertices.iter() {
                let sx = vertex.tex_matrix_scale[0];
                let sy = vertex.tex_matrix_scale[1];
                if sx < 0.5 || sy < 0.5 {
                    saw_static_uv_vertex = true;
                } else {
                    saw_scrolling_uv_vertex = true;
                }
            }
        }
        assert!(
            saw_static_uv_vertex,
            "vivid tap note should include vertices that ignore texture-matrix scroll"
        );
        assert!(
            saw_scrolling_uv_vertex,
            "vivid tap note should include vertices that follow texture-matrix scroll"
        );
    }

    #[test]
    fn ddr_note_receptor_uses_beat_clock_with_mixed_delays() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");
        let slot = ns
            .receptor_off
            .first()
            .expect("ddr-note should define receptor_off for first column");
        let SpriteSource::Animated {
            rate,
            frame_durations,
            ..
        } = slot.source.as_ref()
        else {
            panic!("ddr-note receptor should resolve to animated sprite");
        };
        assert!(
            matches!(rate, AnimationRate::FramesPerBeat(_)),
            "ddr-note receptor expected beat clock animation, got {rate:?}"
        );
        let delays = frame_durations
            .as_ref()
            .expect("ddr-note receptor should preserve per-frame delays");
        assert!(
            delays.len() >= 2,
            "expected at least 2 receptor delays, got {:?}",
            delays
        );
        assert!(
            (delays[0] - 0.2).abs() < 0.01,
            "expected first frame delay near 0.2 beat, got {}",
            delays[0]
        );
        assert!(
            (delays[1] - 0.8).abs() < 0.01,
            "expected second frame delay near 0.8 beat, got {}",
            delays[1]
        );
        assert_eq!(slot.frame_index(0.0, 0.00), 0);
        assert_eq!(slot.frame_index(0.0, 0.19), 0);
        assert_eq!(slot.frame_index(0.0, 0.25), 1);
        assert_eq!(slot.frame_index(0.0, 0.95), 1);
        assert_eq!(slot.frame_index(0.0, 1.05), 0);
    }

    #[test]
    fn ddr_note_hold_body_and_cap_use_per_column_assets() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");

        let expected = [
            ("left hold body inactive", "left hold bottomcap inactive"),
            ("down hold body inactive", "down hold bottomcap inactive"),
            ("up hold body inactive", "up hold bottomcap inactive"),
            ("right hold body inactive", "right hold bottomcap inactive"),
        ];

        for (col, (want_body, want_cap)) in expected.into_iter().enumerate() {
            let visuals = ns.hold_visuals_for_col(col, false);
            let body = visuals
                .body_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold body inactive per column");
            let cap = visuals
                .bottomcap_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold bottomcap inactive per column");
            assert!(
                body.contains(want_body),
                "column {col} expected body containing '{want_body}', got '{body}'"
            );
            assert!(
                cap.contains(want_cap),
                "column {col} expected cap containing '{want_cap}', got '{cap}'"
            );
        }
    }

    #[test]
    fn ddr_note_hold_head_uses_down_hold_head_sheet() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");

        for col in 0..style.num_cols {
            let visuals = ns.hold_visuals_for_col(col, false);
            let inactive = visuals
                .head_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold head inactive");
            let active = visuals
                .head_active
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold head active");
            assert!(
                inactive.contains("down hold head inactive"),
                "column {col} expected Down hold head inactive sheet, got '{inactive}'"
            );
            assert!(
                active.contains("down hold head active"),
                "column {col} expected Down hold head active sheet, got '{active}'"
            );
        }
    }

    #[test]
    fn ddr_note_redir_overrides_default_head_remap() {
        let data =
            noteskin_itg::load_noteskin_data(Path::new("assets/noteskins"), "dance", "ddr-note")
                .expect("dance/ddr-note noteskin data should load");
        let behavior = itg_load_lua_behavior(&data);
        assert!(
            !behavior.remap_head_to_tap,
            "ddr-note should not remap hold heads to tap note via fallback behavior"
        );
        assert!(
            behavior.keep_hold_non_head_button,
            "ddr-note should preserve button on non-head hold parts"
        );
    }

    #[test]
    fn default_skin_still_remaps_hold_head_to_tap() {
        let data =
            noteskin_itg::load_noteskin_data(Path::new("assets/noteskins"), "dance", "default")
                .expect("dance/default noteskin data should load");
        let behavior = itg_load_lua_behavior(&data);
        assert!(
            behavior.remap_head_to_tap,
            "default noteskin should keep hold-head-to-tap remap behavior"
        );
    }

    #[test]
    fn default_skin_blanks_hold_and_roll_explosion() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        assert!(
            ns.hold.explosion.is_none(),
            "default hold explosion should stay blank per NoteSkin.lua"
        );
        assert!(
            ns.roll.explosion.is_none(),
            "default roll explosion should stay blank per NoteSkin.lua"
        );
        for col in 0..style.num_cols {
            let hold_visuals = ns.hold_visuals_for_col(col, false);
            let roll_visuals = ns.hold_visuals_for_col(col, true);
            assert!(
                hold_visuals.explosion.is_none(),
                "default hold visuals should not resolve explosion for col {col}"
            );
            assert!(
                roll_visuals.explosion.is_none(),
                "default roll visuals should not resolve explosion for col {col}"
            );
        }
    }

    #[test]
    fn cel_blank_table_does_not_inherit_default_hold_explosion_blank() {
        let data = noteskin_itg::load_noteskin_data(Path::new("assets/noteskins"), "dance", "cel")
            .expect("dance/cel noteskin data should load");
        let behavior = itg_load_lua_behavior(&data);
        assert!(
            !behavior.blank.contains("hold explosion"),
            "cel should not inherit default hold explosion blank"
        );
        assert!(
            !behavior.blank.contains("roll explosion"),
            "cel should not inherit default roll explosion blank"
        );
        assert!(
            behavior.blank.contains("tap explosion bright")
                && behavior.blank.contains("tap explosion dim"),
            "cel should keep its own tap explosion blank entries"
        );
    }

    #[test]
    fn cel_hold_heads_remap_to_tap_layers() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        for col in 0..style.num_cols {
            let visuals = ns.hold_visuals_for_col(col, false);
            assert!(
                visuals.head_inactive.is_none() && visuals.head_active.is_none(),
                "cel hold heads should use tap-note fallback layers, got inactive={:?} active={:?}",
                visuals
                    .head_inactive
                    .as_ref()
                    .map(|slot| slot.texture_key().to_string()),
                visuals
                    .head_active
                    .as_ref()
                    .map(|slot| slot.texture_key().to_string())
            );
        }
    }

    #[test]
    fn cel_hold_body_resolves_for_all_columns() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        for col in 0..style.num_cols {
            let visuals = ns.hold_visuals_for_col(col, false);
            let body = visuals
                .body_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("cel should provide hold body inactive for each column");
            assert!(
                body.contains("down hold body inactive"),
                "column {col} expected down hold body inactive, got '{body}'"
            );
        }
    }

    #[test]
    fn enchantment_tap_note_uses_linear_frames_animation() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "enchantment")
            .expect("dance/enchantment should load from assets/noteskins");
        let idx = 2 * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
        let slot = ns
            .note_layers
            .get(idx)
            .and_then(|layers| layers.first())
            .expect("enchantment should expose first tap note layer for 4th quant");
        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            ..
        } = slot.source.as_ref()
        else {
            panic!("enchantment tap note should resolve to animated sprite");
        };
        assert_eq!(
            *frame_count, 16,
            "enchantment tap note should use 16 linear frames"
        );
        let delays = frame_durations
            .as_ref()
            .expect("enchantment tap note should preserve linear frame delays");
        assert_eq!(delays.len(), 16, "expected one delay per linear frame");
        assert!(
            (delays[0] - 0.0625).abs() < 1e-4,
            "expected linear frame delay 1/16 beat, got {}",
            delays[0]
        );
        assert_eq!(slot.frame_index(0.0, 0.00), 0);
        assert_eq!(slot.frame_index(0.0, 0.06), 0);
        assert_eq!(slot.frame_index(0.0, 0.07), 1);
        assert_eq!(slot.frame_index(0.0, 1.01), 0);
    }

    #[test]
    fn enchantment_tap_mine_uses_linear_frames_animation() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "enchantment")
            .expect("dance/enchantment should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("enchantment should define first-column mine slot");
        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            ..
        } = mine.source.as_ref()
        else {
            panic!("enchantment mine should resolve to animated sprite");
        };
        assert_eq!(
            *frame_count, 8,
            "enchantment mine should use 8 linear frames"
        );
        let delays = frame_durations
            .as_ref()
            .expect("enchantment mine should preserve linear frame delays");
        assert_eq!(delays.len(), 8, "expected one delay per mine frame");
        assert!(
            (delays[0] - 0.125).abs() < 1e-4,
            "expected linear frame delay 1/8 beat, got {}",
            delays[0]
        );
        assert_eq!(mine.frame_index(0.0, 0.00), 0);
        assert_eq!(mine.frame_index(0.0, 0.12), 0);
        assert_eq!(mine.frame_index(0.0, 0.13), 1);
        assert_eq!(mine.frame_index(0.0, 1.01), 0);
    }

    #[test]
    fn ddr_vivid_hold_explosion_uses_four_animated_frames() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-vivid")
            .expect("dance/ddr-vivid should load from assets/noteskins");
        let hold = ns
            .hold
            .explosion
            .as_ref()
            .expect("ddr-vivid should define hold explosion slot");
        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            rate,
            ..
        } = hold.source.as_ref()
        else {
            panic!("ddr-vivid hold explosion should resolve to animated sprite");
        };
        assert_eq!(
            *frame_count, 4,
            "ddr-vivid hold explosion should use 4 frames"
        );
        let delays = frame_durations
            .as_ref()
            .expect("ddr-vivid hold explosion should preserve frame delays");
        assert_eq!(
            delays.len(),
            4,
            "expected one delay per hold explosion frame"
        );
        assert!(
            delays.iter().all(|delay| (*delay - 0.01).abs() < 1e-4),
            "expected all hold explosion frame delays to be 0.01, got {delays:?}"
        );
        assert_eq!(hold.frame_index(0.0, 0.0), 0);
        let advanced = match rate {
            AnimationRate::FramesPerSecond(_) => hold.frame_index(0.011, 0.0),
            AnimationRate::FramesPerBeat(_) => hold.frame_index(0.0, 0.011),
        };
        assert_eq!(
            advanced, 1,
            "ddr-vivid hold explosion should advance to frame 1 after one delay"
        );
    }

    #[test]
    fn explosion_animation_honors_visible_commands() {
        let anim = parse_explosion_animation("visible,false;sleep,0.1;visible,true");
        let at_start = anim.state_at(0.0);
        let mid_sleep = anim.state_at(0.05);
        let after = anim.state_at(0.11);
        assert!(!at_start.visible, "expected animation to start hidden");
        assert!(
            !mid_sleep.visible,
            "expected sleep segment to keep actor hidden"
        );
        assert!(
            after.visible,
            "expected actor to become visible after final command"
        );
    }

    #[test]
    fn cel_roll_glowshift_keeps_diffuse_and_uses_glow_channel() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let roll = ns
            .roll
            .explosion
            .as_ref()
            .expect("cel should define roll explosion");
        assert!(
            roll.texture_key()
                .to_ascii_lowercase()
                .contains("down hold explosion"),
            "cel roll explosion should resolve to down hold explosion texture"
        );

        let draw_0 = roll.model_draw_at(0.0, 0.0);
        let draw_1 = roll.model_draw_at(0.0125, 0.0);
        assert!(
            draw_0.visible && draw_1.visible,
            "roll explosion should be visible while active"
        );
        assert!(
            (draw_0.tint[3] - draw_1.tint[3]).abs() <= 1e-6,
            "glowshift should not modulate diffuse alpha"
        );

        let glow_alphas = [0.0f32, 0.0125, 0.025, 0.0375]
            .iter()
            .filter_map(|&t| {
                roll.model_glow_at(t, 0.0, draw_0.tint[3])
                    .map(|glow| glow[3])
            })
            .collect::<Vec<_>>();
        assert!(
            glow_alphas.len() >= 2,
            "glowshift should emit visible glow for at least part of its cycle"
        );
        let min_alpha = glow_alphas.iter().copied().fold(f32::INFINITY, f32::min);
        let max_alpha = glow_alphas
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (max_alpha - min_alpha) > 0.05,
            "glow alpha should animate over time for glowshift; got {:?}",
            glow_alphas
        );
    }

    #[test]
    fn cel_w1_tap_explosion_uses_visible_dim_path() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let w1 = ns
            .tap_explosions
            .get("W1")
            .expect("cel should define W1 tap explosion");
        assert!(
            w1.animation.initial.visible,
            "cel W1 tap explosion should start visible"
        );
        assert!(
            w1.animation.initial.color[3] > 0.9,
            "cel W1 tap explosion should start from the dim W1 alpha path"
        );
    }

    #[test]
    fn cel_tap_mine_prefers_model_actor_over_texture_fallback() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("cel should define first-column mine slot");
        assert!(
            mine.model.is_some(),
            "cel mine should come from Tap Mine model actor, not _mine texture fallback"
        );
        assert!(
            ns.mine_frames.first().is_some_and(|slot| slot.is_none()),
            "cel mine uses a single model actor and should not duplicate it as a frame layer"
        );
    }

    #[test]
    fn cel_tap_mine_uv_phase_uses_beat_clock_from_metrics() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        assert!(
            ns.animation_is_beat_based,
            "cel metrics use beat-based noteskin animation"
        );
        assert!(
            (ns.note_display_metrics.part_animation[NoteAnimPart::Mine as usize].length - 1.0)
                .abs()
                <= f32::EPSILON,
            "cel tap mine animation length should be 1 beat"
        );
        let phase = ns.tap_mine_uv_phase(0.5, 1.0, 0.0);
        assert!(
            phase <= 1e-6,
            "one beat should wrap tap mine phase to 0 for cel; got {phase}"
        );
    }

    #[test]
    fn cel_tap_mine_does_not_set_model_spin_effect() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("cel should define first-column mine slot");
        assert!(
            matches!(mine.model_effect.mode, ModelEffectMode::None),
            "cel mine should not set model spin effect via parser commands"
        );
    }

    #[test]
    fn cel_tap_mine_uses_milkshape_bone_rotation_timing() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("cel should define first-column mine slot");
        assert!(
            (mine.model_auto_rot_total_frames - 120.0).abs() <= f32::EPSILON,
            "cel mine should use milkshape total frame count for auto-rotation"
        );
        assert!(
            mine.model_auto_rot_z_keys.len() >= 2,
            "cel mine should expose at least two auto-rotation keys"
        );
        let rot_0 = mine.model_draw_at(0.0, 0.0).rot[2];
        let rot_1 = mine.model_draw_at(1.0, 0.0).rot[2];
        let delta = (rot_1 - rot_0 + 540.0).rem_euclid(360.0) - 180.0;
        assert!(
            (delta - 87.3).abs() <= 0.5,
            "cel mine should rotate by ~87.3 degrees after one second; got delta={delta}"
        );
    }

    #[test]
    fn lambda_tap_mine_spin_uses_beat_clock_and_magnitude() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "lambda")
            .expect("dance/lambda should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("lambda should define first-column mine slot");
        assert!(
            matches!(mine.model_effect.mode, ModelEffectMode::Spin),
            "lambda mine init command should enable spin effect"
        );
        assert!(
            matches!(mine.model_effect.clock, ModelEffectClock::Beat),
            "lambda mine spin should run on beat clock"
        );
        let rot_0 = mine.model_draw_at(0.0, 0.0).rot[2];
        let rot_1 = mine.model_draw_at(0.0, 1.0).rot[2];
        let delta = (rot_1 - rot_0 + 540.0).rem_euclid(360.0) - 180.0;
        assert!(
            (delta + 33.0).abs() <= 1e-3,
            "one beat should rotate lambda mine by -33 degrees; got delta={delta}"
        );
    }

    #[test]
    fn ddr_note_tap_mine_keeps_second_model_layer_as_frame() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("ddr-note should define first-column mine slot");
        let frame = ns
            .mine_frames
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("ddr-note should preserve second mine layer");
        assert!(
            mine.model.is_some(),
            "ddr-note mine fill should be model-backed"
        );
        assert!(
            frame.model.is_some(),
            "ddr-note mine frame should be model-backed second layer"
        );
    }
}
