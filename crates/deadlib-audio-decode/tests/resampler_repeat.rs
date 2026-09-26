use deadlib_audio_decode::resample::write_resampler_output;
use std::hint::black_box;

#[path = "resampler_repeat/baseline.rs"]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

type Convert = fn(&[Vec<f32>], usize, usize, &mut Vec<i16>) -> usize;

fn input(channels: usize, frames: usize) -> Vec<Vec<f32>> {
    (0..channels)
        .map(|channel| {
            (0..frames)
                .map(|frame| ((frame * 171 + channel * 997) % 65536) as f32 / 32768.0 - 1.0)
                .collect()
        })
        .collect()
}

#[test]
fn repeated_channels_preserve_every_pcm_sample_and_frame_count() {
    let mut before = Vec::new();
    let mut after = Vec::new();
    let mut cases = 0;
    for frames in [0usize, 1, 3, 4, 5, 17, 256, 1024] {
        for source_channels in [0, 1, 2, 3, 6, 8, 16] {
            for output_channels in [0, 1, 2, 3, 4, 6, 8, 16, 31] {
                for ragged in [false, true] {
                    let mut source = input(source_channels, frames);
                    for (channel, samples) in source.iter_mut().enumerate() {
                        for (frame, sample) in samples.iter_mut().enumerate() {
                            let bits =
                                ((frame * 65537 + channel * 8191) as u32).wrapping_mul(0x9e37_79b9);
                            *sample = match frame % 16 {
                                0 => f32::NAN,
                                1 => f32::INFINITY,
                                2 => f32::NEG_INFINITY,
                                3 => -0.0,
                                4 => 0.5 / 32767.0,
                                5 => -0.5 / 32767.0,
                                6 => f32::MIN_POSITIVE,
                                _ => f32::from_bits(bits),
                            };
                        }
                        if ragged {
                            samples.truncate(frames.saturating_sub(channel + 1));
                        }
                    }
                    for requested in [0, frames.saturating_sub(1), frames, frames + 3] {
                        // Old contents and storage survive successive length changes.
                        before.resize(37, 1234);
                        after.resize(37, 1234);
                        let old_frames = baseline::write_resampler_output(
                            &source,
                            requested,
                            output_channels,
                            &mut before,
                        );
                        let new_frames =
                            write_resampler_output(&source, requested, output_channels, &mut after);
                        assert_eq!(new_frames, old_frames);
                        assert_eq!(
                            after, before,
                            "{source_channels}->{output_channels}, frames {frames}, requested {requested}, ragged {ragged}"
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    eprintln!("{cases} complete PCM outputs matched exactly");
}

#[test]
fn repeated_channel_conversion_reuses_warmed_output_storage() {
    for convert in [
        baseline::write_resampler_output as Convert,
        write_resampler_output,
    ] {
        for (sources, outputs) in [
            (1, 2),
            (2, 2),
            (2, 6),
            (1, 8),
            (3, 8),
            (6, 8),
            (8, 2),
            (16, 16),
        ] {
            let source = input(sources, 1024);
            let mut output = Vec::new();
            convert(&source, 1024, outputs, &mut output);
            let storage = output.as_ptr();
            perf::assert_no_churn(|| {
                for frames in [1024, 17, 0, 1023, 1024] {
                    convert(&source, frames, outputs, &mut output);
                    assert_eq!(output.as_ptr(), storage);
                }
            });
        }
    }
}

#[test]
#[ignore = "paired release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_resampler_repeat() {
    for (name, sources, outputs, frames) in [
        ("mono", 1, 1, 1024),
        ("mono_stereo", 1, 2, 1024),
        ("stereo", 2, 2, 1024),
        ("six_stereo", 6, 2, 1024),
        ("six", 6, 6, 1024),
        ("six_eight", 6, 8, 1024),
        ("stereo_six64", 2, 6, 64),
        ("stereo_six", 2, 6, 1024),
        ("stereo_eight", 2, 8, 1024),
        ("mono_eight", 1, 8, 1024),
        ("three_eight", 3, 8, 1024),
        ("sixteen", 16, 16, 1024),
    ] {
        let source = input(sources, frames);
        let mut output = vec![0; frames * outputs];
        let mut variants = [
            ("before", baseline::write_resampler_output as Convert),
            ("after", write_resampler_output as Convert),
        ];
        if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            variants.reverse();
        }
        for (variant, convert) in variants {
            let convert = black_box(convert);
            perf::measure_sampled(
                &format!("resampler_repeat/{name}/{variant}"),
                8192,
                frames,
                || {
                    let produced = convert(
                        black_box(&source),
                        black_box(frames),
                        black_box(outputs),
                        black_box(&mut output),
                    );
                    black_box((produced, &output));
                },
            );
        }
    }
}

#[test]
#[ignore = "paired resampling plus PCM conversion benchmark"]
fn benchmark_resampler_repeat_pipeline() {
    use deadlib_audio_decode::resample::{OUT_FRAMES_PER_CALL, resampler_params};
    use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
    use rubato::{Async, FixedAsync, Resampler};

    for (sources, outputs) in [(2, 2), (2, 6), (2, 8), (6, 6), (6, 8)] {
        let mut resampler = Async::<f32>::new_sinc(
            48000.0 / 44100.0,
            1.0,
            &resampler_params(),
            OUT_FRAMES_PER_CALL,
            sources,
            FixedAsync::Output,
        )
        .unwrap();
        let source = input(sources, resampler.input_frames_max());
        let output_frames = resampler.output_frames_max();
        let mut planar = vec![vec![0.0; output_frames]; sources];
        let mut pcm = vec![0; output_frames * outputs];
        let mut variants = [
            ("before", baseline::write_resampler_output as Convert),
            ("after", write_resampler_output as Convert),
        ];
        if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            variants.reverse();
        }
        for (variant, convert) in variants {
            resampler.reset();
            let convert = black_box(convert);
            perf::measure_sampled(
                &format!("resampler_repeat/pipeline_{sources}_{outputs}/{variant}"),
                256,
                OUT_FRAMES_PER_CALL,
                || {
                    let input = SequentialSliceOfVecs::new(
                        black_box(&source),
                        sources,
                        resampler.input_frames_next(),
                    )
                    .unwrap();
                    let mut output =
                        SequentialSliceOfVecs::new_mut(&mut planar, sources, output_frames)
                            .unwrap();
                    let (_, frames) = resampler
                        .process_into_buffer(&input, &mut output, None)
                        .unwrap();
                    let produced =
                        convert(black_box(&planar), frames, black_box(outputs), &mut pcm);
                    black_box((produced, &pcm));
                },
            );
        }
    }
}
