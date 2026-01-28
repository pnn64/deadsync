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
#![allow(unused_assignments, dead_code)]
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq)] // <-- removed Eq
pub enum Ease {
    /// StepMania: `linear(t)`
    Linear,
    /// StepMania: `accelerate(t)` (quad-in)
    Accelerate,
    /// StepMania: `decelerate(t)` (quad-out)
    Decelerate,
    /// StepMania: `smooth(t)` — classic in–out S curve.
    Smooth,
    /// StepMania: `ease(time, fEase)` — 1D Bezier curve, fEase in [-100,100].
    EaseInOut { bias: f32 },
}

#[inline(always)]
fn ease_apply(e: Ease, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);

    #[inline(always)]
    fn ease_in_quad(u: f32) -> f32 {
        u * u
    }

    #[inline(always)]
    fn ease_out_quad(u: f32) -> f32 {
        1.0 - (1.0 - u) * (1.0 - u)
    }

    #[inline(always)]
    fn ease_weighted_inout(t: f32, fease: f32) -> f32 {
        // Map fEase [-100,100] → split around 0.5.
        // 0   => d1=d2=0.5 (symmetry)
        // >0  => out-heavy (shorter accel, longer decel)
        // <0  => in-heavy  (longer accel, shorter decel)
        let w = fease.abs().min(100.0) * 0.01; // [0,1]
        let s = if fease > 0.0 {
            1.0
        } else if fease < 0.0 {
            -1.0
        } else {
            0.0
        };
        let delta = 0.5 * w; // [0,0.5]
        let d1 = (0.5 - s * delta).clamp(0.0, 1.0);
        let d2 = (1.0 - d1).max(0.0);

        if d1 == 0.0 {
            return ease_out_quad(t);
        } // +100 → pure ease-out
        if d2 == 0.0 {
            return ease_in_quad(t);
        } // -100 → pure ease-in

        if t <= d1 {
            let u = (t / d1).clamp(0.0, 1.0);
            0.5 * ease_in_quad(u)
        } else {
            let u = ((t - d1) / d2).clamp(0.0, 1.0);
            0.5 + 0.5 * ease_out_quad(u)
        }
    }

    match e {
        Ease::Linear => t,
        Ease::Accelerate => ease_in_quad(t),
        Ease::Decelerate => ease_out_quad(t),
        Ease::Smooth => ease_weighted_inout(t, 0.0),
        Ease::EaseInOut { bias } => eval_ease_p_for_f_ease(t, bias),
    }
}

#[inline(always)]
fn bezier_coeff(c1: f32, c2: f32, c3: f32, c4: f32) -> (f32, f32, f32, f32) {
    let d = c1;
    let c = 3.0 * (c2 - c1);
    let b = 3.0 * (c3 - c2) - c;
    let a = c4 - c1 - c - b;
    (a, b, c, d)
}

#[inline(always)]
fn cubic_eval((a, b, c, d): (f32, f32, f32, f32), t: f32) -> f32 {
    ((a * t + b) * t + c) * t + d
}

#[inline(always)]
fn cubic_slope((a, b, c, _): (f32, f32, f32, f32), t: f32) -> f32 {
    3.0 * a * t * t + 2.0 * b * t + c
}

// ITGmania: RageBezier2D::EvaluateYFromX()
#[inline(always)]
fn bezier_y_from_x(
    x: f32,
    c1x: f32,
    c1y: f32,
    c2x: f32,
    c2y: f32,
    c3x: f32,
    c3y: f32,
    c4x: f32,
    c4y: f32,
) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let px = bezier_coeff(c1x, c2x, c3x, c4x);
    let py = bezier_coeff(c1y, c2y, c3y, c4y);

    let start = px.3;
    let end = px.0 + px.1 + px.2 + px.3;
    let denom = end - start;
    let mut t = if denom.abs() <= f32::EPSILON {
        0.0
    } else {
        (x - start) / denom
    };

    for _ in 0..100 {
        let guessed_x = cubic_eval(px, t);
        let err = x - guessed_x;
        if err.abs() < 0.0001 {
            return cubic_eval(py, t);
        }
        let slope = cubic_slope(px, t);
        if slope.abs() <= f32::EPSILON {
            break;
        }
        t += err / slope;
    }

    cubic_eval(py, t)
}

/// StepMania/ITGmania `bouncebegin` tween curve parameterization.
/// Ported from `itgmania/Themes/_fallback/Scripts/02 Actor.lua`.
#[inline(always)]
pub fn bouncebegin_p(x: f32) -> f32 {
    bezier_y_from_x(x, 0.0, 0.0, 0.42, -0.42, 2.0 / 3.0, 0.3, 1.0, 1.0)
}

/// StepMania/ITGmania `bounceend` tween curve parameterization.
/// Ported from `itgmania/Themes/_fallback/Scripts/02 Actor.lua`.
#[inline(always)]
pub fn bounceend_p(x: f32) -> f32 {
    bezier_y_from_x(x, 0.0, 0.0, 1.0 / 3.0, 0.7, 0.58, 1.42, 1.0, 1.0)
}

/// Evaluate `ease_p` like StepMania/ITGmania `Actor:ease(t, fEase)`.
/// Ported from `itgmania/Themes/_fallback/Scripts/02 Actor.lua`.
#[inline(always)]
pub fn eval_ease_p_for_f_ease(x: f32, f_ease: f32) -> f32 {
    let f = f_ease.clamp(-100.0, 100.0);
    if f == -100.0 {
        return (x.clamp(0.0, 1.0)).powi(2);
    }
    if f == 0.0 {
        return x.clamp(0.0, 1.0);
    }
    if f == 100.0 {
        let u = x.clamp(0.0, 1.0);
        return 1.0 - (1.0 - u) * (1.0 - u);
    }

    // 1D Bezier: {0, x2, y3, 1}
    // x2 = scale(fEase, -100, 100, 0/3, 2/3)
    // y3 = scale(fEase, -100, 100, 1/3, 3/3)
    let s = (f + 100.0) * 0.005; // [0,1]
    let x2 = s * (2.0 / 3.0);
    let y3 = (1.0 / 3.0) + s * (2.0 / 3.0);
    bezier_y_from_x(x, 0.0, 0.0, x2, 0.0, y3, 1.0, 1.0, 1.0)
}

/// Construct `ease(time, fEase)` — fEase in [-100, 100]; 0 = linear.
#[inline(always)]
pub fn ease(dur: f32, f_ease: f32) -> SegmentBuilder {
    let bias = f_ease.clamp(-100.0, 100.0);
    SegmentBuilder::new(Ease::EaseInOut { bias }, dur)
}

/// Construct `smooth(time)` — classic in–out S curve.
#[inline(always)]
pub fn smooth(dur: f32) -> SegmentBuilder {
    SegmentBuilder::new(Ease::Smooth, dur)
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
    pub glow: [f32; 4],
    pub visible: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    pub rot_z: f32, // NEW: degrees
    pub crop_l: f32,
    pub crop_r: f32,
    pub crop_t: f32,
    pub crop_b: f32,
    pub fade_l: f32,
    pub fade_r: f32,
    pub fade_t: f32,
    pub fade_b: f32,
    pub scale: [f32; 2],
}

impl Default for TweenState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hx: 0.5,
            vy: 0.5,
            tint: [1.0, 1.0, 1.0, 1.0],
            glow: [1.0, 1.0, 1.0, 0.0],
            visible: true,
            flip_x: false,
            flip_y: false,
            rot_z: 0.0, // NEW
            fade_l: 0.0,
            fade_r: 0.0,
            fade_t: 0.0,
            fade_b: 0.0,
            crop_l: 0.0,
            crop_r: 0.0,
            crop_t: 0.0,
            crop_b: 0.0,
            scale: [1.0, 1.0],
        }
    }
}

impl Step {
    pub fn fingerprint64(&self) -> u64 {
        #[inline(always)]
        fn mix(h: &mut u64, v: u64) {
            *h ^= v.wrapping_mul(0x9E3779B97F4A7C15);
            *h = h.rotate_left(27) ^ (*h >> 33);
        }
        #[inline(always)]
        fn f32b(f: f32) -> u64 {
            f.to_bits() as u64
        }

        let mut h = 0xcbf29ce484222325u64; // FNV-ish seed
        match self {
            Step::Sleep(d) => {
                mix(&mut h, 0);
                mix(&mut h, f32b(*d));
            }
            Step::Segment(seg) => {
                mix(&mut h, 1);
                // These fields are private but we're in the same module
                match seg.ease {
                    Ease::Linear => mix(&mut h, 0),
                    Ease::Accelerate => mix(&mut h, 1),
                    Ease::Decelerate => mix(&mut h, 2),
                    Ease::Smooth => mix(&mut h, 4),
                    Ease::EaseInOut { bias } => {
                        mix(&mut h, 3);
                        mix(&mut h, f32b(bias));
                    }
                }
                mix(&mut h, f32b(seg.dur));

                // Hash the requested ops (build_ops)
                for op in &seg.build_ops {
                    use super::anim::{BuildOp, Target};
                    #[inline(always)]
                    fn mix_t(h: &mut u64, t: &Target) {
                        match t {
                            Target::Abs(v) => {
                                mix(h, 0);
                                mix(h, f32b(*v));
                            }
                            Target::Rel(v) => {
                                mix(h, 1);
                                mix(h, f32b(*v));
                            }
                        }
                    }
                    match op {
                        BuildOp::X(t) => {
                            mix(&mut h, 10);
                            mix_t(&mut h, t);
                        }
                        BuildOp::Y(t) => {
                            mix(&mut h, 11);
                            mix_t(&mut h, t);
                        }
                        BuildOp::Width(t) => {
                            mix(&mut h, 12);
                            mix_t(&mut h, t);
                        }
                        BuildOp::Height(t) => {
                            mix(&mut h, 13);
                            mix_t(&mut h, t);
                        }
                        BuildOp::ZoomBoth(t) => {
                            mix(&mut h, 14);
                            mix_t(&mut h, t);
                        }
                        BuildOp::ZoomX(t) => {
                            mix(&mut h, 15);
                            mix_t(&mut h, t);
                        }
                        BuildOp::ZoomY(t) => {
                            mix(&mut h, 16);
                            mix_t(&mut h, t);
                        }
                        BuildOp::Tint(r, g, b, a) => {
                            mix(&mut h, 17);
                            mix_t(&mut h, r);
                            mix_t(&mut h, g);
                            mix_t(&mut h, b);
                            mix_t(&mut h, a);
                        }
                        BuildOp::Glow(r, g, b, a) => {
                            mix(&mut h, 18);
                            mix_t(&mut h, r);
                            mix_t(&mut h, g);
                            mix_t(&mut h, b);
                            mix_t(&mut h, a);
                        }
                        BuildOp::Visible(v) => {
                            mix(&mut h, 19);
                            mix(&mut h, u64::from(*v));
                        }
                        BuildOp::FlipX(v) => {
                            mix(&mut h, 20);
                            mix(&mut h, u64::from(*v));
                        }
                        BuildOp::FlipY(v) => {
                            mix(&mut h, 21);
                            mix(&mut h, u64::from(*v));
                        }
                        BuildOp::RotZ(t) => {
                            mix(&mut h, 22);
                            mix_t(&mut h, t);
                        }
                        BuildOp::CropL(t) => {
                            mix(&mut h, 23);
                            mix_t(&mut h, t);
                        }
                        BuildOp::CropR(t) => {
                            mix(&mut h, 24);
                            mix_t(&mut h, t);
                        }
                        BuildOp::CropT(t) => {
                            mix(&mut h, 25);
                            mix_t(&mut h, t);
                        }
                        BuildOp::CropB(t) => {
                            mix(&mut h, 26);
                            mix_t(&mut h, t);
                        }
                        BuildOp::FadeL(t) => {
                            mix(&mut h, 27);
                            mix_t(&mut h, t);
                        }
                        BuildOp::FadeR(t) => {
                            mix(&mut h, 28);
                            mix_t(&mut h, t);
                        }
                        BuildOp::FadeT(t) => {
                            mix(&mut h, 29);
                            mix_t(&mut h, t);
                        }
                        BuildOp::FadeB(t) => {
                            mix(&mut h, 30);
                            mix_t(&mut h, t);
                        }
                    }
                }
            }
        }
        h
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
    Glow(Target, Target, Target, Target),
    Visible(bool),
    FlipX(bool),
    FlipY(bool),
    RotZ(Target),
    CropL(Target),
    CropR(Target),
    CropT(Target),
    CropB(Target),
    FadeL(Target),
    FadeR(Target),
    FadeT(Target),
    FadeB(Target),
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
    ScaleX { from: f32, to: f32 },
    ScaleY { from: f32, to: f32 },
    Tint { from: [f32; 4], to: [f32; 4] },
    Glow { from: [f32; 4], to: [f32; 4] },
    Visible(bool),
    FlipX(bool),
    FlipY(bool),
    RotZ { from: f32, to: f32 },
    CropL { from: f32, to: f32 },
    CropR { from: f32, to: f32 },
    CropT { from: f32, to: f32 },
    CropB { from: f32, to: f32 },
    FadeL { from: f32, to: f32 },
    FadeR { from: f32, to: f32 },
    FadeT { from: f32, to: f32 },
    FadeB { from: f32, to: f32 },
}

impl OpPrepared {
    #[inline(always)]
    fn apply_lerp(&self, s: &mut TweenState, a: f32) {
        match self.kind {
            PreparedKind::X { from, to } => s.x = from + (to - from) * a,
            PreparedKind::Y { from, to } => s.y = from + (to - from) * a,
            PreparedKind::WX { from, to } => s.w = from + (to - from) * a,
            PreparedKind::HY { from, to } => s.h = from + (to - from) * a,
            PreparedKind::ScaleX { from, to } => s.scale[0] = from + (to - from) * a,
            PreparedKind::ScaleY { from, to } => s.scale[1] = from + (to - from) * a,
            PreparedKind::Tint { from, to } => {
                for i in 0..4 {
                    s.tint[i] = from[i] + (to[i] - from[i]) * a;
                }
            }
            PreparedKind::Glow { from, to } => {
                for i in 0..4 {
                    s.glow[i] = from[i] + (to[i] - from[i]) * a;
                }
            }
            PreparedKind::Visible(v) => s.visible = v,
            PreparedKind::FlipX(v) => s.flip_x = v,
            PreparedKind::FlipY(v) => s.flip_y = v,
            PreparedKind::RotZ { from, to } => s.rot_z = from + (to - from) * a,
            PreparedKind::CropL { from, to } => s.crop_l = from + (to - from) * a,
            PreparedKind::CropR { from, to } => s.crop_r = from + (to - from) * a,
            PreparedKind::CropT { from, to } => s.crop_t = from + (to - from) * a,
            PreparedKind::CropB { from, to } => s.crop_b = from + (to - from) * a,
            PreparedKind::FadeL { from, to } => s.fade_l = from + (to - from) * a,
            PreparedKind::FadeR { from, to } => s.fade_r = from + (to - from) * a,
            PreparedKind::FadeT { from, to } => s.fade_t = from + (to - from) * a,
            PreparedKind::FadeB { from, to } => s.fade_b = from + (to - from) * a,
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
        if self.prepared_once {
            return;
        }
        self.prepared.clear();

        for op in &self.build_ops {
            match *op {
                BuildOp::X(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.x + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::X { from: s.x, to },
                    });
                }
                BuildOp::Y(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.y + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::Y { from: s.y, to },
                    });
                }
                BuildOp::Width(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.w + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::WX { from: s.w, to },
                    });
                }
                BuildOp::Height(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.h + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::HY { from: s.h, to },
                    });
                }
                BuildOp::ZoomBoth(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.scale[0] + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::ScaleX {
                            from: s.scale[0],
                            to,
                        },
                    });
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::ScaleY {
                            from: s.scale[1],
                            to,
                        },
                    });
                }
                BuildOp::ZoomX(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.scale[0] + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::ScaleX {
                            from: s.scale[0],
                            to,
                        },
                    });
                }
                BuildOp::ZoomY(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.scale[1] + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::ScaleY {
                            from: s.scale[1],
                            to,
                        },
                    });
                }
                BuildOp::Tint(tr, tg, tb, ta) => {
                    let to0 = match tr {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.tint[0] + dv,
                    };
                    let to1 = match tg {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.tint[1] + dv,
                    };
                    let to2 = match tb {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.tint[2] + dv,
                    };
                    let to3 = match ta {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.tint[3] + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::Tint {
                            from: s.tint,
                            to: [to0, to1, to2, to3],
                        },
                    });
                }
                BuildOp::Glow(gr, gg, gb, ga) => {
                    let to0 = match gr {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.glow[0] + dv,
                    };
                    let to1 = match gg {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.glow[1] + dv,
                    };
                    let to2 = match gb {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.glow[2] + dv,
                    };
                    let to3 = match ga {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.glow[3] + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::Glow {
                            from: s.glow,
                            to: [to0, to1, to2, to3],
                        },
                    });
                }
                BuildOp::Visible(v) => {
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::Visible(v),
                    });
                }
                BuildOp::FlipX(v) => {
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::FlipX(v),
                    });
                }
                BuildOp::FlipY(v) => {
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::FlipY(v),
                    });
                }
                BuildOp::RotZ(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.rot_z + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::RotZ { from: s.rot_z, to },
                    });
                }
                BuildOp::CropL(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.crop_l + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::CropL { from: s.crop_l, to },
                    });
                }
                BuildOp::CropR(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.crop_r + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::CropR { from: s.crop_r, to },
                    });
                }
                BuildOp::CropT(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.crop_t + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::CropT { from: s.crop_t, to },
                    });
                }
                BuildOp::CropB(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.crop_b + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::CropB { from: s.crop_b, to },
                    });
                }
                BuildOp::FadeL(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.fade_l + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::FadeL { from: s.fade_l, to },
                    });
                }
                BuildOp::FadeR(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.fade_r + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::FadeR { from: s.fade_r, to },
                    });
                }
                BuildOp::FadeT(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.fade_t + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::FadeT { from: s.fade_t, to },
                    });
                }
                BuildOp::FadeB(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.fade_b + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::FadeB { from: s.fade_b, to },
                    });
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
        Self {
            ease,
            dur: dur.max(0.0),
            ops: Vec::new(),
        }
    }

    // --- position ---
    pub fn x(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::X(Target::Abs(v)));
        self
    }
    pub fn y(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::Y(Target::Abs(v)));
        self
    }
    pub fn xy(mut self, x: f32, y: f32) -> Self {
        self.ops.push(BuildOp::X(Target::Abs(x)));
        self.ops.push(BuildOp::Y(Target::Abs(y)));
        self
    }
    pub fn addx(mut self, dx: f32) -> Self {
        self.ops.push(BuildOp::X(Target::Rel(dx)));
        self
    }
    pub fn addy(mut self, dy: f32) -> Self {
        self.ops.push(BuildOp::Y(Target::Rel(dy)));
        self
    }

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
    pub fn zoomx(mut self, f: f32) -> Self {
        self.ops.push(BuildOp::ZoomX(Target::Abs(f)));
        self
    }
    pub fn zoomy(mut self, f: f32) -> Self {
        self.ops.push(BuildOp::ZoomY(Target::Abs(f)));
        self
    }
    pub fn addzoomx(mut self, df: f32) -> Self {
        self.ops.push(BuildOp::ZoomX(Target::Rel(df)));
        self
    }
    pub fn addzoomy(mut self, df: f32) -> Self {
        self.ops.push(BuildOp::ZoomY(Target::Rel(df)));
        self
    }

    // --- tint / alpha ---
    pub fn diffuse(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.ops.push(BuildOp::Tint(
            Target::Abs(r),
            Target::Abs(g),
            Target::Abs(b),
            Target::Abs(a),
        ));
        self
    }
    pub fn diffuse_rgb(mut self, r: f32, g: f32, b: f32) -> Self {
        self.ops.push(BuildOp::Tint(
            Target::Abs(r),
            Target::Abs(g),
            Target::Abs(b),
            Target::Rel(0.0),
        ));
        self
    }
    pub fn alpha(mut self, a: f32) -> Self {
        self.ops.push(BuildOp::Tint(
            Target::Rel(0.0),
            Target::Rel(0.0),
            Target::Rel(0.0),
            Target::Abs(a),
        ));
        self
    }

    // --- glow ---
    pub fn glow(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.ops.push(BuildOp::Glow(
            Target::Abs(r),
            Target::Abs(g),
            Target::Abs(b),
            Target::Abs(a),
        ));
        self
    }
    pub fn glow_rgb(mut self, r: f32, g: f32, b: f32) -> Self {
        self.ops.push(BuildOp::Glow(
            Target::Abs(r),
            Target::Abs(g),
            Target::Abs(b),
            Target::Rel(0.0),
        ));
        self
    }
    pub fn glow_alpha(mut self, a: f32) -> Self {
        self.ops.push(BuildOp::Glow(
            Target::Rel(0.0),
            Target::Rel(0.0),
            Target::Rel(0.0),
            Target::Abs(a),
        ));
        self
    }

    // --- instants ---
    pub fn set_visible(mut self, v: bool) -> Self {
        self.ops.push(BuildOp::Visible(v));
        self
    }
    pub fn flip_x(mut self, v: bool) -> Self {
        self.ops.push(BuildOp::FlipX(v));
        self
    }
    pub fn flip_y(mut self, v: bool) -> Self {
        self.ops.push(BuildOp::FlipY(v));
        self
    }

    // --- rotation (degrees) ---  NEW
    pub fn rotationz(mut self, deg: f32) -> Self {
        self.ops.push(BuildOp::RotZ(Target::Abs(deg)));
        self
    }
    pub fn addrotationz(mut self, ddeg: f32) -> Self {
        self.ops.push(BuildOp::RotZ(Target::Rel(ddeg)));
        self
    }

    pub fn cropleft(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::CropL(Target::Abs(v)));
        self
    }
    pub fn cropright(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::CropR(Target::Abs(v)));
        self
    }
    pub fn croptop(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::CropT(Target::Abs(v)));
        self
    }
    pub fn cropbottom(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::CropB(Target::Abs(v)));
        self
    }
    pub fn addcropleft(mut self, dv: f32) -> Self {
        self.ops.push(BuildOp::CropL(Target::Rel(dv)));
        self
    }
    pub fn addcropright(mut self, dv: f32) -> Self {
        self.ops.push(BuildOp::CropR(Target::Rel(dv)));
        self
    }
    pub fn addcroptop(mut self, dv: f32) -> Self {
        self.ops.push(BuildOp::CropT(Target::Rel(dv)));
        self
    }
    pub fn addcropbottom(mut self, dv: f32) -> Self {
        self.ops.push(BuildOp::CropB(Target::Rel(dv)));
        self
    }

    pub fn fadeleft(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::FadeL(Target::Abs(v)));
        self
    }
    pub fn faderight(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::FadeR(Target::Abs(v)));
        self
    }
    pub fn fadetop(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::FadeT(Target::Abs(v)));
        self
    }
    pub fn fadebottom(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::FadeB(Target::Abs(v)));
        self
    }

    pub fn build(self) -> Step {
        Step::Segment(Segment::new(self.ease, self.dur, self.ops))
    }
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
