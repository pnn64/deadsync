use crate::{MusicDecodeContext, MusicStream, OutputFormat, clear_music_pos_map, queued_music_map};
use deadsync_audio::ring::{self, SpscRingI16};
use deadsync_audio::{Cut, activate_music_track, stop_music_track};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub enum StreamCommand {
    PlayMusic {
        path: PathBuf,
        cut: Cut,
        looping: bool,
        rate: f32,
        preserve_pitch: bool,
    },
    StopMusic,
    SetMusicRate(f32),
    SetPreservePitch(bool),
}

pub struct MusicStreamRuntime {
    music_stream: Option<MusicStream>,
    music_ring: Arc<SpscRingI16>,
    output: OutputFormat,
}

impl MusicStreamRuntime {
    pub fn new(music_ring: Arc<SpscRingI16>, output: OutputFormat) -> Self {
        Self {
            music_stream: None,
            music_ring,
            output,
        }
    }

    pub fn handle(&mut self, command: StreamCommand) {
        match command {
            StreamCommand::PlayMusic {
                path,
                cut,
                looping,
                rate,
                preserve_pitch,
            } => self.play(path, cut, looping, rate, preserve_pitch),
            StreamCommand::StopMusic => self.stop(),
            StreamCommand::SetMusicRate(rate) => self.set_rate(rate),
            StreamCommand::SetPreservePitch(enabled) => self.set_preserve_pitch(enabled),
        }
    }

    fn play(&mut self, path: PathBuf, cut: Cut, looping: bool, rate: f32, preserve_pitch: bool) {
        self.stop_decoder();
        ring::ring_clear(&self.music_ring);
        activate_music_track();

        let rate_bits = Arc::new(AtomicU32::new(rate.to_bits()));
        let preserve_pitch_bits = Arc::new(AtomicBool::new(preserve_pitch));
        self.music_stream = Some(crate::spawn_music_decoder_thread(
            path,
            cut,
            looping,
            rate_bits,
            preserve_pitch_bits,
            self.music_ring.clone(),
            MusicDecodeContext {
                output: self.output,
                queued_music_map: queued_music_map(),
            },
        ));
    }

    fn stop(&mut self) {
        self.stop_decoder();
        ring::ring_clear(&self.music_ring);
        stop_music_track();
    }

    fn set_rate(&mut self, rate: f32) {
        if let Some(stream) = &self.music_stream {
            stream.rate_bits.store(rate.to_bits(), Ordering::Release);
        }
        // Drop buffered old-rate samples so the change is heard immediately.
        ring::ring_clear(&self.music_ring);
        clear_music_pos_map();
    }

    fn set_preserve_pitch(&mut self, enabled: bool) {
        if let Some(stream) = &self.music_stream {
            stream.preserve_pitch.store(enabled, Ordering::Release);
            // Drop buffered samples produced with the old mode so the change is
            // heard immediately.
            ring::ring_clear(&self.music_ring);
            clear_music_pos_map();
        }
    }

    fn stop_decoder(&mut self) {
        if let Some(old) = self.music_stream.take() {
            old.stop_signal.store(true, Ordering::Relaxed);
            let _ = old.thread.join();
        }
    }
}

impl Drop for MusicStreamRuntime {
    fn drop(&mut self) {
        self.stop_decoder();
    }
}

pub fn new_music_sample_ring() -> Arc<SpscRingI16> {
    ring::ring_new(ring::RING_CAP_SAMPLES)
}
