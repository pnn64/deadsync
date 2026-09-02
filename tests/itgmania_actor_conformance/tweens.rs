use super::support::{
    actor, actor_rect, assert_array, assert_array_ulp, assert_f32, f32_array, f32_at, fixture,
    samples,
};
use deadlib_present::anim::{
    TweenSeq, TweenState, accelerate, cubic_bezier, decelerate, linear, sleep, spring,
};
use serde_json::Value;

fn curve_seq(name: &str) -> TweenSeq {
    let mut seq = TweenSeq::new(TweenState::default());
    let step = match name {
        "linear" => linear(1.0).x(640.0),
        "accelerate" => accelerate(1.0).x(640.0),
        "decelerate" => decelerate(1.0).x(640.0),
        "spring" => spring(1.0).x(640.0),
        "bezier" => cubic_bezier(1.0, [0.0, 0.15, 0.85, 1.0]).x(640.0),
        _ => panic!("unknown curve actor {name:?}"),
    };
    seq.push(step);
    seq
}

fn assert_curve_actor(name: &str) {
    let oracle = fixture("tween-curves");
    let mut seq = curve_seq(name);
    let mut elapsed = 0.0;
    for sample in samples(&oracle) {
        let time = f32_at(sample, "time");
        seq.update(time - elapsed);
        elapsed = time;
        let expected = f32_array::<3>(&actor(sample, name)["current"]["position"]);
        assert_f32(seq.state().x, expected[0], &format!("{name} x at {time}"));
    }
}

#[test]
fn linear_curve_matches_itgmania() {
    assert_curve_actor("linear");
}

#[test]
fn accelerate_curve_matches_itgmania() {
    assert_curve_actor("accelerate");
}

#[test]
fn decelerate_curve_matches_itgmania() {
    assert_curve_actor("decelerate");
}

#[test]
fn spring_curve_matches_itgmania() {
    assert_curve_actor("spring");
}

#[test]
fn cubic_bezier_curve_matches_itgmania() {
    assert_curve_actor("bezier");
}

fn queue_seq() -> TweenSeq {
    let mut seq = TweenSeq::new(TweenState {
        x: 100.0,
        y: 80.0,
        tint: [1.0, 0.5, 0.25, 1.0],
        ..TweenState::default()
    });
    seq.push(
        linear(1.0)
            .x(300.0)
            .y(160.0)
            .z(10.0)
            .rotationx(15.0)
            .rotationy(25.0)
            .rotationz(35.0)
            .zoomx(1.5)
            .zoomy(0.75),
    );
    seq.push_step(sleep(0.25));
    seq.push(
        spring(1.0)
            .x(420.0)
            .y(300.0)
            .z(-20.0)
            .skewx(0.2)
            .skewy(-0.15)
            .cropleft(0.1)
            .cropright(0.2)
            .croptop(0.05)
            .cropbottom(0.1)
            .fadeleft(0.08)
            .faderight(0.12)
            .fadetop(0.04)
            .fadebottom(0.06)
            .glow(0.2, 0.7, 1.0, 0.6),
    );
    seq
}

fn semantic_queue(native: &Value) -> Vec<&Value> {
    native["tween_queue"]
        .as_array()
        .expect("native tween queue")
        .iter()
        // Actor::Sleep adds an implementation-only zero-duration separator.
        .filter(|entry| f32_at(entry, "duration") > 0.0)
        .collect()
}

#[test]
fn tween_queue_length_and_time_left_match_itgmania() {
    let oracle = fixture("tween-queue");
    let mut seq = queue_seq();
    let mut elapsed = 0.0;
    for sample in samples(&oracle) {
        let time = f32_at(sample, "time");
        seq.update(time - elapsed);
        elapsed = time;
        let native = actor(sample, "subject");
        let native_queue = semantic_queue(native);
        let actual = seq.conformance_snapshot();
        assert_eq!(
            actual.queue.len(),
            native_queue.len(),
            "queue length at {time}"
        );
        assert_f32(
            actual.time_left,
            f32_at(native, "tween_time_left"),
            &format!("queue time left at {time}"),
        );
        for (index, (actual, expected)) in actual.queue.iter().zip(native_queue).enumerate() {
            assert_f32(
                actual.duration,
                f32_at(expected, "duration"),
                &format!("queue[{index}] duration at {time}"),
            );
            assert_f32(
                actual.time_left,
                f32_at(expected, "time_left"),
                &format!("queue[{index}] time left at {time}"),
            );
        }
    }
}

#[test]
fn tween_queue_curve_probes_match_itgmania() {
    let oracle = fixture("tween-queue");
    let native = actor(&samples(&oracle)[0], "subject");
    let native_queue = semantic_queue(native);
    let actual = queue_seq().conformance_snapshot();
    for (index, (actual, expected)) in actual.queue.iter().zip(native_queue).enumerate() {
        assert_array(
            actual.curve_probe,
            f32_array(&expected["curve_probe"]),
            &format!("queue[{index}] curve"),
        );
    }
}

#[test]
fn tween_actor_state_matches_itgmania_at_every_sample() {
    let oracle = fixture("tween-queue");
    let mut seq = queue_seq();
    let mut elapsed = 0.0;
    for sample in samples(&oracle) {
        let time = f32_at(sample, "time");
        seq.update(time - elapsed);
        elapsed = time;
        let state = seq.state();
        let expected = &actor(sample, "subject")["current"];
        assert_array(
            [state.x, state.y, state.z],
            f32_array(&expected["position"]),
            &format!("position at {time}"),
        );
        assert_array(
            [state.rot_x, state.rot_y, state.rot_z],
            f32_array(&expected["rotation"]),
            &format!("rotation at {time}"),
        );
        assert_array(
            [state.scale[0], state.scale[1], 1.0],
            f32_array(&expected["zoom"]),
            &format!("zoom at {time}"),
        );
        assert_array(
            [state.skew_x, state.skew_y],
            f32_array(&expected["skew"]),
            &format!("skew at {time}"),
        );
        assert_array(
            [state.crop_l, state.crop_r, state.crop_t, state.crop_b],
            actor_rect(&expected["crop"]),
            &format!("crop at {time}"),
        );
        assert_array(
            [state.fade_l, state.fade_r, state.fade_t, state.fade_b],
            actor_rect(&expected["fade"]),
            &format!("fade at {time}"),
        );
        // Native and Rust libm differ slightly after spring overshoot. 32 ULP
        // is below 0.000002 here and is restricted to spring-interpolated state.
        assert_array_ulp(
            state.glow,
            f32_array(&expected["glow"]),
            if time > 1.25 && time < 2.25 { 32 } else { 8 },
            &format!("glow at {time}"),
        );
    }
}
