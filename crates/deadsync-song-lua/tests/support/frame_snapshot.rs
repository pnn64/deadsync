//! Retain every original track field and explicit floating-point bit check.
use deadsync_song_lua::{SongLuaOverlayUpdateTrack, SongLuaOverlayUpdateValue};

pub fn tracks(tracks: &[SongLuaOverlayUpdateTrack]) -> impl std::fmt::Debug + '_ {
    tracks
        .iter()
        .map(|track| {
            let samples = track
                .samples
                .iter()
                .map(|sample| {
                    let bits = match sample.value {
                        SongLuaOverlayUpdateValue::F32(value) => Some(value.to_bits()),
                        _ => None,
                    };
                    (sample.time.to_bits(), &sample.value, bits)
                })
                .collect::<Vec<_>>();
            (track.overlay_index, track.target, samples)
        })
        .collect::<Vec<_>>()
}
