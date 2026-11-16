//! Lead-in submission through the actual worker and music-block transport.
use super::super::*;
use deadlib_audio_core::{
    AudioRenderHandle, AudioStreamHandle, CallbackClockSource, CallbackInfo, MixControls,
    MusicMapSeg, OutputBufferMut, RenderState, activate_music_track, music_map_generation,
    music_transport, reset_music_stream_clock_state, reset_music_target_gain,
};

struct Fixture {
    stream: AudioStreamHandle,
    render: AudioRenderHandle,
    control: MusicControl,
    backpressure: MusicBackpressure,
}

impl Fixture {
    fn new(channels: usize) -> Self {
        let generation = music_map_generation();
        let (stream, render) = music_transport(48_000, channels);
        let backpressure =
            MusicBackpressure::new(48_000, stream.writer.capacity_blocks(), generation);
        Self {
            stream,
            render,
            backpressure,
            control: MusicControl {
                stop_signal: AtomicBool::new(false),
                rate_bits: AtomicU32::new(1.0f32.to_bits()),
                preserve_pitch: AtomicBool::new(false),
                generation: AtomicU64::new(generation),
                wake: DecoderWake::new(),
            },
        }
    }

    fn push(&mut self, frames: usize, step: f64) -> f64 {
        let channels = self.stream.writer.channels();
        push_silence(
            &mut self.stream.writer,
            frames,
            channels,
            -0.125,
            step,
            &self.control,
            &mut self.backpressure,
        )
        .unwrap()
    }

    fn capture(mut self, frames: usize) -> (Vec<i16>, Vec<(u64, MusicMapSeg)>) {
        let generation = self.control.generation.load(Ordering::Acquire);
        self.backpressure
            .finish_generation(&self.stream.writer, generation);
        let channels = self.stream.writer.channels();
        let mut render = RenderState::new(self.render, Arc::new(MixControls::new()), channels);
        let mut output = vec![123; frames.max(1) * channels];
        render.render(
            OutputBufferMut::I16(&mut output),
            CallbackInfo {
                anchor_nanos: 1_000_000_000,
                clock: CallbackClockSource::Instant,
            },
            std::iter::empty(),
        );
        let mut segments = Vec::new();
        while let Some(segment) = self.stream.played_map.pop() {
            segments.push(segment);
        }
        (output, segments)
    }
}

#[test]
fn silence_preserves_samples_block_boundaries_and_exact_timing() {
    reset_music_stream_clock_state();
    reset_music_target_gain();
    activate_music_track();
    for channels in [1, 2, 3, 6, 8, 9] {
        for frames in [0, 1, 255, 256, 257, 731, 2048, 2049, 8192] {
            for rate in [0.05, 1.0, 1.2, 8.0] {
                let step = rate / 48_000.0;
                let mut fixture = Fixture::new(channels);
                let end = fixture.push(frames, step);
                assert_eq!(
                    fixture.stream.writer.outstanding_blocks(),
                    frames.div_ceil(256)
                );
                let (samples, segments) = fixture.capture(frames);
                assert!(samples.iter().all(|&sample| sample == 0));
                let mut expected = -0.125f64;
                let mut emitted = 0;
                for (generation, segment) in segments {
                    let count = (frames - emitted).min(256);
                    assert_eq!(generation, music_map_generation());
                    assert_eq!(segment.frames, count as i64);
                    assert_eq!(segment.music_start_sec.to_bits(), expected.to_bits());
                    assert_eq!(segment.music_sec_per_frame.to_bits(), step.to_bits());
                    expected = (count as f64).mul_add(step, expected);
                    emitted += count;
                }
                assert_eq!(emitted, frames);
                assert_eq!(end.to_bits(), expected.to_bits());
            }
        }
    }
}
