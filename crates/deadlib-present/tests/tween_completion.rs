use deadlib_present::anim::{self, TweenSeq, TweenState};

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
