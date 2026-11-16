use deadlib_audio_decode::resample::{PlanarAccum, append_channel_mapped_i16};

#[allow(dead_code)]
#[path = "../../../tests/support/allocations.rs"]
mod perf;

#[test]
fn planar_preserves_every_pcm_value_and_partial_frames() {
    for channels in [1usize, 2, 3, 4, 5, 6, 7, 8, 9] {
        let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
        let mut planar = PlanarAccum::new(channels, 0);
        planar.push_i16_interleaved(&input, channels);
        let frames = input.len() / channels;
        for (index, channel) in planar.channels.iter().enumerate() {
            let expected: Vec<_> = input[..frames * channels]
                .iter()
                .skip(index)
                .step_by(channels)
                .map(|&value| (f32::from(value) / 32768.0).to_bits())
                .collect();
            assert_eq!(
                channel.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                expected
            );
        }
    }
}


#[test]
fn channel_mapping_preserves_prefix_and_complete_frames() {
    let input: Vec<i16> = (i16::MIN..=i16::MAX).collect();
    for in_ch in 0..=9 {
        for out_ch in 0..=9 {
            for length in [0, 1, 3, 16, 257, input.len()] {
                let mut expected = vec![123, -456];
                let frames = if in_ch == 0 || out_ch == 0 {
                    0
                } else {
                    length / in_ch
                };
                for frame in 0..frames {
                    for channel in 0..out_ch {
                        expected.push(input[frame * in_ch + channel % in_ch]);
                    }
                }
                let mut actual = Vec::with_capacity(expected.len());
                actual.extend_from_slice(&[123, -456]);
                let pointer = actual.as_ptr();
                perf::assert_no_churn(|| {
                    assert_eq!(
                        append_channel_mapped_i16(&input[..length], in_ch, out_ch, &mut actual),
                        frames
                    );
                });
                assert_eq!(actual, expected);
                assert_eq!(actual.as_ptr(), pointer);
            }
        }
    }
}
