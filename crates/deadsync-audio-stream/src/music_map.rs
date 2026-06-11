use deadsync_audio::ring::{self, SpscRingMusicSeg};
use deadsync_audio::{
    AudioRenderMaps, PlaybackPosMap, bump_music_map_generation, music_track_start_frame,
};
use std::sync::{Arc, LazyLock, Mutex};

static QUEUED_MUSIC_MAP_SEGS: LazyLock<Arc<SpscRingMusicSeg>> =
    LazyLock::new(|| ring::music_seg_ring_new(ring::MUSIC_SEG_RING_CAP));
static PLAYED_MUSIC_MAP_SEGS: LazyLock<Arc<SpscRingMusicSeg>> =
    LazyLock::new(|| ring::music_seg_ring_new(ring::MUSIC_SEG_RING_CAP));
static PLAYBACK_POS_MAP: LazyLock<Mutex<PlaybackPosMap>> =
    LazyLock::new(|| Mutex::new(PlaybackPosMap::default()));

pub fn force_music_map_runtime() {
    LazyLock::force(&QUEUED_MUSIC_MAP_SEGS);
    LazyLock::force(&PLAYED_MUSIC_MAP_SEGS);
    LazyLock::force(&PLAYBACK_POS_MAP);
}

#[inline(always)]
pub fn queued_music_map() -> Arc<SpscRingMusicSeg> {
    QUEUED_MUSIC_MAP_SEGS.clone()
}

#[inline(always)]
pub fn music_render_maps() -> AudioRenderMaps {
    AudioRenderMaps::new(QUEUED_MUSIC_MAP_SEGS.clone(), PLAYED_MUSIC_MAP_SEGS.clone())
}

#[inline(always)]
pub fn clear_music_pos_map() {
    ring::music_seg_ring_clear(&QUEUED_MUSIC_MAP_SEGS);
    ring::music_seg_ring_clear(&PLAYED_MUSIC_MAP_SEGS);
    PLAYBACK_POS_MAP.lock().unwrap().clear();
    bump_music_map_generation();
}

fn drain_played_map(map: &mut PlaybackPosMap) {
    while let Some(seg) = ring::music_seg_ring_pop(&PLAYED_MUSIC_MAP_SEGS) {
        map.insert(seg);
    }
}

pub fn lookup_music_position(stream_frames: f64, sample_rate: u32) -> Option<(f32, f32)> {
    let mut map = PLAYBACK_POS_MAP.lock().unwrap();
    drain_played_map(&mut map);
    map.search(stream_frames).map(|(music_sec, sec_per_frame)| {
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
        let mut map = PLAYBACK_POS_MAP.lock().unwrap();
        drain_played_map(&mut map);
        map.invert(music_seconds)?
    };
    if !track_frame.is_finite() || track_frame < 0.0 {
        return None;
    }
    Some(music_track_start_frame().saturating_add(track_frame.round() as u64))
}
