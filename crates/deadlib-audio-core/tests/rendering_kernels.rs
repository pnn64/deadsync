//! Complete callback benchmarks and deterministic wire-format render traces.
use deadlib_audio_core::*;
use std::{hint::black_box, sync::Arc};
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod allocations;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

fn push(stream: &mut AudioStreamHandle, input: &[i16], generation: u64) -> usize {
    let mut accepted = 0;
    while accepted < input.len() {
        let n = stream.writer.try_push(
            &input[accepted..],
            MusicBlockTiming {
                generation,
                music_start_sec: accepted as f64 / stream.writer.channels() as f64 / 48000.0,
                music_sec_per_frame: 1.0 / 48000.0,
            },
        );
        if n == 0 {
            break;
        }
        accepted += n;
    }
    accepted
}

fn callback(anchor: u64) -> CallbackInfo {
    CallbackInfo {
        anchor_nanos: anchor,
        clock: CallbackClockSource::Instant,
    }
}

fn effect(
    data: &Arc<[i16]>,
    voice: Option<Arc<SfxVoiceControl>>,
    pan: SfxPan,
    target: u64,
) -> QueuedSfx {
    QueuedSfx {
        data: Arc::clone(data),
        bus: MixBus::new(0),
        generation: 0,
        restart_slot: Some(0),
        pan,
        target_stream_frame: target,
        voice,
    }
}

#[test]
fn unity_render_round_trips_every_pcm_value() {
    for (volume, target) in [(1.0, 1.0), (2.0, 0.5), (0.5, 2.0), (3.0, 1.0 / 3.0)] {
        set_music_target_gain(target);
        snap_music_gain_generation();
        reset_music_stream_clock_state();
        activate_music_track();
        let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
        let (mut stream, handle) = music_transport(48000, 2);
        assert_eq!(
            push(&mut stream, &input, music_map_generation()),
            input.len()
        );
        let controls = Arc::new(MixControls::new());
        controls.set_stream_gain(volume);
        let mut renderer = RenderState::new(handle, controls, 2);
        let mut output = vec![0; input.len()];
        let report = renderer.render(
            OutputBufferMut::I16(&mut output),
            callback(1_000_000),
            std::iter::empty(),
        );
        assert!(!report.output_underrun);
        assert_eq!(output, input);
        stop_music_track();
    }
    reset_music_target_gain();
}

#[test]
#[ignore = "release CPU-cycle and allocation benchmark"]
fn benchmark_rendering_callbacks() {
    for base_samples in [512, 4096] {
        for kind in [
            "silence",
            "music-unit",
            "music-gain",
            "sfx",
            "music-muted",
            "music-muted-ramp",
            "music-muted-negative-zero",
            "music-muted-surround",
        ] {
            for floating in [false, true] {
                let channels = if kind == "music-muted-surround" { 6 } else { 2 };
                let samples = base_samples / 2 * channels;
                reset_music_target_gain();
                snap_music_gain_generation();
                reset_music_stream_clock_state();
                activate_music_track();
                let controls = Arc::new(MixControls::new());
                if kind == "music-gain" {
                    controls.set_stream_gain(0.8);
                }
                if kind.starts_with("music-muted") {
                    controls.set_stream_gain(if kind == "music-muted-negative-zero" {
                        -0.0
                    } else {
                        0.0
                    });
                }
                let (mut stream, handle) = music_transport(48000, channels);
                let mut renderer = RenderState::new(handle, Arc::clone(&controls), channels);
                let data: Vec<_> = (0..samples)
                    .map(|i: usize| i.wrapping_mul(7919) as i16)
                    .collect();
                let sound: Arc<[i16]> = data.clone().into();
                let mut out_i16 = vec![0; samples];
                let mut out_f32 = vec![0.0; samples];
                let generation = music_map_generation();
                let mut anchor = 1_000_000u64;
                let mut ramp_up = false;
                let label = format!(
                    "rendering/{}/{kind}/samples={samples}",
                    if floating { "f32" } else { "i16" }
                );
                let mut work = || {
                    if kind.starts_with("music-muted") && kind != "music-muted" {
                        ramp_up = !ramp_up;
                        set_music_target_gain(if ramp_up { 0.5 } else { 1.0 });
                    }
                    if kind != "silence" {
                        assert_eq!(push(&mut stream, black_box(&data), generation), samples);
                    }
                    let queued = (kind == "sfx").then(|| effect(&sound, None, SfxPan::Center, 0));
                    let out = if floating {
                        OutputBufferMut::F32(black_box(&mut out_f32))
                    } else {
                        OutputBufferMut::I16(black_box(&mut out_i16))
                    };
                    black_box(renderer.render(out, callback(anchor), queued));
                    anchor += 1_000_000;
                    while let Some(map) = stream.played_map.pop() {
                        black_box(map);
                    }
                    black_box((&out_i16, &out_f32));
                };
                let (_, counts) = allocations::measure(&mut work);
                assert_eq!(counts, allocations::Churn::default());
                println!("{label}: allocation {counts:?}");
                bench::measure(&label, 2_000_000 / samples, &mut work);
                stop_music_track();
            }
        }
    }
}

fn word(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[test]
#[ignore = "deterministic cross-build PCM and timing trace"]
fn dump_rendering_trace() {
    let mut trace = Vec::new();
    let mut anchor = 1_000_000u64;
    for channels in [0usize, 1, 2, 3, 6, 8] {
        let ch = channels.max(1);
        reset_music_target_gain();
        snap_music_gain_generation();
        reset_music_stream_clock_state();
        activate_music_track();
        let controls = Arc::new(MixControls::new());
        let (mut stream, handle) = music_transport(48000, channels);
        let mut renderer = RenderState::new(handle, Arc::clone(&controls), channels);
        let source: Arc<[i16]> = (0..8192 * ch)
            .map(|i| [i16::MIN, i16::MAX, -12345, 12345, -1, 0, 1][i % 7])
            .collect();
        let mut voice = Arc::new(SfxVoiceControl::new());
        for step in 0..56 {
            let input_len = [
                0,
                1,
                ch - 1,
                ch,
                17 * ch,
                256 * ch,
                2048 * ch,
                4096 * ch,
                509 * ch,
            ][step % 9];
            let output_len = [
                0,
                1,
                ch - 1,
                ch,
                5 * ch + 1,
                256 * ch,
                2048 * ch + ch - 1,
                4096 * ch + 1,
            ][step % 8];
            if step % 13 == 0 {
                bump_music_map_generation();
            }
            let generation = music_map_generation();
            word(&mut trace, channels as u64);
            word(&mut trace, step as u64);
            word(
                &mut trace,
                push(&mut stream, &source[..input_len], generation) as u64,
            );
            if step % 2 == 0 {
                stream.writer.publish_generation_ready(generation);
            }
            controls.set_stream_gain(if (28..36).contains(&step) {
                -0.0
            } else if step % 11 == 0 {
                0.8
            } else {
                1.0
            });
            controls.set_bus_gain(
                MixBus::new(0),
                [1.0, 0.1, 0.8, 0.375, 1.1, f32::MIN_POSITIVE][step % 6],
            );
            if step == 16 || step == 29 {
                set_music_target_gain(0.5);
            }
            if step == 20 || step == 33 {
                set_music_target_gain(1.0);
            }
            if step == 21 {
                snap_music_gain_generation();
            }
            if step == 23 {
                stop_music_track();
            } else if step == 24 {
                activate_music_track();
            }
            if step == 25 {
                voice.pause(true);
            } else if step == 27 {
                voice.pause(false);
            }
            let queued = if [7, 22, 31, 38].contains(&step) {
                voice = Arc::new(SfxVoiceControl::new());
                Some(effect(
                    &source,
                    Some(Arc::clone(&voice)),
                    if step % 2 == 0 {
                        SfxPan::Left
                    } else {
                        SfxPan::Right
                    },
                    if step == 31 {
                        music_total_frames() + 100
                    } else {
                        0
                    },
                ))
            } else {
                None
            };
            let report = if step % 3 == 0 {
                let mut output = vec![123.0f32; output_len];
                let report =
                    renderer.render(OutputBufferMut::F32(&mut output), callback(anchor), queued);
                word(&mut trace, 1);
                word(&mut trace, output_len as u64);
                for value in output {
                    trace.extend_from_slice(&value.to_bits().to_le_bytes());
                }
                report
            } else {
                let mut output = vec![1234i16; output_len];
                let report =
                    renderer.render(OutputBufferMut::I16(&mut output), callback(anchor), queued);
                word(&mut trace, 0);
                word(&mut trace, output_len as u64);
                for value in output {
                    trace.extend_from_slice(&value.to_le_bytes());
                }
                report
            };
            anchor += 1_000_000;
            word(&mut trace, u64::from(report.output_underrun));
            word(&mut trace, report.callback_gap_ns);
            word(&mut trace, music_total_frames());
            word(&mut trace, u64::from(voice.is_playing()));
            word(&mut trace, stream.writer.outstanding_blocks() as u64);
            word(&mut trace, stream.writer.render_drained_generation());
            while let Some((tag, seg)) = stream.played_map.pop() {
                word(&mut trace, 1);
                word(&mut trace, tag);
                word(&mut trace, seg.stream_frame_start as u64);
                word(&mut trace, seg.frames as u64);
                word(&mut trace, seg.music_start_sec.to_bits());
                word(&mut trace, seg.music_sec_per_frame.to_bits());
            }
            word(&mut trace, 0);
        }
        stop_music_track();
    }
    std::fs::write(
        std::env::var_os("AUDIO_CORE_TRACE_OUT").expect("trace path required"),
        &trace,
    )
    .unwrap();
    println!("render trace: {} bytes", trace.len());
}
