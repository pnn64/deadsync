use super::*;

#[path = "compiled_physics.rs"]
mod physics;

#[path = "compiled_render_before.rs"]
mod before;

fn fixture(count: usize, points: usize, players: usize, angles: bool) -> CompiledSongScene {
    let mut scene = CompiledSongScene {
        joined: std::array::from_fn(|p| p < players),
        screen: [640.0, 480.0],
        ..Default::default()
    };
    for i in 0..count {
        let mut e = Entity {
            position: [
                (i % 5) as f64 * 15.0,
                -((i % 7) as f64) * 12.0,
                1000.0 + i as f64 * 3.0,
            ],
            rotation: angles.then_some([0.1 + i as f64 * 0.001, 0.2]),
            color: Some([0.8, 0.2, 0.6, 0.95]),
            translate: Some([0.002, -0.003]),
            ..Default::default()
        };
        if points == 0 {
            e.ball = Some(Ball {
                radius: 16.0,
                bounce_height: 0.0,
                spawn: [0.0; 3],
                slippy: false,
            });
        } else {
            e.mesh = Some(Mesh {
                points: (0..points)
                    .map(|j| {
                        [
                            (j % 17) as f64 * 0.75 - 6.0,
                            (j % 11) as f64 * 0.5 - 2.0,
                            (j % 7) as f64 * 0.25 - 0.75,
                        ]
                    })
                    .collect(),
                corners: (0..points).map(|j| (j, [j as f32 * 0.01, 0.25])).collect(),
            });
        }
        scene.entities.push(e);
    }
    for player in 0..players {
        scene.entities.push(Entity {
            camera: Some(player),
            position: [player as f64 * 20.0, 10.0, 0.0],
            rotation: angles.then_some([0.03, -0.1]),
            ..Default::default()
        });
    }
    for player in 0..players {
        for entity in 0..count {
            let actor = scene.states.len();
            scene.states.push(SongLuaOverlayState {
                size: Some([48.0, 96.0]),
                ..Default::default()
            });
            scene.bindings.push(Binding {
                player,
                entity,
                actor,
            });
            scene.meshes.push((points > 0).then(|| {
                (0..points)
                    .map(|_| SongLuaOverlayMeshVertex {
                        pos: [0.0; 2],
                        z: 0.0,
                        color: [1.0; 4],
                        uv: [0.0; 2],
                    })
                    .collect::<Vec<_>>()
                    .into()
            }));
        }
    }
    scene.initialize();
    scene
}

fn same_number(a: f64, b: f64) {
    assert!(
        a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
        "{a:?} != {b:?}"
    );
}

fn compare_scene(a: &CompiledSongScene, b: &CompiledSongScene) {
    assert_eq!(format!("{:?}", a.states), format!("{:?}", b.states));
    assert_eq!(a.points.len(), b.points.len());
    for (a, b) in a.points.iter().zip(&b.points) {
        for axis in 0..3 {
            same_number(a[axis], b[axis]);
        }
    }
    assert_eq!(a.meshes.len(), b.meshes.len());
    for (a, b) in a.meshes.iter().zip(&b.meshes) {
        assert_eq!(a.is_some(), b.is_some());
        if let (Some(a), Some(b)) = (a, b) {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b.iter()) {
                for axis in 0..2 {
                    same_number(a.pos[axis] as f64, b.pos[axis] as f64);
                }
                same_number(a.z as f64, b.z as f64);
                assert_eq!(a.uv, b.uv);
                assert_eq!(a.color, b.color);
            }
        }
    }
}

#[test]
fn render_matches_frozen_geometry_cameras_and_retained_meshes() {
    for (count, points, players, angles) in [
        (0, 0, 0, false),
        (1, 0, 1, false),
        (16, 0, 2, true),
        (4, 8, 1, true),
        (8, 32, 2, false),
        (2, 128, 2, true),
    ] {
        let mut old = fixture(count, points, players, angles);
        let mut new = fixture(count, points, players, angles);
        // Duplicate cameras must preserve the first matching entity.
        for scene in [&mut old, &mut new] {
            scene.entities.push(Entity {
                camera: Some(0),
                position: [999.0; 3],
                ..Default::default()
            });
        }
        let retained_old = old.meshes.clone();
        let retained_new = new.meshes.clone();
        let snapshot = format!("{retained_new:?}");
        for frame in 0..8 {
            for scene in [&mut old, &mut new] {
                scene.frame = frame;
                for player in 0..players {
                    scene.entities[count + player].position[0] =
                        frame as f64 * 0.25 + player as f64;
                    if angles {
                        scene.entities[count + player].rotation =
                            Some([0.03 + frame as f64 * 0.01, -0.1]);
                    }
                }
                if count > 0 {
                    scene.entities[0].position[2] = [-100.0, 0.0, 0.5, 1000.0][frame as usize % 4];
                }
            }
            before::render(&mut old);
            new.render();
            compare_scene(&old, &new);
        }
        assert_eq!(format!("{retained_old:?}"), snapshot);
        assert_eq!(format!("{retained_new:?}"), snapshot);
    }
}

#[test]
fn prepared_rotation_preserves_original_arithmetic() {
    for angles in [
        None,
        Some([0.0, -0.0]),
        Some([0.17, -0.31]),
        Some([1e6, -1e-6]),
        Some([f64::NAN, 0.5]),
        Some([0.0, f64::INFINITY]),
    ] {
        for point in [
            [0.0, -0.0, 1.0],
            [13.25, -27.5, 1000.0],
            [f64::INFINITY, 1.0, -2.0],
        ] {
            for inverse in [false, true] {
                let mut old = point;
                let mut new = point;
                before::rotate_for_test(&mut old, angles, inverse);
                rotate(&mut new, angles, inverse);
                for axis in 0..3 {
                    same_number(old[axis], new[axis]);
                }
            }
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_render() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, count, points, players, angles, frames) in [
        ("render_small_balls", 16, 0, 1, true, 200),
        ("render_many_balls", 256, 0, 2, true, 100),
        ("render_boxes", 128, 8, 2, true, 64),
        ("render_dense_mesh", 1, 4096, 2, true, 32),
        ("render_unrotated", 64, 8, 2, false, 128),
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let mut scene = fixture(count, points, players, angles);
                let start = Instant::now();
                let start_cycles = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..frames {
                        if current {
                            scene.render()
                        } else {
                            before::render(&mut scene)
                        };
                        black_box(&scene.states);
                        black_box(&scene.meshes);
                        scene.frame += 1;
                    }
                });
                let used = cycles() - start_cycles;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":frames,
                    "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},
                "samples":samples,"unit":"scene_frames","baseline_commit":"5ffd4a261"})
            );
        }
    }
}
