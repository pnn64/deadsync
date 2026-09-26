// Frozen from 6ce82aafc; exact sample conversion and output sizing.

#[inline]
pub(super) fn write_resampler_output(
    out: &[Vec<f32>],
    produced_frames: usize,
    out_ch: usize,
    out_tmp: &mut Vec<i16>,
) -> usize {
    if out.is_empty() || produced_frames == 0 || out_ch == 0 {
        out_tmp.clear();
        return 0;
    }
    if out.len() == 2 && out_ch == 2 {
        let produced_frames = produced_frames.min(out[0].len()).min(out[1].len());
        let produced_samples = produced_frames * 2;
        resize_output(out_tmp, produced_samples);
        let (output_chunks, output_tail) = out_tmp.as_mut_slice().as_chunks_mut::<8>();
        let (left_chunks, left_tail) = out[0][..produced_frames].as_chunks::<4>();
        let (right_chunks, right_tail) = out[1][..produced_frames].as_chunks::<4>();
        for ((output, left), right) in output_chunks.iter_mut().zip(left_chunks).zip(right_chunks) {
            *output = [
                sample_to_i16(left[0]),
                sample_to_i16(right[0]),
                sample_to_i16(left[1]),
                sample_to_i16(right[1]),
                sample_to_i16(left[2]),
                sample_to_i16(right[2]),
                sample_to_i16(left[3]),
                sample_to_i16(right[3]),
            ];
        }
        for ((output, left), right) in output_tail
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(left_tail)
            .zip(right_tail)
        {
            *output = [sample_to_i16(*left), sample_to_i16(*right)];
        }
        return produced_frames;
    }
    if out.len() == 1 && out_ch == 2 {
        let produced_frames = produced_frames.min(out[0].len());
        let produced_samples = produced_frames * 2;
        resize_output(out_tmp, produced_samples);
        for (frame, sample) in out_tmp
            .as_mut_slice()
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(&out[0][..produced_frames])
        {
            let sample = sample_to_i16(*sample);
            *frame = [sample, sample];
        }
        return produced_frames;
    }
    let produced_frames = produced_frames.min(out.iter().map(Vec::len).min().unwrap_or(0));
    if out_ch == 1 {
        // Mono uses the first channel; the shortest input still limits output.
        write_mono_output(&out[0][..produced_frames], out_tmp);
        return produced_frames;
    }
    let produced_samples = produced_frames.saturating_mul(out_ch);
    resize_output(out_tmp, produced_samples);
    // Output channel `c` reads source channel `c % out.len()`.
    for (frame, output) in out_tmp.chunks_exact_mut(out_ch).enumerate() {
        for (dst, source) in output.iter_mut().zip(out.iter().cycle()) {
            *dst = sample_to_i16(source[frame]);
        }
    }
    produced_frames
}

#[inline(never)]
fn write_mono_output(input: &[f32], output: &mut Vec<i16>) {
    output.clear();
    output.extend(input.iter().copied().map(sample_to_i16));
}

fn resize_output(out_tmp: &mut Vec<i16>, produced_samples: usize) {
    if out_tmp.len() < produced_samples {
        out_tmp.resize(produced_samples, 0);
    } else {
        out_tmp.truncate(produced_samples);
    }
}

#[inline(always)]
fn sample_to_i16(sample: f32) -> i16 {
    // Rust float-to-integer casts already saturate and map NaN to zero.
    (sample * 32767.0).round() as i16
}
