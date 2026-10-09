//! Native Model material clocks, recorded during the existing worker replay.
//!
//! Samples belong to the song and are immutable after compilation. Their
//! capacity is bounded by material count and the replay's chart-end frame count.
//! Gameplay only searches a retained track; it never replays updates, allocates,
//! grows, reads files or evicts samples. The song releases them on transition.
//! Native controls and full-song diagnostics check the recorded UV parameters.

use crate::{SongLuaOverlayKind, SongLuaOverlayModelLayer};
use mlua::{Lua, Table};
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaModelTextureUv {
    pub scale: [f32; 2],
    pub offset: [f32; 2],
    pub shift: [f32; 2],
}

#[derive(Debug, Clone, Copy)]
pub struct SongLuaModelTextureFrame {
    pub delay: f32,
    pub diffuse: SongLuaModelTextureUv,
    pub glow: SongLuaModelTextureUv,
}

#[derive(Debug, Clone, Copy)]
pub struct SongLuaTextureSample {
    pub second: f32,
    pub diffuse: SongLuaModelTextureUv,
    pub glow: SongLuaModelTextureUv,
    /// Independent secondary frame, with the native diffuse translation.
    pub additive: Option<SongLuaModelTextureUv>,
}

struct MaterialClock {
    frames: Arc<[SongLuaModelTextureFrame]>,
    velocity: [f32; 2],
    cycle: f32,
    frame: usize,
    seconds: f32,
}

impl MaterialClock {
    fn new(frames: Arc<[SongLuaModelTextureFrame]>, velocity: [f32; 2]) -> Self {
        Self {
            velocity,
            cycle: frames
                .iter()
                .fold(0.0_f32, |total, frame| total + frame.delay),
            frame: 0,
            seconds: 0.0,
            frames,
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

    fn uv(&self) -> (SongLuaModelTextureUv, SongLuaModelTextureUv) {
        let mut seconds = 0.0_f32;
        for frame in &self.frames[..self.frame] {
            seconds += frame.delay;
        }
        seconds += self.seconds;
        let frame = self.frames[self.frame];
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
    delta: Option<f32>,
    samples: Vec<SongLuaTextureSample>,
}

impl Clock {
    pub(crate) fn new<V>(layer: &SongLuaOverlayModelLayer<V>) -> Option<Self> {
        let frames = &layer.texture_frames;
        if layer.additive_frames.len() <= 1 && (frames.is_empty()
            || (frames.len() == 1 && layer.uv_velocity == [0.0; 2]
                && frames[0].diffuse == frames[0].glow)) {
            return None;
        }
        let frames = if frames.is_empty() {
            let uv = SongLuaModelTextureUv {
                scale: layer.uv_scale, offset: layer.uv_offset, shift: layer.uv_tex_shift,
            };
            Arc::from([SongLuaModelTextureFrame { delay: 1.0, diffuse: uv, glow: uv }])
        } else { Arc::clone(frames) };
        let mut clock = Self {
            diffuse: MaterialClock::new(frames, layer.uv_velocity),
            additive: (!layer.additive_frames.is_empty()).then(||
                MaterialClock::new(Arc::clone(&layer.additive_frames), [0.0; 2])),
            delta: None,
            samples: Vec::new(),
        };
        clock.sample(0.0);
        Some(clock)
    }

    pub(crate) fn update(&mut self, delta: f32) {
        self.diffuse.update(delta);
        if let Some(additive) = &mut self.additive { additive.update(delta); }
    }

    fn set_state(&mut self, state: i32) {
        self.diffuse.set_state(state);
        if let Some(additive) = &mut self.additive { additive.set_state(state); }
    }

    fn sample(&mut self, second: f32) {
        let (diffuse, glow) = self.diffuse.uv();
        let additive = self.additive.as_ref().map(|clock| {
            let mut uv = clock.frames[clock.frame].glow;
            // Model::DrawPrimitives shares the diffuse texture matrix. The
            // secondary material's own offsets and velocity are not applied.
            for axis in 0..2 {
                let shift = (diffuse.offset[axis] - glow.offset[axis])
                    / glow.scale[axis] * uv.scale[axis];
                uv.offset[axis] += shift;
                uv.shift[axis] += shift;
            }
            uv
        });
        self.samples.push(SongLuaTextureSample { second, diffuse, glow, additive });
    }

    pub(crate) fn finish(self) -> Arc<[SongLuaTextureSample]> {
        self.samples.into()
    }
}

struct Capture(FxHashMap<usize, Vec<Option<Clock>>>);

pub(crate) fn install<S, V, A>(
    lua: &Lua,
    overlays: &[crate::SongLuaOverlayCompileActor<SongLuaOverlayKind<S, V, A>>],
    read_layer: fn(&S) -> Option<SongLuaOverlayModelLayer<V>>,
) {
    let actors = overlays
        .iter()
        .filter_map(|overlay| {
            let clocks = match &overlay.actor.kind {
                SongLuaOverlayKind::Model { layers } => {
                    layers.iter().map(Clock::new).collect::<Vec<_>>()
                }
                SongLuaOverlayKind::NoteskinActor { slots, .. } => slots
                    .iter()
                    .map(|slot| read_layer(slot).as_ref().and_then(Clock::new))
                    .collect(),
                _ => return None,
            };
            let mut clocks = clocks;
            if let Some(state) = overlay.actor.initial_state.sprite_state_index {
                for clock in clocks.iter_mut().flatten() {
                    clock.set_state(state as i32);
                    clock.samples.clear();
                    clock.sample(0.0);
                }
            }
            clocks
                .iter()
                .any(Option::is_some)
                .then_some((overlay.table.to_pointer() as usize, clocks))
        })
        .collect();
    lua.set_app_data(Capture(actors));
    crate::lua_util::invalidate_compile_update_plan(lua);
}

pub(crate) fn active(lua: &Lua) -> bool {
    lua.app_data_ref::<Capture>()
        .is_some_and(|capture| !capture.0.is_empty())
}

pub(crate) fn contains(lua: &Lua, actor: &Table) -> bool {
    lua.app_data_ref::<Capture>()
        .is_some_and(|capture| capture.0.contains_key(&(actor.to_pointer() as usize)))
}

pub(crate) fn begin(lua: &Lua, actor: &Table, delta: Option<f64>) {
    if let Some(mut capture) = lua.app_data_mut::<Capture>()
        && let Some(clocks) = capture.0.get_mut(&(actor.to_pointer() as usize))
    {
        for clock in clocks.iter_mut().flatten() {
            clock.delta = delta.map(|delta| delta as f32);
        }
    }
}

pub(crate) fn update(lua: &Lua, actor: &Table) {
    if let Some(mut capture) = lua.app_data_mut::<Capture>()
        && let Some(clocks) = capture.0.get_mut(&(actor.to_pointer() as usize))
    {
        for clock in clocks.iter_mut().flatten() {
            if let Some(delta) = clock.delta.take() {
                clock.update(delta);
            }
        }
    }
}

pub(crate) fn set_state(lua: &Lua, actor: &Table, state: i32) {
    if let Some(mut capture) = lua.app_data_mut::<Capture>()
        && let Some(clocks) = capture.0.get_mut(&(actor.to_pointer() as usize))
    {
        for clock in clocks.iter_mut().flatten() {
            clock.set_state(state);
        }
    }
}

pub(crate) fn sample(lua: &Lua, second: f32) {
    if let Some(mut capture) = lua.app_data_mut::<Capture>() {
        for clocks in capture.0.values_mut() {
            for clock in clocks.iter_mut().flatten() {
                clock.sample(second);
            }
        }
    }
}

pub(crate) fn take<S, V, A>(
    lua: &Lua,
    overlays: &mut [crate::SongLuaOverlayCompileActor<SongLuaOverlayKind<S, V, A>>],
) {
    let Some(mut capture) = lua.remove_app_data::<Capture>() else {
        return;
    };
    for overlay in overlays {
        let Some(clocks) = capture.0.remove(&(overlay.table.to_pointer() as usize)) else {
            continue;
        };
        let samples = clocks
            .into_iter()
            .map(|clock| clock.map_or_else(|| Arc::from([]), Clock::finish))
            .collect::<Vec<_>>();
        match &mut overlay.actor.kind {
            SongLuaOverlayKind::Model { layers } => {
                for (layer, samples) in Arc::make_mut(layers).iter_mut().zip(samples) {
                    layer.texture_samples = samples;
                }
            }
            SongLuaOverlayKind::NoteskinActor {
                texture_samples, ..
            } => *texture_samples = samples.into(),
            _ => {}
        }
    }
}

pub fn model_texture_at(
    samples: &[SongLuaTextureSample],
    second: f32,
) -> Option<SongLuaTextureSample> {
    let end = samples.partition_point(|sample| sample.second <= second);
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
