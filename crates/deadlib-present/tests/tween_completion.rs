use deadlib_present::anim::{self, TweenSeq, TweenState};
use std::hint::black_box;

#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

fn sequence(ops: usize, duration: f32, steps: usize) -> TweenSeq {
    let mut seq = TweenSeq::new(TweenState::default());
    for step in 0..steps {
        let mut builder = anim::linear(duration);
        for index in 0..ops {
            builder = builder.x((step * 100 + index) as f32);
        }
        seq.push(builder);
    }
    seq
}

#[test]
fn completion_preserves_ordered_writes_and_remaining_time() {
    for ops in [0, 1, 12, 16, 17, 32] {
        let mut seq = sequence(ops, 0.5, 2);
        seq.push(anim::linear(0.0).x(-0.0));
        seq.push(anim::linear(0.5).addx(8.0));
        seq.update(0.25);
        let target = ops.saturating_sub(1) as f32;
        assert_eq!(seq.state().x, target * 0.5);
        seq.update(0.5);
        let expected = if ops == 0 { 0.0 } else { target + 50.0 };
        assert_eq!(seq.state().x, expected);
        assert!(!seq.is_empty());
        seq.update(0.375);
        assert_eq!(seq.state().x, 2.0);
        seq.update(0.375);
        assert_eq!(seq.state().x, 8.0);
        assert!(seq.is_empty());
        seq.update(f32::INFINITY);
        assert_eq!(seq.state().x, 8.0);
    }
}

#[test]
#[ignore = "manual release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_tween_completion() {
    const SEQUENCES: usize = 64;
    for (name, ops, duration, steps, dt) in [
        ("sleep", 0, 1.0, 1, 1.0),
        ("inline-one", 1, 1.0, 1, 1.0),
        ("inline-twelve", 12, 1.0, 1, 1.0),
        ("spilled", 32, 1.0, 1, 1.0),
        ("instant-chain", 12, 0.0, 32, 1.0),
        ("timed-chain", 12, 0.125, 32, 4.0),
        ("ongoing", 12, 1.0, 1, 0.125),
        ("empty", 0, 1.0, 0, 1.0),
    ] {
        let mut template = sequence(ops, duration, steps);
        if duration > 0.0 && steps > 0 {
            template.update(0.0625);
        }
        perf::measure_sampled_with_setup(
            name,
            512,
            SEQUENCES * steps.max(1),
            || vec![template.clone(); SEQUENCES],
            |sequences| {
                for seq in sequences {
                    seq.update(black_box(dt));
                    black_box(seq.state());
                    black_box(seq.is_empty());
                }
            },
        );
    }
}

#[test]
#[ignore = "manual steady-update control; --ignored --nocapture --test-threads=1"]
fn benchmark_tween_steady_updates() {
    for (name, steps) in [("steady-ongoing", 1), ("steady-empty", 0)] {
        let mut sequences = vec![sequence(12, 1_000_000.0, steps); 64];
        for seq in &mut sequences {
            seq.update(0.0625);
        }
        perf::measure_sampled(name, 32768, sequences.len(), || {
            for seq in black_box(&mut sequences) {
                seq.update(black_box(0.001));
                black_box(seq.state());
                black_box(seq.is_empty());
            }
        });
    }
}

fn state_bits(state: &TweenState) -> Vec<u32> {
    [
        state.x,
        state.y,
        state.z,
        state.w,
        state.h,
        state.hx,
        state.vy,
        state.rot_x,
        state.rot_y,
        state.rot_z,
        state.skew_x,
        state.skew_y,
        state.crop_l,
        state.crop_r,
        state.crop_t,
        state.crop_b,
        state.fade_l,
        state.fade_r,
        state.fade_t,
        state.fade_b,
    ]
    .into_iter()
    .chain(state.tint)
    .chain(state.glow)
    .chain(state.scale)
    .map(f32::to_bits)
    .chain([
        state.visible as u32,
        state.flip_x as u32,
        state.flip_y as u32,
    ])
    .collect()
}

#[test]
#[ignore = "manual before/after bitwise state capture on stderr"]
fn snapshot_tween_completion() {
    for value in [0.0, -0.0, 0.75, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for ops in [0, 1, 12, 17, 32] {
            for duration in [0.0, 0.5, f32::INFINITY] {
                for ease in [
                    anim::linear,
                    anim::accelerate,
                    anim::decelerate,
                    anim::smooth,
                    anim::spring,
                ] {
                    let mut seq = sequence(ops, duration, 3);
                    seq.push(
                        ease(0.5)
                            .x(value)
                            .alpha(value)
                            .glow_alpha(value)
                            .zoomx(value)
                            .rotationz(value)
                            .flip_x(true),
                    );
                    seq.push_step(anim::sleep(0.25));
                    seq.push(anim::linear(0.0).addx(2.0).set_visible(false));
                    for dt in [
                        0.0,
                        -0.1,
                        f32::NAN,
                        0.125,
                        0.5,
                        0.25,
                        1.0,
                        3.0,
                        f32::INFINITY,
                    ] {
                        seq.update(dt);
                        eprintln!("{:?} {}", state_bits(seq.state()), seq.is_empty());
                    }
                }
            }
        }
    }
}
