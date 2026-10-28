//! Forward-only Microsoft/IMA block decoding with two reusable block buffers.
use crate::{Adpcm, ChunkPayload, Container, Error, Format, ForwardReader, Limits, reserve};
use std::io::Read;

enum AudioState {
    Flat,
    Next,
    Data(u64),
    Silence(u64),
    End,
}

pub struct AdpcmForwardReader<R> {
    reader: ForwardReader<R>,
    encoded: Vec<u8>,
    decoded: Vec<i16>,
    channels: usize,
    frames: u64,
    position: u64,
    cache_frames: usize,
    cache_position: usize,
    state: AudioState,
    capacity: u64,
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
    /// Without leading fact, a raw RF64 ds64 count is only a hint: a different
    /// ordinary trailing fact causes finish to reject the inferred count.
    /// The corresponding BW64 dummy words never provide a duration.
    pub fn from_reader(reader: ForwardReader<R>) -> Result<Self, Error> {
        let frames = reader
            .fact()
            .or(reader
                .ds64_sample_count()
                .filter(|_| reader.container() == Container::Rf64))
            .ok_or(Error::Unsupported(
                "forward ADPCM needs a leading duration or explicit frame count",
            ))?;
        Self::from_reader_with_frames(reader, frames)
    }
    pub fn from_reader_with_frames(reader: ForwardReader<R>, frames: u64) -> Result<Self, Error> {
        if reader.data_bytes_read() != 0 {
            return Err(Error::Invalid("forward ADPCM requires unread data"));
        }
        // Only an encountered fact determines whether ds64 replaces its count.
        // An ordinary trailing fact may override any unused raw ds64 value.
        if let Some(actual) = reader.fact()
            && actual != frames
        {
            return Err(Error::Invalid("forward ADPCM duration mismatch"));
        }
        let codec = Adpcm::from_format(reader.format())?;
        let state = if let Some(data) = reader.data_chunk() {
            codec.validate_frame_count(data.size, frames)?;
            AudioState::Flat
        } else {
            AudioState::Next
        };
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
            state,
            capacity: 0,
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
        self.read_i16_with_metadata(out, |_, _| Ok(()))
    }
    /// Visit ancillary children encountered between compressed wavl segments.
    /// Metadata callback failures poison this forward reader like codec/I/O errors.
    pub fn read_i16_with_metadata(
        &mut self,
        out: &mut [i16],
        mut visit: impl FnMut(crate::ChunkInfo, &mut ChunkPayload<'_, R>) -> Result<(), Error>,
    ) -> Result<usize, Error> {
        if self.failed {
            return Err(Error::Invalid("forward ADPCM reader unusable after error"));
        }
        let frames = (self.frames - self.position).min((out.len() / self.channels) as u64) as usize;
        let mut done = 0;
        self.failed = true;
        while done < frames {
            if self.cache_position == self.cache_frames {
                match self.state {
                    AudioState::Flat => {
                        let n = self.reader.read_data(&mut self.encoded)?;
                        self.decode(n)?;
                    }
                    AudioState::Data(left) if left != 0 => {
                        let n = left.min(self.encoded.len() as u64) as usize;
                        let n = self.reader.read_encoded_child(&mut self.encoded[..n])?;
                        if n == 0 {
                            return Err(Error::Truncated);
                        }
                        self.state = AudioState::Data(left - n as u64);
                        self.decode(n)?;
                    }
                    AudioState::Silence(left) if left != 0 => {
                        let n = left.min((frames - done) as u64) as usize;
                        let dst = &mut out[done * self.channels..(done + n) * self.channels];
                        if self.cache_frames == 0 {
                            dst.fill(0);
                        } else {
                            let start = (self.cache_frames - 1) * self.channels;
                            crate::timeline::repeat(
                                dst,
                                &self.decoded[start..start + self.channels],
                            );
                        }
                        self.state = AudioState::Silence(left - n as u64);
                        self.position += n as u64;
                        done += n;
                        continue;
                    }
                    AudioState::End => {
                        return Err(Error::Invalid("ADPCM fact exceeds wavl capacity"));
                    }
                    _ => {
                        self.next_segment(&mut visit)?;
                        continue;
                    }
                }
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
    fn decode(&mut self, n: usize) -> Result<(), Error> {
        let codec = Adpcm::from_format(self.reader.format())?;
        self.cache_frames = codec.decode_block(&self.encoded[..n], &mut self.decoded)?;
        self.cache_position = 0;
        Ok(())
    }
    fn next_segment(
        &mut self,
        visit: impl FnMut(crate::ChunkInfo, &mut ChunkPayload<'_, R>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let (state, frames) = match self.reader.next_encoded_segment(visit)? {
            Some(crate::forward::EncodedSegment::Data(bytes)) => (
                AudioState::Data(bytes),
                Adpcm::from_format(self.reader.format())?.data_frames(bytes)?,
            ),
            Some(crate::forward::EncodedSegment::Silence(frames)) => {
                (AudioState::Silence(frames), frames)
            }
            None => (AudioState::End, 0),
        };
        self.capacity = self
            .capacity
            .checked_add(frames)
            .ok_or(Error::Limit("ADPCM wavl frame count overflow"))?;
        self.state = state;
        Ok(())
    }
    pub fn finish(self) -> Result<R, Error> {
        self.finish_with_metadata(|_, _| Ok(()))
    }
    pub fn finish_with_metadata(
        mut self,
        mut visit: impl FnMut(crate::ChunkInfo, &mut ChunkPayload<'_, R>) -> Result<(), Error>,
    ) -> Result<R, Error> {
        if self.failed {
            return Err(Error::Invalid("forward ADPCM reader unusable after error"));
        }
        if !matches!(self.state, AudioState::Flat) {
            while !matches!(self.state, AudioState::End) {
                self.next_segment(&mut visit)?;
            }
            if self.frames > self.capacity {
                return Err(Error::Invalid("ADPCM fact exceeds wavl capacity"));
            }
        }
        self.reader.finish_checked(Some(self.frames), visit)
    }
}
