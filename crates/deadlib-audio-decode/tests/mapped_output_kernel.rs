use deadlib_audio_decode::resample::write_channel_mapped_i16;

#[allow(dead_code)]
#[path = "../../../tests/support/allocations.rs"]
mod perf;

#[test]
fn mapped_output_keeps_complete_frames_and_reuses_storage() {
    let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
    for in_ch in 0..=9 {
        for out_ch in 0..=9 {
            for len in [0, 1, 3, 16, 257, input.len()] {
                let mut output = Vec::with_capacity(input.len() * 9);
                output.extend_from_slice(&[123, -456]);
                let pointer = output.as_ptr();
                let frames = if in_ch == 0 || out_ch == 0 {
                    0
                } else {
                    len / in_ch
                };
                let expected: Vec<_> = (0..frames)
                    .flat_map(|frame| {
                        let input = &input;
                        (0..out_ch).map(move |channel| input[frame * in_ch + channel % in_ch])
                    })
                    .collect();
                perf::assert_no_churn(|| {
                    assert_eq!(
                        write_channel_mapped_i16(&input[..len], in_ch, out_ch, &mut output),
                        frames
                    );
                });
                assert_eq!(output, expected, "{in_ch} -> {out_ch}, len={len}");
                assert_eq!(output.as_ptr(), pointer);
            }
        }
    }
}
