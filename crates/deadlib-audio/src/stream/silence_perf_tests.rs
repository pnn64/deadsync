//! Lead-in submission through the actual worker and music-block transport.
use super::super::stretch::perf_tests::bench;
use super::super::*;
use super::allocations;
use deadlib_audio_core::{
    AudioRenderHandle, AudioStreamHandle, CallbackClockSource, CallbackInfo, MixControls,
    MusicMapSeg, OutputBufferMut, RenderState, activate_music_track, music_map_generation,
    music_transport, reset_music_stream_clock_state, reset_music_target_gain,
};
use std::hint::black_box;
use std::time::Instant;

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

#[test]
#[ignore = "write PCM and transport timing for archived binary comparison"]
fn dump_lead_in() {
    reset_music_stream_clock_state();
    reset_music_target_gain();
    activate_music_track();
    let mut bytes = Vec::new();
    for channels in [1, 2, 3, 6, 8] {
        for frames in [0, 1, 255, 256, 257, 731, 2048, 2049, 8192] {
            for rate in [0.05, 1.0, 1.2, 8.0] {
                let mut fixture = Fixture::new(channels);
                let end = fixture.push(frames, rate / 48_000.0);
                bytes.extend_from_slice(&end.to_bits().to_le_bytes());
                let (samples, segments) = fixture.capture(frames);
                for sample in samples {
                    bytes.extend_from_slice(&sample.to_le_bytes());
                }
                bytes.extend_from_slice(&(segments.len() as u64).to_le_bytes());
                for (_, segment) in segments {
                    bytes.extend_from_slice(&segment.stream_frame_start.to_le_bytes());
                    bytes.extend_from_slice(&segment.frames.to_le_bytes());
                    bytes.extend_from_slice(&segment.music_start_sec.to_bits().to_le_bytes());
                    bytes.extend_from_slice(&segment.music_sec_per_frame.to_bits().to_le_bytes());
                }
            }
        }
    }
    std::fs::write(
        std::env::var_os("LEAD_IN_CAPTURE").expect("LEAD_IN_CAPTURE path"),
        bytes,
    )
    .unwrap();
}

#[test]
#[ignore = "CPU-cycle benchmark; one test thread"]
fn benchmark_lead_in() {
    bench::pin_thread();
    for channels in [1, 2, 6, 8] {
        for frames in [1, 731, 2048, 8192] {
            let label = format!("lead-in/{channels}ch/{frames}");
            let mut fixture = Fixture::new(channels);
            let (_, churn) =
                allocations::measure(|| black_box(fixture.push(frames, 1.2 / 48_000.0)));
            println!("{label}: allocation {churn:?}");
            let mut samples = Vec::new();
            for round in 0..12 {
                // Pool allocation/destruction stays outside the timed region.
                let mut fixtures: Vec<_> = (0..64).map(|_| Fixture::new(channels)).collect();
                let start_cycles = bench::cycles();
                let start = Instant::now();
                for fixture in &mut fixtures {
                    black_box(fixture.push(black_box(frames), black_box(1.2 / 48_000.0)));
                }
                let ns = start.elapsed().as_nanos() as f64 / 64.0;
                let cpu = (bench::cycles() - start_cycles) as f64 / 64.0;
                if round > 0 {
                    samples.push((cpu, ns));
                }
            }
            samples.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            let (cpu, ns) = samples[5];
            println!("{label}: cycles/op {cpu:.2}; ns/op {ns:.2}");
        }
    }
}
