// Frozen from b015e8b27; visibility and formatting only.
use super::*;

pub(super) fn merge_scheduled_overlay_samples_from_buffer(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    baseline: &[SongLuaOverlayState],
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
) {
    for track in tracks.iter_mut() {
        sort_overlay_update_samples(&mut track.samples);
    }
    scheduled.sort_by(|left, right| left.start_beat.total_cmp(&right.start_beat));
    for sample in scheduled.drain(..) {
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

pub(super) fn sample_at_or_before(
    samples: &[SongLuaOverlayUpdateSample],
    beat: f32,
) -> Option<&SongLuaOverlayUpdateSample> {
    samples.iter().rev().find(|current| current.beat <= beat)
}
