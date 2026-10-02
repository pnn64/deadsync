use super::*;
use std::hint::black_box;
use std::sync::Arc;

#[path = "scheduled_merge_baseline.rs"]
mod baseline;

type Indices = FxHashMap<(usize, SongLuaOverlayUpdateTarget), usize>;

fn value_bits(value: &SongLuaOverlayUpdateValue) -> (String, Vec<u32>) {
    use SongLuaOverlayUpdateValue as V;
    let bits = match value {
        V::F32(v) => vec![v.to_bits()],
        V::Vec2(v) => v.map(f32::to_bits).to_vec(),
        V::Vec3(v) => v.map(f32::to_bits).to_vec(),
        V::Vec4(v) => v.map(f32::to_bits).to_vec(),
        V::Vec5(v) => v.map(f32::to_bits).to_vec(),
        V::VertexColors(v) => v.iter().flatten().map(|v| v.to_bits()).collect(),
        _ => Vec::new(),
    };
    (format!("{value:?}"), bits)
}

fn assert_tracks(actual: &[SongLuaOverlayUpdateTrack], expected: &[SongLuaOverlayUpdateTrack]) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert_eq!((a.overlay_index, a.target), (b.overlay_index, b.target));
        assert_eq!(a.samples.len(), b.samples.len());
        for (a, b) in a.samples.iter().zip(&b.samples) {
            assert_eq!(a.beat.to_bits(), b.beat.to_bits());
            assert_eq!(value_bits(&a.value), value_bits(&b.value));
        }
    }
}

fn samples(beats: impl IntoIterator<Item = f32>) -> Vec<SongLuaOverlayUpdateSample> {
    beats
        .into_iter()
        .enumerate()
        .map(|(i, beat)| SongLuaOverlayUpdateSample {
            beat,
            value: SongLuaOverlayUpdateValue::F32(i as f32),
        })
        .collect()
}

fn assert_lookup(values: &[SongLuaOverlayUpdateSample], beat: f32) {
    let old = baseline::sample_at_or_before(values, beat);
    let new = overlay_sample_at_or_before(values, beat);
    // Compare the selected position as well as its value: ties must retain the
    // exact last eligible sample, including its owner and signed-zero beat.
    assert_eq!(old.map(std::ptr::from_ref), new.map(std::ptr::from_ref));
}

const EDGES: [f32; 16] = [
    f32::NEG_INFINITY,
    -1.0,
    -f32::EPSILON,
    -0.0,
    0.0,
    f32::EPSILON,
    2.0 * f32::EPSILON,
    3.0 * f32::EPSILON,
    1.0,
    1.0 + f32::EPSILON,
    f32::INFINITY,
    f32::from_bits(0xffc00001),
    f32::from_bits(0xff800001),
    f32::from_bits(0x7fc00003),
    f32::from_bits(0x7f800001),
    16.0,
];

fn next(rng: &mut u64) -> u32 {
    *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*rng >> 32) as u32
}

#[test]
fn scheduled_merge_lookup_preserves_signed_zero_nan_prefixes_payloads_and_infinities() {
    for len in [0, 1, 2, 16, 31, 32, 33, 128, 1024] {
        let mut values = samples(EDGES.into_iter().cycle().take(len));
        values.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        for beat in EDGES {
            assert_lookup(&values, beat);
            assert_lookup(&values, beat + f32::EPSILON);
        }
    }
    let mut zeros = samples([-0.0, 0.0]);
    zeros.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    let found = overlay_sample_at_or_before(&zeros, -0.0).unwrap();
    assert_eq!(found.beat.to_bits(), 0.0_f32.to_bits());
    assert_eq!(found.value, SongLuaOverlayUpdateValue::F32(1.0));
}

#[test]
fn scheduled_merge_lookup_matches_parent_for_large_sorted_histories_and_random_float_bits() {
    for seed in 1..=12 {
        let mut rng = seed;
        let mut values = samples((0..4096).map(|_| f32::from_bits(next(&mut rng))));
        values.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        for _ in 0..256 {
            assert_lookup(&values, f32::from_bits(next(&mut rng)));
        }
        for beat in EDGES {
            assert_lookup(&values, beat);
        }
    }
}

fn scheduled(
    actor: usize,
    target: SongLuaOverlayUpdateTarget,
    start: f32,
    end: f32,
    value: SongLuaOverlayUpdateValue,
) -> SongLuaScheduledOverlaySample {
    SongLuaScheduledOverlaySample {
        dispatch_seconds: None,
        frame_advance: 0.0,
        overlay_index: actor,
        target,
        start_seconds: f64::from(start),
        end_seconds: f64::from(end),
        start_beat: start,
        end_beat: end,
        easing: None,
        opt1: None,
        from: SongLuaOverlayUpdateValue::None,
        value,
    }
}

struct Fixture {
    baseline: Vec<SongLuaOverlayState>,
    tracks: Vec<SongLuaOverlayUpdateTrack>,
    indices: Indices,
    pending: Vec<SongLuaScheduledOverlaySample>,
}

impl Fixture {
    fn new(count: usize, history: usize, colors: bool, mode: &str) -> Self {
        let baseline = vec![
            SongLuaOverlayState {
                x: -9.0,
                vertex_colors: colors.then_some([[0.25; 4]; 4]),
                ..Default::default()
            };
            count
        ];
        let target = if colors {
            SongLuaOverlayUpdateTarget::VertexColors
        } else {
            SongLuaOverlayUpdateTarget::X
        };
        let mut tracks = Vec::with_capacity(count);
        let mut indices = Indices::with_capacity_and_hasher(count, Default::default());
        let mut pending = Vec::with_capacity(count);
        for (actor, state) in baseline.iter().enumerate() {
            if mode != "new_track" {
                let value = overlay_state_update_value(state, target);
                let mut values = Vec::with_capacity(history + 4);
                values.extend((0..history).map(|i| SongLuaOverlayUpdateSample {
                    beat: i as f32,
                    value: value.clone(),
                }));
                indices.insert((actor, target), tracks.len());
                tracks.push(SongLuaOverlayUpdateTrack {
                    overlay_index: actor,
                    target,
                    samples: values,
                });
            }
            if mode != "empty" {
                let (start, end) = match mode {
                    "step_before" => (-2.0, -2.0),
                    "step_after" => (history as f32 + 1.0, history as f32 + 1.0),
                    "tween_before" => (-2.0, -1.0),
                    "tween_middle" => (history as f32 * 0.5, history as f32 * 0.5 + 0.5),
                    "tween_after" => (history as f32 + 1.0, history as f32 + 2.0),
                    "new_track" => (0.5, 1.0),
                    _ => panic!("unknown mode"),
                };
                pending.push(scheduled(
                    actor,
                    target,
                    start,
                    end,
                    if colors {
                        SongLuaOverlayUpdateValue::VertexColors(Arc::new([[0.75; 4]; 4]))
                    } else {
                        SongLuaOverlayUpdateValue::F32(17.0)
                    },
                ));
            }
        }
        Self {
            baseline,
            tracks,
            indices,
            pending,
        }
    }

    fn merge(&mut self, old: bool) {
        if old {
            baseline::merge_scheduled_overlay_samples_from_buffer(
                &mut self.tracks,
                &mut self.indices,
                &self.baseline,
                &mut self.pending,
            );
        } else {
            merge_scheduled_overlay_samples_from_buffer(
                &mut self.tracks,
                &mut self.indices,
                &self.baseline,
                &mut self.pending,
            );
        }
        black_box(&self.tracks);
    }
}

fn compare(mut old: Fixture, mut new: Fixture) -> Fixture {
    let old_capacity = old.pending.capacity();
    let new_capacity = new.pending.capacity();
    old.merge(true);
    new.merge(false);
    assert!(old.pending.is_empty() && new.pending.is_empty());
    assert_eq!(old.pending.capacity(), old_capacity);
    assert_eq!(new.pending.capacity(), new_capacity);
    assert_eq!(new.indices, old.indices);
    assert_tracks(&new.tracks, &old.tracks);
    new
}

#[test]
fn scheduled_merge_matches_parent_for_steps_anchors_existing_empty_and_new_tracks() {
    for count in [1, 2, 16] {
        for history in [0, 1, 16, 128] {
            for colors in [false, true] {
                for mode in [
                    "empty",
                    "step_before",
                    "step_after",
                    "tween_before",
                    "tween_middle",
                    "tween_after",
                    "new_track",
                ] {
                    compare(
                        Fixture::new(count, history, colors, mode),
                        Fixture::new(count, history, colors, mode),
                    );
                }
            }
        }
    }
}

#[test]
fn scheduled_merge_preserves_stable_ties_epsilon_bridges_rewinds_and_nonfinite_beats() {
    for seed in 1..=16 {
        let mut rng = seed;
        let mut old = Fixture::new(4, 0, true, "empty");
        let mut new = Fixture::new(4, 0, true, "empty");
        for fixture in [&mut old, &mut new] {
            for track in &mut fixture.tracks {
                track.samples = samples(EDGES.into_iter().rev().cycle().take(64));
            }
        }
        for _ in 0..12 {
            for _ in 0..24 {
                let actor = next(&mut rng) as usize % 4;
                let start = EDGES[next(&mut rng) as usize % EDGES.len()];
                let end = EDGES[next(&mut rng) as usize % EDGES.len()];
                let value = SongLuaOverlayUpdateValue::F32(f32::from_bits(next(&mut rng)));
                let sample = scheduled(actor, SongLuaOverlayUpdateTarget::X, start, end, value);
                old.pending.push(sample.clone());
                new.pending.push(sample);
            }
            old.merge(true);
            new.merge(false);
            assert_eq!(new.indices, old.indices);
            assert_tracks(&new.tracks, &old.tracks);
        }
    }
}

#[test]
fn scheduled_merge_canonicalizes_untouched_tracks_with_no_pending_work() {
    let mut old = Fixture::new(4, 0, true, "empty");
    let mut new = Fixture::new(4, 0, true, "empty");
    for fixture in [&mut old, &mut new] {
        for track in &mut fixture.tracks {
            track.samples = samples([
                1.0,
                0.0,
                f32::EPSILON,
                2.0 * f32::EPSILON,
                -0.0,
                f32::NEG_INFINITY,
                f32::from_bits(0xffc00001),
                f32::from_bits(0x7fc00003),
            ]);
        }
    }
    let mut new = compare(old, new);
    let original = new.tracks.clone();
    new.merge(false);
    assert_tracks(&new.tracks, &original);
    for track in &new.tracks {
        assert!(
            track
                .samples
                .is_sorted_by(|a, b| a.beat.total_cmp(&b.beat).is_le())
        );
    }
}

#[test]
fn scheduled_merge_retains_color_owners_for_required_anchors_and_end_values() {
    let mut old = Fixture::new(1, 1, true, "tween_after");
    let mut new = Fixture::new(1, 1, true, "tween_after");
    let SongLuaOverlayUpdateValue::VertexColors(current) = &new.tracks[0].samples[0].value else {
        unreachable!()
    };
    let current = Arc::clone(current);
    let SongLuaOverlayUpdateValue::VertexColors(next) = &new.pending[0].value else {
        unreachable!()
    };
    let next = Arc::clone(next);
    old.merge(true);
    new.merge(false);
    assert_tracks(&new.tracks, &old.tracks);
    assert_eq!(new.tracks[0].samples.len(), 3);
    let SongLuaOverlayUpdateValue::VertexColors(anchor) = &new.tracks[0].samples[1].value else {
        unreachable!()
    };
    let SongLuaOverlayUpdateValue::VertexColors(end) = &new.tracks[0].samples[2].value else {
        unreachable!()
    };
    assert!(Arc::ptr_eq(anchor, &current));
    assert!(Arc::ptr_eq(end, &next));
    let step = compare(
        Fixture::new(1, 1, true, "step_before"),
        Fixture::new(1, 1, true, "step_before"),
    );
    assert_eq!(step.tracks[0].samples.len(), 2);
    assert_eq!(step.tracks[0].samples[0].beat, -2.0);
}

#[test]
fn scheduled_merge_ordered_appends_preserve_last_write_and_epsilon_connected_runs() {
    let mut old = Fixture::new(1, 0, false, "empty");
    let mut new = Fixture::new(1, 0, false, "empty");
    for fixture in [&mut old, &mut new] {
        fixture.tracks[0].samples = samples([0.0, 2.0 * f32::EPSILON]);
        fixture.pending = [
            (2.0 * f32::EPSILON, 3.0 * f32::EPSILON, 17.0),
            (3.0 * f32::EPSILON, 4.0 * f32::EPSILON, 23.0),
            (4.0 * f32::EPSILON, 4.0 * f32::EPSILON, 41.0),
        ]
        .into_iter()
        .map(|(start, end, value)| {
            scheduled(
                0,
                SongLuaOverlayUpdateTarget::X,
                start,
                end,
                SongLuaOverlayUpdateValue::F32(value),
            )
        })
        .collect();
    }
    let new = compare(old, new);
    assert_eq!(new.tracks[0].samples.len(), 2);
    assert_eq!(new.tracks[0].samples[1].beat, 4.0 * f32::EPSILON);
    assert_eq!(
        new.tracks[0].samples[1].value,
        SongLuaOverlayUpdateValue::F32(41.0)
    );
}

#[test]
fn scheduled_merge_step_snapshots_and_borrowed_lookup_have_zero_temporary_churn() {
    let mut old = Fixture::new(16, 1, true, "step_before");
    let mut new = Fixture::new(16, 1, true, "step_before");
    crate::perf::assert_reduced_churn(|| old.merge(true), || new.merge(false));
    assert_tracks(&new.tracks, &old.tracks);
    let mut next = Fixture::new(16, 1, true, "step_before");
    crate::perf::assert_no_churn(|| next.merge(false));
    let mut empty = Fixture::new(16, 1024, false, "empty");
    crate::perf::assert_no_churn(|| empty.merge(false));
    let values = samples((0..8192).map(|i| i as f32));
    crate::perf::assert_no_churn(|| {
        for beat in [-1.0, 1.0, 4096.0, 8193.0, f32::NAN] {
            black_box(overlay_sample_at_or_before(
                black_box(&values),
                black_box(beat),
            ));
        }
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially"]
fn scheduled_merge_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for len in [0, 16, 256, 8192] {
        let values = samples((0..len).map(|i| i as f32));
        for (position, beat) in [
            ("before", -1.0),
            ("middle", len as f32 * 0.5),
            ("after", len as f32 + 1.0),
        ] {
            assert_lookup(&values, beat);
            for old in order {
                crate::perf::measure_sampled(
                    &format!(
                        "merge_lookup_{len}_{position}/{}",
                        if old { "old" } else { "new" }
                    ),
                    if len < 256 { 1024 } else { 128 },
                    32,
                    || {
                        for _ in 0..32 {
                            black_box(if black_box(old) {
                                baseline::sample_at_or_before(black_box(&values), black_box(beat))
                            } else {
                                overlay_sample_at_or_before(black_box(&values), black_box(beat))
                            });
                        }
                    },
                );
            }
        }
    }
    for count in [1, 16, 64] {
        for history in [16, 1024] {
            for old in order {
                let mut fixture = Fixture::new(count, history, false, "empty");
                crate::perf::measure_sampled(
                    &format!(
                        "merge_canonical_{count}_{history}/{}",
                        if old { "old" } else { "new" }
                    ),
                    128,
                    count * history,
                    || fixture.merge(old),
                );
            }
        }
    }
    for history in [16, 1024] {
        for colors in [false, true] {
            for mode in [
                "step_before",
                "step_after",
                "tween_before",
                "tween_middle",
                "tween_after",
            ] {
                for old in order {
                    crate::perf::measure_sampled_with_setup(
                        &format!(
                            "merge_full_{history}_colors_{colors}_{mode}/{}",
                            if old { "old" } else { "new" }
                        ),
                        64,
                        16,
                        || Fixture::new(16, history, colors, mode),
                        |fixture| fixture.merge(old),
                    );
                }
            }
        }
    }
    for colors in [false, true] {
        for old in order {
            crate::perf::measure_sampled_with_setup(
                &format!(
                    "merge_new_tracks_colors_{colors}/{}",
                    if old { "old" } else { "new" }
                ),
                128,
                16,
                || Fixture::new(16, 0, colors, "new_track"),
                |fixture| fixture.merge(old),
            );
        }
    }
}
