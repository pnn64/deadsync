//! File-backed flat/wavl ADPCM with one reusable encoded/decoded block.
use crate::{Adpcm, ChunkInfo, Container, Error, Format, Limits, Reader, reserve};
use std::io::{Read, Seek};

pub struct AdpcmReader<R> {
    reader: Reader<R>,
    encoded: Vec<u8>,
    decoded: Vec<i16>,
    channels: usize,
    block_frames: u64,
    total_frames: u64,
    position: u64,
    cache_offset: Option<u64>,
    cache_frames: usize,
    failed: bool,
}
impl<R: Read + Seek> AdpcmReader<R> {
    pub fn new(inner: R) -> Result<Self, Error> {
        Self::with_limits(inner, Limits::default())
    }
    pub fn with_limits(inner: R, limits: Limits) -> Result<Self, Error> {
        Self::from_reader(Reader::with_limits(inner, limits)?)
    }
    /// Consume an already scanned container without scanning it again.
    pub fn from_reader(mut reader: Reader<R>) -> Result<Self, Error> {
        let total_frames = reader
            .fact()
            .or(reader
                .ds64_sample_count()
                .filter(|_| reader.container() == Container::Rf64))
            .ok_or(Error::Invalid("ADPCM requires fact sample count"))?;
        let data_size = reader.audio_chunk().size;
        let (channels, block_frames, block_bytes, scratch_samples) = {
            let (format, timeline) = reader.format_and_timeline();
            let codec = Adpcm::from_format(format)?;
            if let Some(index) = timeline {
                let mut frames = 0u64;
                let mut hold = 0;
                for (i, entry) in index.entries.iter_mut().enumerate() {
                    entry.start = frames;
                    entry.hold = hold;
                    if entry.chunk.id == *b"data" {
                        entry.frames = codec.data_frames(entry.chunk.size)?;
                        if entry.frames != 0 {
                            hold = i as u64 + 1;
                        }
                    }
                    frames = frames
                        .checked_add(entry.frames)
                        .ok_or(Error::Limit("ADPCM wavl frame count overflow"))?;
                }
                if total_frames > frames {
                    return Err(Error::Invalid("ADPCM fact exceeds wavl capacity"));
                }
                index.frames = frames;
            } else {
                codec.validate_frame_count(data_size, total_frames)?;
            }
            (
                codec.channels(),
                codec.frames_per_block() as u64,
                codec.block_bytes(),
                codec.scratch_samples(),
            )
        };
        let mut encoded = Vec::new();
        reserve(&mut encoded, block_bytes)?;
        encoded.resize(block_bytes, 0);
        let mut decoded = Vec::new();
        reserve(&mut decoded, scratch_samples)?;
        decoded.resize(scratch_samples, 0);
        Ok(Self {
            reader,
            encoded,
            decoded,
            channels,
            block_frames,
            total_frames,
            position: 0,
            cache_offset: None,
            cache_frames: 0,
            failed: false,
        })
    }
    pub fn format(&self) -> Format<'_> {
        self.reader.format()
    }
    pub fn container(&self) -> Container {
        self.reader.container()
    }
    pub fn chunks(&self) -> &[ChunkInfo] {
        self.reader.chunks()
    }
    /// Retained wavl children, including ancillary metadata and original offsets.
    pub fn wavl_chunks(&self) -> Option<impl Iterator<Item = ChunkInfo> + '_> {
        self.reader.wavl_chunks()
    }
    pub fn frames(&self) -> u64 {
        self.total_frames
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn into_inner(self) -> R {
        self.reader.into_inner()
    }
    pub fn seek_frame(&mut self, frame: u64) -> Result<(), Error> {
        if frame > self.total_frames {
            return Err(Error::Invalid("frame position"));
        }
        let audio = self.reader.audio_chunk();
        let offset = if let Some(index) = self.reader.timeline() {
            if frame == self.total_frames {
                audio.offset + audio.size
            } else {
                let entry = index
                    .at(frame)
                    .ok_or(Error::Invalid("ADPCM wavl frame index"))?;
                if entry.chunk.id == *b"data" {
                    self.block_offset(entry.chunk, frame - entry.start)?
                } else if entry.hold == 0 {
                    audio.offset + 4
                } else {
                    let held = index.entries[(entry.hold - 1) as usize];
                    self.block_offset(held.chunk, held.frames - 1)?
                }
            }
        } else {
            let relative = (frame / self.block_frames)
                .checked_mul(self.encoded.len() as u64)
                .ok_or(Error::Invalid("encoded seek overflow"))?;
            audio.offset + relative.min(audio.size)
        };
        self.failed = true;
        self.reader.seek_audio(offset)?;
        self.position = frame;
        self.failed = false;
        Ok(())
    }
    /// Fill complete interleaved i16 frames; return sample count. A failed call
    /// may have written a prefix and requires a successful seek before reuse.
    pub fn read_i16(&mut self, out: &mut [i16]) -> Result<usize, Error> {
        if self.failed {
            return Err(Error::Invalid(
                "ADPCM reader requires a successful seek after error",
            ));
        }
        let frames =
            (self.total_frames - self.position).min((out.len() / self.channels) as u64) as usize;
        self.failed = true;
        let mut done = 0;
        while done < frames {
            if let Some(index) = self.reader.timeline() {
                let entry = index
                    .at(self.position)
                    .ok_or(Error::Invalid("ADPCM wavl frame index"))?;
                let n = (entry.start + entry.frames - self.position).min((frames - done) as u64)
                    as usize;
                let dst = &mut out[done * self.channels..(done + n) * self.channels];
                if entry.chunk.id == *b"data" {
                    self.read_data(entry.chunk, entry.start, dst)?;
                } else {
                    if entry.hold == 0 {
                        dst.fill(0);
                    } else {
                        let held = index.entries[(entry.hold - 1) as usize];
                        let last = held.frames - 1;
                        self.load_block(held.chunk, last)?;
                        let skip = (last % self.block_frames) as usize;
                        crate::timeline::repeat(
                            dst,
                            &self.decoded[skip * self.channels..(skip + 1) * self.channels],
                        );
                    }
                    self.position += n as u64;
                }
                done += n;
            } else {
                self.read_data(
                    self.reader.audio_chunk(),
                    0,
                    &mut out[..frames * self.channels],
                )?;
                done = frames;
            }
        }
        self.failed = false;
        Ok(done * self.channels)
    }
    fn block_offset(&self, chunk: ChunkInfo, local: u64) -> Result<u64, Error> {
        let relative = (local / self.block_frames)
            .checked_mul(self.encoded.len() as u64)
            .ok_or(Error::Invalid("encoded seek overflow"))?;
        if relative >= chunk.size {
            return Err(Error::Invalid("ADPCM block byte extent"));
        }
        chunk
            .offset
            .checked_add(relative)
            .ok_or(Error::Invalid("encoded seek overflow"))
    }
    fn load_block(&mut self, chunk: ChunkInfo, local: u64) -> Result<(), Error> {
        let offset = self.block_offset(chunk, local)?;
        if self.cache_offset != Some(offset) {
            self.cache_offset = None;
            let available = chunk.size - (offset - chunk.offset);
            let n = available.min(self.encoded.len() as u64) as usize;
            self.reader.read_audio(offset, &mut self.encoded[..n])?;
            let codec = Adpcm::from_format(self.reader.format())?;
            self.cache_frames = codec.decode_block(&self.encoded[..n], &mut self.decoded)?;
            self.cache_offset = Some(offset);
        }
        Ok(())
    }
    fn read_data(&mut self, chunk: ChunkInfo, start: u64, out: &mut [i16]) -> Result<(), Error> {
        let frames = out.len() / self.channels;
        let mut done = 0;
        while done < frames {
            let local = self.position - start;
            let skip = (local % self.block_frames) as usize;
            self.load_block(chunk, local)?;
            let available = self
                .cache_frames
                .checked_sub(skip)
                .ok_or(Error::Invalid("ADPCM block sample extent"))?;
            let n = available.min(frames - done);
            if n == 0 {
                return Err(Error::Invalid("ADPCM block has no available frames"));
            }
            out[done * self.channels..(done + n) * self.channels]
                .copy_from_slice(&self.decoded[skip * self.channels..(skip + n) * self.channels]);
            self.position += n as u64;
            done += n;
        }
        Ok(())
    }
    pub fn read_metadata(&mut self, c: ChunkInfo, out: &mut [u8]) -> Result<usize, Error> {
        // Metadata changes the underlying cursor, but never the cached PCM or
        // logical compressed position. Every uncached block read seeks explicitly.
        let result = self.reader.read_metadata(c, out);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}
