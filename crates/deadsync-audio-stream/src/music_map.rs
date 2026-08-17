use deadlib_audio_core::{
    PlaybackPosMap, PlayedMapReader, music_map_generation, music_track_start_frame,
};

/// Application-thread reader for the audio callback's played-position stream.
///
/// The audio callback publishes bounded timing segments through the lock-free
/// transport. The application owns the sole reader and its derived map for the
/// process lifetime, so snapshots and assist-tick planning require neither a
/// global lookup nor synchronization.
pub struct MusicClock {
    played: Option<PlayedMapReader>,
    map: PlaybackPosMap,
    generation: u64,
    sample_rate: u32,
}

impl MusicClock {
    pub(crate) fn new(played: PlayedMapReader, sample_rate: u32) -> Self {
        Self {
            played: Some(played),
            map: PlaybackPosMap::default(),
            generation: music_map_generation(),
            sample_rate: sample_rate.max(1),
        }
    }

    /// Construct the inert reader used when startup continues without audio.
    pub fn without_audio() -> Self {
        Self {
            played: None,
            map: PlaybackPosMap::default(),
            generation: music_map_generation(),
            sample_rate: 1,
        }
    }

    pub(crate) const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn apply_generation(&mut self, generation: u64) {
        if self.generation != generation {
            self.generation = generation;
            self.map.clear();
        }
    }

    fn drain(&mut self) {
        let generation = music_map_generation();
        self.apply_generation(generation);
        while let Some((segment_generation, segment)) =
            self.played.as_mut().and_then(PlayedMapReader::pop)
        {
            if segment_generation == generation {
                self.map.insert(segment);
            }
        }
    }

    pub(crate) fn lookup(&mut self, stream_frames: f64) -> Option<(f32, f32)> {
        self.drain();
        self.map
            .search(stream_frames)
            .map(|(music_sec, sec_per_frame)| {
                (
                    music_sec as f32,
                    (sec_per_frame * self.sample_rate as f64) as f32,
                )
            })
    }

    pub fn assist_tick_stream_frame(&mut self, music_seconds: f64) -> Option<u64> {
        if !music_seconds.is_finite() {
            return None;
        }
        self.drain();
        let track_frame = self.map.invert(music_seconds)?;
        if !track_frame.is_finite() || track_frame < 0.0 {
            return None;
        }
        Some(music_track_start_frame().saturating_add(track_frame.round() as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::MusicClock;
    use deadlib_audio_core::MusicMapSeg;

    #[test]
    fn generation_change_discards_previous_track_map() {
        let mut clock = MusicClock::without_audio();
        clock.map.insert(MusicMapSeg {
            stream_frame_start: 100,
            frames: 1_000,
            music_start_sec: 2.0,
            music_sec_per_frame: 1.0 / 48_000.0,
        });
        assert!(clock.map.search(500.0).is_some());

        clock.apply_generation(clock.generation.wrapping_add(1));

        assert!(clock.map.search(500.0).is_none());
    }

    #[test]
    fn unchanged_generation_retains_current_track_map() {
        let mut clock = MusicClock::without_audio();
        clock.map.insert(MusicMapSeg {
            stream_frame_start: 100,
            frames: 1_000,
            music_start_sec: 2.0,
            music_sec_per_frame: 1.0 / 48_000.0,
        });

        clock.apply_generation(clock.generation);

        assert!(clock.map.search(500.0).is_some());
    }

    #[test]
    fn inert_clock_rejects_unmapped_assist_ticks() {
        let mut clock = MusicClock::without_audio();

        assert_eq!(clock.assist_tick_stream_frame(1.0), None);
        assert_eq!(clock.assist_tick_stream_frame(f64::NAN), None);
    }
}
