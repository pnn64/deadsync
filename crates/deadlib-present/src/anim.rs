//! StepMania-like tween segments with a tiny queueing system.
//!
//! Usage sketch:
//! ```ignore
//! use deadlib_present::anim::*;
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
use smallvec::SmallVec;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq)] // <-- removed Eq
pub enum Ease {
    /// `StepMania`: `linear(t)`
    Linear,
    /// `StepMania`: `accelerate(t)` (quad-in)
    Accelerate,
    /// `StepMania`: `decelerate(t)` (quad-out)
    Decelerate,
    /// `StepMania`: `smooth(t)` — classic in–out S curve.
    Smooth,
    /// `StepMania`: `ease(time, fEase)` — 1D Bezier curve, fEase in [-100,100].
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
        (1.0 - u).mul_add(-(1.0 - u), 1.0)
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
            0.5f32.mul_add(ease_out_quad(u), 0.5)
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
    let b = 3.0f32.mul_add(c3 - c2, -c);
    let a = c4 - c1 - c - b;
    (a, b, c, d)
}

#[inline(always)]
fn cubic_eval((a, b, c, d): (f32, f32, f32, f32), t: f32) -> f32 {
    a.mul_add(t, b).mul_add(t, c).mul_add(t, d)
}

#[inline(always)]
fn cubic_slope((a, b, c, _): (f32, f32, f32, f32), t: f32) -> f32 {
    (2.0 * b).mul_add(t, 3.0 * a * t * t) + c
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
#[must_use]
pub fn bouncebegin_p(x: f32) -> f32 {
    bezier_y_from_x(x, 0.0, 0.0, 0.42, -0.42, 2.0 / 3.0, 0.3, 1.0, 1.0)
}

/// StepMania/ITGmania `bounceend` tween curve parameterization.
/// Ported from `itgmania/Themes/_fallback/Scripts/02 Actor.lua`.
#[inline(always)]
#[must_use]
pub fn bounceend_p(x: f32) -> f32 {
    bezier_y_from_x(x, 0.0, 0.0, 1.0 / 3.0, 0.7, 0.58, 1.42, 1.0, 1.0)
}

/// Evaluate `ease_p` like StepMania/ITGmania `Actor:ease(t, fEase)`.
/// Ported from `itgmania/Themes/_fallback/Scripts/02 Actor.lua`.
#[inline(always)]
#[must_use]
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
        return (1.0 - u).mul_add(-(1.0 - u), 1.0);
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
#[must_use]
pub fn ease(dur: f32, f_ease: f32) -> SegmentBuilder {
    let bias = f_ease.clamp(-100.0, 100.0);
    SegmentBuilder::new(Ease::EaseInOut { bias }, dur)
}

/// Construct `smooth(time)` — classic in–out S curve.
#[inline(always)]
#[must_use]
pub fn smooth(dur: f32) -> SegmentBuilder {
    SegmentBuilder::new(Ease::Smooth, dur)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectClock {
    Time,
    Beat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectMode {
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
pub struct EffectState {
    pub clock: EffectClock,
    pub mode: EffectMode,
    pub color1: [f32; 4],
    pub color2: [f32; 4],
    pub period: f32,
    pub offset: f32,
    // ITGmania Actor::SetEffectTiming():
    // ramp_to_half, hold_at_half, ramp_to_full, hold_at_full, hold_at_zero.
    pub timing: [f32; 5],
    pub magnitude: [f32; 3],
}

impl Default for EffectState {
    fn default() -> Self {
        Self {
            clock: EffectClock::Time,
            mode: EffectMode::None,
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
pub const fn effect_clock_units(effect: EffectState, time: f32, beat: f32) -> f32 {
    match effect.clock {
        EffectClock::Time => time,
        EffectClock::Beat => beat,
    }
}

#[inline(always)]
#[must_use]
pub fn effect_mix(effect: EffectState, time: f32, beat: f32) -> Option<f32> {
    if !matches!(
        effect.mode,
        EffectMode::DiffuseRamp
            | EffectMode::DiffuseShift
            | EffectMode::GlowShift
            | EffectMode::Pulse
            | EffectMode::Bob
            | EffectMode::Bounce
            | EffectMode::Wag
    ) {
        return None;
    }
    let t = effect.timing;
    let total = (t[0] + t[1] + t[2] + t[3] + t[4]).max(1e-6);
    let units = effect_clock_units(effect, time, beat) + effect.offset;
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

#[inline(always)]
#[must_use]
pub fn glowshift_mix(through: f32) -> f32 {
    ((through + 0.25) * 2.0 * std::f32::consts::PI)
        .sin()
        .mul_add(0.5, 0.5)
        .clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug)]
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
    pub rot_x: f32, // degrees
    pub rot_y: f32, // degrees
    pub rot_z: f32, // degrees
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
            rot_x: 0.0,
            rot_y: 0.0,
            rot_z: 0.0,
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

#[derive(Clone, Copy, Debug)]
enum Target {
    Abs(f32),
    Rel(f32),
}

#[derive(Clone, Debug)]
enum BuildOp {
    X(Target),
    Y(Target),
    XY(Target, Target),
    Size(Target, Target),
    ZoomBoth(Target),
    ZoomXY(Target, Target),
    ZoomX(Target),
    ZoomY(Target),
    ZoomTo(f32, f32),
    Tint(Target, Target, Target, Target),
    TintRgb(f32, f32, f32),
    TintAlpha(f32),
    Glow(Target, Target, Target, Target),
    GlowRgb(f32, f32, f32),
    Visible(bool),
    FlipX(bool),
    FlipY(bool),
    RotX(Target),
    RotY(Target),
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
    XY { from: [f32; 2], to: [f32; 2] },
    WH { from: [f32; 2], to: [f32; 2] },
    ScaleX { from: f32, to: f32 },
    ScaleY { from: f32, to: f32 },
    ScaleBoth { from: [f32; 2], to: f32 },
    ScaleXY { from: [f32; 2], to: [f32; 2] },
    Tint { from: [f32; 4], to: [f32; 4] },
    TintRgb { from: [f32; 4], to: [f32; 3] },
    TintAlpha { from: [f32; 4], to: f32 },
    Glow { from: [f32; 4], to: [f32; 4] },
    GlowRgb { from: [f32; 4], to: [f32; 3] },
    Visible(bool),
    FlipX(bool),
    FlipY(bool),
    RotX { from: f32, to: f32 },
    RotY { from: f32, to: f32 },
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

#[inline]
fn identity_interpolation_is_exact(value: f32) -> bool {
    value.is_finite() && value.to_bits() != (-0.0_f32).to_bits()
}

type BuildOps = SmallVec<[BuildOp; 12]>;
type PreparedOps = SmallVec<[OpPrepared; 16]>;

impl OpPrepared {
    #[inline(always)]
    fn apply_lerp(&self, s: &mut TweenState, a: f32) {
        match self.kind {
            PreparedKind::X { from, to } => s.x = (to - from).mul_add(a, from),
            PreparedKind::Y { from, to } => s.y = (to - from).mul_add(a, from),
            PreparedKind::XY { from, to } => {
                s.x = (to[0] - from[0]).mul_add(a, from[0]);
                s.y = (to[1] - from[1]).mul_add(a, from[1]);
            }
            PreparedKind::WH { from, to } => {
                s.w = (to[0] - from[0]).mul_add(a, from[0]);
                s.h = (to[1] - from[1]).mul_add(a, from[1]);
            }
            PreparedKind::ScaleX { from, to } => s.scale[0] = (to - from).mul_add(a, from),
            PreparedKind::ScaleY { from, to } => s.scale[1] = (to - from).mul_add(a, from),
            PreparedKind::ScaleBoth { from, to } => {
                s.scale[0] = (to - from[0]).mul_add(a, from[0]);
                s.scale[1] = (to - from[1]).mul_add(a, from[1]);
            }
            PreparedKind::ScaleXY { from, to } => {
                s.scale[0] = (to[0] - from[0]).mul_add(a, from[0]);
                s.scale[1] = (to[1] - from[1]).mul_add(a, from[1]);
            }
            PreparedKind::Tint { from, to } => {
                for i in 0..4 {
                    s.tint[i] = (to[i] - from[i]).mul_add(a, from[i]);
                }
            }
            PreparedKind::TintRgb { from, to } => {
                for i in 0..3 {
                    s.tint[i] = (to[i] - from[i]).mul_add(a, from[i]);
                }
                s.tint[3] = if a.is_finite() {
                    from[3]
                } else {
                    0.0_f32.mul_add(a, from[3])
                };
            }
            PreparedKind::TintAlpha { from, to } => {
                if a.is_finite() {
                    s.tint[0] = from[0];
                    s.tint[1] = from[1];
                    s.tint[2] = from[2];
                } else {
                    for (output, input) in s.tint[..3].iter_mut().zip(from) {
                        *output = 0.0_f32.mul_add(a, input);
                    }
                }
                s.tint[3] = (to - from[3]).mul_add(a, from[3]);
            }
            PreparedKind::Glow { from, to } => {
                for i in 0..4 {
                    s.glow[i] = (to[i] - from[i]).mul_add(a, from[i]);
                }
            }
            PreparedKind::GlowRgb { from, to } => {
                for i in 0..3 {
                    s.glow[i] = (to[i] - from[i]).mul_add(a, from[i]);
                }
                s.glow[3] = if a.is_finite() {
                    from[3]
                } else {
                    0.0_f32.mul_add(a, from[3])
                };
            }
            PreparedKind::Visible(v) => s.visible = v,
            PreparedKind::FlipX(v) => s.flip_x = v,
            PreparedKind::FlipY(v) => s.flip_y = v,
            PreparedKind::RotX { from, to } => s.rot_x = (to - from).mul_add(a, from),
            PreparedKind::RotY { from, to } => s.rot_y = (to - from).mul_add(a, from),
            PreparedKind::RotZ { from, to } => s.rot_z = (to - from).mul_add(a, from),
            PreparedKind::CropL { from, to } => s.crop_l = (to - from).mul_add(a, from),
            PreparedKind::CropR { from, to } => s.crop_r = (to - from).mul_add(a, from),
            PreparedKind::CropT { from, to } => s.crop_t = (to - from).mul_add(a, from),
            PreparedKind::CropB { from, to } => s.crop_b = (to - from).mul_add(a, from),
            PreparedKind::FadeL { from, to } => s.fade_l = (to - from).mul_add(a, from),
            PreparedKind::FadeR { from, to } => s.fade_r = (to - from).mul_add(a, from),
            PreparedKind::FadeT { from, to } => s.fade_t = (to - from).mul_add(a, from),
            PreparedKind::FadeB { from, to } => s.fade_b = (to - from).mul_add(a, from),
        }
    }

    #[inline(always)]
    fn apply_final(&self, s: &mut TweenState) {
        self.apply_lerp(s, 1.0);
    }
}

/// Compact source description for a StepMania-like tween segment.
///
/// This is constructed in actor builders every frame. Runtime-only elapsed and
/// prepared-operation storage lives in `RuntimeSegment` to keep this payload
/// allocation-free without embedding that storage in every builder step.
#[derive(Clone, Debug)]
pub struct Segment {
    ease: Ease,
    dur: f32,
    build_ops: BuildOps,
}

impl Segment {
    const fn new(ease: Ease, dur: f32, build_ops: BuildOps) -> Self {
        Self {
            ease,
            dur: dur.max(0.0),
            build_ops,
        }
    }
}

/// Runtime-only segment state kept out of per-frame actor builders.
#[derive(Clone, Debug)]
struct RuntimeSegment {
    ease: Ease,
    dur: f32,
    elapsed: f32,
    // ops requested by the user (absolute/relative); compiled to prepared ops on first tick
    build_ops: BuildOps,
    prepared: PreparedOps,
    prepared_once: bool,
}

impl RuntimeSegment {
    fn new(segment: Segment) -> Self {
        Self {
            ease: segment.ease,
            dur: segment.dur,
            elapsed: 0.0,
            build_ops: segment.build_ops,
            prepared: SmallVec::new(),
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
                BuildOp::XY(tx, ty) => {
                    let to_x = match tx {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.x + dv,
                    };
                    let to_y = match ty {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.y + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::XY {
                            from: [s.x, s.y],
                            to: [to_x, to_y],
                        },
                    });
                }
                BuildOp::Size(tw, th) => {
                    let to_w = match tw {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.w + dv,
                    };
                    let to_h = match th {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.h + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::WH {
                            from: [s.w, s.h],
                            to: [to_w, to_h],
                        },
                    });
                }
                BuildOp::ZoomBoth(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.scale[0] + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::ScaleBoth { from: s.scale, to },
                    });
                }
                BuildOp::ZoomXY(tx, ty) => {
                    let to_x = match tx {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.scale[0] + dv,
                    };
                    let to_y = match ty {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.scale[1] + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::ScaleXY {
                            from: s.scale,
                            to: [to_x, to_y],
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
                BuildOp::ZoomTo(w, h) => {
                    let to_x = if s.w == 0.0 { 0.0 } else { w / s.w };
                    let to_y = if s.h == 0.0 { 0.0 } else { h / s.h };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::ScaleXY {
                            from: s.scale,
                            to: [to_x, to_y],
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
                BuildOp::TintRgb(r, g, b) => {
                    let kind = if identity_interpolation_is_exact(s.tint[3]) {
                        PreparedKind::TintRgb {
                            from: s.tint,
                            to: [r, g, b],
                        }
                    } else {
                        PreparedKind::Tint {
                            from: s.tint,
                            to: [r, g, b, s.tint[3] + 0.0],
                        }
                    };
                    self.prepared.push(OpPrepared { kind });
                }
                BuildOp::TintAlpha(a) => {
                    let kind = if s.tint[..3]
                        .iter()
                        .copied()
                        .all(identity_interpolation_is_exact)
                    {
                        PreparedKind::TintAlpha {
                            from: s.tint,
                            to: a,
                        }
                    } else {
                        PreparedKind::Tint {
                            from: s.tint,
                            to: [s.tint[0] + 0.0, s.tint[1] + 0.0, s.tint[2] + 0.0, a],
                        }
                    };
                    self.prepared.push(OpPrepared { kind });
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
                BuildOp::GlowRgb(r, g, b) => {
                    let kind = if identity_interpolation_is_exact(s.glow[3]) {
                        PreparedKind::GlowRgb {
                            from: s.glow,
                            to: [r, g, b],
                        }
                    } else {
                        PreparedKind::Glow {
                            from: s.glow,
                            to: [r, g, b, s.glow[3] + 0.0],
                        }
                    };
                    self.prepared.push(OpPrepared { kind });
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
                BuildOp::RotX(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.rot_x + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::RotX { from: s.rot_x, to },
                    });
                }
                BuildOp::RotY(t) => {
                    let to = match t {
                        Target::Abs(v) => v,
                        Target::Rel(dv) => s.rot_y + dv,
                    };
                    self.prepared.push(OpPrepared {
                        kind: PreparedKind::RotY { from: s.rot_y, to },
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

    fn update(&mut self, s: &mut TweenState, dt: f32) -> bool {
        // returns true if finished
        if self.dur == 0.0 {
            self.prepare_if_needed(s);
            return true;
        }

        self.prepare_if_needed(s);

        self.elapsed = (self.elapsed + dt).min(self.dur);

        // The caller finalizes completed segments after taking them out of
        // `current`, so avoid applying the same endpoint twice here.
        if self.elapsed >= self.dur {
            return true;
        }

        let a = ease_apply(self.ease, self.elapsed / self.dur);

        for p in &self.prepared {
            p.apply_lerp(s, a);
        }

        false
    }
}

/// Public builder API (mirrors `StepMania` commands inside a time segment).
#[derive(Clone, Debug)]
pub struct SegmentBuilder {
    ease: Ease,
    dur: f32,
    ops: BuildOps,
}

impl SegmentBuilder {
    fn new(ease: Ease, dur: f32) -> Self {
        Self {
            ease,
            dur: dur.max(0.0),
            ops: SmallVec::new(),
        }
    }

    // --- position ---
    #[must_use]
    pub fn x(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::X(Target::Abs(v)));
        self
    }
    #[must_use]
    pub fn y(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::Y(Target::Abs(v)));
        self
    }
    #[must_use]
    pub fn xy(mut self, x: f32, y: f32) -> Self {
        self.ops.push(BuildOp::XY(Target::Abs(x), Target::Abs(y)));
        self
    }
    #[must_use]
    pub fn addx(mut self, dx: f32) -> Self {
        self.ops.push(BuildOp::X(Target::Rel(dx)));
        self
    }
    #[must_use]
    pub fn addy(mut self, dy: f32) -> Self {
        self.ops.push(BuildOp::Y(Target::Rel(dy)));
        self
    }

    // --- absolute size (StepMania: SetWidth/SetHeight/setsize) ---
    #[must_use]
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.ops.push(BuildOp::Size(Target::Abs(w), Target::Abs(h)));
        self
    }

    // --- StepMania zoom semantics (scale factors) ---
    #[must_use]
    pub fn zoom(mut self, f: f32, g: f32) -> Self {
        if (f - g).abs() < f32::EPSILON {
            self.ops.push(BuildOp::ZoomBoth(Target::Abs(f)));
        } else {
            self.ops
                .push(BuildOp::ZoomXY(Target::Abs(f), Target::Abs(g)));
        }
        self
    }
    #[must_use]
    pub fn zoomx(mut self, f: f32) -> Self {
        self.ops.push(BuildOp::ZoomX(Target::Abs(f)));
        self
    }
    #[must_use]
    pub fn zoomy(mut self, f: f32) -> Self {
        self.ops.push(BuildOp::ZoomY(Target::Abs(f)));
        self
    }
    #[must_use]
    pub fn addzoomx(mut self, df: f32) -> Self {
        self.ops.push(BuildOp::ZoomX(Target::Rel(df)));
        self
    }
    #[must_use]
    pub fn addzoomy(mut self, df: f32) -> Self {
        self.ops.push(BuildOp::ZoomY(Target::Rel(df)));
        self
    }

    // --- zoomto (StepMania: zoomto/zoomtowidth/zoomtoheight) ---
    #[must_use]
    pub fn zoomto(mut self, w: f32, h: f32) -> Self {
        self.ops.push(BuildOp::ZoomTo(w, h));
        self
    }

    // --- tint / alpha ---
    #[must_use]
    pub fn diffuse(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.ops.push(BuildOp::Tint(
            Target::Abs(r),
            Target::Abs(g),
            Target::Abs(b),
            Target::Abs(a),
        ));
        self
    }
    #[must_use]
    pub fn diffuse_rgb(mut self, r: f32, g: f32, b: f32) -> Self {
        self.ops.push(BuildOp::TintRgb(r, g, b));
        self
    }
    #[must_use]
    pub fn alpha(mut self, a: f32) -> Self {
        self.ops.push(BuildOp::TintAlpha(a));
        self
    }

    // --- glow ---
    #[must_use]
    pub fn glow(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.ops.push(BuildOp::Glow(
            Target::Abs(r),
            Target::Abs(g),
            Target::Abs(b),
            Target::Abs(a),
        ));
        self
    }
    #[must_use]
    pub fn glow_rgb(mut self, r: f32, g: f32, b: f32) -> Self {
        self.ops.push(BuildOp::GlowRgb(r, g, b));
        self
    }
    #[must_use]
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
    #[must_use]
    pub fn set_visible(mut self, v: bool) -> Self {
        self.ops.push(BuildOp::Visible(v));
        self
    }
    #[must_use]
    pub fn flip_x(mut self, v: bool) -> Self {
        self.ops.push(BuildOp::FlipX(v));
        self
    }
    #[must_use]
    pub fn flip_y(mut self, v: bool) -> Self {
        self.ops.push(BuildOp::FlipY(v));
        self
    }

    // --- rotation (degrees) ---  NEW
    #[must_use]
    pub fn rotationx(mut self, deg: f32) -> Self {
        self.ops.push(BuildOp::RotX(Target::Abs(deg)));
        self
    }
    #[must_use]
    pub fn addrotationx(mut self, ddeg: f32) -> Self {
        self.ops.push(BuildOp::RotX(Target::Rel(ddeg)));
        self
    }

    #[must_use]
    pub fn rotationy(mut self, deg: f32) -> Self {
        self.ops.push(BuildOp::RotY(Target::Abs(deg)));
        self
    }
    #[must_use]
    pub fn addrotationy(mut self, ddeg: f32) -> Self {
        self.ops.push(BuildOp::RotY(Target::Rel(ddeg)));
        self
    }

    #[must_use]
    pub fn rotationz(mut self, deg: f32) -> Self {
        self.ops.push(BuildOp::RotZ(Target::Abs(deg)));
        self
    }
    #[must_use]
    pub fn addrotationz(mut self, ddeg: f32) -> Self {
        self.ops.push(BuildOp::RotZ(Target::Rel(ddeg)));
        self
    }

    #[must_use]
    pub fn cropleft(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::CropL(Target::Abs(v)));
        self
    }
    #[must_use]
    pub fn cropright(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::CropR(Target::Abs(v)));
        self
    }
    #[must_use]
    pub fn croptop(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::CropT(Target::Abs(v)));
        self
    }
    #[must_use]
    pub fn cropbottom(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::CropB(Target::Abs(v)));
        self
    }
    #[must_use]
    pub fn addcropleft(mut self, dv: f32) -> Self {
        self.ops.push(BuildOp::CropL(Target::Rel(dv)));
        self
    }
    #[must_use]
    pub fn addcropright(mut self, dv: f32) -> Self {
        self.ops.push(BuildOp::CropR(Target::Rel(dv)));
        self
    }
    #[must_use]
    pub fn addcroptop(mut self, dv: f32) -> Self {
        self.ops.push(BuildOp::CropT(Target::Rel(dv)));
        self
    }
    #[must_use]
    pub fn addcropbottom(mut self, dv: f32) -> Self {
        self.ops.push(BuildOp::CropB(Target::Rel(dv)));
        self
    }

    #[must_use]
    pub fn fadeleft(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::FadeL(Target::Abs(v)));
        self
    }
    #[must_use]
    pub fn faderight(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::FadeR(Target::Abs(v)));
        self
    }
    #[must_use]
    pub fn fadetop(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::FadeT(Target::Abs(v)));
        self
    }
    #[must_use]
    pub fn fadebottom(mut self, v: f32) -> Self {
        self.ops.push(BuildOp::FadeB(Target::Abs(v)));
        self
    }

    #[must_use]
    pub fn build(self) -> Step {
        Step(Segment::new(self.ease, self.dur, self.ops))
    }
}

/// Construct a `linear(t)` segment builder.
#[must_use]
pub fn linear(dur: f32) -> SegmentBuilder {
    SegmentBuilder::new(Ease::Linear, dur)
}

/// Construct an `accelerate(t)` (quad-in) segment builder.
#[must_use]
pub fn accelerate(dur: f32) -> SegmentBuilder {
    SegmentBuilder::new(Ease::Accelerate, dur)
}

/// Construct a `decelerate(t)` (quad-out) segment builder.
#[must_use]
pub fn decelerate(dur: f32) -> SegmentBuilder {
    SegmentBuilder::new(Ease::Decelerate, dur)
}

/// Delay with no property changes (`StepMania`: `sleep(t)`).
#[must_use]
pub fn sleep(dur: f32) -> Step {
    Step(Segment::new(Ease::Linear, dur.max(0.0), SmallVec::new()))
}

/// A queued step (segment or sleep).
///
/// Sleeps are zero-operation linear segments, so the source representation does
/// not need a size-inflating enum. The segment remains inline because boxing it
/// would allocate during per-frame actor construction.
#[derive(Clone, Debug)]
pub struct Step(Segment);

impl From<Step> for RuntimeSegment {
    fn from(step: Step) -> Self {
        Self::new(step.0)
    }
}

#[derive(Clone, Debug)]
pub struct TweenSeq {
    state: TweenState,
    queue: VecDeque<Step>,
    current: Option<RuntimeSegment>,
}

impl TweenSeq {
    #[must_use]
    pub const fn new(initial: TweenState) -> Self {
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

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.current.is_none() && self.queue.is_empty()
    }

    #[must_use]
    pub const fn state(&self) -> &TweenState {
        &self.state
    }

    pub const fn state_mut(&mut self) -> &mut TweenState {
        &mut self.state
    }

    /// # Panics
    ///
    /// Panics if an internal state invariant is violated.
    pub fn update(&mut self, mut dt: f32) {
        while dt > 0.0 {
            // pull a step if needed
            if self.current.is_none() {
                self.current = self.queue.pop_front().map(RuntimeSegment::from);
                if self.current.is_none() {
                    // nothing to do
                    break;
                }
            }

            // drive current step
            let seg = self.current.as_mut().unwrap();
            let before = seg.elapsed;
            let finished_now = seg.update(&mut self.state, dt);
            let consumed = (seg.elapsed - before).max(0.0);
            dt -= consumed;

            if finished_now {
                // Take the finished step out of `current`.
                if let Some(seg) = self.current.take() {
                    // Snap to exact targets. Sleeps have no prepared operations.
                    for p in &seg.prepared {
                        p.apply_final(&mut self.state);
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

#[cfg(any(test, feature = "bench-support"))]
pub mod bench_support {
    use super::{Ease, OpPrepared, PreparedKind, TweenState, ease_apply};
    use std::hint::black_box;

    #[derive(Clone, Copy)]
    enum LegacyPreparedKind {
        X { from: f32, to: f32 },
        Y { from: f32, to: f32 },
        Width { from: f32, to: f32 },
        Height { from: f32, to: f32 },
        ScaleX { from: f32, to: f32 },
        ScaleY { from: f32, to: f32 },
        Tint { from: [f32; 4], to: [f32; 4] },
        Glow { from: [f32; 4], to: [f32; 4] },
    }

    impl LegacyPreparedKind {
        #[inline(always)]
        fn apply(self, state: &mut TweenState, amount: f32) {
            match self {
                Self::X { from, to } => state.x = (to - from).mul_add(amount, from),
                Self::Y { from, to } => state.y = (to - from).mul_add(amount, from),
                Self::Width { from, to } => state.w = (to - from).mul_add(amount, from),
                Self::Height { from, to } => state.h = (to - from).mul_add(amount, from),
                Self::ScaleX { from, to } => {
                    state.scale[0] = (to - from).mul_add(amount, from);
                }
                Self::ScaleY { from, to } => {
                    state.scale[1] = (to - from).mul_add(amount, from);
                }
                Self::Tint { from, to } => {
                    for i in 0..4 {
                        state.tint[i] = (to[i] - from[i]).mul_add(amount, from[i]);
                    }
                }
                Self::Glow { from, to } => {
                    for i in 0..4 {
                        state.glow[i] = (to[i] - from[i]).mul_add(amount, from[i]);
                    }
                }
            }
        }
    }

    #[inline(always)]
    fn amount(index: usize) -> f32 {
        ((index.wrapping_mul(1_103).wrapping_add(97) & 0xffff) as f32) * (1.0 / 65_535.0)
    }

    #[inline(always)]
    fn scalar_checksum(mut checksum: u64, value: f32) -> u64 {
        checksum = checksum.rotate_left(9) ^ u64::from(value.to_bits());
        checksum.wrapping_mul(0x9e37_79b1_85eb_ca87)
    }

    #[inline(always)]
    fn pair_checksum(checksum: u64, values: [f32; 2]) -> u64 {
        scalar_checksum(scalar_checksum(checksum, values[0]), values[1])
    }

    #[inline(always)]
    fn color_checksum(mut checksum: u64, values: [f32; 4]) -> u64 {
        for value in values {
            checksum = scalar_checksum(checksum, value);
        }
        checksum
    }

    #[inline(always)]
    #[must_use]
    pub fn xy_pair_legacy(evaluations: usize) -> u64 {
        let ops = black_box([
            LegacyPreparedKind::X {
                from: -320.25,
                to: 854.75,
            },
            LegacyPreparedKind::Y {
                from: 720.5,
                to: -48.125,
            },
        ]);
        let mut state = TweenState::default();
        let mut checksum = 0;
        for index in 0..evaluations {
            let amount = black_box(amount(index));
            for op in ops {
                op.apply(&mut state, amount);
            }
            checksum = pair_checksum(checksum, [state.x, state.y]);
        }
        checksum
    }

    #[must_use]
    pub fn xy_pair_current(evaluations: usize) -> u64 {
        let from = [-320.25, 720.5];
        let to = [854.75, -48.125];
        let op = black_box(OpPrepared {
            kind: PreparedKind::XY { from, to },
        });
        let mut state = TweenState::default();
        let mut checksum = 0;
        for index in 0..evaluations {
            op.apply_lerp(&mut state, black_box(amount(index)));
            checksum = pair_checksum(checksum, [state.x, state.y]);
        }
        checksum
    }

    #[must_use]
    pub fn size_pair_legacy(evaluations: usize) -> u64 {
        let ops = black_box([
            LegacyPreparedKind::Width {
                from: 1920.5,
                to: 256.25,
            },
            LegacyPreparedKind::Height {
                from: -64.75,
                to: 720.125,
            },
        ]);
        let mut state = TweenState::default();
        let mut checksum = 0;
        for index in 0..evaluations {
            let amount = black_box(amount(index));
            for op in ops {
                op.apply(&mut state, amount);
            }
            checksum = pair_checksum(checksum, [state.w, state.h]);
        }
        checksum
    }

    #[must_use]
    pub fn size_pair_current(evaluations: usize) -> u64 {
        let from = [1920.5, -64.75];
        let to = [256.25, 720.125];
        let op = black_box(OpPrepared {
            kind: PreparedKind::WH { from, to },
        });
        let mut state = TweenState::default();
        let mut checksum = 0;
        for index in 0..evaluations {
            op.apply_lerp(&mut state, black_box(amount(index)));
            checksum = pair_checksum(checksum, [state.w, state.h]);
        }
        checksum
    }

    #[must_use]
    pub fn scale_pair_legacy(evaluations: usize) -> u64 {
        let ops = black_box([
            LegacyPreparedKind::ScaleX {
                from: 0.375,
                to: 1.625,
            },
            LegacyPreparedKind::ScaleY {
                from: 1.875,
                to: 1.625,
            },
        ]);
        let mut state = TweenState::default();
        let mut checksum = 0;
        for index in 0..evaluations {
            let amount = black_box(amount(index));
            for op in ops {
                op.apply(&mut state, amount);
            }
            checksum = pair_checksum(checksum, state.scale);
        }
        checksum
    }

    #[must_use]
    pub fn scale_pair_current(evaluations: usize) -> u64 {
        let from = [0.375, 1.875];
        let to = 1.625;
        let op = black_box(OpPrepared {
            kind: PreparedKind::ScaleBoth { from, to },
        });
        let mut state = TweenState::default();
        let mut checksum = 0;
        for index in 0..evaluations {
            op.apply_lerp(&mut state, black_box(amount(index)));
            checksum = pair_checksum(checksum, state.scale);
        }
        checksum
    }

    #[must_use]
    pub fn scale_xy_pair_legacy(evaluations: usize) -> u64 {
        let ops = black_box([
            LegacyPreparedKind::ScaleX {
                from: 0.375,
                to: 2.125,
            },
            LegacyPreparedKind::ScaleY {
                from: 1.875,
                to: 0.625,
            },
        ]);
        let mut state = TweenState::default();
        let mut checksum = 0;
        for index in 0..evaluations {
            let amount = black_box(amount(index));
            for op in ops {
                op.apply(&mut state, amount);
            }
            checksum = pair_checksum(checksum, state.scale);
        }
        checksum
    }

    #[must_use]
    pub fn scale_xy_pair_current(evaluations: usize) -> u64 {
        let op = black_box(OpPrepared {
            kind: PreparedKind::ScaleXY {
                from: [0.375, 1.875],
                to: [2.125, 0.625],
            },
        });
        let mut state = TweenState::default();
        let mut checksum = 0;
        for index in 0..evaluations {
            op.apply_lerp(&mut state, black_box(amount(index)));
            checksum = pair_checksum(checksum, state.scale);
        }
        checksum
    }

    #[must_use]
    pub fn zoom_to_pair_legacy(evaluations: usize) -> u64 {
        let width = black_box(320.0_f32);
        let height = black_box(240.0_f32);
        let ops = black_box([
            LegacyPreparedKind::ScaleX {
                from: 0.75,
                to: 1280.0 / width,
            },
            LegacyPreparedKind::ScaleY {
                from: 1.25,
                to: 1080.0 / height,
            },
        ]);
        let mut state = TweenState::default();
        let mut checksum = 0;
        for index in 0..evaluations {
            let amount = black_box(amount(index));
            for op in ops {
                op.apply(&mut state, amount);
            }
            checksum = pair_checksum(checksum, state.scale);
        }
        checksum
    }

    #[must_use]
    pub fn zoom_to_pair_current(evaluations: usize) -> u64 {
        let width = black_box(320.0_f32);
        let height = black_box(240.0_f32);
        let op = black_box(OpPrepared {
            kind: PreparedKind::ScaleXY {
                from: [0.75, 1.25],
                to: [1280.0 / width, 1080.0 / height],
            },
        });
        let mut state = TweenState::default();
        let mut checksum = 0;
        for index in 0..evaluations {
            op.apply_lerp(&mut state, black_box(amount(index)));
            checksum = pair_checksum(checksum, state.scale);
        }
        checksum
    }

    #[must_use]
    pub fn tint_alpha_legacy(evaluations: usize) -> u64 {
        let from = [0.125, 0.25, 0.5, 0.75];
        let op = black_box(LegacyPreparedKind::Tint {
            from,
            to: [from[0], from[1], from[2], 0.9375],
        });
        let mut state = TweenState {
            tint: from,
            ..TweenState::default()
        };
        let mut checksum = 0;
        for index in 0..evaluations {
            op.apply(&mut state, black_box(amount(index)));
            checksum = color_checksum(checksum, state.tint);
        }
        checksum
    }

    #[must_use]
    pub fn tint_alpha_current(evaluations: usize) -> u64 {
        let from = [0.125, 0.25, 0.5, 0.75];
        let op = black_box(OpPrepared {
            kind: PreparedKind::TintAlpha { from, to: 0.9375 },
        });
        let mut state = TweenState {
            tint: from,
            ..TweenState::default()
        };
        let mut checksum = 0;
        for index in 0..evaluations {
            op.apply_lerp(&mut state, black_box(amount(index)));
            checksum = color_checksum(checksum, state.tint);
        }
        checksum
    }

    #[must_use]
    pub fn tint_rgb_legacy(evaluations: usize) -> u64 {
        let from = [0.125, 0.25, 0.5, 0.75];
        let op = black_box(LegacyPreparedKind::Tint {
            from,
            to: [0.875, 0.625, 0.375, from[3]],
        });
        let mut state = TweenState {
            tint: from,
            ..TweenState::default()
        };
        let mut checksum = 0;
        for index in 0..evaluations {
            op.apply(&mut state, black_box(amount(index)));
            checksum = color_checksum(checksum, state.tint);
        }
        checksum
    }

    #[must_use]
    pub fn tint_rgb_current(evaluations: usize) -> u64 {
        let from = [0.125, 0.25, 0.5, 0.75];
        let op = black_box(OpPrepared {
            kind: PreparedKind::TintRgb {
                from,
                to: [0.875, 0.625, 0.375],
            },
        });
        let mut state = TweenState {
            tint: from,
            ..TweenState::default()
        };
        let mut checksum = 0;
        for index in 0..evaluations {
            op.apply_lerp(&mut state, black_box(amount(index)));
            checksum = color_checksum(checksum, state.tint);
        }
        checksum
    }

    #[must_use]
    pub fn glow_rgb_legacy(evaluations: usize) -> u64 {
        let from = [0.75, 0.5, 0.25, 0.125];
        let op = black_box(LegacyPreparedKind::Glow {
            from,
            to: [0.0625, 0.3125, 0.6875, from[3]],
        });
        let mut state = TweenState {
            glow: from,
            ..TweenState::default()
        };
        let mut checksum = 0;
        for index in 0..evaluations {
            op.apply(&mut state, black_box(amount(index)));
            checksum = color_checksum(checksum, state.glow);
        }
        checksum
    }

    #[must_use]
    pub fn glow_rgb_current(evaluations: usize) -> u64 {
        let from = [0.75, 0.5, 0.25, 0.125];
        let op = black_box(OpPrepared {
            kind: PreparedKind::GlowRgb {
                from,
                to: [0.0625, 0.3125, 0.6875],
            },
        });
        let mut state = TweenState {
            glow: from,
            ..TweenState::default()
        };
        let mut checksum = 0;
        for index in 0..evaluations {
            op.apply_lerp(&mut state, black_box(amount(index)));
            checksum = color_checksum(checksum, state.glow);
        }
        checksum
    }

    fn completion_ops() -> [OpPrepared; 4] {
        [
            OpPrepared {
                kind: PreparedKind::X {
                    from: -320.25,
                    to: 854.75,
                },
            },
            OpPrepared {
                kind: PreparedKind::Y {
                    from: 720.5,
                    to: -48.125,
                },
            },
            OpPrepared {
                kind: PreparedKind::Tint {
                    from: [0.125, 0.25, 0.5, 0.75],
                    to: [0.875, 0.625, 0.375, 0.9375],
                },
            },
            OpPrepared {
                kind: PreparedKind::Glow {
                    from: [0.75, 0.5, 0.25, 0.125],
                    to: [0.0625, 0.3125, 0.6875, 1.0],
                },
            },
        ]
    }

    #[inline(always)]
    fn completion_checksum(mut checksum: u64, state: &TweenState) -> u64 {
        checksum = pair_checksum(checksum, [state.x, state.y]);
        for value in state.tint.into_iter().chain(state.glow) {
            checksum = scalar_checksum(checksum, value);
        }
        checksum
    }

    #[must_use]
    pub fn segment_completion_legacy(evaluations: usize) -> u64 {
        let ops = black_box(completion_ops());
        let mut state = TweenState::default();
        let mut checksum = 0;
        for _ in 0..evaluations {
            let amount = black_box(ease_apply(Ease::Decelerate, 1.0));
            for op in &ops {
                op.apply_lerp(&mut state, amount);
            }
            for op in &ops {
                op.apply_final(&mut state);
            }
            checksum = completion_checksum(checksum, &state);
        }
        checksum
    }

    #[must_use]
    pub fn segment_completion_current(evaluations: usize) -> u64 {
        let ops = black_box(completion_ops());
        let mut state = TweenState::default();
        let mut checksum = 0;
        for _ in 0..evaluations {
            for op in &ops {
                op.apply_final(&mut state);
            }
            checksum = completion_checksum(checksum, &state);
        }
        checksum
    }
}

#[cfg(test)]
mod tests {
    use super::{TweenSeq, TweenState, bench_support, ease, linear};

    fn assert_bits(actual: f32, expected: f32, field: &str) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{field} interpolation changed: actual={actual:?} expected={expected:?}"
        );
    }

    #[test]
    fn specialized_operations_match_legacy_checksums() {
        for evaluations in [0, 1, 2, 17, 257, 65_536, 131_071] {
            assert_eq!(
                bench_support::xy_pair_current(evaluations),
                bench_support::xy_pair_legacy(evaluations),
                "XY behavior diverged after {evaluations} evaluations"
            );
            assert_eq!(
                bench_support::size_pair_current(evaluations),
                bench_support::size_pair_legacy(evaluations),
                "width-height behavior diverged after {evaluations} evaluations"
            );
            assert_eq!(
                bench_support::scale_pair_current(evaluations),
                bench_support::scale_pair_legacy(evaluations),
                "equal-axis zoom behavior diverged after {evaluations} evaluations"
            );
            assert_eq!(
                bench_support::scale_xy_pair_current(evaluations),
                bench_support::scale_xy_pair_legacy(evaluations),
                "non-uniform zoom behavior diverged after {evaluations} evaluations"
            );
            assert_eq!(
                bench_support::zoom_to_pair_current(evaluations),
                bench_support::zoom_to_pair_legacy(evaluations),
                "zoomto behavior diverged after {evaluations} evaluations"
            );
            assert_eq!(
                bench_support::segment_completion_current(evaluations),
                bench_support::segment_completion_legacy(evaluations),
                "segment endpoint behavior diverged after {evaluations} evaluations"
            );
            assert_eq!(
                bench_support::tint_alpha_current(evaluations),
                bench_support::tint_alpha_legacy(evaluations),
                "alpha-only tint behavior diverged after {evaluations} evaluations"
            );
            assert_eq!(
                bench_support::tint_rgb_current(evaluations),
                bench_support::tint_rgb_legacy(evaluations),
                "RGB-only tint behavior diverged after {evaluations} evaluations"
            );
            assert_eq!(
                bench_support::glow_rgb_current(evaluations),
                bench_support::glow_rgb_legacy(evaluations),
                "RGB-only glow behavior diverged after {evaluations} evaluations"
            );
        }
    }

    #[test]
    fn specialized_color_tweens_preserve_channels_and_interpolation() {
        let initial = TweenState {
            tint: [0.125, 0.25, 0.5, 0.75],
            glow: [0.75, 0.5, 0.25, 0.125],
            ..TweenState::default()
        };

        let mut tint_rgb = TweenSeq::new(initial);
        tint_rgb.push(linear(1.0).diffuse_rgb(0.875, 0.625, 0.375));
        tint_rgb.update(0.375);
        let state = tint_rgb.state();

        for (index, target) in [0.875_f32, 0.625, 0.375].into_iter().enumerate() {
            assert_bits(
                state.tint[index],
                (target - initial.tint[index]).mul_add(0.375, initial.tint[index]),
                &format!("specialized tint RGB {index}"),
            );
        }
        assert_bits(state.tint[3], initial.tint[3], "specialized tint alpha");

        let mut alpha = TweenSeq::new(initial);
        alpha.push(linear(1.0).alpha(0.9375));
        alpha.update(0.375);
        let state = alpha.state();
        for index in 0..3 {
            assert_bits(
                state.tint[index],
                initial.tint[index],
                &format!("specialized alpha tint RGB {index}"),
            );
        }
        assert_bits(
            state.tint[3],
            (0.9375_f32 - initial.tint[3]).mul_add(0.375, initial.tint[3]),
            "specialized tint alpha",
        );

        let mut glow_rgb = TweenSeq::new(initial);
        glow_rgb.push(linear(1.0).glow_rgb(0.0625, 0.3125, 0.6875));
        glow_rgb.update(0.375);
        let state = glow_rgb.state();
        for (index, target) in [0.0625_f32, 0.3125, 0.6875].into_iter().enumerate() {
            assert_bits(
                state.glow[index],
                (target - initial.glow[index]).mul_add(0.375, initial.glow[index]),
                &format!("specialized glow RGB {index}"),
            );
        }
        assert_bits(state.glow[3], initial.glow[3], "specialized glow alpha");
    }

    #[test]
    fn specialized_color_tweens_preserve_legacy_overwrite_behavior() {
        let initial = TweenState {
            tint: [0.125, 0.25, 0.5, 0.75],
            glow: [0.75, 0.5, 0.25, -0.0],
            ..TweenState::default()
        };
        let mut ordered = TweenSeq::new(initial);
        ordered.push(linear(1.0).diffuse_rgb(0.875, 0.625, 0.375).alpha(0.9375));
        ordered.update(0.375);
        for index in 0..3 {
            assert_bits(
                ordered.state().tint[index],
                initial.tint[index],
                &format!("ordered tint RGB {index}"),
            );
        }

        ordered.state_mut().tint[0] = 1.0;
        ordered.update(0.125);
        assert_bits(
            ordered.state().tint[0],
            initial.tint[0],
            "active alpha tween reasserts untouched RGB",
        );

        let mut negative_zero = TweenSeq::new(initial);
        negative_zero.push(linear(1.0).glow_rgb(0.0625, 0.3125, 0.6875));
        negative_zero.update(0.375);
        let legacy_alpha =
            ((initial.glow[3] + 0.0) - initial.glow[3]).mul_add(0.375, initial.glow[3]);
        assert_bits(
            negative_zero.state().glow[3],
            legacy_alpha,
            "negative-zero glow alpha",
        );

        let mut non_finite_amount = TweenSeq::new(initial);
        non_finite_amount.push(ease(1.0, f32::NAN).alpha(0.9375));
        non_finite_amount.update(0.375);
        assert!(
            non_finite_amount.state().tint.into_iter().all(f32::is_nan),
            "a non-finite interpolation amount must affect untouched tint channels as before"
        );

        let mut non_finite_amount = TweenSeq::new(initial);
        non_finite_amount.push(ease(1.0, f32::NAN).diffuse_rgb(0.875, 0.625, 0.375));
        non_finite_amount.update(0.375);
        assert!(
            non_finite_amount.state().tint.into_iter().all(f32::is_nan),
            "a non-finite interpolation amount must affect untouched tint alpha as before"
        );

        let mut non_finite_amount = TweenSeq::new(initial);
        non_finite_amount.push(ease(1.0, f32::NAN).glow_rgb(0.0625, 0.3125, 0.6875));
        non_finite_amount.update(0.375);
        assert!(
            non_finite_amount.state().glow.into_iter().all(f32::is_nan),
            "a non-finite interpolation amount must affect untouched glow alpha as before"
        );
    }

    #[test]
    fn fused_scale_pairs_match_independent_interpolation() {
        let initial = TweenState {
            w: 320.0,
            h: 240.0,
            scale: [0.375, 1.875],
            ..TweenState::default()
        };

        let mut zoom = TweenSeq::new(initial);
        zoom.push(linear(1.0).zoom(2.125, 0.625));
        zoom.update(0.375);
        assert_bits(
            zoom.state().scale[0],
            (2.125_f32 - initial.scale[0]).mul_add(0.375, initial.scale[0]),
            "non-uniform scale x",
        );
        assert_bits(
            zoom.state().scale[1],
            (0.625_f32 - initial.scale[1]).mul_add(0.375, initial.scale[1]),
            "non-uniform scale y",
        );

        let mut zoom_to = TweenSeq::new(initial);
        zoom_to.push(linear(1.0).zoomto(1280.0, 1080.0));
        zoom_to.update(0.375);
        assert_bits(
            zoom_to.state().scale[0],
            (4.0_f32 - initial.scale[0]).mul_add(0.375, initial.scale[0]),
            "zoomto scale x",
        );
        assert_bits(
            zoom_to.state().scale[1],
            (4.5_f32 - initial.scale[1]).mul_add(0.375, initial.scale[1]),
            "zoomto scale y",
        );

        let mut zero_width = TweenSeq::new(TweenState {
            w: 0.0,
            h: 240.0,
            scale: [0.75, 1.25],
            ..TweenState::default()
        });
        zero_width.push(linear(1.0).zoomto(1280.0, 1080.0));
        zero_width.update(1.0);
        assert_bits(zero_width.state().scale[0], 0.0, "zero-width zoomto x");
        assert_bits(zero_width.state().scale[1], 4.5, "zero-width zoomto y");
    }

    #[test]
    fn completed_segments_snap_once_and_consume_following_steps() {
        let mut tween = TweenSeq::new(TweenState::default());
        tween.push(linear(0.25).xy(100.0, 200.0).diffuse(0.25, 0.5, 0.75, 1.0));
        tween.push(linear(0.0).size(640.0, 480.0).zoom(2.0, 3.0));
        tween.push(linear(0.5).xy(-20.0, 40.0));

        tween.update(1.0);
        let state = tween.state();
        assert!(tween.is_empty());
        assert_bits(state.x, -20.0, "completed x");
        assert_bits(state.y, 40.0, "completed y");
        assert_bits(state.w, 640.0, "instant width");
        assert_bits(state.h, 480.0, "instant height");
        assert_bits(state.scale[0], 2.0, "instant scale x");
        assert_bits(state.scale[1], 3.0, "instant scale y");
        for (index, (actual, expected)) in state
            .tint
            .into_iter()
            .zip([0.25, 0.5, 0.75, 1.0])
            .enumerate()
        {
            assert_bits(actual, expected, &format!("completed tint {index}"));
        }
    }

    #[test]
    fn fused_pair_tweens_match_independent_interpolation() {
        let initial = TweenState {
            x: -320.25,
            y: 720.5,
            w: 1920.5,
            h: -64.75,
            scale: [0.375, 1.875],
            ..TweenState::default()
        };
        let mut tween = TweenSeq::new(initial);
        tween.push(
            linear(1.0)
                .xy(854.75, -48.125)
                .size(256.25, 720.125)
                .zoom(1.625, 1.625),
        );

        let mut elapsed = 0.0_f32;
        for dt in [0.0, 0.03125, 0.125, 0.21875, 0.375, 0.5] {
            tween.update(dt);
            elapsed = (elapsed + dt).min(1.0);
            let state = tween.state();
            assert_bits(
                state.x,
                (854.75 - initial.x).mul_add(elapsed, initial.x),
                "x",
            );
            assert_bits(
                state.y,
                (-48.125 - initial.y).mul_add(elapsed, initial.y),
                "y",
            );
            assert_bits(
                state.w,
                (256.25 - initial.w).mul_add(elapsed, initial.w),
                "width",
            );
            assert_bits(
                state.h,
                (720.125 - initial.h).mul_add(elapsed, initial.h),
                "height",
            );
            assert_bits(
                state.scale[0],
                (1.625 - initial.scale[0]).mul_add(elapsed, initial.scale[0]),
                "scale x",
            );
            assert_bits(
                state.scale[1],
                (1.625 - initial.scale[1]).mul_add(elapsed, initial.scale[1]),
                "scale y",
            );
        }
        assert!(tween.is_empty());
    }

    #[test]
    fn fused_operations_preserve_builder_order() {
        let initial = TweenState {
            x: 1.0,
            y: 2.0,
            scale: [0.5, 1.5],
            ..TweenState::default()
        };
        let mut tween = TweenSeq::new(initial);
        tween.push(
            linear(1.0)
                .xy(10.0, 20.0)
                .x(30.0)
                .size(100.0, 200.0)
                .zoom(2.0, 2.0)
                .zoomx(4.0),
        );
        tween.update(0.5);
        let state = tween.state();

        assert_bits(state.x, (30.0_f32 - 1.0).mul_add(0.5, 1.0), "ordered x");
        assert_bits(state.y, (20.0_f32 - 2.0).mul_add(0.5, 2.0), "ordered y");
        assert_bits(state.w, 50.0, "ordered width");
        assert_bits(state.h, 100.0, "ordered height");
        assert_bits(
            state.scale[0],
            (4.0_f32 - 0.5).mul_add(0.5, 0.5),
            "ordered scale x",
        );
        assert_bits(
            state.scale[1],
            (2.0_f32 - 1.5).mul_add(0.5, 1.5),
            "ordered scale y",
        );
    }
}
