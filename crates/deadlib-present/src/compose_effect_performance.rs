use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
#[allow(
    unsafe_code,
    reason = "benchmark-only Windows affinity and cycle-count FFI"
)]
mod bench;
#[path = "compose_effect_original.rs"]
mod original;

#[derive(Clone, Copy, Debug)]
struct State {
    color: [f32; 4],
    scale: [f32; 2],
    rotation: [f32; 3],
}

type Apply = fn(anim::EffectState, f32, &mut State);

#[inline(never)]
fn sprite(effect: anim::EffectState, elapsed: f32, state: &mut State) {
    apply_effect_to_sprite(
        effect,
        elapsed,
        &mut state.color,
        &mut state.scale,
        &mut state.rotation,
    );
}

#[inline(never)]
fn text(effect: anim::EffectState, elapsed: f32, state: &mut State) {
    apply_effect_to_text(effect, elapsed, &mut state.color, &mut state.scale);
}

#[inline(never)]
fn portable_sprite(effect: anim::EffectState, elapsed: f32, state: &mut State) {
    apply_effect_to_sprite_portable(
        effect,
        elapsed,
        &mut state.color,
        &mut state.scale,
        &mut state.rotation,
    );
}

#[inline(never)]
fn portable_text(effect: anim::EffectState, elapsed: f32, state: &mut State) {
    apply_effect_to_text_portable(effect, elapsed, &mut state.color, &mut state.scale);
}

fn reference_sprite(effect: anim::EffectState, elapsed: f32, state: &mut State) {
    original::apply_effect_to_sprite(
        effect,
        elapsed,
        &mut state.color,
        &mut state.scale,
        &mut state.rotation,
    );
}

fn reference_text(effect: anim::EffectState, elapsed: f32, state: &mut State) {
    original::apply_effect_to_text(effect, elapsed, &mut state.color, &mut state.scale);
}

fn compare(actual: State, expected: State) -> usize {
    let fields = |s: State| s.color.into_iter().chain(s.scale).chain(s.rotation);
    let mut nan_differences = 0;
    for (field, (a, b)) in fields(actual).zip(fields(expected)).enumerate() {
        if a.is_nan() && b.is_nan() {
            nan_differences += usize::from(a.to_bits() != b.to_bits());
        } else {
            assert_eq!(a.to_bits(), b.to_bits(), "field {field}: {a:?} != {b:?}");
        }
    }
    nan_differences
}

fn modes() -> [anim::EffectMode; 12] {
    use anim::EffectMode::*;
    [
        None,
        DiffuseBlink,
        DiffuseRamp,
        DiffuseShift,
        GlowBlink,
        GlowRamp,
        GlowShift,
        Pulse,
        Bob,
        Bounce,
        Wag,
        Spin,
    ]
}

fn check_effects(current: Apply, portable: Apply, reference: Apply) {
    let values = [
        0.0,
        -0.0,
        0.125,
        -1.25,
        1.0,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        f32::MAX,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::from_bits(0x7fc0_1234),
        f32::from_bits(0xff80_1234),
    ];
    let timings = [
        [0.5, 0.0, 0.5, 0.0, 0.0],
        [0.0, 0.2, 0.0, 0.3, 0.1],
        [0.1, 0.2, 0.3, 0.4, 0.5],
        [0.0; 5],
        [-0.25; 5],
        [f32::EPSILON; 5],
        [f32::INFINITY; 5],
        [f32::NAN; 5],
    ];
    let mut nan_differences = [0; 2];
    let mut cases = 0;
    let mut check = |effect, elapsed, initial: State| {
        let mut expected = initial;
        reference(effect, elapsed, &mut expected);
        for (index, apply) in [current, portable].into_iter().enumerate() {
            let mut actual = initial;
            apply(
                black_box(effect),
                black_box(elapsed),
                black_box(&mut actual),
            );
            nan_differences[index] += compare(actual, expected);
        }
        cases += 1;
    };
    for mode in modes() {
        for clock in [anim::EffectClock::Time, anim::EffectClock::Beat] {
            for timing in timings {
                for v in values {
                    for elapsed in values.into_iter().chain([
                        -0.001,
                        0.25,
                        0.5,
                        0.75,
                        1.0 - f32::EPSILON,
                        1.0 + f32::EPSILON,
                    ]) {
                        let effect = anim::EffectState {
                            mode,
                            clock,
                            timing,
                            offset: v,
                            period: v,
                            color1: [0.15, v, 0.8, -0.0],
                            color2: [v, 0.7, -v, 1.0],
                            magnitude: [v, 1.3, -0.1],
                        };
                        check(
                            effect,
                            elapsed,
                            State {
                                color: [v, 0.7, -v, 0.9],
                                scale: [0.6, v],
                                rotation: [v, -0.0, -370.0],
                            },
                        );
                    }
                }
            }
        }
    }
    let mut seed = 0x2631_795a_u32;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        f32::from_bits(seed)
    };
    for mode in modes() {
        for index in 0_usize..1024 {
            let effect = anim::EffectState {
                mode,
                color1: std::array::from_fn(|_| random()),
                color2: std::array::from_fn(|_| random()),
                magnitude: std::array::from_fn(|_| random()),
                timing: if index.is_multiple_of(2) {
                    timings[index % timings.len()]
                } else {
                    std::array::from_fn(|_| random())
                },
                offset: random(),
                period: random(),
                ..Default::default()
            };
            let initial = State {
                color: std::array::from_fn(|_| random()),
                scale: std::array::from_fn(|_| random()),
                rotation: std::array::from_fn(|_| random()),
            };
            check(effect, random(), initial);
        }
    }
    println!(
        "{cases} effects match original; NaN payload differences runtime/portable: {nan_differences:?}"
    );
}

#[test]
fn sprite_effects_match_original() {
    check_effects(sprite, portable_sprite, reference_sprite);
}

#[test]
fn text_effects_match_original() {
    check_effects(text, portable_text, reference_text);
}

fn fixture(mode: anim::EffectMode, index: usize) -> (anim::EffectState, f32, State) {
    (
        anim::EffectState {
            mode,
            color1: [0.15, 0.35, 0.8, 0.3],
            color2: [0.9, 0.7, 0.2, 1.0],
            magnitude: [0.3, 1.3, -0.1],
            offset: index as f32 / 128.0,
            ..Default::default()
        },
        index as f32 / 64.0,
        State {
            color: [0.6, 0.7, 0.8, 0.9],
            scale: [0.6, 1.2],
            rotation: [23.0, -14.5, 380.0],
        },
    )
}

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_effects() {
    bench::pin_thread();
    let rotation = std::env::var("EFFECT_BENCH_ROTATION")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let cases = [
        anim::EffectMode::None,
        anim::EffectMode::DiffuseBlink,
        anim::EffectMode::DiffuseRamp,
        anim::EffectMode::DiffuseShift,
        anim::EffectMode::Pulse,
        anim::EffectMode::Spin,
    ];
    for (kind, functions) in [
        ("sprite", [sprite as Apply, portable_sprite]),
        ("text", [text as Apply, portable_text]),
    ] {
        for index in 0..cases.len() {
            let mode = cases[(index + rotation) % cases.len()];
            let fixtures: Vec<_> = (0..64).map(|i| fixture(mode, i)).collect();
            let iterations = 4096;
            let operations = iterations * fixtures.len();
            for round in 0..10 {
                for k in 0..2 {
                    let variant = (round + k + rotation) % 2;
                    let apply = black_box(functions[variant]);
                    let start_cycles = bench::cycles();
                    let start = Instant::now();
                    for _ in 0..iterations {
                        for (effect, elapsed, initial) in &fixtures {
                            let mut state = *initial;
                            apply(
                                black_box(*effect),
                                black_box(*elapsed),
                                black_box(&mut state),
                            );
                            black_box(state);
                        }
                    }
                    let ns = start.elapsed().as_nanos() as f64 / operations as f64;
                    let cycles = (bench::cycles() - start_cycles) as f64 / operations as f64;
                    println!(
                        "effect,{kind},{mode:?},{variant},{round},{operations},{cycles:.4},{ns:.4}"
                    );
                }
            }
        }
    }
}
