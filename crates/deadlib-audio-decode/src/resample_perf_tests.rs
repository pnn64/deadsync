//! Resampler buffer and storage regressions.
use super::*;

#[path = "../../../tests/support/allocations.rs"]
mod allocations;


#[test]
fn draining_large_packets_never_moves_the_unread_tail() {
    let packet: Vec<_> = (0..65536 * 2).map(|i| i as i16).collect();
    let mut queue = PlanarAccum::new(2, 65536);
    for _ in 0..3 {
        queue.push_i16_interleaved(&packet, 2);
        let pointers: Vec<_> = queue.channels.iter().map(Vec::as_ptr).collect();
        for consumed in (256..65536).step_by(256) {
            queue.consume_frames(256);
            assert_eq!(queue.start_frame, consumed);
            for (channel, &ptr) in queue.channels.iter().zip(&pointers) {
                assert_eq!(channel.as_ptr(), ptr);
                assert_eq!(channel.capacity(), 65536);
                assert_eq!(channel.len(), 65536);
            }
        }
        queue.consume_frames(256);
        assert!(queue.is_empty());
        assert_eq!(queue.start_frame, 0);
    }
}

#[test]
fn planar_packets_preserve_all_pcm_values_and_reuse_consumed_capacity() {
    use std::collections::VecDeque;

    for channels in [1, 2, 3, 6, 8] {
        let input: Vec<_> = (0..65_536 * channels)
            .map(|i| ((i / channels) as u16).wrapping_add((i % channels) as u16 * 97) as i16)
            .collect();
        let mut queue = PlanarAccum::new(channels, 1024);
        let pointers: Vec<_> = queue.channels.iter().map(Vec::as_ptr).collect();
        let mut expected: Vec<_> = (0..channels)
            .map(|_| VecDeque::with_capacity(1024))
            .collect();
        let mut position = 0;
        let mut packet = 0;
        while position < 65_536 {
            let frames = [1, 7, 16, 63, 64, 65, 127, 128, 129, 255, 256, 257][packet % 12]
                .min(65_536 - position);
            let end = (position + frames) * channels;
            let tail = usize::from(channels > 1 && end < input.len() && packet % 2 == 0);
            let samples = &input[position * channels..end + tail];
            for frame in samples.chunks_exact(channels) {
                for (output, sample) in expected.iter_mut().zip(frame) {
                    output.push_back(f32::from(*sample) / 32768.0);
                }
            }
            allocations::assert_no_churn(|| queue.push_i16_interleaved(samples, channels));
            assert_eq!(queue.available_frames(), expected[0].len());
            for (index, output) in queue.channels.iter().enumerate() {
                assert_eq!(output.as_ptr(), pointers[index]);
                assert_eq!(output.capacity(), 1024);
                assert!(
                    output[queue.start_frame..]
                        .iter()
                        .map(|v| v.to_bits())
                        .eq(expected[index].iter().map(|v| v.to_bits())),
                    "channels {channels}, channel {index}, packet {packet}"
                );
            }
            let consumed = if packet % 11 == 0 {
                queue.available_frames()
            } else {
                queue.available_frames() / 2
            };
            allocations::assert_no_churn(|| queue.consume_frames(consumed));
            for channel in &mut expected {
                drop(channel.drain(..consumed));
            }
            assert_eq!(queue.available_frames(), expected[0].len());
            assert_eq!(queue.is_empty(), expected[0].is_empty());
            position += frames;
            packet += 1;
        }
        allocations::assert_no_churn(|| queue.consume_frames(queue.available_frames()));
        assert!(queue.is_empty());
    }
}
