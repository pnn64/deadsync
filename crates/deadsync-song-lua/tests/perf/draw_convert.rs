use super::super::*;

#[path = "draw_convert_before.rs"]
mod before;

fn fixture(owners: usize, frames: usize, calls: usize) -> (Capture, FxHashMap<usize, DrawSource>) {
    let sources = (1..=owners)
        .map(|p| (p, DrawSource::Overlay(p - 1)))
        .collect();
    let mut capture = Capture::default();
    for frame in 0..frames {
        capture.frames.push(RawFrame {
            second: frame as f32 / 60.0,
            immediate: false,
            owners: (1..=owners).collect(),
            calls: (0..calls)
                .map(|i| RawCall {
                    owner: i % owners + 1,
                    op: RawOp::Draw(
                        i % owners + 1,
                        SongLuaOverlayState {
                            x: i as f32,
                            ..Default::default()
                        },
                        Context::default(),
                        None,
                    ),
                })
                .collect(),
        });
    }
    (capture, sources)
}

fn compare(build: impl Fn() -> (Capture, FxHashMap<usize, DrawSource>)) {
    let (input, sources) = build();
    let old = before::convert(input, &sources);
    let (input, sources) = build();
    let new = convert(input, &sources);
    assert_eq!(format!("{old:?}"), format!("{new:?}"));
}

#[test]
fn conversion_preserves_sparse_reordered_duplicate_and_immediate_owners() {
    for size in [1, 2, 8, 32, 128] {
        compare(|| {
            let (mut capture, mut sources) = fixture(size, 6, 37);
            capture.frames[0].owners.reverse();
            capture.frames[1].owners.truncate(1);
            capture.frames[1].calls.retain(|call| call.owner == 1);
            capture.frames[2].owners.clear();
            capture.frames[2].calls.clear();
            capture.frames[3].owners.extend([1, 1]);
            capture.frames[4].immediate = true;
            capture.frames[4].owners = vec![1];
            capture.frames[4].calls.retain(|call| call.owner == 1);
            // Distinct pointers can resolve to the same overlay. Calls still
            // belong to the first matching output frame, including duplicates.
            sources.insert(size + 1, DrawSource::Overlay(0));
            capture.frames[5].owners.insert(0, size + 1);
            (capture, sources)
        });
    }
    compare(|| fixture(0, 0, 0));
    compare(|| fixture(0, 4, 0));
}

#[test]
fn conversion_preserves_error_order_and_diagnostics() {
    for error in 0..9 {
        compare(|| {
            let (mut capture, mut sources) = fixture(4, 3, 4);
            capture.labels.insert(900, "Quad 'missing texture'".into());
            let frame = &mut capture.frames[1];
            frame.second = 1.2345678;
            match error {
                0 => {
                    sources.remove(&1);
                }
                1 => {
                    frame.calls[0].owner = 900;
                }
                2 => {
                    frame.calls[0].op = RawOp::Begin(900, true);
                }
                3 => {
                    frame.calls[0].op = RawOp::Finish(900);
                }
                4 => {
                    frame.calls[0].op = RawOp::Draw(
                        900,
                        SongLuaOverlayState::default(),
                        Context::default(),
                        None,
                    );
                }
                5 => {
                    frame.immediate = true;
                    frame.owners = vec![2, 3];
                }
                6 => {
                    frame.calls[0].op = RawOp::Draw(
                        900,
                        SongLuaOverlayState::default(),
                        Context::default(),
                        None,
                    );
                    frame.calls[1].owner = 901;
                }
                7 => {
                    frame.immediate = true;
                    frame.owners.clear();
                    frame.calls[0].op = RawOp::Begin(900, false);
                }
                _ => {
                    sources.insert(1, DrawSource::Player(0));
                }
            }
            (capture, sources)
        });
    }
}

#[test]
fn conversion_moves_retained_payloads_and_all_source_variants() {
    for current in [false, true] {
        let parents: Arc<[SongLuaOverlayState]> = Arc::from([SongLuaOverlayState {
            x: f32::from_bits(0x7fc01234),
            y: -0.0,
            ..Default::default()
        }]);
        let vertices: Arc<[SongLuaOverlayMeshVertex]> = Arc::from([SongLuaOverlayMeshVertex {
            pos: [-0.0, f32::INFINITY],
            z: f32::from_bits(0x7fc05678),
            color: [1.0; 4],
            uv: [-0.0, 0.5],
        }]);
        let (mut capture, mut sources) = fixture(2, 1, 0);
        let kinds = [
            DrawSource::Overlay(8),
            DrawSource::Player(1),
            DrawSource::Judgment(0),
            DrawSource::Combo(1),
            DrawSource::SongForeground,
            DrawSource::ScreenLayer(2),
        ];
        for (i, source) in kinds.iter().enumerate() {
            sources.insert(100 + i, *source);
            capture.frames[0].calls.push(RawCall {
                owner: 1,
                op: RawOp::Draw(
                    100 + i,
                    parents[0],
                    Context {
                        parents: parents.clone(),
                        camera: Some(parents[0]),
                        ..Default::default()
                    },
                    Some(vertices.clone()),
                ),
            });
        }
        capture.frames[0].calls.push(RawCall {
            owner: 1,
            op: RawOp::Begin(2, true),
        });
        capture.frames[0].calls.push(RawCall {
            owner: 2,
            op: RawOp::Finish(2),
        });
        let result = if current {
            convert(capture, &sources)
        } else {
            before::convert(capture, &sources)
        }
        .unwrap();
        assert_eq!(result[0].1.owner_ends, vec![7]);
        assert_eq!(result[1].1.owner_ends, vec![1]);
        for (op, source) in result[0].1.ops.iter().zip(kinds) {
            let DrawOp::Draw {
                source: actual,
                state,
                parents: p,
                vertices: Some(v),
                camera: Some(camera),
            } = op
            else {
                panic!("draw payload lost")
            };
            assert_eq!(*actual, source);
            assert!(Arc::ptr_eq(p, &parents));
            assert!(Arc::ptr_eq(v, &vertices));
            assert_eq!(state.x.to_bits(), 0x7fc01234);
            assert_eq!(camera.y.to_bits(), (-0.0f32).to_bits());
            assert_eq!(v[0].z.to_bits(), 0x7fc05678);
        }
    }
}

#[test]
fn conversion_matches_varied_frame_sequences_and_large_overlay_indices() {
    for seed in 0..80 {
        compare(|| {
            let (mut capture, mut sources) = fixture(17, 8, 0);
            for (index, frame) in capture.frames.iter_mut().enumerate() {
                frame.owners.retain(|owner| (owner + seed + index) % 3 != 0);
                let offset = (seed + index) % frame.owners.len();
                frame.owners.rotate_left(offset);
                frame.immediate = (seed + index) % 4 == 0;
                frame.second = [0.0, -0.0, f32::INFINITY, f32::NAN][index % 4];
                if index % 3 == 0 {
                    frame.owners.push(frame.owners[0]);
                }
                for i in 0..(seed * 13 + index * 7) % 70 {
                    let owner = frame.owners[i % frame.owners.len()];
                    frame.calls.push(RawCall {
                        owner,
                        op: RawOp::Draw(
                            owner,
                            SongLuaOverlayState::default(),
                            Context::default(),
                            None,
                        ),
                    });
                }
            }
            sources.insert(1, DrawSource::Overlay(usize::MAX));
            sources.insert(2, DrawSource::Overlay(usize::MAX));
            (capture, sources)
        });
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_conversion() {
    use super::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, owners, frames, calls) in [
        ("convert_single_owner", 1, 128, 32),
        ("convert_two_owners", 2, 128, 64),
        ("convert_eight_owners", 8, 64, 128),
        ("convert_many_owners", 32, 64, 256),
        ("convert_wide_owners", 128, 32, 256),
        ("convert_sparse", 32, 128, 4),
        ("convert_quiet", 32, 128, 0),
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let (input, sources) = fixture(owners, frames, calls);
                let start = Instant::now();
                let start_cycles = cycles();
                let (output, churn) = allocations::measure(|| {
                    if current {
                        convert(input, &sources)
                    } else {
                        before::convert(input, &sources)
                    }
                });
                let used = cycles() - start_cycles;
                let nanos = start.elapsed().as_nanos();
                assert!(output.is_ok());
                black_box(&output);
                drop(output);
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":frames,
                        "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current {"after"} else {"before"},
                "samples":samples,"unit":"raw_frames","baseline_commit":"276198421"})
            );
        }
    }
}
