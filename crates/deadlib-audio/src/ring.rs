use crate::MusicMapSeg;
use rtrb::{Consumer, Producer, PushError, RingBuffer};
use std::sync::atomic::{AtomicU64, Ordering};

pub const PREROLL_IN_FRAMES: u64 = 8;
pub const RING_CAP_SAMPLES: usize = 1 << 16;
pub const MUSIC_SEG_RING_CAP: usize = 1 << 11;
pub const MUSIC_BLOCK_FRAMES: usize = 256;
const MIN_MUSIC_BLOCKS: usize = 4;
static PLAYED_MAP_DROPS: AtomicU64 = AtomicU64::new(0);

#[inline(always)]
pub fn played_map_drops() -> u64 {
    PLAYED_MAP_DROPS.load(Ordering::Relaxed)
}

/// Timing attached to the same ownership transfer as its decoded samples.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicBlockTiming {
    pub generation: u64,
    pub music_start_sec: f64,
    pub music_sec_per_frame: f64,
}

pub(crate) struct MusicBlock {
    samples: Box<[i16]>,
    sample_len: usize,
    timing: MusicBlockTiming,
}

impl MusicBlock {
    #[inline(always)]
    pub(crate) fn samples(&self) -> &[i16] {
        &self.samples[..self.sample_len]
    }

    #[inline(always)]
    pub(crate) fn timing(&self) -> MusicBlockTiming {
        self.timing
    }
}

/// Decoder-owned endpoint for the fixed music-block pool.
///
/// The decoder worker is the sole owner. Its lifetime is the output stream, its
/// capacity is approximately [`RING_CAP_SAMPLES`], and exhaustion sleeps on the
/// worker without adding work to the audio callback. Blocks are allocated once
/// by [`music_transport`] and destroyed only when the output backend is torn
/// down. Queue occupancy is available from rtrb during profiling; a miss costs
/// one bounded pop and no allocation, lock, I/O, pruning, or destruction.
pub struct MusicBlockWriter {
    ready: Producer<MusicBlock>,
    recycled: Consumer<MusicBlock>,
    spare: Option<MusicBlock>,
    channels: usize,
}

impl MusicBlockWriter {
    #[inline(always)]
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Copy and publish at most one fixed-size block, returning samples accepted.
    /// A return value of zero is backpressure; the caller may sleep and retry.
    pub fn try_push(&mut self, samples: &[i16], timing: MusicBlockTiming) -> usize {
        let channels = self.channels;
        let sample_len = samples.len().min(MUSIC_BLOCK_FRAMES * channels) / channels * channels;
        if sample_len == 0 {
            return 0;
        }
        let Some(mut block) = self.spare.take().or_else(|| self.recycled.pop().ok()) else {
            return 0;
        };
        block.samples[..sample_len].copy_from_slice(&samples[..sample_len]);
        block.sample_len = sample_len;
        block.timing = timing;
        match self.ready.push(block) {
            Ok(()) => sample_len,
            Err(PushError::Full(mut block)) => {
                // Conservation makes this unreachable after taking a recycled
                // block, but retain the allocation and let the worker retry.
                block.sample_len = 0;
                self.spare = Some(block);
                0
            }
        }
    }
}

pub struct PlayedMapReader {
    played: Consumer<TaggedMusicMapSeg>,
}

impl PlayedMapReader {
    #[inline(always)]
    pub fn pop(&mut self) -> Option<(u64, MusicMapSeg)> {
        self.played
            .pop()
            .ok()
            .map(|tagged| (tagged.generation, tagged.seg))
    }
}

pub struct AudioStreamHandle {
    pub writer: MusicBlockWriter,
    pub played_map: PlayedMapReader,
}

pub struct AudioRenderHandle {
    ready: Consumer<MusicBlock>,
    recycled: Producer<MusicBlock>,
    played: Producer<TaggedMusicMapSeg>,
}

impl AudioRenderHandle {
    #[inline(always)]
    pub(crate) fn pop_block(&mut self) -> Option<MusicBlock> {
        self.ready.pop().ok()
    }

    #[inline(always)]
    pub(crate) fn recycle_block(&mut self, block: MusicBlock) -> Result<(), MusicBlock> {
        self.recycled
            .push(block)
            .map_err(|PushError::Full(block)| block)
    }

    #[inline(always)]
    pub(crate) fn push_played(&mut self, generation: u64, seg: MusicMapSeg) {
        if self
            .played
            .push(TaggedMusicMapSeg { generation, seg })
            .is_err()
        {
            PLAYED_MAP_DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Clone, Copy)]
struct TaggedMusicMapSeg {
    generation: u64,
    seg: MusicMapSeg,
}

/// Build the session-lifetime music transport after device preparation.
///
/// Ownership is split decoder -> callback for ready blocks, callback -> decoder
/// for recycling, and callback -> game for played timing. The pool is warmed
/// here, capped near 65,536 interleaved samples, never evicts, and performs no
/// callback-time allocation, locking, waiting, scanning, or destruction. A
/// reset is a generation comparison over at most the fixed block count. Played
/// timing has a hard 2,048-record cap and saturates by dropping new records;
/// [`played_map_drops`] exposes those misses for telemetry.
pub fn music_transport(channels: usize) -> (AudioStreamHandle, AudioRenderHandle) {
    let channels = channels.max(1);
    let samples_per_block = MUSIC_BLOCK_FRAMES * channels;
    let block_count = RING_CAP_SAMPLES
        .div_ceil(samples_per_block)
        .max(MIN_MUSIC_BLOCKS);
    let (ready, ready_consumer) = RingBuffer::new(block_count);
    let (mut recycle_producer, recycled) = RingBuffer::new(block_count);
    for _ in 0..block_count {
        let block = MusicBlock {
            samples: vec![0; samples_per_block].into_boxed_slice(),
            sample_len: 0,
            timing: MusicBlockTiming::default(),
        };
        recycle_producer
            .push(block)
            .expect("fresh recycle queue has one slot per block");
    }
    let (played, played_consumer) = RingBuffer::new(MUSIC_SEG_RING_CAP);
    (
        AudioStreamHandle {
            writer: MusicBlockWriter {
                ready,
                recycled,
                spare: None,
                channels,
            },
            played_map: PlayedMapReader {
                played: played_consumer,
            },
        },
        AudioRenderHandle {
            ready: ready_consumer,
            recycled: recycle_producer,
            played,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::{Duration, Instant};

    fn pool_blocks(channels: usize) -> usize {
        RING_CAP_SAMPLES
            .div_ceil(MUSIC_BLOCK_FRAMES * channels.max(1))
            .max(MIN_MUSIC_BLOCKS)
    }

    fn recycle(render: &mut AudioRenderHandle, block: MusicBlock) {
        assert!(render.recycle_block(block).is_ok());
    }

    #[test]
    fn pool_survives_saturation_and_recycling() {
        let channels = 2;
        let block_samples = MUSIC_BLOCK_FRAMES * channels;
        let block_count = pool_blocks(channels);
        let (mut stream, mut render) = music_transport(channels);
        let samples = vec![17; block_samples];
        let timing = MusicBlockTiming {
            generation: 7,
            music_start_sec: 1.25,
            music_sec_per_frame: 1.0 / 48_000.0,
        };

        assert_eq!(stream.writer.recycled.slots(), block_count);
        assert_eq!(render.ready.slots(), 0);
        for _ in 0..block_count {
            assert_eq!(stream.writer.try_push(&samples, timing), block_samples);
        }
        assert_eq!(stream.writer.recycled.slots(), 0);
        assert_eq!(render.ready.slots(), block_count);
        assert_eq!(stream.writer.try_push(&samples, timing), 0);

        let held = render.pop_block().expect("full queue has a block");
        assert_eq!(held.samples(), samples);
        assert_eq!(held.timing(), timing);
        assert_eq!(
            stream.writer.recycled.slots() + render.ready.slots() + 1,
            block_count
        );
        assert_eq!(stream.writer.try_push(&samples, timing), 0);
        recycle(&mut render, held);
        assert_eq!(stream.writer.try_push(&samples, timing), block_samples);
        assert_eq!(render.ready.slots(), block_count);

        let mut drained = 0;
        while let Some(block) = render.pop_block() {
            assert_eq!(block.samples(), samples);
            recycle(&mut render, block);
            drained += 1;
        }
        assert_eq!(drained, block_count);
        assert_eq!(stream.writer.recycled.slots(), block_count);
        assert_eq!(render.ready.slots(), 0);
        assert!(stream.writer.spare.is_none());
    }

    #[test]
    fn partial_pushes_preserve_samples_and_timing() {
        let channels = 3;
        let total_frames = MUSIC_BLOCK_FRAMES * 4 + 137;
        let source: Vec<i16> = (0..total_frames * channels)
            .map(|i| ((i as i32 * 73 + 19) % 65_536 - 32_768) as i16)
            .collect();
        let frame_chunks = [1, 511, 7, 300, 342];
        assert_eq!(frame_chunks.iter().sum::<usize>(), total_frames);
        let generation = 91;
        let start_sec = -0.25;
        let sec_per_frame = 1.5 / 48_000.0;
        let (mut stream, mut render) = music_transport(channels);
        let mut expected = Vec::new();
        let mut frame_start = 0;

        for chunk_frames in frame_chunks {
            let frame_end = frame_start + chunk_frames;
            let mut sample_start = frame_start * channels;
            let sample_end = frame_end * channels;
            while sample_start < sample_end {
                let timing = MusicBlockTiming {
                    generation,
                    music_start_sec: start_sec + (sample_start / channels) as f64 * sec_per_frame,
                    music_sec_per_frame: sec_per_frame,
                };
                let accepted = stream
                    .writer
                    .try_push(&source[sample_start..sample_end], timing);
                assert!(accepted > 0);
                expected.push((
                    source[sample_start..sample_start + accepted].to_vec(),
                    timing,
                ));
                sample_start += accepted;
            }
            frame_start = frame_end;
        }

        for (samples, timing) in expected {
            let block = render
                .pop_block()
                .expect("every accepted push is published");
            assert_eq!(block.samples(), samples);
            assert_eq!(block.timing(), timing);
            recycle(&mut render, block);
        }
        assert!(render.pop_block().is_none());
        assert_eq!(stream.writer.recycled.slots(), pool_blocks(channels));
    }

    fn stress_sample(block: u64, index: usize) -> i16 {
        let bits = block
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add((index as u64).wrapping_mul(1_442_695_040_888_963_407));
        (bits ^ (bits >> 32)) as u16 as i16
    }

    fn hash_sample(hash: u64, sample: i16) -> u64 {
        hash.rotate_left(7).wrapping_mul(0x9e37_79b1_85eb_ca87) ^ u64::from(sample as u16)
    }

    #[test]
    fn concurrent_transfer_preserves_order_and_checksum() {
        const BLOCKS: u64 = 1_024;
        const TIMEOUT: Duration = Duration::from_secs(5);
        let channels = 2;
        let block_samples = MUSIC_BLOCK_FRAMES * channels;
        let block_count = pool_blocks(channels);
        let (stream, mut render) = music_transport(channels);
        let AudioStreamHandle {
            mut writer,
            played_map: _played_map,
        } = stream;
        let producer = thread::spawn(move || {
            let deadline = Instant::now() + TIMEOUT;
            let mut samples = vec![0; block_samples];
            let mut checksum = 0;
            for sequence in 0..BLOCKS {
                for (index, sample) in samples.iter_mut().enumerate() {
                    *sample = stress_sample(sequence, index);
                    checksum = hash_sample(checksum, *sample);
                }
                let timing = MusicBlockTiming {
                    generation: sequence + 1,
                    music_start_sec: sequence as f64 * 0.25,
                    music_sec_per_frame: 1.0 / 48_000.0,
                };
                while writer.try_push(&samples, timing) == 0 {
                    assert!(Instant::now() < deadline, "producer timed out");
                    thread::yield_now();
                }
            }
            (writer, checksum)
        });

        let deadline = Instant::now() + TIMEOUT;
        let mut checksum = 0;
        for sequence in 0..BLOCKS {
            let block = loop {
                if let Some(block) = render.pop_block() {
                    break block;
                }
                assert!(Instant::now() < deadline, "consumer timed out");
                thread::yield_now();
            };
            assert_eq!(block.samples().len(), block_samples);
            assert_eq!(block.timing().generation, sequence + 1);
            assert_eq!(block.timing().music_start_sec, sequence as f64 * 0.25);
            for (index, sample) in block.samples().iter().copied().enumerate() {
                assert_eq!(sample, stress_sample(sequence, index));
                checksum = hash_sample(checksum, sample);
            }
            recycle(&mut render, block);
        }

        let (writer, produced_checksum) = producer.join().expect("producer did not panic");
        assert_eq!(checksum, produced_checksum);
        assert_eq!(writer.recycled.slots(), block_count);
        assert_eq!(render.ready.slots(), 0);
        assert!(writer.spare.is_none());
    }

    #[test]
    fn saturated_played_map_counts_dropped_records() {
        let (_stream, mut render) = music_transport(2);
        let before = played_map_drops();
        let seg = MusicMapSeg {
            stream_frame_start: 0,
            frames: 1,
            music_start_sec: 0.0,
            music_sec_per_frame: 1.0 / 48_000.0,
        };

        for _ in 0..=MUSIC_SEG_RING_CAP {
            render.push_played(1, seg);
        }

        assert!(played_map_drops() > before);
    }
}
