use super::offset::{apply_global_offset_delta, apply_song_offset_delta};
use super::{
    AutosyncOffsetCorrection, MAX_COLS, SongTimeNs, State, apply_autosync_offset_sample,
    autoplay_blocks_scoring, autosync_row_hits_enabled, collect_autosync_row_hit_offsets,
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
    if !autosync_row_hits_enabled(
        state.replay_mode,
        autoplay_blocks_scoring(state),
        state.autosync_mode,
        state.course_display_totals.is_some(),
    ) {
        return;
    }

    let mut offsets = [0; MAX_COLS];
    let count = collect_autosync_row_hit_offsets(
        &state.notes,
        &state.row_entries[row_entry_index],
        &mut offsets,
    );
    for note_off_by_ns in offsets.into_iter().take(count) {
        apply_autosync_offset_correction(state, note_off_by_ns);
    }
}
