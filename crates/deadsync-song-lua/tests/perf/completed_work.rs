use super::*;
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;

#[path = "completed_work_baseline.rs"]
mod baseline;

type TrackIndex = HashMap<(usize, SongLuaOverlayUpdateTarget), usize>;

fn sample(index: usize, start: f32, end: f32) -> SongLuaScheduledOverlaySample {
    SongLuaScheduledOverlaySample {
        overlay_index: index % 3,
        target: [SongLuaOverlayUpdateTarget::X, SongLuaOverlayUpdateTarget::Y][index % 2],
        start_seconds: f64::from(start),
        end_seconds: f64::from(end),
        start_beat: start,
        end_beat: end,
        easing: None,
        opt1: Some(-0.0),
        from: SongLuaOverlayUpdateValue::F32(-0.0),
        value: SongLuaOverlayUpdateValue::F32(index as f32),
    }
}

struct Fixture {
    baseline: Vec<SongLuaOverlayState>,
    update: Vec<SongLuaOverlayState>,
    next: Vec<SongLuaOverlayState>,
    tracks: Vec<SongLuaOverlayUpdateTrack>,
    indices: TrackIndex,
    scheduled: Vec<SongLuaScheduledOverlaySample>,
    completed: Vec<SongLuaScheduledOverlaySample>,
}

impl Fixture {
    fn new() -> Self {
        let baseline = vec![SongLuaOverlayState::default(); 3];
        Self {
            update: baseline.clone(),
            next: baseline.clone(),
            baseline,
            tracks: vec![],
            indices: TrackIndex::new(),
            scheduled: vec![],
            completed: vec![],
        }
    }
    fn advance(&mut self, old: bool, beat: f32) {
        if old {
            baseline::merge_completed_scheduled_overlay_samples(
                &mut self.tracks,
                &mut self.indices,
                &self.baseline,
                &mut self.update,
                &mut self.next,
                &mut self.scheduled,
                beat,
            );
        } else {
            merge_completed_scheduled_overlay_samples_into(
                &mut self.tracks,
                &mut self.indices,
                &self.baseline,
                &mut self.update,
                &mut self.next,
                &mut self.scheduled,
                beat,
                &mut self.completed,
            );
        }
        black_box((
            &self.tracks,
            &self.update,
            &self.next,
            &self.scheduled,
            &self.completed,
        ));
    }
    fn batch(&mut self, old: bool, samples: &[SongLuaScheduledOverlaySample], cold: bool) {
        self.update.copy_from_slice(&self.baseline);
        self.next.copy_from_slice(&self.baseline);
        for track in &mut self.tracks {
            track.samples.truncate(1);
        }
        self.scheduled.clear();
        self.scheduled.extend_from_slice(samples);
        if cold {
            self.completed = Vec::new();
        }
        self.advance(old, 4.0);
    }
}

fn assert_same(a: &Fixture, b: &Fixture) {
    assert_eq!(a.indices, b.indices);
    assert_eq!(a.update, b.update);
    assert_eq!(a.next, b.next);
    assert!(a.completed.is_empty());
    assert!(b.completed.is_empty());
    assert_eq!(a.scheduled.len(), b.scheduled.len());
    for (a, b) in a.scheduled.iter().zip(&b.scheduled) {
        assert_eq!(
            (a.overlay_index, a.target, &a.easing),
            (b.overlay_index, b.target, &b.easing)
        );
        assert_eq!(
            [a.start_beat, a.end_beat].map(f32::to_bits),
            [b.start_beat, b.end_beat].map(f32::to_bits)
        );
        assert_eq!(
            [a.start_seconds, a.end_seconds].map(f64::to_bits),
            [b.start_seconds, b.end_seconds].map(f64::to_bits)
        );
        assert_eq!(a.opt1.map(f32::to_bits), b.opt1.map(f32::to_bits));
        assert_eq!(a.from, b.from);
        assert_eq!(a.value, b.value);
    }
    assert_eq!(a.tracks.len(), b.tracks.len());
    for (a, b) in a.tracks.iter().zip(&b.tracks) {
        assert_eq!(
            (a.overlay_index, a.target, a.samples.len()),
            (b.overlay_index, b.target, b.samples.len())
        );
        for (a, b) in a.samples.iter().zip(&b.samples) {
            assert_eq!(a.beat.to_bits(), b.beat.to_bits());
            assert_eq!(a.value, b.value);
        }
    }
}

#[test]
fn lua_work_completed_tweens_match_parent_for_ties_rewinds_and_nonfinite_times() {
    let beats = [
        -2.0,
        -0.0,
        0.0,
        f32::EPSILON,
        2.0 * f32::EPSILON,
        1.0,
        1.0 + f32::EPSILON,
        3.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc00003),
    ];
    for seed in 0..32 {
        let mut old = Fixture::new();
        let mut new = Fixture::new();
        let samples: Vec<_> = (0..64)
            .map(|i| {
                sample(
                    i,
                    beats[(i * 7 + seed) % beats.len()],
                    beats[(i * 3 + seed) % beats.len()],
                )
            })
            .collect();
        old.scheduled = samples.clone();
        new.scheduled = samples;
        // Existing unrelated tracks must still receive the same canonicalization.
        let initial = SongLuaOverlayUpdateTrack {
            overlay_index: 0,
            target: SongLuaOverlayUpdateTarget::Z,
            samples: vec![
                SongLuaOverlayUpdateSample {
                    beat: 3.0,
                    value: SongLuaOverlayUpdateValue::F32(7.0),
                },
                SongLuaOverlayUpdateSample {
                    beat: -0.0,
                    value: SongLuaOverlayUpdateValue::F32(2.0),
                },
            ],
        };
        old.tracks.push(initial.clone());
        new.tracks.push(initial);
        old.indices.insert((0, SongLuaOverlayUpdateTarget::Z), 0);
        new.indices.insert((0, SongLuaOverlayUpdateTarget::Z), 0);
        old.update.truncate(2);
        new.update.truncate(2);
        old.next.truncate(1);
        new.next.truncate(1);
        for beat in [-3.0, 0.0, 1.0, -1.0, 3.0, f32::NAN, f32::INFINITY] {
            old.advance(true, beat);
            new.advance(false, beat);
            assert_same(&new, &old);
        }
    }
}

#[test]
fn lua_work_completed_tweens_release_metadata_and_keep_shared_value_identity() {
    let colors = Arc::new([[0.25, -0.0, 0.5, 1.0]; 4]);
    let mut input = sample(0, 0.0, 1.0);
    input.target = SongLuaOverlayUpdateTarget::VertexColors;
    input.value = SongLuaOverlayUpdateValue::VertexColors(colors.clone());
    input.easing = Some("decelerate".into());
    let mut old = Fixture::new();
    let mut new = Fixture::new();
    old.scheduled.push(input.clone());
    new.scheduled.push(input);
    old.advance(true, 1.0);
    new.advance(false, 1.0);
    assert_same(&new, &old);
    let SongLuaOverlayUpdateValue::VertexColors(value) =
        &new.tracks[0].samples.last().unwrap().value
    else {
        panic!("lost shared colors")
    };
    assert!(Arc::ptr_eq(value, &colors));
    assert_eq!(Arc::strong_count(&colors), 3);
    assert!(new.completed.capacity() > 0);
    assert!(new.completed.is_empty());
    let capacity = new.completed.capacity();
    new.advance(false, 2.0);
    assert_eq!(new.completed.capacity(), capacity);
}

#[test]
fn lua_work_completed_tweens_reuse_output_and_scratch_without_churn() {
    let samples: Vec<_> = (0..8).map(|i| sample(i, 0.0, 1.0)).collect();
    let mut fixture = Fixture::new();
    fixture.batch(false, &samples, false);
    let capacity = fixture.completed.capacity();
    crate::perf::assert_no_churn(|| {
        for _ in 0..16 {
            fixture.batch(false, &samples, false);
        }
        fixture.advance(false, 5.0);
    });
    assert_eq!(fixture.completed.capacity(), capacity);
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_work_bench_completed() {
    for (count, mode, cold) in [
        (0, "all", false),
        (1, "all", false),
        (8, "all", false),
        (128, "all", false),
        (128, "none", false),
        (128, "half", false),
        (128, "reverse", false),
        (128, "all", true),
    ] {
        let mut samples: Vec<_> = (0..count)
            .map(|i| {
                sample(
                    i,
                    (i % 8) as f32 * 0.125,
                    if mode == "none" || (mode == "half" && i % 2 == 0) {
                        8.0
                    } else {
                        1.0 + (i % 8) as f32 * 0.125
                    },
                )
            })
            .collect();
        if mode == "reverse" {
            samples.reverse();
        }
        let mut old = Fixture::new();
        let mut new = Fixture::new();
        old.batch(true, &samples, cold);
        new.batch(false, &samples, cold);
        assert_same(&new, &old);
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for is_old in order {
            crate::perf::measure_sampled(
                &format!(
                    "completed_{count}_{mode}_cold_{cold}/{}",
                    if is_old { "old" } else { "new" }
                ),
                128,
                count.max(1),
                || {
                    if is_old {
                        old.batch(true, &samples, cold);
                    } else {
                        new.batch(false, &samples, cold);
                    }
                },
            );
        }
    }
}
