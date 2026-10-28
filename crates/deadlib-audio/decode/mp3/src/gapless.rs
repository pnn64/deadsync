//! Bounded tagged gapless PCM from non-seekable blocking byte streams.
use crate::{
    DECODER_DELAY, Error, FrameHeader, FrameStatus, Metadata, Pcm, StreamDecoder, StreamOptions,
    StreamStats, reserve,
    tags::{Gapless, VbrHeader},
};
use std::{collections::VecDeque, io::Read};

pub struct GaplessFrame<'a> {
    pub header: FrameHeader,
    pub pcm: Pcm<'a>,
    /// Next emitted sample coordinate, after metadata/delay/padding removal.
    pub position: u64,
    pub raw_position: u64,
    pub byte_offset: u64,
    pub skipped_bytes: u64,
    pub status: FrameStatus,
}
struct Buffered {
    header: FrameHeader,
    data: Vec<f32>,
    samples: usize,
    first: usize,
    raw_position: u64,
    byte_offset: u64,
    skipped_bytes: u64,
    status: FrameStatus,
}
/// Applies the same default tagged mapping as `IndexedDecoder` without seeking
/// or caching a complete file. Stored PCM is bounded by trailing padding plus
/// two physical frames (including the borrowed/partially emitted front frame).
/// Truncated/false declared frame counts are detected at EOF,
/// after a valid prefix may already have been returned.
pub struct GaplessDecoder<R> {
    raw: StreamDecoder<R>,
    queue: VecDeque<Buffered>,
    queued: usize,
    initialized: bool,
    first_header: Option<FrameHeader>,
    frames: u64,
    audio_samples: u64,
    trim: Option<Gapless>,
    declared_frames: Option<u32>,
    skip: u64,
    hold: usize,
    position: u64,
    eof: bool,
    failed: bool,
}
impl<R: Read> GaplessDecoder<R> {
    pub fn new(source: R) -> Self {
        Self::with_options(source, StreamOptions::default())
    }
    pub fn with_options(source: R, options: StreamOptions) -> Self {
        Self {
            raw: StreamDecoder::with_options(source, options),
            queue: VecDeque::new(),
            queued: 0,
            initialized: false,
            first_header: None,
            frames: 0,
            audio_samples: 0,
            trim: None,
            declared_frames: None,
            skip: 0,
            hold: 0,
            position: 0,
            eof: false,
            failed: false,
        }
    }
    pub fn metadata(&self) -> &Metadata {
        self.raw.metadata()
    }
    /// Statistics retain raw physical timing, including the metadata frame.
    pub fn stats(&self) -> StreamStats {
        self.raw.stats()
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn last_recovered_error(&self) -> Option<&Error> {
        self.raw.last_recovered_error()
    }
    pub fn skipped_bytes(&self) -> u64 {
        self.raw.skipped_bytes()
    }
    pub fn discarded_tail_bytes(&self) -> usize {
        self.raw.discarded_tail_bytes()
    }
    pub fn into_parts(self) -> (R, Vec<u8>) {
        self.raw.into_parts()
    }
    pub fn reset(&mut self, source: R) {
        self.raw.reset(source);
        self.queue.clear();
        self.queued = 0;
        self.initialized = false;
        self.first_header = None;
        self.frames = 0;
        self.audio_samples = 0;
        self.trim = None;
        self.declared_frames = None;
        self.skip = 0;
        self.hold = 0;
        self.position = 0;
        self.eof = false;
        self.failed = false;
    }
    fn finish(&mut self) -> Result<(), Error> {
        if let Some(trim) = self.trim {
            if let Some(frames) = self.declared_frames
                && u64::from(frames) != self.frames.saturating_sub(1)
            {
                return Err(Error::Invalid(
                    "Xing frame count differs from physical stream",
                ));
            }
            if self.audio_samples < u64::from(trim.encoder_delay) + u64::from(trim.encoder_padding)
            {
                return Err(Error::Invalid("gapless delay/padding exceeds audio length"));
            }
        }
        // Everything left has been held back as trailing padding. The public
        // default stops at physical EOF; it does not synthesize a DSP drain.
        self.queue.clear();
        self.queued = 0;
        self.eof = true;
        Ok(())
    }
    fn read_more(&mut self) -> Result<(), Error> {
        let Some(out) = self.raw.next_frame()? else {
            return self.finish();
        };
        let samples = out.pcm.samples();
        let mut data = Vec::new();
        reserve(&mut data, samples * out.pcm.channels())?;
        for plane in out.pcm.planes() {
            data.extend_from_slice(plane);
        }
        let mut frame = Buffered {
            header: out.header,
            data,
            samples,
            first: 0,
            raw_position: out.position,
            byte_offset: out.byte_offset,
            skipped_bytes: out.skipped_bytes,
            status: out.status,
        };
        self.frames = self
            .frames
            .checked_add(1)
            .ok_or(Error::LimitExceeded("gapless frame count"))?;
        if !self.initialized {
            self.initialized = true;
            self.first_header = Some(frame.header);
            if let Some(VbrHeader::Xing(tag)) = self.raw.metadata().vbr()? {
                self.trim = tag.gapless;
                self.declared_frames = tag.frames;
                if let Some(trim) = self.trim {
                    self.skip = u64::from(trim.encoder_delay) + DECODER_DELAY;
                    self.hold =
                        u64::from(trim.encoder_padding).saturating_sub(DECODER_DELAY) as usize;
                }
                return Ok(()); // The recognized Xing/Info physical frame.
            }
        }
        if self.trim.is_some() {
            let first = self.first_header.unwrap();
            if !frame.header.compatible(first) || frame.header.channels() != first.channels() {
                return Err(Error::Unsupported("gapless metadata across format changes"));
            }
        }
        self.audio_samples = self
            .audio_samples
            .checked_add(samples as u64)
            .ok_or(Error::LimitExceeded("gapless sample count"))?;
        let skip = self.skip.min(samples as u64);
        self.skip -= skip;
        frame.first = skip as usize;
        if frame.first != samples {
            self.queue.try_reserve(1).map_err(|_| Error::Allocation)?;
            self.queued = self
                .queued
                .checked_add(samples - frame.first)
                .ok_or(Error::LimitExceeded("gapless queued samples"))?;
            self.queue.push_back(frame);
        }
        Ok(())
    }
    fn advance(&mut self) -> Result<Option<(usize, usize, u64)>, Error> {
        while self
            .queue
            .front()
            .is_some_and(|frame| frame.first == frame.samples)
        {
            self.queue.pop_front();
        }
        while !self.eof && self.queued <= self.hold {
            self.read_more()?;
        }
        if self.eof {
            return Ok(None);
        }
        let frame = self.queue.front_mut().unwrap();
        let first = frame.first;
        let count = (self.queued - self.hold).min(frame.samples - first);
        let position = self.position;
        self.position = self
            .position
            .checked_add(count as u64)
            .ok_or(Error::LimitExceeded("gapless emitted position"))?;
        self.queued -= count;
        frame.first += count;
        Ok(Some((first, first + count, position)))
    }
    pub fn next_frame(&mut self) -> Result<Option<GaplessFrame<'_>>, Error> {
        if self.failed {
            return Err(Error::Invalid("gapless decoder requires reset after error"));
        }
        match self.advance() {
            Ok(Some((first, last, position))) => {
                let frame = self.queue.front().unwrap();
                let pcm = Pcm::planar(&frame.data, frame.header.channels(), frame.samples)
                    .slice(first..last)
                    .unwrap();
                Ok(Some(GaplessFrame {
                    header: frame.header,
                    pcm,
                    position,
                    raw_position: frame.raw_position + first as u64,
                    byte_offset: frame.byte_offset,
                    skipped_bytes: frame.skipped_bytes,
                    status: frame.status,
                }))
            }
            Ok(None) => Ok(None),
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
}
