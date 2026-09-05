use deadlib_audio_core::{
    CallbackClockSource, CallbackInfo, MixControls, MusicMapSeg, OutputBufferMut, RenderState,
    activate_music_track, music_map_generation, music_transport, reset_music_stream_clock_state,
    reset_music_target_gain,
};
use deadsync_audio_stream::{Cut, MusicDecodeContext, OutputFormat, spawn_music_decoder_thread};
use std::path::PathBuf;
use std::sync::Arc;

struct Wav(PathBuf);

impl Wav {
    fn new(samples: &[i16], hz: u32) -> Self {
        let path =
            std::env::temp_dir().join(format!("deadsync-playback-{}.wav", std::process::id()));
        let size = (samples.len() * 2) as u32;
        let mut bytes = Vec::with_capacity(44 + size as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + size).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
        bytes.extend_from_slice(&hz.to_le_bytes());
        bytes.extend_from_slice(&(hz * 2).to_le_bytes());
        bytes.extend_from_slice(b"\x02\0\x10\0data");
        bytes.extend_from_slice(&size.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
}

impl Drop for Wav {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}

fn decode(wav: &Wav, cut: Cut, rate: f32, hz: u32) -> (Vec<i16>, Vec<MusicMapSeg>) {
    reset_music_stream_clock_state();
    reset_music_target_gain();
    activate_music_track();
    let generation = music_map_generation();
    let (mut stream, render) = music_transport(hz, 2);
    // Fixtures produce less than the decoder's high watermark, so the worker
    // completes before rendering. No wall-clock sleeps or audio device needed.
    let worker = spawn_music_decoder_thread(
        wav.0.clone(),
        cut,
        false,
        rate,
        true,
        stream.writer,
        MusicDecodeContext {
            output: OutputFormat {
                sample_rate_hz: hz,
                channels: 2,
            },
            generation,
        },
    );
    let _writer = worker.thread.join().unwrap();
    let mut renderer = RenderState::new(render, Arc::new(MixControls::new()), 2);
    let mut output = vec![0; hz as usize];
    renderer.render(
        OutputBufferMut::I16(&mut output),
        CallbackInfo {
            anchor_nanos: 1_000_000_000,
            clock: CallbackClockSource::Instant,
        },
        std::iter::empty(),
    );
    let mut timing = Vec::new();
    while let Some((tag, segment)) = stream.played_map.pop() {
        assert_eq!(tag, generation);
        timing.push(segment);
    }
    let frames: i64 = timing.iter().map(|s| s.frames).sum();
    output.truncate(frames as usize * 2);
    (output, timing)
}

#[test]
fn stretched_worker_preserves_cuts_fades_channels_and_timing() {
    // One test owns the process-global callback clock and music gain.
    let wav = Wav::new(&vec![12_000; 12_000], 48_000);
    for rate in [0.8, 1.2] {
        for hz in [48_000, 44_100] {
            let cut = Cut {
                length_sec: 0.1,
                fade_out_sec: 0.02,
                ..Cut::default()
            };
            let (output, timing) = decode(&wav, cut, rate, hz);
            assert_eq!(output.len(), (hz / 10 * 2) as usize);
            assert!(output.as_chunks::<2>().0.iter().all(|p| p[0] == p[1]));
            assert!((output[2_000] - 12_000).abs() <= 1);
            assert!(output.last().unwrap().abs() < 30);
            assert_eq!(timing[0].music_start_sec, 0.0);
            for pair in timing.windows(2) {
                let end =
                    pair[0].music_start_sec + pair[0].frames as f64 * pair[0].music_sec_per_frame;
                assert!((end - pair[1].music_start_sec).abs() < 1e-10);
            }
            assert!(
                timing.iter().all(
                    |s| (s.music_sec_per_frame - f64::from(rate) / f64::from(hz)).abs() < 1e-12
                )
            );
        }
        let (output, _) = decode(
            &wav,
            Cut {
                length_sec: 0.1,
                fade_in_sec: 0.02,
                ..Cut::default()
            },
            rate,
            48_000,
        );
        assert_eq!(output[0], 0);
        assert!((output[960] - 6_000).abs() < 10);
        assert_eq!(output[2_560], 12_000);
    }
    drop(wav);

    // A seeked impulse remains at the cut onset at either stretch rate.
    let mut source = vec![0; 8_000];
    source[1_008] = 20_000;
    let wav = Wav::new(&source, 48_000);
    for rate in [0.8, 1.2] {
        let (output, timing) = decode(
            &wav,
            Cut {
                start_sec: 1_008.0 / 48_000.0,
                length_sec: 0.05,
                ..Cut::default()
            },
            rate,
            48_000,
        );
        assert!((output[0] - 20_000).abs() <= 1);
        assert!((timing[0].music_start_sec - 1_008.0 / 48_000.0).abs() < 2.0 / 48_000.0);
    }
    drop(wav);

    // An EOF inside SOLA's first window needs its own drain, with no sinc tail.
    let wav = Wav::new(&vec![12_000; 731], 48_000);
    let (output, timing) = decode(&wav, Cut::default(), 1.2, 48_000);
    assert_eq!(output, vec![12_000; 731 * 2]);
    assert_eq!(timing.iter().map(|s| s.frames).sum::<i64>(), 731);

    let (output, timing) = decode(
        &wav,
        Cut {
            start_sec: -0.03,
            ..Cut::default()
        },
        1.2,
        48_000,
    );
    assert_eq!(&output[..1_200 * 2], &vec![0; 1_200 * 2]);
    assert_eq!(&output[1_200 * 2..], &vec![12_000; 731 * 2]);
    assert_eq!(timing[0].music_start_sec, -0.03);
    drop(wav);

    live_and_looped_playback();
}

fn live_and_looped_playback() {
    use deadlib_audio_core::{
        bump_music_map_generation, music_total_frames, music_track_start_frame,
    };
    use deadsync_audio_stream::{MusicStreamRuntime, StreamCommand};
    use std::time::{Duration, Instant};

    // EOF is inside SOLA's first window; each iteration must retain its tail.
    let wav = Wav::new(&vec![12_000; 731], 48_000);
    reset_music_stream_clock_state();
    let (mut stream, render) = music_transport(48_000, 2);
    let mut runtime = MusicStreamRuntime::new(
        stream.writer,
        OutputFormat {
            sample_rate_hz: 48_000,
            channels: 2,
        },
    );
    let mut generation = music_map_generation();
    runtime.handle(StreamCommand::PlayMusic {
        path: wav.0.clone(),
        cut: Cut::default(),
        looping: true,
        rate: 1.2,
        preserve_pitch: true,
        generation,
    });
    let mut renderer = RenderState::new(render, Arc::new(MixControls::new()), 2);
    let mut output = [0; 256 * 2];
    for (phase, (rate, preserve_pitch)) in [
        (1.2, true),
        (0.8, true),
        (1.0, true),
        (1.2, false),
        (1.2, true),
    ]
    .into_iter()
    .enumerate()
    {
        if phase > 0 {
            generation = bump_music_map_generation();
            runtime.handle(StreamCommand::SetMusicRate { rate, generation });
            runtime.handle(StreamCommand::SetPreservePitch {
                enabled: preserve_pitch,
                generation,
            });
        }
        let mut captured = Vec::new();
        let mut timing = Vec::new();
        // The clock is only a deadlock watchdog; callbacks use injected time.
        let deadline = Instant::now() + Duration::from_secs(5);
        while captured.len() < 731 * 2 * 4 {
            assert!(
                Instant::now() < deadline,
                "decoder stalled in phase {phase}"
            );
            let start = music_total_frames();
            renderer.render(
                OutputBufferMut::I16(&mut output),
                CallbackInfo {
                    anchor_nanos: 1_000_000_000 + start * 1_000_000_000 / 48_000,
                    clock: CallbackClockSource::Instant,
                },
                std::iter::empty(),
            );
            while let Some((tag, segment)) = stream.played_map.pop() {
                if tag != generation {
                    assert!(
                        timing.is_empty(),
                        "old generation resumed after the transition"
                    );
                    continue;
                }
                let callback_frame = start.saturating_sub(music_track_start_frame());
                let first = (segment.stream_frame_start as u64 - callback_frame) as usize * 2;
                let end = first + segment.frames as usize * 2;
                captured.extend_from_slice(&output[first..end]);
                timing.push(segment);
            }
            std::thread::yield_now();
        }
        if preserve_pitch || rate == 1.0 {
            assert!(captured.iter().all(|&sample| sample == 12_000));
            assert!(
                timing
                    .iter()
                    .all(|s| (s.music_sec_per_frame - f64::from(rate) / 48_000.0).abs() < 1e-12)
            );
            let starts: Vec<_> = timing
                .iter()
                .filter(|s| s.music_start_sec == 0.0)
                .map(|s| s.stream_frame_start)
                .collect();
            assert!(starts.len() >= 2);
            // Callback gaps can occur in this deliberately unpaced harness;
            // count only frames the decoder actually emitted between loop starts.
            let mut iteration_frames = None;
            for segment in &timing {
                if segment.music_start_sec == 0.0 {
                    if let Some(frames) = iteration_frames {
                        assert_eq!(frames, 731);
                    }
                    iteration_frames = Some(0);
                }
                if let Some(frames) = &mut iteration_frames {
                    *frames += segment.frames;
                }
            }
        } else {
            assert!(captured.iter().any(|&sample| sample > 11_000));
            assert!(
                timing
                    .iter()
                    .any(|s| s.music_sec_per_frame > 1.1 / 48_000.0)
            );
        }
    }
    drop(runtime);
}
