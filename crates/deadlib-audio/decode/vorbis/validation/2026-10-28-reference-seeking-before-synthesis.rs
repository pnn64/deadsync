//! Granule-aware overlap state used for reference stream/seek scheduling.
use super::{DecodeOptions, PacketDecoder, PcmBlock, packet_header, zeroed};
use crate::{Error, Setup};

#[cfg(test)]
#[path = "synthesis_reference.rs"]
mod reference_tests;

/// Timing attached to one raw Vorbis packet, independently of its container.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PacketTiming {
    /// Sequence bits, interpreted as the reference's signed 64-bit packet number.
    pub packet_number: u64,
    /// None (or Some(-1)) means the reference's unknown-position sentinel.
    pub granule_position: Option<i64>,
    pub end_of_stream: bool,
}

/// Observable reference synthesis clocks and queued PCM after an operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SynthesisState {
    /// Last inserted packet number; the signed -1 sentinel is None.
    pub packet_number: Option<u64>,
    pub granule_position: Option<i64>,
    /// Counts overlap spans, including track-only packets. None is the -1
    /// sentinel; other signed values preserve the reference counter wrap.
    pub sample_count: Option<i64>,
    pub block_size: usize,
    pub queued_frames: usize,
    pub end_of_stream: bool,
}

/// Raw synthesis with libvorbis-style granule, sequence and PCM queue handling.
/// Decode/track a packet, inspect borrowed PCM, then consume it before inserting
/// another block. Track-only packets update clocks and block sizes without
/// decoding floors, residues or time-domain samples. No Ogg framing is required.
/// Floor, residue, custom-float and IEEE policies remain explicit setup/decode
/// options. This type adds no synthetic final flush and performs no resampling.
pub struct SynthesisDecoder {
    decoder: PacketDecoder,
    pcm: Vec<f32>,
    size: usize,
    center: usize,
    returned: i32,
    current: i32,
    previous_size: usize,
    sequence: i64,
    granule: i64,
    samples: i64,
    eos: bool,
    position: u64,
    truncated: bool,
}
impl SynthesisDecoder {
    pub fn new(setup: Setup) -> Result<Self, Error> {
        Self::with_options(setup, DecodeOptions::default())
    }
    pub fn required_storage(setup: &Setup) -> usize {
        PacketDecoder::required_storage(setup)
            + setup.channels() as usize * setup.block_sizes()[1] * core::mem::size_of::<f32>()
    }
    pub fn with_options(setup: Setup, options: DecodeOptions) -> Result<Self, Error> {
        if Self::required_storage(&setup) > options.limits.max_decode_memory {
            return Err(Error::LimitExceeded("decode memory"));
        }
        let channels = setup.channels() as usize;
        let size = setup.block_sizes()[1];
        let previous_size = setup.block_sizes()[0];
        Ok(Self {
            decoder: PacketDecoder::with_options(setup, options)?,
            pcm: zeroed(channels * size)?,
            size,
            center: size / 2,
            returned: -1,
            current: (size / 2) as i32,
            previous_size,
            sequence: -1,
            granule: -1,
            samples: -1,
            eos: false,
            position: 0,
            truncated: false,
        })
    }
    pub fn setup(&self) -> &Setup {
        self.decoder.setup()
    }
    pub fn storage_bytes(&self) -> usize {
        self.decoder.storage_bytes() + self.pcm.len() * core::mem::size_of::<f32>()
    }
    /// Count of frames consumed through this API since reset, independently of
    /// declared granule positions and track-only advancement.
    pub fn position(&self) -> u64 {
        self.position
    }
    /// Restart clocks and priming while retaining the current block size and overlap
    /// storage, as vorbis_synthesis_restart does. Ordinary valid replay primes
    /// before exposing samples; explicit track/read scheduling can expose retained
    /// buffer contents just as in the reference.
    pub fn reset(&mut self) {
        self.decoder.reset();
        self.center = self.size / 2;
        self.returned = -1;
        self.current = (self.size / 2) as i32;
        self.sequence = -1;
        self.granule = -1;
        self.samples = -1;
        self.eos = false;
        self.position = 0;
        self.truncated = false;
    }
    fn frames(&self) -> usize {
        if self.returned >= 0 && self.current > self.returned {
            (self.current - self.returned) as usize
        } else {
            0
        }
    }
    pub fn state(&self) -> SynthesisState {
        SynthesisState {
            packet_number: (self.sequence != -1).then_some(self.sequence as u64),
            granule_position: (self.granule != -1).then_some(self.granule),
            sample_count: (self.samples != -1).then_some(self.samples),
            block_size: self.previous_size,
            queued_frames: self.frames(),
            end_of_stream: self.eos,
        }
    }
    pub fn pcm(&self) -> PcmBlock<'_> {
        let frames = self.frames();
        let start = if frames == 0 {
            0
        } else {
            self.returned as usize
        };
        PcmBlock {
            data: &self.pcm[start..],
            stride: self.size,
            channels: self.setup().channels() as usize,
            frames,
            start_frame: self.position,
            block_size: self.previous_size,
            truncated: self.truncated,
        }
    }
    /// Consume a nonnegative number of frames using the reference read guard.
    /// In normal use request at most pcm().frames. The reference also permits
    /// advancing its initial -1 priming cursor before any PCM was reported.
    /// Errors leave the queue and consumed-frame position unchanged.
    pub fn consume(&mut self, frames: usize) -> Result<(), Error> {
        let n = i32::try_from(frames).map_err(|_| Error::Invalid("synthesis read count"))?;
        let next = self.returned.wrapping_add(n);
        if n != 0 && next > self.current {
            return Err(Error::Invalid("synthesis read count"));
        }
        let position = self
            .position
            .checked_add(frames as u64)
            .ok_or(Error::LimitExceeded("PCM frame position"))?;
        self.returned = next;
        self.position = position;
        Ok(())
    }
    /// False means a discarded type/mode/window prefix. A valid first block
    /// primes overlap and can report zero PCM. Pending PCM makes insertion fail;
    /// the existing queue/clocks remain unchanged.
    pub fn submit(&mut self, packet: &[u8], timing: PacketTiming) -> Result<bool, Error> {
        let Some(block) = self.decoder.decode_time(packet)? else {
            return Ok(false);
        };
        self.insert(block.header.block_size, timing, true, block.truncated)?;
        Ok(true)
    }
    /// Inspect the same complete type/mode/window prefix as synthesis, then
    /// insert a block without reading any floor/residue payload or producing a
    /// new time-domain block. Existing queued PCM still prevents insertion.
    pub fn track(&mut self, packet: &[u8], timing: PacketTiming) -> Result<bool, Error> {
        let Some((header, _)) = packet_header(self.setup(), &self.decoder.limits, packet)? else {
            return Ok(false);
        };
        self.insert(header.block_size, timing, false, false)?;
        Ok(true)
    }
    fn insert(
        &mut self,
        size: usize,
        timing: PacketTiming,
        decode: bool,
        truncated: bool,
    ) -> Result<(), Error> {
        if self.current > self.returned && self.returned != -1 {
            return Err(Error::Invalid("unconsumed synthesis PCM"));
        }
        let previous = self.previous_size;
        let span = (previous + size) / 4;
        if decode {
            let small = self.setup().block_sizes()[0] / 2;
            let large = self.size / 2;
            let half = size / 2;
            let (this, prior) = if self.center != 0 {
                (large, 0)
            } else {
                (0, large)
            };
            let window = if previous == self.size && size == self.size && self.size != small * 2 {
                &self.decoder.long_slope
            } else {
                &self.decoder.short_slope
            };
            for channel in 0..self.setup().channels() as usize {
                let old = &self.pcm[channel * self.size..(channel + 1) * self.size];
                let pcm = &mut self.decoder.pcm[channel * large..(channel + 1) * large];
                pcm.copy_from_slice(&old[prior..prior + large]);
                let time = &self.decoder.current[channel * self.size..channel * self.size + size];
                if previous == self.size && size == self.size {
                    for i in 0..large {
                        pcm[i] = pcm[i] * window[large - 1 - i] + time[i] * window[i];
                    }
                } else if previous == self.size && size != self.size {
                    let offset = large / 2 - small / 2;
                    for i in 0..small {
                        pcm[offset + i] =
                            pcm[offset + i] * window[small - 1 - i] + time[i] * window[i];
                    }
                } else if previous != self.size && size == self.size {
                    let offset = large / 2 - small / 2;
                    for i in 0..small {
                        pcm[i] = pcm[i] * window[small - 1 - i] + time[offset + i] * window[i];
                    }
                    pcm[small..large / 2 + small / 2].copy_from_slice(&time[offset + small..large]);
                } else {
                    for i in 0..small {
                        pcm[i] = pcm[i] * window[small - 1 - i] + time[i] * window[i];
                    }
                }
                if !self.decoder.allow_nonfinite && pcm.iter().any(|v| !v.is_finite()) {
                    return Err(Error::Invalid("nonfinite overlap output"));
                }
            }
            // Reuse raw synthesis scratch to keep finite-output errors
            // transactional without adding another persistent PCM allocation.
            for channel in 0..self.setup().channels() as usize {
                let pcm = &mut self.pcm[channel * self.size..(channel + 1) * self.size];
                pcm[prior..prior + large]
                    .copy_from_slice(&self.decoder.pcm[channel * large..(channel + 1) * large]);
                pcm[this..this + half].copy_from_slice(
                    &self.decoder.current[channel * self.size + half..channel * self.size + size],
                );
            }
            self.center = if self.center != 0 { 0 } else { large };
            if self.returned == -1 {
                self.returned = this as i32;
                self.current = this as i32;
            } else {
                self.returned = prior as i32;
                self.current = (prior + span) as i32;
            }
            self.truncated = truncated;
        }
        self.previous_size = size;
        let sequence = timing.packet_number as i64;
        if self.sequence == -1 || self.sequence.wrapping_add(1) != sequence {
            self.granule = -1;
            self.samples = -1;
        }
        self.sequence = sequence;
        self.samples = if self.samples == -1 {
            0
        } else {
            self.samples.wrapping_add(span as i64)
        };
        let gp = timing.granule_position.unwrap_or(-1);
        if self.granule == -1 {
            if gp != -1 {
                self.granule = gp;
                if self.samples > gp {
                    let mut extra =
                        ((self.samples.wrapping_sub(gp) as core::ffi::c_long) as i128).max(0);
                    if timing.end_of_stream {
                        extra = extra.min(self.current.wrapping_sub(self.returned) as i128);
                        self.current = self.current.wrapping_sub(extra as i32);
                    } else {
                        self.returned = self.returned.wrapping_add(extra as i32);
                        if self.returned > self.current {
                            self.returned = self.current;
                        }
                    }
                }
            }
        } else {
            self.granule = self.granule.wrapping_add(span as i64);
            if gp != -1 && self.granule != gp {
                if self.granule > gp && timing.end_of_stream {
                    let extra = ((self.granule.wrapping_sub(gp) as core::ffi::c_long) as i128)
                        .min(self.current.wrapping_sub(self.returned) as i128)
                        .max(0);
                    self.current = self.current.wrapping_sub(extra as i32);
                }
                self.granule = gp;
            }
        }
        self.eos |= timing.end_of_stream;
        Ok(())
    }
}
