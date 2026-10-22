// Frozen from 5ffd4a261; visibility of the clock and its four methods is widened for tests.
use super::*;
pub(super) struct MaterialClock {
    frames: Arc<[SongLuaModelTextureFrame]>,
    velocity: [f32; 2],
    cycle: f32,
    frame: usize,
    seconds: f32,
}

impl MaterialClock {
    pub(super) fn new(frames: Arc<[SongLuaModelTextureFrame]>, velocity: [f32; 2]) -> Self {
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

    pub(super) fn update(&mut self, delta: f32) {
        // ModelTypes.cpp AnimatedTexture::Update uses strict > and advances
        // once, including zero-delta updates when an earlier jump left excess.
        self.seconds += delta;
        if self.seconds > self.frames[self.frame].delay {
            self.seconds -= self.frames[self.frame].delay;
            self.frame = (self.frame + 1) % self.frames.len();
        }
    }

    pub(super) fn uv(&self) -> (SongLuaModelTextureUv, SongLuaModelTextureUv) {
        let mut seconds = 0.0_f32;
        for frame in &self.frames[..self.frame] {
            seconds += frame.delay;
        }
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

    pub(super) fn set_state(&mut self, state: i32) {
        // AnimatedTexture::SetState clamps the index without resetting age.
        self.frame = state.clamp(0, self.frames.len() as i32 - 1) as usize;
    }
}
