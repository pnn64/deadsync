use deadsync_rules::judgment::JudgeGrade;

use super::offset::{apply_global_offset_delta, apply_song_offset_delta};
use super::{
    AutosyncMode, AutosyncOffsetCorrection, SongTimeNs, State, apply_autosync_offset_sample,
    autoplay_blocks_scoring,
};

#[inline(always)]
fn apply_autosync_offset_correction(state: &mut State, note_off_by_ns: SongTimeNs) {
    let result = apply_autosync_offset_sample(
        &mut state.autosync_offset_samples,
        &mut state.autosync_offset_sample_count,
        state.autosync_mode,
        note_off_by_ns,
    );
    if let Some(stddev) = result.standard_deviation {
        state.autosync_standard_deviation = stddev;
    }
    match result.correction {
        Some(AutosyncOffsetCorrection::Song(mean)) => {
            let _ = apply_song_offset_delta(state, mean);
        }
        Some(AutosyncOffsetCorrection::Machine(mean)) => {
            let _ = apply_global_offset_delta(state, mean);
        }
        None => {}
    }
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
