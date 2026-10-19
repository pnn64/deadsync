use super::super::*;

#[path = "compiled_physics_before.rs"]
mod before;

fn fixture(
    background: usize,
    boxes: usize,
    balls: usize,
    rotated: bool,
    contact: bool,
) -> CompiledSongScene {
    let mut scene = CompiledSongScene {
        sounds: std::array::from_fn(|i| PathBuf::from(format!("physics-test/sound-{i}.ogg"))),
        ..Default::default()
    };
    scene.entities.extend((0..background).map(|i| Entity {
        position: [i as f64, 100.0, 50.0],
        ..Default::default()
    }));
    for i in 0..boxes {
        scene.entities.push(Entity {
            position: [i as f64 * 10.0, 0.0, 0.0],
            half_box: Some([3.0; 3]),
            rotation: rotated.then_some([0.13, 0.21]),
            box_slippy: i % 2 == 0,
            ..Default::default()
        });
    }
    for i in 0..balls {
        let position = if contact {
            [i as f64 * 10.0, 3.5, 0.0]
        } else {
            [i as f64, 20.0, 15.0]
        };
        scene.entities.push(Entity {
            position,
            ball: Some(Ball {
                radius: 1.0,
                bounce_height: position[1],
                spawn: position,
                slippy: false,
            }),
            physics: Some(Physics {
                velocity: if contact {
                    [0.0, -0.1, 0.0]
                } else {
                    [0.001, 0.0, 0.0]
                },
                gravity: contact,
                collision: false,
            }),
            input: Some(i % 2),
            ..Default::default()
        });
    }
    scene
}

fn ready(
    background: usize,
    boxes: usize,
    balls: usize,
    rotated: bool,
    contact: bool,
) -> CompiledSongScene {
    let mut scene = fixture(background, boxes, balls, rotated, contact);
    scene.initialize();
    scene.frame = 59;
    scene.held = [[true, false, true, false], [false, true, false, true]];
    scene
}

fn compare(
    old: &CompiledSongScene,
    new: &CompiledSongScene,
    a: &SongLuaLiveOutput,
    b: &SongLuaLiveOutput,
) {
    // Debug retains signed zero and every finite value. Explicit scalar checks
    // below additionally compare bits; nonfinite results compare classification.
    assert_eq!(format!("{:?}", old.entities), format!("{:?}", new.entities));
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    assert_eq!(format!("{:?}", old.random), format!("{:?}", new.random));
    assert_eq!(
        (old.frame, old.running, old.winner),
        (new.frame, new.running, new.winner)
    );
    assert_eq!(format!("{:?}", old.states), format!("{:?}", new.states));
    for (a, b) in old.entities.iter().zip(&new.entities) {
        for (x, y) in a.position.into_iter().zip(b.position) {
            assert!(x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()));
        }
        if let (Some(a), Some(b)) = (a.physics, b.physics) {
            for (x, y) in a.velocity.into_iter().zip(b.velocity) {
                assert!(x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()));
            }
        }
    }
}

#[test]
fn physics_matches_authored_order_inputs_movement_follow_and_respawn() {
    for (background, boxes, balls, rotated, contact) in [
        (0, 0, 0, false, false),
        (0, 8, 2, true, true),
        (80, 16, 6, true, false),
        (0, 8, 128, false, true),
        (64, 16, 2, false, false),
    ] {
        let build = || {
            let mut scene = ready(background, boxes, balls, rotated, contact);
            if balls > 0 {
                let first = background + boxes;
                scene.entities[first].position[1] = -60.0;
                scene.entities.push(Entity {
                    follow: Some((first, [0.0, 1.0, -2.0])),
                    ..Default::default()
                });
            }
            if boxes > 0 {
                let e = &mut scene.entities[background];
                e.spin = Some([0.0001, -0.0002]);
                e.physics = Some(Physics {
                    velocity: [0.0; 3],
                    gravity: false,
                    collision: true,
                });
                e.moves = vec![([0.01, 0.0, 0.0], 3.0), ([-0.01, 0.0, 0.0], 2.0)];
            }
            scene.initialize();
            scene.frame = 58;
            scene
        };
        let mut old = build();
        let mut new = build();
        for frame in 0..100 {
            for scene in [&mut old, &mut new] {
                scene.held = if frame % 3 == 0 {
                    [[false; 4]; 2]
                } else {
                    [[true, false, true, false], [false, true, false, true]]
                };
            }
            let mut a = SongLuaLiveOutput::default();
            let mut b = SongLuaLiveOutput::default();
            before::tick(&mut old, &mut a);
            new.tick(&mut b);
            compare(&old, &new, &a, &b);
        }
    }
}

#[test]
fn physics_preserves_collision_audio_rng_and_sequential_hybrid_entities() {
    let build = || {
        let mut scene = ready(128, 4, 3, false, true);
        // A collider can itself be a ball. Earlier responses may move it before
        // a later collider is visited, so snapshots must remain per collider.
        let e = &mut scene.entities[128];
        e.ball = Some(Ball {
            radius: 1.0,
            bounce_height: 0.0,
            spawn: e.position,
            slippy: true,
        });
        e.physics = Some(Physics {
            velocity: [0.01, -0.02, 0.0],
            gravity: false,
            collision: true,
        });
        scene.entities[0].physics = Some(Physics {
            velocity: [0.0; 3],
            gravity: false,
            collision: true,
        });
        scene.entities[1].checkpoint = Some([30.0; 3]);
        scene.entities[1].position = [0.0; 3];
        scene.entities[2].goal = Some([30.0; 3]);
        scene.entities[2].position = [0.0; 3];
        scene.initialize();
        scene.frame = 70;
        scene
    };
    let mut old = build();
    let mut new = build();
    let mut saw_collision = false;
    let mut saw_checkpoint = false;
    for _ in 0..20 {
        let mut a = SongLuaLiveOutput::default();
        let mut b = SongLuaLiveOutput::default();
        before::tick(&mut old, &mut a);
        new.tick(&mut b);
        compare(&old, &new, &a, &b);
        saw_collision |= a.sounds.iter().any(|s| {
            s.path.to_string_lossy().contains("sound-3")
                || s.path.to_string_lossy().contains("sound-4")
                || s.path.to_string_lossy().contains("sound-5")
        });
        saw_checkpoint |= a
            .sounds
            .iter()
            .any(|s| s.path.to_string_lossy().contains("sound-1"));
    }
    assert!(saw_collision && saw_checkpoint);
}

#[test]
fn physics_preserves_float_edges_and_quiet_collision_flag_resets() {
    for value in [
        0.0,
        -0.0,
        1e-100,
        1e100,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ] {
        let build = || {
            let mut scene = ready(128, 2, 2, true, false);
            scene.entities[0].physics = Some(Physics {
                velocity: [value, 0.0, -0.0],
                gravity: false,
                collision: true,
            });
            scene.entities[128].rotation = Some([value, -0.0]);
            scene.entities[130].position = [value, 0.0, -0.0];
            scene.entities[131].ball.as_mut().unwrap().radius = value;
            scene.initialize();
            scene
        };
        let mut old = build();
        let mut new = build();
        for _ in 0..4 {
            let mut a = SongLuaLiveOutput::default();
            let mut b = SongLuaLiveOutput::default();
            before::tick(&mut old, &mut a);
            new.tick(&mut b);
            compare(&old, &new, &a, &b);
        }
    }
}

#[test]
fn physics_initialization_refreshes_ordered_queries_with_exact_storage() {
    use crate::draw_capture::perf::allocations;
    let mut scene = fixture(128, 4, 3, false, false);
    let (_, churn) = allocations::measure(|| scene.initialize());
    assert_eq!(scene.ball_indices.as_ref(), &[132, 133, 134]);
    assert_eq!(scene.physics_indices.as_ref(), &[132, 133, 134]);
    assert!(scene.indexed_queries);
    assert_eq!(
        (churn.allocs, churn.reallocs, churn.allocated_bytes),
        (2, 0, 6 * std::mem::size_of::<usize>())
    );
    scene.entities[132].ball = None;
    scene.entities[132].physics = None;
    scene.entities[2].physics = Some(Physics {
        velocity: [0.0; 3],
        gravity: false,
        collision: false,
    });
    scene.initialize();
    assert_eq!(scene.ball_indices.as_ref(), &[133, 134]);
    assert_eq!(scene.physics_indices.as_ref(), &[2, 133, 134]);
    scene.entities.clear();
    let (_, churn) = allocations::measure(|| scene.initialize());
    assert!(scene.ball_indices.is_empty() && scene.physics_indices.is_empty());
    assert!(!scene.indexed_queries);
    assert_eq!((churn.allocs, churn.reallocs), (0, 0));
    let mut dense = fixture(0, 8, 128, false, false);
    let (_, churn) = allocations::measure(|| dense.initialize());
    assert!(!dense.indexed_queries);
    assert!(dense.ball_indices.is_empty() && dense.physics_indices.is_empty());
    assert_eq!((churn.allocs, churn.reallocs), (0, 0));
    let mut small = fixture(0, 8, 2, false, false);
    let (_, churn) = allocations::measure(|| small.initialize());
    assert!(!small.indexed_queries);
    assert!(small.ball_indices.is_empty() && small.physics_indices.is_empty());
    assert_eq!((churn.allocs, churn.reallocs), (0, 0));
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_physics() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, background, boxes, balls, rotated, contact, setup, ticks) in [
        ("physics_small", 0, 8, 2, true, false, false, 256),
        ("physics_sparse", 1000, 32, 2, true, false, false, 64),
        ("physics_many_balls", 0, 32, 128, true, false, false, 32),
        ("physics_unrotated", 1000, 32, 2, false, false, false, 64),
        ("physics_static", 2000, 0, 0, false, false, false, 64),
        ("physics_contact", 128, 8, 2, false, true, false, 128),
        ("physics_init_empty", 0, 0, 0, false, false, true, 1000),
        ("physics_init_sparse", 1000, 32, 2, true, false, true, 1000),
        ("physics_init_many", 0, 32, 128, true, false, true, 1000),
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let mut scene = ready(background, boxes, balls, rotated, contact);
                // Setup measurements begin with no query buffers. Subsequent
                // initializations replace them and include their destruction.
                if setup {
                    scene.ball_indices = Box::default();
                    scene.physics_indices = Box::default();
                }
                let mut output = SongLuaLiveOutput::default();
                let start = Instant::now();
                let start_cycles = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..ticks {
                        if setup {
                            if current {
                                scene.initialize()
                            } else {
                                before::initialize(&mut scene)
                            }
                            black_box(&scene);
                        } else {
                            if current {
                                scene.tick(&mut output)
                            } else {
                                before::tick(&mut scene, &mut output)
                            };
                            black_box(&scene.entities);
                            black_box(&output);
                            output.sounds.clear();
                        }
                    }
                });
                let used = cycles() - start_cycles;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":ticks,
                    "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},
                "samples":samples,"unit":if setup {"initializations"}else{"simulation_ticks"},"baseline_commit":"20586889b"})
            );
        }
    }
}
