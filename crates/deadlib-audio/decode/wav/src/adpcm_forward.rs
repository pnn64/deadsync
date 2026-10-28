//! Forward-only Microsoft/IMA block decoding with two reusable block buffers.
use crate::{Adpcm, ChunkPayload, Container, Error, Format, ForwardReader, Limits, reserve};
use std::io::Read;

pub struct AdpcmForwardReader<R> {
    reader: ForwardReader<R>,
    encoded: Vec<u8>,
    decoded: Vec<i16>,
    channels: usize,
    frames: u64,
    position: u64,
    cache_frames: usize,
    cache_position: usize,
    failed: bool,
}
impl<R: Read> AdpcmForwardReader<R> {
    pub fn new(inner: R) -> Result<Self, Error> {
        Self::from_reader(ForwardReader::new(inner)?)
    }
    pub fn with_limits(inner: R, limits: Limits) -> Result<Self, Error> {
        Self::from_reader(ForwardReader::with_limits(inner, limits)?)
    }
    /// Exact forward decoding needs a duration known before audio. For a
    /// trailing fact, supply the externally known count via from_reader_with_frames;
    /// finish checks that the encountered fact/ds64 agrees with that count.
    pub fn from_reader(reader: ForwardReader<R>) -> Result<Self, Error> {
        let frames = reader
            .fact()
            .or(reader.ds64_sample_count())
            .ok_or(Error::Unsupported(
                "forward ADPCM needs a leading duration or explicit frame count",
            ))?;
        Self::from_reader_with_frames(reader, frames)
    }
    pub fn from_reader_with_frames(reader: ForwardReader<R>, frames: u64) -> Result<Self, Error> {
        if reader.data_bytes_read() != 0 {
            return Err(Error::Invalid("forward ADPCM requires unread data"));
        }
        if let Some(actual) = reader.fact().or(reader.ds64_sample_count())
            && actual != frames
        {
            return Err(Error::Invalid("forward ADPCM duration mismatch"));
        }
        let codec = Adpcm::from_format(reader.format())?;
        codec.validate_frame_count(
            reader
                .data_chunk()
                .ok_or(Error::Unsupported("segmented ADPCM data"))?
                .size,
            frames,
        )?;
        let mut encoded = Vec::new();
        reserve(&mut encoded, codec.block_bytes())?;
        encoded.resize(codec.block_bytes(), 0);
        let mut decoded = Vec::new();
        reserve(&mut decoded, codec.scratch_samples())?;
        decoded.resize(codec.scratch_samples(), 0);
        let channels = codec.channels();
        Ok(Self {
            reader,
            encoded,
            decoded,
            channels,
            frames,
            position: 0,
            cache_frames: 0,
            cache_position: 0,
            failed: false,
        })
    }
    pub fn container(&self) -> Container {
        self.reader.container()
    }
    pub fn format(&self) -> Format<'_> {
        self.reader.format()
    }
    pub fn frames(&self) -> u64 {
        self.frames
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn into_inner(self) -> R {
        self.reader.into_inner()
    }
    /// Fill complete interleaved frames, returning sample count. A codec/I/O
    /// error poisons this forward stream; output may contain a decoded prefix.
    pub fn read_i16(&mut self, out: &mut [i16]) -> Result<usize, Error> {
        if self.failed {
            return Err(Error::Invalid("forward ADPCM reader unusable after error"));
        }
        let frames = (self.frames - self.position).min((out.len() / self.channels) as u64) as usize;
        let mut done = 0;
        self.failed = true;
        while done < frames {
            if self.cache_position == self.cache_frames {
                let n = self.reader.read_data(&mut self.encoded)?;
                let codec = Adpcm::from_format(self.reader.format())?;
                self.cache_frames = codec.decode_block(&self.encoded[..n], &mut self.decoded)?;
                self.cache_position = 0;
            }
            let n = (self.cache_frames - self.cache_position).min(frames - done);
            if n == 0 {
                return Err(Error::Invalid("ADPCM block has no available frames"));
            }
            out[done * self.channels..(done + n) * self.channels].copy_from_slice(
                &self.decoded[self.cache_position * self.channels
                    ..(self.cache_position + n) * self.channels],
            );
            self.cache_position += n;
            self.position += n as u64;
            done += n;
        }
        self.failed = false;
        Ok(done * self.channels)
    }
    pub fn finish(self) -> Result<R, Error> {
        self.finish_with_metadata(|_, _| Ok(()))
    }
    pub fn finish_with_metadata(
        self,
        visit: impl FnMut(crate::ChunkInfo, &mut ChunkPayload<'_, R>) -> Result<(), Error>,
    ) -> Result<R, Error> {
        if self.failed {
            return Err(Error::Invalid("forward ADPCM reader unusable after error"));
        }
        self.reader.finish_checked(Some(self.frames), visit)
    }
}
