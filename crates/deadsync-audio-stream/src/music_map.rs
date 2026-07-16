use deadsync_audio::{
    PlaybackPosMap, PlayedMapReader, bump_music_map_generation, music_map_generation,
    music_track_start_frame,
};
use std::sync::{LazyLock, Mutex};

#[derive(Default)]
struct MusicMapRuntime {
    played: Option<PlayedMapReader>,
    map: PlaybackPosMap,
}

static MUSIC_MAP_RUNTIME: LazyLock<Mutex<MusicMapRuntime>> =
    LazyLock::new(|| Mutex::new(MusicMapRuntime::default()));

pub fn force_music_map_runtime() {
    LazyLock::force(&MUSIC_MAP_RUNTIME);
}

pub fn install_played_map(played: PlayedMapReader) {
    let mut runtime = MUSIC_MAP_RUNTIME.lock().unwrap();
    runtime.played = Some(played);
    runtime.map.clear();
}

#[inline(always)]
pub fn clear_music_pos_map() -> u64 {
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
    while let Some((seg_generation, seg)) = runtime.played.as_mut().and_then(PlayedMapReader::pop) {
        if seg_generation == generation {
            runtime.map.insert(seg);
        }
    }
}

pub fn lookup_music_position(stream_frames: f64, sample_rate: u32) -> Option<(f32, f32)> {
    let mut runtime = MUSIC_MAP_RUNTIME.lock().unwrap();
    drain_played_map(&mut runtime);
    runtime
        .map
        .search(stream_frames)
        .map(|(music_sec, sec_per_frame)| {
            (
                music_sec as f32,
                (sec_per_frame * sample_rate as f64) as f32,
            )
        })
}

pub fn assist_tick_stream_frame_for_music_seconds(music_seconds: f64) -> Option<u64> {
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
