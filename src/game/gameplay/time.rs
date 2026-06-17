use super::State;

pub(crate) use deadsync_core::song_time::{
    INVALID_SONG_TIME_NS, normalized_song_rate, scaled_song_delta_ns, song_time_ns_add_seconds,
    song_time_ns_delta_seconds, song_time_ns_from_seconds, song_time_ns_invalid,
    song_time_ns_to_seconds,
};

#[inline(always)]
pub(super) fn current_music_time_s(state: &State) -> f32 {
    song_time_ns_to_seconds(state.current_music_time_ns)
}
