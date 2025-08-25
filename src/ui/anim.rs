// src/ui/anim.rs
//! StepMania-like tween segments with a tiny queueing system.
//!
//! Usage sketch:
//! ```ignore
//! use crate::ui::anim::*;
//!
//! // initialize per-actor animation state
//! let mut tw = TweenSeq::new(TweenState::default());
//!
//! // queue a few segments (chained like StepMania commands)
//! tw.push(linear(0.40).xy(640.0, 360.0).zoom(256.0, 256.0).alpha(1.0));
//! tw.push(decelerate(0.25).addx(120.0));
//! tw.push(sleep(0.10));
//! tw.push(accelerate(0.30).diffuse_rgb(1.0, 0.25, 0.25));
//!
//! // each frame
//! tw.update(dt);
//! let s = tw.state();
//! let actor = act!(sprite("logo.png"):
//!     align(0.5, 0.5):
//!     xy(s.x, s.y):
//!     zoomto(s.w, s.h):
//!     diffuse(s.tint[0], s.tint[1], s.tint[2], s.tint[3])
//! );
//! ```
#![allow(unused_assignments,dead_code)]
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ease {
    /// StepMania: `linear(t)`
    Linear,
    /// StepMania: `accelerate(t)` (quad-in)
    Accelerate,
    /// StepMania: `decelerate(t)` (quad-out)
    Decelerate,
}

fn ease_apply(e: Ease, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match e {
        Ease::Linear => t,
        Ease::Accelerate => t * t,
        Ease::Decelerate => 1.0 - (1.0 - t) * (1.0 - t),
    }
}

#[derive(Clone, Debug)]
pub struct TweenState {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub hx: f32,
    pub vy: f32,
    pub tint: [f32; 4],
    pub visible: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    pub rot_z: f32, // NEW: degrees
    pub crop_l: f32,
    pub crop_r: f32,
    pub crop_t: f32,
    pub crop_b: f32,
}

impl Default for TweenState {
    fn default() -> Self {
        Self {
            x: 0.0, y: 0.0, w: 0.0, h: 0.0,
            hx: 0.5, vy: 0.5,
            tint: [1.0, 1.0, 1.0, 1.0],
            visible: true,
            flip_x: false,
            flip_y: false,
            rot_z: 0.0, // NEW
            crop_l: 0.0, crop_r: 0.0, crop_t: 0.0, crop_b: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Target {
    Abs(f32),
    Rel(f32),
}

#[derive(Clone, Debug)]
enum BuildOp {
    X(Target),
    Y(Target),
    Width(Target),
    Height(Target),
    ZoomBoth(Target),
    ZoomX(Target),
    ZoomY(Target),
    Tint(Target, Target, Target, Target),
    Visible(bool),
    FlipX(bool),
    FlipY(bool),
    RotZ(Target),
    CropL(Target),
    CropR(Target),
    CropT(Target),
    CropB(Target),
}

#[derive(Clone, Debug)]
struct OpPrepared {
    kind: PreparedKind,
}

#[derive(Clone, Debug)]
enum PreparedKind {
    X { from: f32, to: f32 },
    Y { from: f32, to: f32 },
    WX { from: f32, to: f32 },
    HY { from: f32, to: f32 },
    Tint { from: [f32; 4], to: [f32; 4] },
    Visible(bool),
    FlipX(bool),
    FlipY(bool),
    RotZ { from: f32, to: f32 },
    CropL { from: f32, to: f32 },
    CropR { from: f32, to: f32 },
    CropT { from: f32, to: f32 },
    CropB { from: f32, to: f32 },    
}

impl OpPrepared {
    #[inline(always)]
    fn apply_lerp(&self, s: &mut TweenState, a: f32) {
        match self.kind {
            PreparedKind::X { from, to }  => s.x = from + (to - from) * a,
            PreparedKind::Y { from, to }  => s.y = from + (to - from) * a,
            PreparedKind::WX { from, to } => s.w = from + (to - from) * a,
            PreparedKind::HY { from, to } => s.h = from + (to - from) * a,
            PreparedKind::Tint { from, to } => {
                for i in 0..4 { s.tint[i] = from[i] + (to[i] - from[i]) * a; }
            }
            PreparedKind::Visible(v) => s.visible = v,
            PreparedKind::FlipX(v) => s.flip_x = v,
            PreparedKind::FlipY(v) => s.flip_y = v,
            PreparedKind::RotZ { from, to } => s.rot_z = from + (to - from) * a,
            PreparedKind::CropL { from, to } => s.crop_l = from + (to - from) * a,
            PreparedKind::CropR { from, to } => s.crop_r = from + (to - from) * a,
            PreparedKind::CropT { from, to } => s.crop_t = from + (to - from) * a,
            PreparedKind::CropB { from, to } => s.crop_b = from + (to - from) * a,            
        }
    }

    #[inline(always)]
    fn apply_final(&self, s: &mut TweenState) {
        self.apply_lerp(s, 1.0);
    }
}

/// A single StepMania-like segment (e.g., `linear(0.4)` + property ops).
#[derive(Clone, Debug)]
pub struct Segment {
    ease: Ease,
    dur: f32,
    elapsed: f32,
    // ops requested by the user (absolute/relative); compiled to prepared ops on first tick
    build_ops: Vec<BuildOp>,
    prepared: Vec<OpPrepared>,
    prepared_once: bool,
}

impl Segment {
    fn new(ease: Ease, dur: f32, build_ops: Vec<BuildOp>) -> Self {
        Self {
            ease,
            dur: dur.max(0.0),
            elapsed: 0.0,
            build_ops,
            prepared: Vec::new(),
            prepared_once: false,
        }
    }

    fn prepare_if_needed(&mut self, s: &TweenState) {
        if self.prepared_once { return; }
        self.prepared.clear();

        for op in &self.build_ops {
            match *op {
                BuildOp::X(t) => {
                    let to = match t { Target::Abs(v) => v, Target::Rel(dv) => s.x + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::X { from: s.x, to } });
                }
                BuildOp::Y(t) => {
                    let to = match t { Target::Abs(v) => v, Target::Rel(dv) => s.y + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::Y { from: s.y, to } });
                }
                BuildOp::Width(t) => {
                    let to = match t { Target::Abs(v) => v, Target::Rel(dv) => s.w + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::WX { from: s.w, to } });
                }
                BuildOp::Height(t) => {
                    let to = match t { Target::Abs(v) => v, Target::Rel(dv) => s.h + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::HY { from: s.h, to } });
                }
                BuildOp::ZoomBoth(t) => {
                    let (fx, fy) = match t { Target::Abs(v) => (v, v), Target::Rel(dv) => (1.0 + dv, 1.0 + dv) };
                    self.prepared.push(OpPrepared { kind: PreparedKind::WX { from: s.w, to: s.w * fx } });
                    self.prepared.push(OpPrepared { kind: PreparedKind::HY { from: s.h, to: s.h * fy } });
                }
                BuildOp::ZoomX(t) => {
                    let f = match t { Target::Abs(v) => v, Target::Rel(dv) => 1.0 + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::WX { from: s.w, to: s.w * f } });
                }
                BuildOp::ZoomY(t) => {
                    let f = match t { Target::Abs(v) => v, Target::Rel(dv) => 1.0 + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::HY { from: s.h, to: s.h * f } });
                }
                BuildOp::Tint(tr, tg, tb, ta) => {
                    let to0 = match tr { Target::Abs(v) => v, Target::Rel(dv) => s.tint[0] + dv };
                    let to1 = match tg { Target::Abs(v) => v, Target::Rel(dv) => s.tint[1] + dv };
                    let to2 = match tb { Target::Abs(v) => v, Target::Rel(dv) => s.tint[2] + dv };
                    let to3 = match ta { Target::Abs(v) => v, Target::Rel(dv) => s.tint[3] + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::Tint { from: s.tint, to: [to0, to1, to2, to3] } });
                }
                BuildOp::Visible(v) => {
                    self.prepared.push(OpPrepared { kind: PreparedKind::Visible(v) });
                }
                BuildOp::FlipX(v) => {
                    self.prepared.push(OpPrepared { kind: PreparedKind::FlipX(v) });
                }
                BuildOp::FlipY(v) => {
                    self.prepared.push(OpPrepared { kind: PreparedKind::FlipY(v) });
                }
                BuildOp::RotZ(t) => {
                    let to = match t { Target::Abs(v) => v, Target::Rel(dv) => s.rot_z + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::RotZ { from: s.rot_z, to } });
                }
                BuildOp::CropL(t) => {
                    let to = match t { Target::Abs(v) => v, Target::Rel(dv) => s.crop_l + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::CropL { from: s.crop_l, to } });
                }
                BuildOp::CropR(t) => {
                    let to = match t { Target::Abs(v) => v, Target::Rel(dv) => s.crop_r + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::CropR { from: s.crop_r, to } });
                }
                BuildOp::CropT(t) => {
                    let to = match t { Target::Abs(v) => v, Target::Rel(dv) => s.crop_t + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::CropT { from: s.crop_t, to } });
                }
                BuildOp::CropB(t) => {
                    let to = match t { Target::Abs(v) => v, Target::Rel(dv) => s.crop_b + dv };
                    self.prepared.push(OpPrepared { kind: PreparedKind::CropB { from: s.crop_b, to } });
                }                
            }
        }

        self.prepared_once = true;
    }

    fn update(&mut self, s: &mut TweenState, mut dt: f32) -> bool {
        // returns true if finished
        if self.dur == 0.0 {
            // apply final immediately
            self.prepare_if_needed(s);
            for p in &self.prepared {
                p.apply_final(s);
            }
            return true;
        }

        self.prepare_if_needed(s);

        let was_elapsed = self.elapsed;
        self.elapsed = (self.elapsed + dt).min(self.dur);
        dt -= self.elapsed - was_elapsed;

        let a = ease_apply(self.ease, self.elapsed / self.dur);

        for p in &self.prepared {
            p.apply_lerp(s, a);
        }

        self.elapsed >= self.dur
    }
}

/// Public builder API (mirrors StepMania commands inside a time segment).
#[derive(Clone, Debug)]
pub struct SegmentBuilder {
    ease: Ease,
    dur: f32,
    ops: Vec<BuildOp>,
}

impl SegmentBuilder {
    fn new(ease: Ease, dur: f32) -> Self {
        Self { ease, dur: dur.max(0.0), ops: Vec::new() }
    }

    // --- position ---
    pub fn x(mut self, v: f32) -> Self { self.ops.push(BuildOp::X(Target::Abs(v))); self }
    pub fn y(mut self, v: f32) -> Self { self.ops.push(BuildOp::Y(Target::Abs(v))); self }
    pub fn xy(mut self, x: f32, y: f32) -> Self {
        self.ops.push(BuildOp::X(Target::Abs(x)));
        self.ops.push(BuildOp::Y(Target::Abs(y)));
        self
    }
    pub fn addx(mut self, dx: f32) -> Self { self.ops.push(BuildOp::X(Target::Rel(dx))); self }
    pub fn addy(mut self, dy: f32) -> Self { self.ops.push(BuildOp::Y(Target::Rel(dy))); self }

    // --- absolute size (zoomto/setsize) ---
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.ops.push(BuildOp::Width(Target::Abs(w)));
        self.ops.push(BuildOp::Height(Target::Abs(h)));
        self
    }

    // --- StepMania zoom semantics (scale factors) ---
    pub fn zoom(mut self, f: f32, g: f32) -> Self {
        if (f - g).abs() < f32::EPSILON {
            self.ops.push(BuildOp::ZoomBoth(Target::Abs(f)));
        } else {
            self.ops.push(BuildOp::ZoomX(Target::Abs(f)));
            self.ops.push(BuildOp::ZoomY(Target::Abs(g)));
        }
        self
    }
    pub fn zoomx(mut self, f: f32) -> Self { self.ops.push(BuildOp::ZoomX(Target::Abs(f))); self }
    pub fn zoomy(mut self, f: f32) -> Self { self.ops.push(BuildOp::ZoomY(Target::Abs(f))); self }
    pub fn addzoomx(mut self, df: f32) -> Self { self.ops.push(BuildOp::ZoomX(Target::Rel(df))); self }
    pub fn addzoomy(mut self, df: f32) -> Self { self.ops.push(BuildOp::ZoomY(Target::Rel(df))); self }

    // --- tint / alpha ---
    pub fn diffuse(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.ops.push(BuildOp::Tint(Target::Abs(r), Target::Abs(g), Target::Abs(b), Target::Abs(a)));
        self
    }
    pub fn diffuse_rgb(mut self, r: f32, g: f32, b: f32) -> Self {
        self.ops.push(BuildOp::Tint(Target::Abs(r), Target::Abs(g), Target::Abs(b), Target::Rel(0.0)));
        self
    }
    pub fn alpha(mut self, a: f32) -> Self {
        self.ops.push(BuildOp::Tint(Target::Rel(0.0), Target::Rel(0.0), Target::Rel(0.0), Target::Abs(a)));
        self
    }

    // --- instants ---
    pub fn set_visible(mut self, v: bool) -> Self { self.ops.push(BuildOp::Visible(v)); self }
    pub fn flip_x(mut self, v: bool) -> Self { self.ops.push(BuildOp::FlipX(v)); self }
    pub fn flip_y(mut self, v: bool) -> Self { self.ops.push(BuildOp::FlipY(v)); self }

    // --- rotation (degrees) ---  NEW
    pub fn rotationz(mut self, deg: f32) -> Self { self.ops.push(BuildOp::RotZ(Target::Abs(deg))); self }
    pub fn addrotationz(mut self, ddeg: f32) -> Self { self.ops.push(BuildOp::RotZ(Target::Rel(ddeg))); self }

    pub fn cropleft(mut self, v: f32) -> Self { self.ops.push(BuildOp::CropL(Target::Abs(v))); self }
    pub fn cropright(mut self, v: f32) -> Self { self.ops.push(BuildOp::CropR(Target::Abs(v))); self }
    pub fn croptop(mut self, v: f32) -> Self { self.ops.push(BuildOp::CropT(Target::Abs(v))); self }
    pub fn cropbottom(mut self, v: f32) -> Self { self.ops.push(BuildOp::CropB(Target::Abs(v))); self }
    pub fn addcropleft(mut self, dv: f32) -> Self { self.ops.push(BuildOp::CropL(Target::Rel(dv))); self }
    pub fn addcropright(mut self, dv: f32) -> Self { self.ops.push(BuildOp::CropR(Target::Rel(dv))); self }
    pub fn addcroptop(mut self, dv: f32) -> Self { self.ops.push(BuildOp::CropT(Target::Rel(dv))); self }
    pub fn addcropbottom(mut self, dv: f32) -> Self { self.ops.push(BuildOp::CropB(Target::Rel(dv))); self }

    pub fn build(self) -> Step { Step::Segment(Segment::new(self.ease, self.dur, self.ops)) }
}

/// Construct a `linear(t)` segment builder.
pub fn linear(dur: f32) -> SegmentBuilder {
    SegmentBuilder::new(Ease::Linear, dur)
}

/// Construct an `accelerate(t)` (quad-in) segment builder.
pub fn accelerate(dur: f32) -> SegmentBuilder {
    SegmentBuilder::new(Ease::Accelerate, dur)
}

/// Construct a `decelerate(t)` (quad-out) segment builder.
pub fn decelerate(dur: f32) -> SegmentBuilder {
    SegmentBuilder::new(Ease::Decelerate, dur)
}

/// Delay with no property changes (StepMania: `sleep(t)`).
pub fn sleep(dur: f32) -> Step {
    Step::Sleep(dur.max(0.0))
}

/// A queued step (segment or sleep).
#[derive(Clone, Debug)]
pub enum Step {
    Segment(Segment),
    Sleep(f32),
}

#[derive(Clone, Debug)]
pub struct TweenSeq {
    state: TweenState,
    queue: VecDeque<Step>,
    current: Option<Step>,
}

impl TweenSeq {
    pub fn new(initial: TweenState) -> Self {
        Self {
            state: initial,
            queue: VecDeque::new(),
            current: None,
        }
    }

    pub fn clear(&mut self) {
        self.queue.clear();
        self.current = None;
    }

    pub fn push(&mut self, step: SegmentBuilder) {
        self.queue.push_back(step.build());
    }

    pub fn push_step(&mut self, step: Step) {
        self.queue.push_back(step);
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_none() && self.queue.is_empty()
    }

    pub fn state(&self) -> &TweenState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut TweenState {
        &mut self.state
    }

    pub fn update(&mut self, mut dt: f32) {
        while dt > 0.0 {
            // pull a step if needed
            if self.current.is_none() {
                self.current = self.queue.pop_front();
                if self.current.is_none() {
                    // nothing to do
                    break;
                }
            }

            // drive current step
            let finished_now = match self.current.as_mut().unwrap() {
                Step::Sleep(t) => {
                    let take = dt.min(*t);
                    *t -= take;
                    dt -= take;
                    *t <= 0.0
                }
                Step::Segment(seg) => {
                    // Segment::update consumes only part of dt (capped by segment duration)
                    let before = seg.elapsed;
                    let done = seg.update(&mut self.state, dt);
                    let consumed = (seg.elapsed - before).max(0.0);
                    dt -= consumed;
                    done
                }
            };

            if finished_now {
                // Take the finished step out of `current`.
                if let Some(step) = self.current.take() {
                    // If it was a segment, snap to exact targets.
                    if let Step::Segment(seg) = step {
                        for p in &seg.prepared {
                            p.apply_final(&mut self.state);
                        }
                    }
                }
                // Loop continues to consume remaining dt on next steps.
            } else {
                // Current step still running; exit this update.
                break;
            }
        }
    }
}
