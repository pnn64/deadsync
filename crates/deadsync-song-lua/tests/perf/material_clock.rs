use super::*;
#[path = "material_clock_before.rs"]
mod before;

fn frames(delays: &[f32]) -> Arc<[SongLuaModelTextureFrame]> {
    delays
        .iter()
        .enumerate()
        .map(|(i, &delay)| SongLuaModelTextureFrame {
            texture_key: None,
            delay,
            diffuse: SongLuaModelTextureUv {
                scale: [1.0 + i as f32 * 0.125, -1.0],
                offset: [i as f32 * 0.03, -0.25],
                shift: [0.125, 0.25],
            },
            glow: SongLuaModelTextureUv {
                scale: [0.5, 0.75],
                offset: [-0.1, i as f32 * 0.05],
                shift: [0.25, -0.5],
            },
        })
        .collect::<Vec<_>>()
        .into()
}

fn uv_bits((a, b): (SongLuaModelTextureUv, SongLuaModelTextureUv)) -> [[Option<u32>; 6]; 2] {
    [a, b].map(|uv| {
        [
            uv.scale[0],
            uv.scale[1],
            uv.offset[0],
            uv.offset[1],
            uv.shift[0],
            uv.shift[1],
        ]
        .map(|v| if v.is_nan() { None } else { Some(v.to_bits()) })
    })
}

#[test]
fn material_prefixes_preserve_updates_setstate_and_float_order() {
    for delays in [
        vec![1.0],
        vec![0.1, 0.2, 0.3],
        vec![0.0; 4],
        vec![0.1, -0.05, 0.0, 0.2],
        vec![f32::INFINITY, 0.2, f32::NEG_INFINITY],
        vec![f32::NAN, 0.1],
        (0..129).map(|i| ((i % 7) + 1) as f32 * 0.0125).collect(),
    ] {
        let data = frames(&delays);
        for velocity in [[0.0; 2], [-0.5, 0.75]] {
            let mut old = before::MaterialClock::new(data.clone(), velocity);
            let mut new = MaterialClock::new(data.clone(), velocity);
            for step in 0..600 {
                if step % 11 == 0 {
                    let index = step as i32 % (delays.len() as i32 + 5) - 2;
                    old.set_state(index);
                    new.set_state(index);
                }
                let delta = if step % 7 == 0 {
                    0.0
                } else {
                    ((step % 9) as f32 - 1.0) * 0.025
                };
                old.update(delta);
                new.update(delta);
                assert_eq!(
                    uv_bits(old.uv()),
                    uv_bits(new.uv()),
                    "delays={delays:?}, step={step}"
                );
            }
            for delta in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN, -0.0] {
                let mut old = before::MaterialClock::new(data.clone(), velocity);
                let mut new = MaterialClock::new(data.clone(), velocity);
                old.set_state(delays.len() as i32 - 1);
                new.set_state(delays.len() as i32 - 1);
                old.update(delta);
                new.update(delta);
                assert_eq!(uv_bits(old.uv()), uv_bits(new.uv()));
            }
        }
    }
}

#[test]
fn additive_state_clocks_preserve_uv_and_initialization_churn() {
    use crate::draw_capture::perf::allocations;
    let data = frames(
        &(0..64)
            .map(|i| (i % 7 + 1) as f32 * 0.01)
            .collect::<Vec<_>>(),
    );
    let (mut old, old_churn) =
        allocations::measure(|| before::MaterialClock::new(data.clone(), [0.0; 2]));
    let (mut new, new_churn) = allocations::measure(|| MaterialClock::state_only(data.clone()));
    assert_eq!(old_churn, new_churn);
    for index in -1..66 {
        old.set_state(index);
        new.set_state(index);
        old.update(0.125);
        new.update(0.125);
        assert_eq!(uv_bits(old.uv()), uv_bits(new.uv()));
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_material_clock() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, size, kind) in [
        ("clock_uv_1", 1, 0),
        ("clock_uv_64", 64, 0),
        ("clock_uv_256", 256, 0),
        ("clock_replay_64", 64, 1),
        ("clock_init_1", 1, 2),
        ("clock_init_64", 64, 2),
        ("clock_additive_init_64", 64, 3),
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let data = frames(
                &(0..size)
                    .map(|i| (i % 7 + 1) as f32 * 0.0125)
                    .collect::<Vec<_>>(),
            );
            let mut samples = Vec::new();
            let count = 20_000;
            for sample in 0..10 {
                let mut old = before::MaterialClock::new(data.clone(), [0.25, -0.5]);
                old.set_state(size as i32 - 1);
                let mut new = MaterialClock::new(data.clone(), [0.25, -0.5]);
                new.set_state(size as i32 - 1);
                let start = Instant::now();
                let start_cycles = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..count {
                        if kind == 2 || kind == 3 {
                            if current {
                                black_box(if kind == 3 {
                                    MaterialClock::state_only(black_box(data.clone()))
                                } else {
                                    MaterialClock::new(black_box(data.clone()), [0.25, -0.5])
                                });
                            } else {
                                black_box(before::MaterialClock::new(
                                    black_box(data.clone()),
                                    if kind == 3 { [0.0; 2] } else { [0.25, -0.5] },
                                ));
                            }
                        } else if current {
                            if kind == 1 {
                                new.update(0.01)
                            };
                            black_box(black_box(&new).uv());
                        } else {
                            if kind == 1 {
                                old.update(0.01)
                            };
                            black_box(black_box(&old).uv());
                        }
                    }
                });
                let used = cycles() - start_cycles;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":count,
                    "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},
                "samples":samples,"unit":if kind>=2 {"initializations"} else {"ticks"},"baseline_commit":"5ffd4a261"})
            );
        }
    }
}
