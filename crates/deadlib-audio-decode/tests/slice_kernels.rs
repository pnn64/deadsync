use deadlib_audio_decode::resample::{
    append_channel_mapped_i16, write_resampler_output,
};

#[allow(dead_code)]
#[path = "../../../tests/support/allocations.rs"]
mod perf;

#[test]
fn pcm_rounding_matches_reference_at_every_half_boundary_and_float_extremes() {
    let mut input = Vec::new();
    for integer in -32770..=32770 {
        let center = (integer as f32 + 0.5) / 32767.0;
        input.extend([
            center.next_down().next_down(),
            center.next_down(),
            center,
            center.next_up(),
            center.next_up().next_up(),
        ]);
    }
    // Sample every exponent, both signs and NaN kinds, plus arbitrary mantissas.
    for exponent in 0..=255u32 {
        for sign in [0, 1 << 31] {
            for mantissa in [0, 1, 0x3f_ffff, 0x40_0000, 0x7f_fffe, 0x7f_ffff] {
                input.push(f32::from_bits(sign | (exponent << 23) | mantissa));
            }
        }
    }
    let mut state = 0x1234_5678u32;
    for _ in 0..1_000_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        input.push(f32::from_bits(state));
    }
    let expected: Vec<_> = input
        .iter()
        .map(|sample| (sample * 32767.0).round() as i16)
        .collect();
    let mut output = Vec::with_capacity(input.len());
    let input = [input];
    perf::assert_no_churn(|| {
        assert_eq!(
            write_resampler_output(&input, input[0].len(), 1, &mut output),
            input[0].len()
        );
    });
    assert_eq!(output, expected);
}

#[test]
fn mono_append_preserves_all_samples_prefix_tail_and_allocation() {
    let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
    for channels in [2usize, 3, 4, 6, 8, 9] {
        for tail in 0..channels {
            let len = input.len() - tail;
            let mut output = Vec::with_capacity(2 + len / channels);
            output.extend([123, -456]);
            let pointer = output.as_ptr();
            let expected: Vec<_> = [123, -456]
                .into_iter()
                .chain((0..len / channels).map(|frame| input[frame * channels]))
                .collect();
            perf::assert_no_churn(|| {
                assert_eq!(
                    append_channel_mapped_i16(&input[..len], channels, 1, &mut output),
                    len / channels
                );
            });
            assert_eq!(output, expected);
            assert_eq!(output.as_ptr(), pointer);
        }
    }
}
