//! Native fixed-block analysis and staged scalar quantization in Vorbis VQ books.
use crate::{
    Comments, Error, Identification, Limits, Mdct, bitpack::BitWriter, floor_table::INVERSE_DB,
};
use core::f64::consts::PI;
use deadlib_audio_decode_ogg::{PacketWriter, WriteOptions};
use std::io::Write;

const PARTITION: usize = 16;
const PCM_LIMIT: f32 = 4.0;

// The leading radix-16 digit of a small positive/negative scalar is eight/seven.
// A complete tree assigns those four pairs three bits, the four corners eight,
// and the remaining 248 entries nine: 4/8 + 4/256 + 248/512 = 1.
const fn first_length(entry: usize) -> u8 {
    match entry {
        119 | 120 | 135 | 136 => 3,
        0 | 15 | 240 | 255 => 8,
        _ => 9,
    }
}
// Entry-order assignment in a nine-bit left-edge domain. This is generated at
// compile time, independent of reference books; one shared 512-byte table packs
// each LSB-first word and its length. No per-encoder table or initialization.
const fn first_words() -> [u16; 256] {
    let mut words = [0; 256];
    let mut free = [None; 10];
    free[0] = Some(0u16);
    let mut entry = 0;
    while entry < 256 {
        let length = first_length(entry) as usize;
        let mut best = None;
        let mut depth = 0;
        while depth <= length {
            if let Some(code) = free[depth] {
                if let Some((_, previous)) = best {
                    if code < previous {
                        best = Some((depth, code));
                    }
                } else {
                    best = Some((depth, code));
                }
            }
            depth += 1;
        }
        let Some((depth, code)) = best else {
            panic!("first-stage Huffman tree overpopulation");
        };
        free[depth] = None;
        let mut split = depth + 1;
        while split <= length {
            assert!(free[split].is_none());
            free[split] = Some(code | (1 << (9 - split)));
            split += 1;
        }
        words[entry] = (code.reverse_bits() >> 7) | ((length as u16) << 12);
        entry += 1;
    }
    let mut depth = 0;
    while depth < free.len() {
        assert!(free[depth].is_none());
        depth += 1;
    }
    words
}
static FIRST_WORDS: [u16; 256] = first_words();

/// Fixed-block encoder policy. This initial native encoder uses a flat adaptive
/// floor per channel, uncoupled mapping zero, and residue one. Precision controls
/// quantization, not a target bitrate or perceptual-quality guarantee.
#[derive(Clone, Copy, Debug)]
pub struct EncodeOptions {
    pub channels: u8,
    pub sample_rate: u32,
    /// Power of two, 64..=8192; both identification block sizes use this value.
    pub block_size: usize,
    /// One to four additive radix-16 stages: 4/8/12/16 bits per nonzero scalar.
    pub residue_passes: u8,
    pub max_packet_bytes: usize,
    /// Owned raw-encoder buffer/table elements, excluding caller input,
    /// comments, transport storage, allocator overhead and struct metadata.
    pub max_memory: usize,
    pub max_frames: u64,
}
impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            channels: 2,
            sample_rate: 48000,
            block_size: 1024,
            residue_passes: 3,
            max_packet_bytes: 64 << 20,
            max_memory: 128 << 20,
            max_frames: i64::MAX as u64 - 4096,
        }
    }
}

/// Borrowed audio packet. Absolute numbering starts at three, after the Vorbis
/// headers. Granules count retained frames per channel; the final packet carries
/// the exact submitted length, including a partially filled final overlap.
pub struct EncodedPacket<'a> {
    pub data: &'a [u8],
    pub number: u64,
    pub granule_position: u64,
    pub last_in_stream: bool,
}

/// Reusable raw Vorbis I encoder with arbitrary input chunk boundaries.
/// Input is in mapping-zero channel order, finite and in [-4, 4]. No implicit
/// clipping, resampling, channel reordering or container output is performed.
/// Normal push/finish/reset operations allocate nothing after construction.
pub struct PacketEncoder {
    options: EncodeOptions,
    identification: Identification,
    setup: Vec<u8>,
    transform: Mdct,
    window: Vec<f32>,
    input: Vec<f32>,
    time: Vec<f32>,
    spectrum: Vec<f32>,
    quantized: Vec<u16>,
    classes: Vec<u8>,
    floors: Vec<u8>,
    nonzero: Vec<bool>,
    bits: BitWriter,
    storage: usize,
    write_half: usize,
    filled: usize,
    frames: u64,
    packets: u64,
    finished: bool,
    failed: bool,
}
impl PacketEncoder {
    pub fn new(options: EncodeOptions) -> Result<Self, Error> {
        let n = options.block_size;
        let identification = Identification {
            channels: options.channels,
            sample_rate: options.sample_rate,
            bitrate_maximum: 0,
            bitrate_nominal: 0,
            bitrate_minimum: 0,
            block_sizes: [n, n],
        };
        identification.validate()?;
        if !(1..=4).contains(&options.residue_passes) {
            return Err(Error::Invalid("encoder residue passes"));
        }
        let half = n / 2;
        if options.max_frames > i64::MAX as u64 - half as u64 {
            return Err(Error::Invalid("encoder frame limit exceeds granule range"));
        }
        let channels = options.channels as usize;
        let partitions = half / PARTITION;
        let packet_bytes = (1
            + channels * 17
            + channels * partitions * (9 + 64 * options.residue_passes as usize))
            .div_ceil(8);
        let setup = setup_packet(n, options.residue_passes)?;
        if packet_bytes.max(setup.len()).max(30) > options.max_packet_bytes {
            return Err(Error::LimitExceeded("encoder packet bytes"));
        }
        let required = n * 14
            + n * channels * 4
            + n * 8
            + half * 4
            + channels * half * 2
            + channels * partitions
            + channels * 2
            + packet_bytes
            + setup.capacity();
        if required > options.max_memory {
            return Err(Error::LimitExceeded("encode memory"));
        }
        let mut window = zeroed(n)?;
        for (i, value) in window.iter_mut().enumerate() {
            *value = ((PI * (i as f64 + 0.5) / n as f64).sin().powi(2) * PI / 2.0).sin() as f32;
        }
        let transform = Mdct::new(n)?;
        let input = zeroed(channels * n)?;
        let time = zeroed(n)?;
        let spectrum = zeroed(half)?;
        let quantized = zeroed(channels * half)?;
        let classes = zeroed(channels * partitions)?;
        let floors = zeroed(channels)?;
        let nonzero = zeroed(channels)?;
        let bits = BitWriter::with_capacity(packet_bytes)?;
        let storage = transform.storage_bytes()
            + setup.capacity()
            + window.capacity() * 4
            + input.capacity() * 4
            + time.capacity() * 4
            + spectrum.capacity() * 4
            + quantized.capacity() * 2
            + classes.capacity()
            + floors.capacity()
            + nonzero.capacity()
            + bits.capacity_bytes();
        if storage > options.max_memory {
            return Err(Error::LimitExceeded("encode memory"));
        }
        Ok(Self {
            options,
            identification,
            setup,
            transform,
            window,
            input,
            time,
            spectrum,
            quantized,
            classes,
            floors,
            nonzero,
            bits,
            storage,
            write_half: 1,
            filled: 0,
            frames: 0,
            packets: 0,
            finished: false,
            failed: false,
        })
    }
    pub fn identification(&self) -> &Identification {
        &self.identification
    }
    pub fn setup_packet(&self) -> &[u8] {
        &self.setup
    }
    pub fn position(&self) -> u64 {
        self.frames
    }
    pub fn storage_bytes(&self) -> usize {
        self.storage
    }
    /// Begin another raw stream with the same setup, retaining all capacity.
    /// The caller must restart its transport and emit headers for that stream.
    pub fn reset(&mut self) {
        self.input.fill(0.0);
        self.bits.reset();
        self.write_half = 1;
        self.filled = 0;
        self.frames = 0;
        self.packets = 0;
        self.finished = false;
        self.failed = false;
    }
    fn ready(&self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if self.finished {
            return Err(Error::Invalid("PCM after encoder finish"));
        }
        Ok(())
    }
    fn validate_frames(&self, frames: usize) -> Result<(), Error> {
        if self
            .frames
            .checked_add(frames as u64)
            .is_none_or(|n| n > self.options.max_frames)
        {
            return Err(Error::LimitExceeded("encoder input frames"));
        }
        Ok(())
    }
    /// Shape, range, and frame-limit errors preserve the current stream.
    /// Transform/allocation/callback errors are terminal until reset.
    pub fn push_interleaved(
        &mut self,
        pcm: &[f32],
        emit: impl FnMut(EncodedPacket<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.ready()?;
        let channels = self.options.channels as usize;
        if !pcm.len().is_multiple_of(channels) {
            return Err(Error::Invalid("interleaved PCM length"));
        }
        let frames = pcm.len() / channels;
        self.validate_frames(frames)?;
        if pcm.iter().any(|x| !x.is_finite() || x.abs() > PCM_LIMIT) {
            return Err(Error::Invalid(
                "encoder PCM must be finite and within [-4, 4]",
            ));
        }
        self.push(
            frames,
            |frame, channel| pcm[frame * channels + channel],
            emit,
        )
    }
    pub fn push_planar(
        &mut self,
        pcm: &[&[f32]],
        emit: impl FnMut(EncodedPacket<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.ready()?;
        if pcm.len() != self.options.channels as usize {
            return Err(Error::Invalid("planar PCM channels"));
        }
        let frames = pcm[0].len();
        if pcm.iter().any(|plane| plane.len() != frames) {
            return Err(Error::Invalid("planar PCM length"));
        }
        self.validate_frames(frames)?;
        if pcm
            .iter()
            .any(|plane| plane.iter().any(|x| !x.is_finite() || x.abs() > PCM_LIMIT))
        {
            return Err(Error::Invalid(
                "encoder PCM must be finite and within [-4, 4]",
            ));
        }
        self.push(frames, |frame, channel| pcm[channel][frame], emit)
    }
    fn push(
        &mut self,
        frames: usize,
        sample: impl Fn(usize, usize) -> f32,
        mut emit: impl FnMut(EncodedPacket<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.failed = true;
        let result = (|| {
            let n = self.options.block_size;
            let half = n / 2;
            let mut consumed = 0;
            while consumed < frames {
                let count = (half - self.filled).min(frames - consumed);
                for ch in 0..self.options.channels as usize {
                    let start = ch * n + self.write_half * half + self.filled;
                    for i in 0..count {
                        self.input[start + i] = sample(consumed + i, ch);
                    }
                }
                self.filled += count;
                self.frames += count as u64;
                consumed += count;
                if self.filled == half {
                    self.encode(false, &mut emit)?;
                }
            }
            Ok(())
        })();
        self.failed = result.is_err();
        result
    }
    /// Pad the analysis tail and emit EOS at the exact submitted frame count.
    /// Idempotent; a zero-length stream emits one zero-energy priming packet.
    pub fn finish(
        &mut self,
        mut emit: impl FnMut(EncodedPacket<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if self.finished {
            return Ok(());
        }
        self.failed = true;
        let result = (|| {
            if self.filled != 0 {
                self.pad();
                self.encode(false, &mut emit)?;
            }
            self.pad();
            self.encode(true, &mut emit)?;
            self.finished = true;
            Ok(())
        })();
        self.failed = result.is_err();
        result
    }
    fn pad(&mut self) {
        let n = self.options.block_size;
        for ch in 0..self.options.channels as usize {
            let start = ch * n + self.write_half * (n / 2);
            self.input[start + self.filled..start + n / 2].fill(0.0);
        }
    }
    fn encode(
        &mut self,
        last: bool,
        emit: &mut impl FnMut(EncodedPacket<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let n = self.options.block_size;
        let half = n / 2;
        let channels = self.options.channels as usize;
        let partitions = half / PARTITION;
        let passes = self.options.residue_passes as usize;
        let levels = 1u32 << (passes * 4);
        let zero = levels / 2;
        for ch in 0..channels {
            let old = ch * n + (self.write_half ^ 1) * half;
            let new = ch * n + self.write_half * half;
            for i in 0..half {
                self.time[i] = self.input[old + i] * self.window[i];
                self.time[half + i] = self.input[new + i] * self.window[half + i];
            }
            self.transform.forward(&self.time, &mut self.spectrum)?;
            let mut peak = 0.0f32;
            for &value in &self.spectrum {
                if !value.is_finite() {
                    return Err(Error::Invalid("nonfinite encoder spectrum"));
                }
                peak = peak.max(value.abs());
            }
            let floor = INVERSE_DB.partition_point(|&x| x < peak / 28.0).min(255);
            let scale = levels as f32 / (64.0 * INVERSE_DB[floor]);
            self.floors[ch] = floor as u8;
            for (out, &value) in self.quantized[ch * half..(ch + 1) * half]
                .iter_mut()
                .zip(&self.spectrum)
            {
                *out = (value * scale + zero as f32)
                    .round()
                    .clamp(0.0, levels as f32 - 1.0) as u16;
            }
            let mut active = false;
            for p in 0..partitions {
                let start = ch * half + p * PARTITION;
                let coded = self.quantized[start..start + PARTITION]
                    .iter()
                    .any(|&x| x as u32 != zero);
                self.classes[ch * partitions + p] = u8::from(coded);
                active |= coded;
            }
            self.nonzero[ch] = active;
        }
        self.bits.reset();
        self.bits.write(0, 1)?; // audio; one short mode has a zero-bit mode number
        for ch in 0..channels {
            self.bits.write(u32::from(self.nonzero[ch]), 1)?;
            if self.nonzero[ch] {
                self.bits.write(self.floors[ch] as u32, 8)?;
                self.bits.write(self.floors[ch] as u32, 8)?;
            }
        }
        for pass in 0..passes {
            let shift = 4 * (passes - 1 - pass);
            for p in 0..partitions {
                if pass == 0 {
                    for ch in 0..channels {
                        if self.nonzero[ch] {
                            self.bits
                                .write(self.classes[ch * partitions + p] as u32, 1)?;
                        }
                    }
                }
                for ch in 0..channels {
                    if !self.nonzero[ch] || self.classes[ch * partitions + p] == 0 {
                        continue;
                    }
                    let start = ch * half + p * PARTITION;
                    let pairs = self.quantized[start..start + PARTITION].as_chunks::<2>().0;
                    if pass == 0 {
                        for pair in pairs {
                            let entry =
                                ((pair[0] >> shift) & 15) | (((pair[1] >> shift) & 15) << 4);
                            let word = FIRST_WORDS[entry as usize];
                            self.bits.write(word as u32, (word >> 12) as u32)?;
                        }
                    } else {
                        for pair in pairs {
                            let entry =
                                ((pair[0] >> shift) & 15) | (((pair[1] >> shift) & 15) << 4);
                            self.bits.write((entry as u8).reverse_bits() as u32, 8)?;
                        }
                    }
                }
            }
        }
        let granule = if last {
            self.frames
        } else {
            self.packets * half as u64
        };
        emit(EncodedPacket {
            data: self.bits.finish_packet()?,
            number: self.packets + 3,
            granule_position: granule,
            last_in_stream: last,
        })?;
        self.packets += 1;
        self.write_half ^= 1;
        self.filled = 0;
        Ok(())
    }
}

/// Native Ogg paginator around PacketEncoder; emits headers on construction.
/// Drop/into_inner never implicitly finish. Write failures are terminal.
pub struct OggEncoder<W: Write> {
    codec: PacketEncoder,
    pages: PacketWriter<W>,
}
impl<W: Write> OggEncoder<W> {
    pub fn new(
        writer: W,
        serial: u32,
        options: EncodeOptions,
        comments: &Comments<'_>,
    ) -> Result<Self, Error> {
        Self::with_page_options(writer, serial, options, comments, WriteOptions::default())
    }
    pub fn with_page_options(
        writer: W,
        serial: u32,
        options: EncodeOptions,
        comments: &Comments<'_>,
        page_options: WriteOptions,
    ) -> Result<Self, Error> {
        let codec = PacketEncoder::new(options)?;
        let limits = Limits {
            max_packet_bytes: options.max_packet_bytes,
            max_comment_bytes: options
                .max_packet_bytes
                .min(Limits::default().max_comment_bytes),
            ..Limits::default()
        };
        let identification = codec.identification.encode()?;
        let comment = comments.encode(&limits)?;
        let mut pages = PacketWriter::with_options(writer, serial, page_options);
        pages.write_packet(&identification, 0, false)?;
        pages.write_packet(&comment, 0, false)?;
        pages.write_packet(codec.setup_packet(), 0, false)?;
        pages.flush_page()?;
        Ok(Self { codec, pages })
    }
    pub fn position(&self) -> u64 {
        self.codec.position()
    }
    pub fn encoder(&self) -> &PacketEncoder {
        &self.codec
    }
    pub fn get_ref(&self) -> &W {
        self.pages.get_ref()
    }
    pub fn into_inner(self) -> W {
        self.pages.into_inner()
    }
    pub fn push_interleaved(&mut self, pcm: &[f32]) -> Result<(), Error> {
        let pages = &mut self.pages;
        self.codec.push_interleaved(pcm, |p| {
            pages
                .write_packet(p.data, p.granule_position, p.last_in_stream)
                .map_err(Error::from)
        })
    }
    pub fn push_planar(&mut self, pcm: &[&[f32]]) -> Result<(), Error> {
        let pages = &mut self.pages;
        self.codec.push_planar(pcm, |p| {
            pages
                .write_packet(p.data, p.granule_position, p.last_in_stream)
                .map_err(Error::from)
        })
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        let pages = &mut self.pages;
        self.codec.finish(|p| {
            pages
                .write_packet(p.data, p.granule_position, p.last_in_stream)
                .map_err(Error::from)
        })
    }
}

fn zeroed<T: Default + Clone>(len: usize) -> Result<Vec<T>, Error> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(len)
        .map_err(|_| Error::Allocation)?;
    values.resize(len, T::default());
    Ok(values)
}
fn setup_packet(block: usize, passes: u8) -> Result<Vec<u8>, Error> {
    let mut bits = BitWriter::new();
    bits.write(passes as u32, 8)?; // one classifier plus one book per stage
    // Classifier {zero, coded}; radix-16^2 VQ books. The leading stage has a
    // nonuniform complete tree; later stages retain ordered eight-bit trees.
    for book in 0..=passes {
        bits.write(0x564342, 24)?;
        bits.write(if book == 0 { 1 } else { 2 }, 16)?;
        bits.write(if book == 0 { 2 } else { 256 }, 24)?;
        if book == 1 {
            bits.write(0, 1)?; // unordered, all entries present
            bits.write(0, 1)?;
            for entry in 0..256 {
                bits.write(first_length(entry) as u32 - 1, 5)?;
            }
        } else {
            bits.write(1, 1)?;
            bits.write(if book == 0 { 0 } else { 7 }, 5)?;
            bits.write(
                if book == 0 { 2 } else { 256 },
                if book == 0 { 2 } else { 9 },
            )?;
        }
        bits.write(u32::from(book != 0), 4)?;
        if book != 0 {
            bits.write(if book == 1 { float_power(5, true) } else { 0 }, 32)?;
            bits.write(float_power(2 - 4 * (book as i32 - 1), false), 32)?;
            bits.write(3, 4)?; // four quantizer bits
            bits.write(0, 1)?;
            for value in 0..16 {
                bits.write(value, 4)?;
            }
        }
    }
    bits.write(0, 6)?;
    bits.write(0, 16)?; // one time transform, type zero
    bits.write(0, 6)?;
    bits.write(1, 16)?; // one floor, type one
    bits.write(0, 5)?;
    bits.write(0, 2)?; // no partitions, multiplier one
    bits.write(block.trailing_zeros() - 1, 4)?;
    bits.write(0, 6)?;
    bits.write(1, 16)?; // one residue, type one
    bits.write(0, 24)?;
    bits.write((block / 2) as u32, 24)?;
    bits.write(PARTITION as u32 - 1, 24)?;
    bits.write(1, 6)?;
    bits.write(0, 8)?;
    bits.write(0, 3)?;
    bits.write(0, 1)?; // zero partition: no stages
    let cascade = (1u32 << passes) - 1;
    bits.write(cascade & 7, 3)?;
    bits.write(u32::from(cascade > 7), 1)?;
    if cascade > 7 {
        bits.write(cascade >> 3, 5)?;
    }
    for book in 1..=passes {
        bits.write(book as u32, 8)?;
    }
    bits.write(0, 6)?;
    bits.write(0, 16)?; // one mapping zero
    bits.write(0, 1)?;
    bits.write(0, 1)?;
    bits.write(0, 2)?; // one submap, no coupling
    bits.write(0, 8)?;
    bits.write(0, 8)?;
    bits.write(0, 8)?;
    bits.write(0, 6)?;
    bits.write(0, 1)?; // one short mode
    bits.write(0, 16)?;
    bits.write(0, 16)?;
    bits.write(0, 8)?;
    bits.write(1, 1)?;
    let body = bits.finish()?;
    let mut packet = Vec::new();
    packet
        .try_reserve_exact(7 + body.len())
        .map_err(|_| Error::Allocation)?;
    packet.extend_from_slice(b"\x05vorbis");
    packet.extend_from_slice(&body);
    Ok(packet)
}
// Exact powers of two in the Vorbis custom float representation:
// mantissa 2^20 times 2^(exponent-788), with the independent sign bit.
fn float_power(power: i32, negative: bool) -> u32 {
    (u32::from(negative) << 31) | (((power + 768) as u32) << 21) | (1 << 20)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Setup, bitpack::BitReader};

    #[test]
    fn first_stage_words_decode_all_entries_with_exact_lengths() {
        let encoder = PacketEncoder::new(EncodeOptions::default()).unwrap();
        let setup = Setup::parse(
            encoder.setup_packet(),
            encoder.identification(),
            &Limits::default(),
        )
        .unwrap();
        let book = &setup.codebooks()[1];
        let mut bits = BitWriter::new();
        for &word in &FIRST_WORDS {
            bits.write(word as u32, (word >> 12) as u32).unwrap();
        }
        let length = bits.position();
        let wire = bits.finish().unwrap();
        let mut bits = BitReader::new(&wire);
        for entry in 0..256 {
            let before = bits.position();
            assert_eq!(book.decode_scalar(&mut bits).unwrap(), entry as u32);
            assert_eq!(bits.position() - before, first_length(entry) as usize);
        }
        assert_eq!(bits.position(), length);
    }
}
