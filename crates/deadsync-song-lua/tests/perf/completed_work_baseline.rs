// Frozen from b63f6a0c9d58926bb863e2aae5cbc7e94690d476; only test visibility is adapted.
use super::*;

pub(super) fn merge_completed_scheduled_overlay_samples(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<(usize, SongLuaOverlayUpdateTarget), usize>,
    baseline: &[SongLuaOverlayState],
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
    beat: f32,
) {
    if scheduled.is_empty() {
        return;
    }
    // Keep pending tweens (and their capacity) in place across sample ticks.
    // The common case where none have completed does not allocate or move them.
    let mut completed: Vec<_> = scheduled
        .extract_if(.., |sample| sample.end_beat <= beat + f32::EPSILON)
        .collect();
    if !completed.is_empty() {
        completed.sort_by(|left, right| left.end_beat.total_cmp(&right.end_beat));
        for sample in &completed {
            if let Some(state) = update_states.get_mut(sample.overlay_index) {
                set_overlay_state_update_value(state, sample.target, &sample.value);
            }
            if let Some(state) = to_states.get_mut(sample.overlay_index) {
                set_overlay_state_update_value(state, sample.target, &sample.value);
            }
        }
        merge_scheduled_overlay_samples(tracks, track_indices, baseline, completed);
    }
}

pub(super) fn merge_scheduled_overlay_samples(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<(usize, SongLuaOverlayUpdateTarget), usize>,
    baseline: &[SongLuaOverlayState],
    mut scheduled: Vec<SongLuaScheduledOverlaySample>,
) {
    for track in tracks.iter_mut() {
        sort_overlay_update_samples(&mut track.samples);
    }
    scheduled.sort_by(|left, right| left.start_beat.total_cmp(&right.start_beat));
    for sample in scheduled {
        let track_index = *track_indices
            .entry((sample.overlay_index, sample.target))
            .or_insert_with(|| {
                let index = tracks.len();
                tracks.push(SongLuaOverlayUpdateTrack {
                    overlay_index: sample.overlay_index,
                    target: sample.target,
                    samples: vec![SongLuaOverlayUpdateSample {
                        beat: 0.0,
                        value: overlay_state_update_value(
                            &baseline[sample.overlay_index],
                            sample.target,
                        ),
                    }],
                });
                index
            });
        let track = &mut tracks[track_index];
        let current = track
            .samples
            .iter()
            .rev()
            .find(|current| current.beat <= sample.start_beat + f32::EPSILON)
            .map(|current| current.value.clone())
            .unwrap_or_else(|| {
                overlay_state_update_value(&baseline[sample.overlay_index], sample.target)
            });
        let has_start = sample.end_beat > sample.start_beat + f32::EPSILON;
        let first_beat = if has_start {
            sample.start_beat
        } else {
            sample.end_beat
        };
        let ordered = track
            .samples
            .last()
            .is_none_or(|last| last.beat.total_cmp(&first_beat).is_le());
        if ordered {
            // The existing prefix is sorted and compacted. An ordered append
            // can only merge with its last sample, preserving last-write wins.
            if has_start {
                append_ordered_overlay_sample(
                    &mut track.samples,
                    SongLuaOverlayUpdateSample {
                        beat: sample.start_beat,
                        value: current,
                    },
                );
            }
            append_ordered_overlay_sample(
                &mut track.samples,
                SongLuaOverlayUpdateSample {
                    beat: sample.end_beat,
                    value: sample.value,
                },
            );
            continue;
        }
        if has_start {
            track.samples.push(SongLuaOverlayUpdateSample {
                beat: sample.start_beat,
                value: current,
            });
        }
        track.samples.push(SongLuaOverlayUpdateSample {
            beat: sample.end_beat,
            value: sample.value,
        });
        sort_overlay_update_samples(&mut track.samples);
    }
    for track in tracks {
        sort_overlay_update_samples(&mut track.samples);
    }
}
