use super::*;
use std::{hint::black_box, time::Instant};

#[path = "../../../tests/support/kernel_bench.rs"]
#[allow(
    unsafe_code,
    reason = "benchmark-only Windows affinity and cycle-count FFI"
)]
mod bench;
#[path = "anim_original.rs"]
mod original;

fn values(s: &TweenState) -> [f32; 30] {
    [
        s.x, s.y, s.z, s.w, s.h, s.hx, s.vy, s.rot_x, s.rot_y, s.rot_z, s.skew_x, s.skew_y,
        s.crop_l, s.crop_r, s.crop_t, s.crop_b, s.fade_l, s.fade_r, s.fade_t, s.fade_b, s.tint[0],
        s.tint[1], s.tint[2], s.tint[3], s.glow[0], s.glow[1], s.glow[2], s.glow[3], s.scale[0],
        s.scale[1],
    ]
}

fn compare(actual: &TweenState, expected: &TweenState) -> usize {
    assert_eq!(
        (actual.visible, actual.flip_x, actual.flip_y),
        (expected.visible, expected.flip_x, expected.flip_y)
    );
    let mut nan_differences = 0;
    for (field, (a, b)) in values(actual).into_iter().zip(values(expected)).enumerate() {
        if a.is_nan() && b.is_nan() {
            nan_differences += usize::from(a.to_bits() != b.to_bits());
        } else {
            assert_eq!(a.to_bits(), b.to_bits(), "field {field}: {a:?} != {b:?}");
        }
    }
    nan_differences
}

fn initial(v: f32) -> TweenState {
    TweenState {
        x: v,
        y: -v,
        z: v,
        w: v,
        h: -v,
        hx: v,
        vy: -v,
        tint: [v, -v, v, -v],
        glow: [-v, v, -v, v],
        scale: [v, -v],
        rot_x: v,
        rot_y: -v,
        rot_z: v,
        skew_x: -v,
        skew_y: v,
        crop_l: v,
        crop_r: -v,
        crop_t: v,
        crop_b: -v,
        fade_l: v,
        fade_r: -v,
        fade_t: v,
        fade_b: -v,
        ..TweenState::default()
    }
}

fn operations(v: f32) -> Vec<BuildOp> {
    use BuildOp::*;
    use Target::{Abs as A, Rel as R};
    vec![
        X(A(v)),
        Y(R(v)),
        Z(A(-v)),
        XY(A(v), R(-v)),
        Size(A(v), A(-v)),
        ZoomBoth(A(v)),
        ZoomXY(A(v), R(-v)),
        ZoomX(A(v)),
        ZoomY(R(v)),
        ZoomTo(v, -v),
        Tint(A(v), R(v), A(-v), R(-v)),
        TintRgb(v, -v, v),
        TintAlpha(-v),
        Glow(A(-v), R(v), A(v), R(-v)),
        GlowRgb(-v, v, -v),
        Visible(false),
        FlipX(true),
        FlipY(true),
        RotX(A(v)),
        RotY(R(v)),
        RotZ(A(-v)),
        SkewX(A(v)),
        SkewY(R(v)),
        CropL(A(v)),
        CropR(A(-v)),
        CropT(R(v)),
        CropB(R(-v)),
        FadeL(A(v)),
        FadeR(A(-v)),
        FadeT(R(v)),
        FadeB(R(-v)),
    ]
}

fn segment(v: f32, len: usize, offset: usize, ease: Ease, dur: f32) -> Segment {
    let ops = operations(v);
    let ops = (0..len)
        .map(|i| ops[(i + offset) % ops.len()].clone())
        .collect();
    Segment::new(ease, dur, ops)
}

fn portable_apply(segment: &RuntimeSegment, state: &mut TweenState) {
    segment.apply_current(state);
}

const EASES: [Ease; 9] = [
    Ease::Linear,
    Ease::Accelerate,
    Ease::Decelerate,
    Ease::Smooth,
    Ease::EaseInOut { bias: 37.0 },
    Ease::EaseInOut { bias: -37.0 },
    Ease::EaseInOut { bias: f32::NAN },
    Ease::Spring,
    Ease::CubicBezier {
        control: [-0.25, 0.75, -1.0, 2.0],
    },
];
const LENGTHS: [usize; 10] = [0, 1, 2, 3, 4, 5, 12, 16, 17, 32];
const SPECIAL: [f32; 14] = [
    0.0,
    -0.0,
    f32::from_bits(1),
    -f32::from_bits(1),
    f32::MIN_POSITIVE,
    -f32::MIN_POSITIVE,
    1.0,
    -1.0,
    f32::MAX,
    -f32::MAX,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::from_bits(0x7fc12345),
    f32::from_bits(0xff812345),
];

#[test]
fn property_batches_match_original_and_portable() {
    let mut cases = 0;
    let mut nan_differences = 0;
    let mut check = |from: f32, to: f32, len: usize, ease: Ease, dt: f32| {
        let source = segment(
            to,
            len,
            cases % 31,
            ease,
            if cases % 23 == 0 { 0.0 } else { 1.0 },
        );
        let state = initial(from);
        let mut old = RuntimeSegment::new(&source);
        old.prepare(source.build_ops.clone(), &state);
        old.elapsed = 0.125;
        let mut new = old.clone();
        let (mut expected, mut actual, mut portable) = (state, state, state);
        let pointer = new.prepared.as_ptr();
        let capacity = new.prepared.capacity();
        let finished = original::runtime_update(&mut old, &mut expected, dt);
        assert_eq!(new.update(&mut actual, dt), finished);
        assert_eq!(new.elapsed.to_bits(), old.elapsed.to_bits());
        assert_eq!(new.prepared.as_ptr(), pointer);
        assert_eq!(new.prepared.capacity(), capacity);
        nan_differences += compare(&actual, &expected);
        if !finished && !old.prepared.is_empty() {
            portable_apply(&old, &mut portable);
        }
        nan_differences += compare(&portable, &expected);
        cases += 1;
    };
    for from in SPECIAL {
        for to in SPECIAL {
            for len in LENGTHS {
                for ease in EASES {
                    for dt in [0.125, 0.5, 2.0] {
                        check(from, to, len, ease, dt);
                    }
                }
            }
        }
    }
    let mut random = 17u32;
    for i in 0..20_000 {
        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
        let from = f32::from_bits(random);
        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
        check(
            from,
            f32::from_bits(random),
            LENGTHS[i % LENGTHS.len()],
            EASES[i % EASES.len()],
            0.375,
        );
    }
    println!(
        "{cases} property batches match original and portable; {nan_differences} NaN-bit differences"
    );
}

#[test]
fn queued_tweens_match_original() {
    let mut states = 0;
    let mut nan_differences = 0;
    for len in LENGTHS {
        for ease in EASES {
            for v in [-2.75, -0.0, 0.125, 1.75] {
                let mut old = TweenSeq::new(initial(v));
                for (i, dur) in [0.0, 0.25, 0.5, 1.0].into_iter().enumerate() {
                    old.push_step(Step(segment(v + 0.375, len, i, ease, dur)));
                    old.push_step(sleep(0.125));
                }
                let mut new = old.clone();
                for dt in [
                    0.0,
                    -1.0,
                    f32::NAN,
                    0.03125,
                    0.125,
                    0.25,
                    0.5,
                    0.03125,
                    0.125,
                    0.5,
                    2.0,
                    f32::INFINITY,
                ] {
                    // Active alpha/RGB operations must still restore captured channels.
                    old.state.x = 42.25;
                    new.state.x = 42.25;
                    old.state.tint[0] = -0.25;
                    new.state.tint[0] = -0.25;
                    original::update(&mut old, dt);
                    new.update(dt);
                    nan_differences += compare(new.state(), old.state());
                    assert_eq!(new.is_empty(), old.is_empty());
                    assert_eq!(new.queue.len(), old.queue.len());
                    assert_eq!(
                        new.current.as_ref().map(|s| s.elapsed.to_bits()),
                        old.current.as_ref().map(|s| s.elapsed.to_bits())
                    );
                    states += 1;
                }
                assert!(new.is_empty());
            }
        }
    }
    println!("{states} queued tween states match original; {nan_differences} NaN-bit differences");
}

fn fixtures(case: usize) -> Vec<TweenSeq> {
    (0..64)
        .map(|i| {
            let mut seq = TweenSeq::new(TweenState {
                x: 100.25 + i as f32,
                y: -23.5,
                tint: [0.125, 0.375, 0.25, 0.7],
                ..Default::default()
            });
            match case {
                0 => {}
                1 => seq.push_step(sleep(10.0)),
                2 => seq.push(linear(1.0).x(500.75)),
                3 => seq.push(linear(1.0).xy(500.75, 300.5)),
                4 => seq.push(decelerate(1.0).xy(500.75, 300.5)),
                5 => seq.push(ease(1.0, 37.0).x(500.75)),
                6 => seq.push(
                    ease(1.0, -37.0)
                        .xy(500.75, 300.5)
                        .alpha(0.2)
                        .rotationz(180.0),
                ),
                10 | 11 => seq.push(
                    (if case == 10 {
                        linear(1.0)
                    } else {
                        decelerate(1.0)
                    })
                    .xy(500.75, 300.5)
                    .diffuse(0.9, 0.2, 0.8, 0.3)
                    .zoom(1.25, 0.75)
                    .rotationz(180.0),
                ),
                12 | 13 => seq.push(
                    (if case == 12 {
                        linear(1.0)
                    } else {
                        decelerate(1.0)
                    })
                    .xy(500.75, 300.5)
                    .alpha(0.2)
                    .rotationz(180.0),
                ),
                _ => seq.push(
                    (match case {
                        7 => linear(1.0),
                        8 => decelerate(1.0),
                        _ => ease(1.0, 37.0),
                    })
                    .xy(500.75, 300.5)
                    .size(96.0, 72.0)
                    .diffuse(0.9, 0.2, 0.8, 0.3)
                    .glow(0.4, 0.9, 0.6, 0.2)
                    .zoom(1.25, 0.75)
                    .rotationx(27.0)
                    .rotationy(55.0)
                    .rotationz(180.0)
                    .skewx(0.2)
                    .skewy(-0.3)
                    .cropleft(0.2)
                    .cropright(0.1)
                    .croptop(0.25)
                    .cropbottom(0.3)
                    .fadeleft(0.1)
                    .faderight(0.2)
                    .fadetop(0.3)
                    .fadebottom(0.4),
                ),
            }
            seq.update(1.0 / 120.0);
            seq
        })
        .collect()
}

#[inline(never)]
fn production(seqs: &mut [TweenSeq], elapsed: f32, dt: f32) {
    for seq in seqs {
        if let Some(current) = &mut seq.current {
            current.elapsed = elapsed;
        }
        seq.update(dt);
        black_box(seq.state());
    }
}

#[test]
#[ignore = "release CPU benchmark for archived before/after binaries"]
fn benchmark_tween_batches() {
    bench::pin_thread();
    let rotation = std::env::var("TWEEN_BENCH_ROTATION")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let names = [
        "idle",
        "sleep",
        "linear-x",
        "linear-xy",
        "decelerate-xy",
        "ease-x",
        "ease-mixed",
        "linear-many",
        "decelerate-many",
        "ease-many",
        "linear-four",
        "decelerate-four",
        "linear-three",
        "decelerate-three",
    ];
    for j in 0..names.len() {
        let case = (j + rotation) % names.len();
        let mut seqs = fixtures(case);
        let iterations = 2000;
        for round in 0..10 {
            let start_cycles = bench::cycles();
            let start = Instant::now();
            for i in 0..iterations {
                production(
                    black_box(&mut seqs),
                    black_box(0.125 + (i % 5) as f32 * 0.125),
                    black_box(1.0 / 120.0),
                );
            }
            let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
            let cycles = (bench::cycles() - start_cycles) as f64 / iterations as f64;
            println!(
                "tween-batch,{},{round},{iterations},{cycles:.4},{ns:.4}",
                names[case]
            );
        }
    }
}
