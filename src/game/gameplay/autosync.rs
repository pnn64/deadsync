use deadsync_rules::judgment::JudgeGrade;

use super::offset::{apply_global_offset_delta, apply_song_offset_delta};
use super::{
    AUTOSYNC_OFFSET_SAMPLE_COUNT, AUTOSYNC_STDDEV_MAX_SECONDS, AutosyncMode, SongTimeNs, State,
    autoplay_blocks_scoring, autosync_mean_ns, autosync_stddev_seconds, song_time_ns_invalid,
    song_time_ns_to_seconds,
};

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

    let row_len = usize::from(state.row_entries[row_entry_index].nonmine_note_count);
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
