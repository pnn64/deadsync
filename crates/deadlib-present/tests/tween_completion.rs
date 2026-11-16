use deadlib_present::anim::{self, TweenSeq, TweenState};

#[path = "../../../tests/support/allocations.rs"]
mod allocations;

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
fn relative_targets_capture_state_when_the_step_activates() {
    let mut seq = TweenSeq::new(TweenState::default());
    seq.push(anim::linear(1.0).addx(8.0));
    seq.state_mut().x = 10.0;
    for dt in [0.0, -1.0, f32::NAN] {
        seq.update(dt);
        assert_eq!(seq.state().x, 10.0);
    }
    seq.state_mut().x = 20.0;
    seq.update(0.25);
    assert_eq!(seq.state().x, 22.0);
    seq.state_mut().x = 500.0;
    seq.update(0.25);
    assert_eq!(seq.state().x, 24.0);
    let mut cloned = seq.clone();
    seq.update(0.5);
    cloned.update(0.5);
    assert_eq!(seq.state().x, 28.0);
    assert_eq!(cloned.state().x, 28.0);
    assert!(seq.is_empty());
    seq.push_step(anim::sleep(0.25));
    seq.push(anim::linear(0.0).addx(2.0));
    seq.update(0.5);
    assert_eq!(seq.state().x, 30.0);
    seq.clear();
    assert!(seq.is_empty());
}

#[test]
fn active_spilled_operations_clone_and_clear_independently() {
    let mut seq = sequence(32, 1.0, 2);
    seq.update(0.25);
    let mut cloned = seq.clone();
    seq.clear();
    seq.state_mut().x = -1.0;
    seq.update(1.0);
    assert_eq!(seq.state().x, -1.0);
    cloned.update(1.0);
    assert_eq!(cloned.state().x, 56.0);
    cloned.update(1.0);
    assert_eq!(cloned.state().x, 131.0);
    assert!(cloned.is_empty());
}

#[test]
fn small_weighted_tweens_preserve_fused_results_through_completion_and_clone() {
    let initial = TweenState {
        x: 0.125,
        z: 0.5,
        rot_z: 15.25,
        skew_x: 0.0625,
        ..TweenState::default()
    };
    let from = [initial.x, initial.z, initial.rot_z, initial.skew_x];
    let targets = [640.25, -40.75, 135.5, 0.3];
    for bias in [
        -101.0,
        -100.0,
        -99.0,
        -50.0,
        -0.0,
        0.0,
        50.0,
        99.0,
        100.0,
        101.0,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NAN,
    ] {
        for ops in 0..=4 {
            let mut builder = anim::ease(1.0, bias);
            for (index, target) in targets.into_iter().enumerate().take(ops) {
                builder = match index {
                    0 => builder.x(target),
                    1 => builder.z(target),
                    2 => builder.rotationz(target),
                    _ => builder.skewx(target),
                };
            }
            let mut seq = TweenSeq::new(initial);
            seq.push(builder);
            // Include preparation, active interpolation, and completion in the
            // allocation checks. Clone after the first update has prepared ops.
            allocations::assert_no_churn(|| seq.update(1.0 / 64.0));
            let mut cloned = seq.clone();
            let mut elapsed = 1.0 / 64.0;
            for dt in [0.0, 0.125, 0.375, 0.5] {
                allocations::assert_no_churn(|| {
                    seq.update(dt);
                    cloned.update(dt);
                });
                elapsed = (elapsed + dt).min(1.0);
                let progress = anim::eval_ease_p_for_f_ease(elapsed, bias);
                let mut expected = from;
                for index in 0..ops {
                    expected[index] = if elapsed == 1.0 {
                        targets[index]
                    } else {
                        (targets[index] - from[index]).mul_add(progress, from[index])
                    };
                }
                for state in [seq.state(), cloned.state()] {
                    assert_eq!(
                        [state.x, state.z, state.rot_z, state.skew_x].map(f32::to_bits),
                        expected.map(f32::to_bits),
                        "bias {bias}, ops {ops}, elapsed {elapsed}"
                    );
                    assert_eq!(state.y.to_bits(), initial.y.to_bits());
                    assert_eq!(state.tint, initial.tint);
                    assert_eq!(state.visible, initial.visible);
                }
                assert_eq!(seq.is_empty(), elapsed == 1.0);
                assert_eq!(cloned.is_empty(), seq.is_empty());
            }
        }
    }
}
