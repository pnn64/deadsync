//! Native Model material clocks, recorded during the existing worker replay.
//!
//! Samples belong to the song and are immutable after compilation. Their
//! capacity is bounded by material count and the replay's chart-end frame count.
//! Gameplay only searches a retained track; it never replays updates, allocates,
//! grows, reads files or evicts samples. The song releases them on transition.
//! Native controls and full-song diagnostics check the recorded UV parameters.

use crate::SongLuaOverlayModelLayer;

use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaModelTextureUv {
    pub scale: [f32; 2],
    pub offset: [f32; 2],
    pub shift: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct SongLuaModelTextureFrame {
    /// Prewarmed image for this state; None retains the layer's static source.
    pub texture_key: Option<Arc<str>>,
    pub delay: f32,
    pub diffuse: SongLuaModelTextureUv,
    pub glow: SongLuaModelTextureUv,
}

#[derive(Debug, Clone, Copy)]
pub struct SongLuaTextureSample {
    pub second: f32,
    /// Native material state indices; image identities live once in the layer.
    pub diffuse_state: u16,
    pub additive_state: Option<u16>,
    pub diffuse: SongLuaModelTextureUv,
    pub glow: SongLuaModelTextureUv,
    /// Independent secondary frame, with the native diffuse translation.
    pub additive: Option<SongLuaModelTextureUv>,
}

struct MaterialClock {
    frames: Arc<[SongLuaModelTextureFrame]>,
    velocity: [f32; 2],
    cycle: f32,
    frame_starts: Box<[f32]>,
    frame: usize,
    seconds: f32,
}

impl MaterialClock {
    #[inline]
    fn new(frames: Arc<[SongLuaModelTextureFrame]>, velocity: [f32; 2]) -> Self {
        let (cycle, frame_starts) = if frames.len() > 1 {
            let mut starts = Vec::with_capacity(frames.len());
            let mut cycle = 0.0_f32;
            for frame in frames.iter() {
                starts.push(cycle);
                cycle += frame.delay;
            }
            (cycle, starts.into_boxed_slice())
        } else {
            (
                frames
                    .iter()
                    .fold(0.0_f32, |total, frame| total + frame.delay),
                Box::default(),
            )
        };
        Self {
            velocity,
            cycle,
            frame_starts,
            frame: 0,
            seconds: 0.0,
            frames,
        }
    }

    #[inline]
    fn state_only(frames: Arc<[SongLuaModelTextureFrame]>) -> Self {
        Self {
            cycle: frames
                .iter()
                .fold(0.0_f32, |total, frame| total + frame.delay),
            frames,
            velocity: [0.0; 2],
            frame_starts: Box::default(),
            frame: 0,
            seconds: 0.0,
        }
    }

    fn update(&mut self, delta: f32) {
        // ModelTypes.cpp AnimatedTexture::Update uses strict > and advances
        // once, including zero-delta updates when an earlier jump left excess.
        self.seconds += delta;
        if self.seconds > self.frames[self.frame].delay {
            self.seconds -= self.frames[self.frame].delay;
            self.frame = (self.frame + 1) % self.frames.len();
        }
    }

    #[inline]
    fn uv(&self) -> (SongLuaModelTextureUv, SongLuaModelTextureUv) {
        // Preserve the original f32 accumulation order, including +0 at frame
        // zero. State-only additive clocks need no cache; keep the same UV
        // semantics if one is sampled directly.
        let mut seconds = if self.frame_starts.is_empty() {
            self.frames[..self.frame]
                .iter()
                .fold(0.0_f32, |total, frame| total + frame.delay)
        } else {
            self.frame_starts[self.frame]
        };
        seconds += self.seconds;
        let frame = &self.frames[self.frame];
        let mut diffuse = frame.diffuse;
        for axis in 0..2 {
            let shift = self.velocity[axis] * (seconds / self.cycle) * diffuse.scale[axis];
            diffuse.offset[axis] += shift;
            diffuse.shift[axis] += shift;
        }
        (diffuse, frame.glow)
    }

    fn set_state(&mut self, state: i32) {
        // AnimatedTexture::SetState clamps the index without resetting age.
        self.frame = state.clamp(0, self.frames.len() as i32 - 1) as usize;
    }
}

pub(crate) struct Clock {
    diffuse: MaterialClock,
    additive: Option<MaterialClock>,

    samples: Vec<SongLuaTextureSample>,
}

impl Clock {
    pub(crate) fn new<V>(layer: &SongLuaOverlayModelLayer<V>) -> Option<Self> {
        let frames = &layer.texture_frames;
        if layer.additive_frames.len() <= 1
            && (frames.is_empty()
                || (frames.len() == 1
                    && layer.uv_velocity == [0.0; 2]
                    && frames[0].diffuse == frames[0].glow))
        {
            return None;
        }
        let frames = if frames.is_empty() {
            let uv = SongLuaModelTextureUv {
                scale: layer.uv_scale,
                offset: layer.uv_offset,
                shift: layer.uv_tex_shift,
            };
            Arc::from([SongLuaModelTextureFrame {
                texture_key: None,
                delay: 1.0,
                diffuse: uv,
                glow: uv,
            }])
        } else {
            Arc::clone(frames)
        };
        let mut clock = Self {
            diffuse: MaterialClock::new(frames, layer.uv_velocity),
            additive: (!layer.additive_frames.is_empty())
                .then(|| MaterialClock::state_only(Arc::clone(&layer.additive_frames))),
            samples: Vec::new(),
        };
        clock.sample(0.0);
        Some(clock)
    }

    pub(crate) fn with_initial_state<V>(
        layer: &SongLuaOverlayModelLayer<V>,
        state: Option<u32>,
    ) -> Option<Self> {
        let mut clock = Self::new(layer)?;
        if let Some(state) = state {
            clock.set_state(state as i32);
            clock.samples.clear();
            clock.sample(0.0);
        }
        Some(clock)
    }

    pub(crate) fn update(&mut self, delta: f32) {
        self.diffuse.update(delta);
        if let Some(additive) = &mut self.additive {
            additive.update(delta);
        }
    }

    pub(crate) fn set_state(&mut self, state: i32) {
        self.diffuse.set_state(state);
        if let Some(additive) = &mut self.additive {
            additive.set_state(state);
        }
    }

    pub(crate) fn sample(&mut self, second: f32) {
        let (diffuse, glow) = self.diffuse.uv();
        let additive = self.additive.as_ref().map(|clock| {
            let mut uv = clock.frames[clock.frame].glow;
            // Model::DrawPrimitives shares the diffuse texture matrix. The
            // secondary material's own offsets and velocity are not applied.
            for axis in 0..2 {
                let shift =
                    (diffuse.offset[axis] - glow.offset[axis]) / glow.scale[axis] * uv.scale[axis];
                uv.offset[axis] += shift;
                uv.shift[axis] += shift;
            }
            uv
        });
        self.samples.push(SongLuaTextureSample {
            second,
            diffuse,
            glow,
            additive,
            diffuse_state: self.diffuse.frame as u16,
            additive_state: self.additive.as_ref().map(|clock| clock.frame as u16),
        });
    }

    pub(crate) fn finish(self) -> Arc<[SongLuaTextureSample]> {
        self.samples.into()
    }
}

#[cfg(test)]
mod perf {
    include!("../tests/perf/material_clock.rs");
}

pub fn model_texture_at(
    samples: &[SongLuaTextureSample],
    second: f32,
) -> Option<SongLuaTextureSample> {
    // Foreground age subtracts its song-time origin in f32. At an exact baked
    // frame that subtraction can land one ULP below the sample timestamp;
    // do not hold the previous material frame for another update.
    let end = samples.partition_point(|sample| sample.second <= second.next_up());
    end.checked_sub(1).map(|index| samples[index])
}

/// Feed an explicit native update schedule to the same material clock used by
/// worker replay. Controls render these samples through production builders.
#[cfg(feature = "test-support")]
pub fn replay_model_texture<V>(
    layer: &SongLuaOverlayModelLayer<V>,
    updates: &[(f32, f32)],
) -> Arc<[SongLuaTextureSample]> {
    let Some(mut clock) = Clock::new(layer) else {
        return Arc::from([]);
    };
    clock.samples.clear();
    for &(second, delta) in updates {
        clock.update(delta);
        clock.sample(second);
    }
    clock.finish()
}
