use super::{SongTimeNs, State};

pub(crate) use deadsync_core::song_time::{
    INVALID_SONG_TIME_NS, normalized_song_rate, song_time_ns_delta_seconds,
    song_time_ns_from_seconds, song_time_ns_invalid, song_time_ns_to_seconds,
};
use deadsync_gameplay::assist_row_no_offset_for_timing;

#[inline(always)]
pub(super) fn current_music_time_s(state: &State) -> f32 {
    song_time_ns_to_seconds(state.current_music_time_ns)
}

#[inline(always)]
pub(super) fn assist_row_no_offset(state: &State, music_time: f32) -> i32 {
    assist_row_no_offset_ns(state, song_time_ns_from_seconds(music_time))
}

#[inline(always)]
pub(super) fn assist_row_no_offset_ns(state: &State, music_time_ns: SongTimeNs) -> i32 {
    assist_row_no_offset_for_timing(&state.timing, state.global_offset_seconds, music_time_ns)
}
