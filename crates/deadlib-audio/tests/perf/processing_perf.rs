use super::*;
use std::hint::black_box;

#[test]
#[ignore = "manual release benchmark"]
fn hot_path_bench() {
    for channels in [2, 16] {
        let input = vec![12000; 512 * channels];
        let mut output = Vec::with_capacity(OUT_FRAMES_PER_CALL * channels);
        let mut stages = MusicStages::new(channels, 48000, 44100);
        stages.set_rate(1.0, true).unwrap();
        crate::perf::measure(&format!("audio_{channels}ch_chunks"), 512, || {
            stages.push(black_box(&input));
            while stages.pull(&mut output, channels).unwrap().is_some() {
                black_box(&output);
            }
        });
    }
}

#[test]
#[ignore = "manual before/after output capture"]
fn hot_path_snapshot() {
    let mut output_bytes = Vec::new();
    for channels in [1, 2, 8, 16] {
        for input_hz in [44100, 48000] {
            for rate in [0.8, 1.0, 1.2] {
                for preserve in [false, true] {
                    let input: Vec<i16> = (0..4093 * channels)
                        .map(|i| ((i * 7919 % 30001) as i16) - 15000)
                        .collect();
                    let mut stages = MusicStages::new(channels, input_hz, 48000);
                    stages.set_rate(rate, preserve).unwrap();
                    if stages.is_direct() {
                        continue;
                    }
                    let mut out = Vec::new();
                    for repeat in 0..2 {
                        if repeat != 0 {
                            stages.reset();
                        }
                        for packet in input.chunks(317 * channels) {
                            stages.push(packet);
                            while let Some(step) = stages.pull(&mut out, channels).unwrap() {
                                output_bytes.extend_from_slice(&step.to_le_bytes());
                                for value in &out {
                                    output_bytes.extend_from_slice(&value.to_le_bytes());
                                }
                            }
                        }
                        stages.finish();
                        while let Some(step) = stages.pull(&mut out, channels).unwrap() {
                            output_bytes.extend_from_slice(&step.to_le_bytes());
                            for value in &out {
                                output_bytes.extend_from_slice(&value.to_le_bytes());
                            }
                        }
                    }
                }
            }
        }
    }
    std::fs::write(
        std::env::var_os("DEADSYNC_PERF_SNAPSHOT").expect("snapshot path"),
        output_bytes,
    )
    .unwrap();
}

#[test]
fn planar_offsets_match_packed_input_through_history_and_reset() {
    for channels in [1, 2, 8, 16] {
        let mut offset =
            new_resampler(44100.0 / 48000.0, RESAMPLE_MAX_RELATIVE_RATIO, channels).unwrap();
        let mut packed =
            new_resampler(44100.0 / 48000.0, RESAMPLE_MAX_RELATIVE_RATIO, channels).unwrap();
        let max = offset.input_frames_max();
        let mut storage = vec![vec![0.0; max + 37]; channels];
        let mut contiguous = vec![vec![0.0; max]; channels];
        let mut out_offset = vec![vec![0.0; offset.output_frames_max()]; channels];
        let mut out_packed = out_offset.clone();
        for packet in 0..12 {
            if packet == 7 {
                offset.reset();
                packed.reset();
            }
            let start = [0, 17, 37][packet % 3];
            let need = offset.input_frames_next();
            for ch in 0..channels {
                storage[ch].fill(-999.0);
                for frame in 0..need {
                    let value = ((frame * 173 + ch * 7919 + packet * 37) % 30001) as f32 / 32768.0;
                    contiguous[ch][frame] = value;
                    storage[ch][start + frame] = value;
                }
            }
            let a = process_resampler(&mut offset, &storage, start, &mut out_offset).unwrap();
            let b = process_resampler(&mut packed, &contiguous, 0, &mut out_packed).unwrap();
            assert_eq!(a, b);
            assert_eq!(
                out_offset, out_packed,
                "channels={channels}, packet={packet}"
            );
        }
    }
}

#[test]
fn wide_resampling_has_no_warm_chunk_churn() {
    let channels = 16;
    let input = vec![12000; 512 * channels];
    let mut output = Vec::with_capacity(OUT_FRAMES_PER_CALL * channels);
    let mut stages = MusicStages::new(channels, 48000, 44100);
    stages.set_rate(1.0, true).unwrap();
    let mut chunk = || {
        stages.push(&input);
        while stages.pull(&mut output, channels).unwrap().is_some() {
            black_box(&output);
        }
    };
    for _ in 0..100 {
        chunk();
    }
    crate::perf::assert_no_churn(|| {
        for _ in 0..100 {
            chunk();
        }
    });
}
