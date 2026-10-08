//! Differential checks and paired benchmarks against main e8705d58b3.
use super::*;
use std::hint::black_box;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

#[test]
fn planar_queue_matches_original_across_packet_boundaries() {
    for channels in [1, 2, 3, 6] {
        for capacity in [0, 17, 4096] {
            let mut old = original::PlanarAccum::new(channels, capacity);
            let mut new = PlanarAccum::new(channels, capacity);
            let mut seed = 0x1234_5678_u32;
            for step in 0..500 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                if step % 5 < 2 {
                    // Include incomplete input frames and every i16 sign.
                    let samples: Vec<_> = (0..seed as usize % (4097 * channels))
                        .map(|i| (i as u32).wrapping_mul(seed) as i16)
                        .collect();
                    old.push_i16_interleaved(&samples, channels);
                    new.push_i16_interleaved(&samples, channels);
                } else if step % 37 == 0 {
                    old.clear();
                    new.clear();
                } else {
                    let count = seed as usize % 8193;
                    old.consume_frames(count);
                    new.consume_frames(count);
                }
                assert_eq!(old.available_frames(), new.available_frames());
                assert_eq!(old.is_empty(), new.is_empty());
                for (a, b) in old.channels.iter().zip(&new.channels) {
                    assert_eq!(&a[old.start_frame..], &b[new.start_frame..]);
                }
            }
        }
    }
}

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
fn resampler_output_matches_original_for_all_channel_layouts() {
    let values = [
        0.0,
        -0.0,
        0.5,
        -0.5,
        1.0,
        -1.0,
        1.5,
        -1.5,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::from_bits(1),
    ];
    for in_ch in 0..=8 {
        for frames in [0, 1, 3, 4, 5, 7, 255, 256, 257] {
            for shortened in [false, true] {
                let input: Vec<Vec<f32>> = (0..in_ch)
                    .map(|c| {
                        let len = if shortened && c + 1 == in_ch {
                            frames / 2
                        } else {
                            frames
                        };
                        (0..len).map(|i| values[(i + c) % values.len()]).collect()
                    })
                    .collect();
                for out_ch in 0..=8 {
                    let mut old = vec![123; 5000];
                    let mut new = old.clone();
                    for produced in [0, 1, frames, frames + 3] {
                        assert_eq!(
                            original::write_resampler_output(&input, produced, out_ch, &mut old),
                            write_resampler_output(&input, produced, out_ch, &mut new)
                        );
                        assert_eq!(
                            old, new,
                            "{in_ch} -> {out_ch}, frames={frames}, short={shortened}"
                        );
                        assert_eq!(new.capacity(), 5000);
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run alone with --ignored --nocapture --test-threads=1"]
fn benchmark_planar_packet_drain() {
    for frames in [1152, 4096, 65535] {
        let packet: Vec<_> = (0..frames * 2).map(|i| i as i16).collect();
        let mut old = original::PlanarAccum::new(2, frames + 256);
        let mut new = PlanarAccum::new(2, frames + 256);
        paired_bench::compare(
            &format!("stereo packet {frames}, drain 256"),
            200,
            |current| {
                macro_rules! drain {
                    ($queue:expr) => {{
                        let queue = &mut $queue;
                        queue.clear();
                        for _ in 0..8 {
                            queue.push_i16_interleaved(black_box(&packet), black_box(2));
                            while queue.available_frames() >= 256 {
                                for channel in &queue.channels {
                                    black_box(&channel[queue.start_frame..queue.start_frame + 256]);
                                }
                                queue.consume_frames(black_box(256));
                            }
                        }
                        black_box(&queue.channels);
                    }};
                }
                if current {
                    drain!(new);
                } else {
                    drain!(old);
                }
            },
        );
        assert_eq!(old.available_frames(), new.available_frames());
        for (a, b) in old.channels.iter().zip(&new.channels) {
            assert_eq!(&a[old.start_frame..], &b[new.start_frame..]);
            assert!(b.capacity() <= a.capacity());
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run alone with --ignored --nocapture --test-threads=1"]
fn benchmark_resampler_channel_conversion() {
    for (in_ch, out_ch) in [(6, 2), (8, 2), (2, 2), (1, 2), (6, 6)] {
        for frames in [256, 4096] {
            let input: Vec<Vec<f32>> = (0..in_ch)
                .map(|c| {
                    (0..frames)
                        .map(|i| (i as f32 * 0.31 + c as f32).sin())
                        .collect()
                })
                .collect();
            let mut old = Vec::with_capacity(frames * out_ch);
            let mut new = Vec::with_capacity(frames * out_ch);
            paired_bench::compare(
                &format!("resampler {in_ch}->{out_ch}, {frames} frames"),
                2000,
                |current| {
                    let output = if current { &mut new } else { &mut old };
                    let convert = if current {
                        write_resampler_output
                    } else {
                        original::write_resampler_output
                    };
                    black_box(convert(
                        black_box(&input),
                        black_box(frames),
                        black_box(out_ch),
                        output,
                    ));
                    black_box(output);
                },
            );
            assert_eq!(old, new);
            assert_eq!(new.capacity(), frames * out_ch);
        }
    }
}

#[allow(dead_code)]
mod original {
    use super::super::{resize_output, sample_to_i16};
    const PLANAR_COMPACT_THRESHOLD_FRAMES: usize = 2048;

    pub struct PlanarAccum {
        pub channels: Vec<Vec<f32>>,
        pub start_frame: usize,
    }

    impl PlanarAccum {
        #[must_use]
        pub fn new(channels: usize, capacity_frames: usize) -> Self {
            let mut planar = Vec::with_capacity(channels);
            for _ in 0..channels {
                planar.push(Vec::with_capacity(capacity_frames));
            }
            Self {
                channels: planar,
                start_frame: 0,
            }
        }

        #[inline(always)]
        #[must_use]
        pub fn available_frames(&self) -> usize {
            self.channels
                .first()
                .map_or(0, |channel| channel.len().saturating_sub(self.start_frame))
        }

        #[inline(always)]
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.available_frames() == 0
        }

        #[inline]
        pub fn push_i16_interleaved(&mut self, interleaved: &[i16], channels: usize) {
            if interleaved.is_empty() || channels == 0 {
                return;
            }
            debug_assert_eq!(channels, self.channels.len());
            if channels == 1 {
                if let [channel] = self.channels.as_mut_slice() {
                    channel.extend(
                        interleaved
                            .iter()
                            .map(|sample| f32::from(*sample) / 32768.0),
                    );
                    return;
                }
            } else if channels == 2
                && let [left, right] = self.channels.as_mut_slice()
            {
                let frames = interleaved.as_chunks::<2>().0;
                left.extend(frames.iter().map(|frame| f32::from(frame[0]) / 32768.0));
                right.extend(frames.iter().map(|frame| f32::from(frame[1]) / 32768.0));
                return;
            }
            let frames = interleaved.len() / channels;
            for channel in &mut self.channels {
                channel.reserve(frames);
            }
            for frame in interleaved.chunks_exact(channels) {
                for (channel, sample) in self.channels.iter_mut().zip(frame.iter()) {
                    channel.push(f32::from(*sample) / 32768.0);
                }
            }
        }

        pub fn consume_frames(&mut self, frames: usize) {
            let total_frames = self.channels.first().map_or(0, Vec::len);
            self.start_frame = (self.start_frame + frames).min(total_frames);
            if self.start_frame == 0 {
                return;
            }
            let remaining_frames = total_frames - self.start_frame;
            if remaining_frames == 0 {
                self.clear();
                return;
            }
            if self.start_frame < PLANAR_COMPACT_THRESHOLD_FRAMES
                && self.start_frame * 2 < total_frames
            {
                return;
            }
            for channel in &mut self.channels {
                channel.copy_within(self.start_frame.., 0);
                channel.truncate(remaining_frames);
            }
            self.start_frame = 0;
        }

        pub fn clear(&mut self) {
            self.start_frame = 0;
            for channel in &mut self.channels {
                channel.clear();
            }
        }
    }

    #[inline]
    pub fn write_resampler_output(
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
            for ((output, left), right) in
                output_chunks.iter_mut().zip(left_chunks).zip(right_chunks)
            {
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
            let (converted, repeated) = output.split_at_mut(out.len().min(out_ch));
            for (dst, source) in converted.iter_mut().zip(out) {
                *dst = sample_to_i16(source[frame]);
            }
            // Extra channels reuse this frame's PCM instead of converting it again.
            for (dst, &sample) in repeated.iter_mut().zip(converted.iter().cycle()) {
                *dst = sample;
            }
        }
        produced_frames
    }

    // Keep the vector append loop separate from the existing stereo conversion
    // loops so its buffer-growth machinery does not enlarge their function body.
    #[inline(never)]
    fn write_mono_output(input: &[f32], output: &mut Vec<i16>) {
        output.clear();
        output.extend(input.iter().copied().map(sample_to_i16));
    }
}
