//! File-backed ADPCM with one reusable decoded block and constant-time addressing.
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
    cache_block: Option<u64>,
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
    pub fn from_reader(reader: Reader<R>) -> Result<Self, Error> {
        let codec = Adpcm::from_format(reader.format())?;
        let total_frames = reader
            .fact()
            .or(reader
                .ds64_sample_count()
                .filter(|_| reader.container() == Container::Rf64))
            .ok_or(Error::Invalid("ADPCM requires fact sample count"))?;
        codec.validate_frame_count(
            reader
                .data_chunk()
                .ok_or(Error::Unsupported("segmented ADPCM data"))?
                .size,
            total_frames,
        )?;
        let channels = codec.channels();
        let block_frames = codec.frames_per_block() as u64;
        let mut encoded = Vec::new();
        reserve(&mut encoded, codec.block_bytes())?;
        encoded.resize(codec.block_bytes(), 0);
        let mut decoded = Vec::new();
        reserve(&mut decoded, codec.scratch_samples())?;
        decoded.resize(codec.scratch_samples(), 0);
        Ok(Self {
            reader,
            encoded,
            decoded,
            channels,
            block_frames,
            total_frames,
            position: 0,
            cache_block: None,
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
        let block = frame / self.block_frames;
        let offset = block
            .checked_mul(self.encoded.len() as u64)
            .ok_or(Error::Invalid("encoded seek overflow"))?
            .min(self.reader.data_chunk().unwrap().size);
        self.failed = true;
        self.reader.seek_encoded(offset)?;
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
            let block = self.position / self.block_frames;
            let skip = (self.position % self.block_frames) as usize;
            if self.cache_block != Some(block) {
                self.cache_block = None;
                let offset = block
                    .checked_mul(self.encoded.len() as u64)
                    .ok_or(Error::Invalid("encoded seek overflow"))?;
                let n = self.reader.read_encoded(offset, &mut self.encoded)?;
                let codec = Adpcm::from_format(self.reader.format())?;
                self.cache_frames = codec.decode_block(&self.encoded[..n], &mut self.decoded)?;
                self.cache_block = Some(block);
            }
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
        self.failed = false;
        Ok(done * self.channels)
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
