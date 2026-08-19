use deadlib_audio_core::{
    PlaybackPosMap, PlayedMapReader, bump_music_map_generation, music_map_generation,
    music_track_start_frame,
};
use std::sync::{LazyLock, Mutex};

#[derive(Default)]
struct MusicMapRuntime {
    played: Option<PlayedMapReader>,
    map: PlaybackPosMap,
}

/// Process-global played-position reader restored from the pre-v0.5.654
/// implementation.
///
/// The audio callback remains the sole producer. Application clock snapshots
/// serialize access to the reader and its derived map through this mutex, and
/// timeline resets synchronously discard every queued segment before handing
/// the new generation to the decoder.
static MUSIC_MAP_RUNTIME: LazyLock<Mutex<MusicMapRuntime>> =
    LazyLock::new(|| Mutex::new(MusicMapRuntime::default()));

fn force_music_map_runtime() {
    LazyLock::force(&MUSIC_MAP_RUNTIME);
}

fn install_played_map(played: PlayedMapReader) {
    let mut runtime = MUSIC_MAP_RUNTIME.lock().unwrap();
    runtime.played = Some(played);
    runtime.map.clear();
}

/// Clear the derived map and drain the callback-to-application transport at a
/// music timeline boundary. This intentionally matches the behavior before
/// v0.5.654 rather than deferring cleanup until the next clock snapshot.
#[inline(always)]
pub(crate) fn clear_music_pos_map() -> u64 {
    let mut runtime = MUSIC_MAP_RUNTIME.lock().unwrap();
    let generation = bump_music_map_generation();
    runtime.map.clear();
    while runtime
        .played
        .as_mut()
        .and_then(PlayedMapReader::pop)
        .is_some()
    {}
    generation
}

fn drain_played_map(runtime: &mut MusicMapRuntime) {
    let generation = music_map_generation();
    while let Some((segment_generation, segment)) =
        runtime.played.as_mut().and_then(PlayedMapReader::pop)
    {
        if segment_generation == generation {
            runtime.map.insert(segment);
        }
    }
}

fn lookup_music_position(stream_frames: f64, sample_rate: u32) -> Option<(f32, f32)> {
    let mut runtime = MUSIC_MAP_RUNTIME.lock().unwrap();
    drain_played_map(&mut runtime);
    runtime
        .map
        .search(stream_frames)
        .map(|(music_seconds, seconds_per_frame)| {
            (
                music_seconds as f32,
                (seconds_per_frame * f64::from(sample_rate)) as f32,
            )
        })
}

fn assist_tick_stream_frame_for_music_seconds(music_seconds: f64) -> Option<u64> {
    if !music_seconds.is_finite() {
        return None;
    }
    let track_frame = {
        let mut runtime = MUSIC_MAP_RUNTIME.lock().unwrap();
        drain_played_map(&mut runtime);
        runtime.map.invert(music_seconds)?
    };
    if !track_frame.is_finite() || track_frame < 0.0 {
        return None;
    }
    Some(music_track_start_frame().saturating_add(track_frame.round() as u64))
}

/// Compatibility handle retained so the later application-owned audio-control
/// refactor does not need to be unwound. The played-position reader and map are
/// global again; this handle stores only the immutable output sample rate.
pub struct MusicClock {
    sample_rate: u32,
}

impl MusicClock {
    pub(crate) fn new(played: PlayedMapReader, sample_rate: u32) -> Self {
        install_played_map(played);
        Self {
            sample_rate: sample_rate.max(1),
        }
    }

    /// Construct the inert handle used when startup continues without audio.
    pub fn without_audio() -> Self {
        force_music_map_runtime();
        Self { sample_rate: 1 }
    }

    pub(crate) const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub(crate) fn lookup(&mut self, stream_frames: f64) -> Option<(f32, f32)> {
        lookup_music_position(stream_frames, self.sample_rate)
    }

    pub fn assist_tick_stream_frame(&mut self, music_seconds: f64) -> Option<u64> {
        assist_tick_stream_frame_for_music_seconds(music_seconds)
    }
}
