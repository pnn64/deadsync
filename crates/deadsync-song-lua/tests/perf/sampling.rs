use super::*;
use std::hint::black_box;

// Frozen pre-optimization algorithm: sorting is stable and the LAST sample in
// each epsilon-connected run wins, including its timestamp.
fn legacy_sort(samples: &mut Vec<SongLuaOverlayUpdateSample>) {
    samples.sort_by(|left, right| left.beat.total_cmp(&right.beat));
    let mut merged: Vec<SongLuaOverlayUpdateSample> = Vec::with_capacity(samples.len());
    for sample in samples.drain(..) {
        if let Some(last) = merged.last_mut()
            && (last.beat - sample.beat).abs() <= f32::EPSILON
        {
            *last = sample;
        } else {
            merged.push(sample);
        }
    }
    *samples = merged;
}

fn samples(beats: impl IntoIterator<Item = f32>) -> Vec<SongLuaOverlayUpdateSample> {
    beats
        .into_iter()
        .enumerate()
        .map(|(index, beat)| SongLuaOverlayUpdateSample {
            beat,
            value: SongLuaOverlayUpdateValue::F32(index as f32),
        })
        .collect()
}

fn assert_samples_eq(
    actual: &[SongLuaOverlayUpdateSample],
    expected: &[SongLuaOverlayUpdateSample],
) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.beat.to_bits(), expected.beat.to_bits());
        assert_eq!(actual.value, expected.value);
    }
}

#[test]
fn sample_compaction_matches_legacy_order_and_last_write() {
    let edge_beats = [
        0.0,
        -0.0,
        f32::EPSILON,
        2.0 * f32::EPSILON,
        3.0 * f32::EPSILON,
        1.0,
        1.0 + f32::EPSILON,
        -1.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::from_bits(0xffc00001),
    ];
    for len in [0, 1, 2, 3, 16, 129, 1024] {
        for seed in 1..=16_u64 {
            let mut rng = seed;
            let mut actual = samples((0..len).map(|_| {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                edge_beats[(rng >> 32) as usize % edge_beats.len()]
            }));
            let mut expected = actual.clone();
            legacy_sort(&mut expected);
            sort_overlay_update_samples(&mut actual);
            assert_samples_eq(&actual, &expected);
        }
    }
    let mut chain = samples([0.0, f32::EPSILON, 2.0 * f32::EPSILON]);
    sort_overlay_update_samples(&mut chain);
    assert_eq!(
        chain,
        vec![SongLuaOverlayUpdateSample {
            beat: 2.0 * f32::EPSILON,
            value: SongLuaOverlayUpdateValue::F32(2.0),
        }]
    );
}

fn scheduled_sample(index: usize, end: f32) -> SongLuaScheduledOverlaySample {
    SongLuaScheduledOverlaySample {
        overlay_index: index % 2,
        target: SongLuaOverlayUpdateTarget::X,
        start_seconds: f64::from(end - 0.5),
        end_seconds: f64::from(end),
        start_beat: end - 0.5,
        end_beat: end,
        easing: Some("linear".to_owned()),
        opt1: None,
        from: SongLuaOverlayUpdateValue::F32(0.0),
        value: SongLuaOverlayUpdateValue::F32(index as f32 + 10.0),
    }
}

// The old partition-and-rebuild path, retained only as a behavioral oracle.
fn legacy_complete(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    indices: &mut std::collections::HashMap<(usize, SongLuaOverlayUpdateTarget), usize>,
    baseline: &[SongLuaOverlayState],
    update: &mut [SongLuaOverlayState],
    next: &mut [SongLuaOverlayState],
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
    beat: f32,
) {
    let (mut completed, pending): (Vec<SongLuaScheduledOverlaySample>, Vec<_>) =
        std::mem::take(scheduled)
            .into_iter()
            .partition(|s| s.end_beat <= beat + f32::EPSILON);
    *scheduled = pending;
    if !completed.is_empty() {
        completed.sort_by(|a, b| a.end_beat.total_cmp(&b.end_beat));
        for sample in &completed {
            if let Some(state) = update.get_mut(sample.overlay_index) {
                set_overlay_state_update_value(state, sample.target, &sample.value);
            }
            if let Some(state) = next.get_mut(sample.overlay_index) {
                set_overlay_state_update_value(state, sample.target, &sample.value);
            }
        }
        merge_scheduled_overlay_samples(tracks, indices, baseline, completed);
    }
}

#[test]
fn completion_matches_legacy_across_interleaved_tweens_and_boundaries() {
    let baseline = [SongLuaOverlayState::default(); 2];
    let mut actual_update = baseline;
    let mut actual_next = baseline;
    let mut expected_update = baseline;
    let mut expected_next = baseline;
    let mut actual_tracks = Vec::new();
    let mut expected_tracks = Vec::new();
    let mut actual_indices = std::collections::HashMap::new();
    let mut expected_indices = std::collections::HashMap::new();
    let ends = [2.0, 0.5, 1.0, 0.5, 8.0, 2.0, 4.0];
    let mut actual = ends
        .into_iter()
        .enumerate()
        .map(|(i, end)| scheduled_sample(i, end))
        .collect();
    let mut expected = ends
        .into_iter()
        .enumerate()
        .map(|(i, end)| scheduled_sample(i, end))
        .collect();
    for beat in [
        f32::NAN,
        -1.0,
        0.25,
        0.5 - 2.0 * f32::EPSILON,
        0.5 - f32::EPSILON,
        0.5,
        1.0,
        0.75,
        2.0,
        3.0,
        4.0,
        8.0,
        9.0,
    ] {
        merge_completed_scheduled_overlay_samples(
            &mut actual_tracks,
            &mut actual_indices,
            &baseline,
            &mut actual_update,
            &mut actual_next,
            &mut actual,
            beat,
        );
        legacy_complete(
            &mut expected_tracks,
            &mut expected_indices,
            &baseline,
            &mut expected_update,
            &mut expected_next,
            &mut expected,
            beat,
        );
        assert_eq!(actual_update, expected_update);
        assert_eq!(actual_next, expected_next);
        assert_eq!(actual_tracks, expected_tracks);
        assert_eq!(actual_indices, expected_indices);
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(&expected) {
            assert_eq!(actual.overlay_index, expected.overlay_index);
            assert_eq!(actual.end_beat, expected.end_beat);
            assert_eq!(actual.value, expected.value);
            assert_eq!(actual.easing, expected.easing);
        }
    }
    assert_eq!(actual_next[0].x, 14.0);
    assert_eq!(actual_next[1].x, 15.0);
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn sampling_hot_path_bench() {
    for count in [16, 256, 4096] {
        let original = samples((0..count).map(|i| (i / 2) as f32));
        let mut buffer = original.clone();
        crate::perf::measure_sampled(&format!("compact_{count}"), 512, count, || {
            buffer.clone_from(black_box(&original));
            sort_overlay_update_samples(black_box(&mut buffer));
            black_box(&buffer);
        });
    }
    for count in [16, 256, 4096] {
        let mut scheduled = (0..count).map(|i| scheduled_sample(i, 10.0)).collect();
        let baseline = [SongLuaOverlayState::default(); 2];
        let mut update = baseline;
        let mut next = baseline;
        let mut tracks = Vec::new();
        let mut indices = std::collections::HashMap::new();
        crate::perf::measure_sampled(&format!("pending_{count}"), 512, count, || {
            merge_completed_scheduled_overlay_samples(
                &mut tracks,
                &mut indices,
                &baseline,
                &mut update,
                &mut next,
                black_box(&mut scheduled),
                black_box(1.0),
            );
        });
    }
}

#[test]
fn pending_tweens_keep_their_storage_without_churn() {
    let mut scheduled: Vec<_> = (0..256).map(|i| scheduled_sample(i, 10.0)).collect();
    let storage = scheduled.as_ptr();
    let baseline = [SongLuaOverlayState::default(); 2];
    let mut update = baseline;
    let mut next = baseline;
    let mut tracks = Vec::new();
    let mut indices = std::collections::HashMap::new();
    crate::perf::assert_no_churn(|| {
        for tick in 0..64 {
            merge_completed_scheduled_overlay_samples(
                &mut tracks,
                &mut indices,
                &baseline,
                &mut update,
                &mut next,
                &mut scheduled,
                tick as f32 / 60.0,
            );
        }
    });
    assert_eq!(scheduled.as_ptr(), storage);
    assert_eq!(scheduled.len(), 256);
    assert_eq!(update, baseline);
    assert_eq!(next, baseline);
    assert!(tracks.is_empty());
}

#[test]
fn ordered_tracks_compact_without_churn_and_retain_growth_capacity() {
    let original = samples((0..4096).map(|i| (i / 2) as f32));
    let mut buffer = original.clone();
    let capacity = buffer.capacity();
    crate::perf::assert_no_churn(|| {
        for _ in 0..4 {
            buffer.clone_from(&original);
            sort_overlay_update_samples(&mut buffer);
        }
    });
    assert_eq!(buffer.capacity(), capacity);
    assert_eq!(buffer.len(), 2048);
    for (i, sample) in buffer.iter().enumerate() {
        assert_eq!(
            sample.value,
            SongLuaOverlayUpdateValue::F32((i * 2 + 1) as f32)
        );
    }
}

#[test]
fn compaction_drops_superseded_owned_values_and_keeps_the_last_owner() {
    let removed = std::sync::Arc::new([[0.0; 4]; 4]);
    let removed_weak = std::sync::Arc::downgrade(&removed);
    let kept = std::sync::Arc::new([[1.0; 4]; 4]);
    let mut values = vec![
        SongLuaOverlayUpdateSample {
            beat: 1.0,
            value: SongLuaOverlayUpdateValue::VertexColors(removed),
        },
        SongLuaOverlayUpdateSample {
            beat: 1.0,
            value: SongLuaOverlayUpdateValue::VertexColors(kept.clone()),
        },
    ];
    sort_overlay_update_samples(&mut values);
    assert!(removed_weak.upgrade().is_none());
    assert_eq!(values.len(), 1);
    assert_eq!(std::sync::Arc::strong_count(&kept), 2);
    assert_eq!(
        values[0].value,
        SongLuaOverlayUpdateValue::VertexColors(kept)
    );
}
