use crate::game::judgment::JudgeGrade;

use super::offset::{apply_global_offset_delta, apply_song_offset_delta};
use super::{
    AUTOSYNC_OFFSET_SAMPLE_COUNT, AUTOSYNC_STDDEV_MAX_SECONDS, AutosyncMode, SongTimeNs, State,
    autoplay_blocks_scoring, song_time_ns_invalid, song_time_ns_span_seconds,
    song_time_ns_to_seconds,
};

#[inline(always)]
fn autosync_mean_ns(samples: &[SongTimeNs; AUTOSYNC_OFFSET_SAMPLE_COUNT]) -> SongTimeNs {
    let mut sum = 0i128;
    for value in samples {
        sum += i128::from(*value);
    }
    let count = AUTOSYNC_OFFSET_SAMPLE_COUNT as i128;
    let rounded = if sum >= 0 {
        (sum + count / 2) / count
    } else {
        (sum - count / 2) / count
    };
    rounded.clamp(i64::MIN as i128, i64::MAX as i128) as SongTimeNs
}

#[inline(always)]
fn autosync_stddev_seconds(
    samples: &[SongTimeNs; AUTOSYNC_OFFSET_SAMPLE_COUNT],
    mean_ns: SongTimeNs,
) -> f32 {
    let mut dev = 0.0_f64;
    for value in samples {
        let d = song_time_ns_span_seconds(i128::from(*value) - i128::from(mean_ns)) as f64;
        dev += d * d;
    }
    (dev / AUTOSYNC_OFFSET_SAMPLE_COUNT as f64).sqrt() as f32
}

#[inline(always)]
fn apply_autosync_offset_correction(state: &mut State, note_off_by_ns: SongTimeNs) {
    if song_time_ns_invalid(note_off_by_ns) || state.autosync_mode == AutosyncMode::Off {
        return;
    }
    let sample_ix = state
        .autosync_offset_sample_count
        .min(AUTOSYNC_OFFSET_SAMPLE_COUNT.saturating_sub(1));
    state.autosync_offset_samples[sample_ix] = note_off_by_ns;
    state.autosync_offset_sample_count = state.autosync_offset_sample_count.saturating_add(1);
    if state.autosync_offset_sample_count < AUTOSYNC_OFFSET_SAMPLE_COUNT {
        return;
    }

    let mean_ns = autosync_mean_ns(&state.autosync_offset_samples);
    let stddev = autosync_stddev_seconds(&state.autosync_offset_samples, mean_ns);
    if stddev < AUTOSYNC_STDDEV_MAX_SECONDS {
        let mean = song_time_ns_to_seconds(mean_ns);
        match state.autosync_mode {
            AutosyncMode::Off => {}
            AutosyncMode::Song => {
                if state.course_display_totals.is_none() {
                    let _ = apply_song_offset_delta(state, mean);
                }
            }
            AutosyncMode::Machine => {
                let _ = apply_global_offset_delta(state, mean);
            }
        }
    }

    state.autosync_standard_deviation = stddev;
    state.autosync_offset_sample_count = 0;
}

#[inline(always)]
pub(super) fn apply_autosync_for_row_hits(state: &mut State, row_entry_index: usize) {
    if state.replay_mode
        || autoplay_blocks_scoring(state)
        || state.autosync_mode == AutosyncMode::Off
    {
        return;
    }
    // ITG parity: AdjustSync::HandleAutosync() is disabled in course mode.
    if state.course_display_totals.is_some() {
        return;
    }

    let row_len = state.row_entries[row_entry_index]
        .nonmine_note_indices
        .len();
    let mut i = 0;
    while i < row_len {
        let note_index = state.row_entries[row_entry_index].nonmine_note_indices[i];
        let maybe_note_offset_ns = state.notes[note_index]
            .result
            .as_ref()
            .and_then(|judgment| {
                if matches!(
                    judgment.grade,
                    JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
                ) {
                    // ITG's fNoteOffset is positive when stepping early.
                    Some(-judgment.time_error_music_ns)
                } else {
                    None
                }
            });
        if let Some(note_off_by_ns) = maybe_note_offset_ns {
            apply_autosync_offset_correction(state, note_off_by_ns);
        }
        i += 1;
    }
}
