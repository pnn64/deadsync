//! Exact physical indexing, tagged gapless coordinates and stateful seeking.
use crate::{
    Error, Frame, FrameDecoder, FrameHeader, FrameStatus, FreeFormat, Metadata, PacketReader, Pcm,
    ReaderOptions, Recovery, StreamOptions, reserve, tags::VbrHeader,
};
use std::io::{ErrorKind, Read, Seek, SeekFrom};

/// Delay convention used by the native Layer III hybrid/synthesis pipeline.
pub const DECODER_DELAY: u64 = 529;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SampleMode {
    /// Includes all physical frames, decoder delay and padding.
    Raw,
    /// Excludes Xing/Info frames and applies recognized LAME-style delay/padding.
    #[default]
    Gapless,
    /// Encoder delay/padding supplied by the caller when tags are unavailable
    /// or intentionally overridden. Recognized Xing/Info frames are excluded.
    Trim(crate::tags::Gapless),
}
#[derive(Clone, Copy, Debug, Default)]
pub struct FileOptions {
    pub stream: StreamOptions,
    pub samples: SampleMode,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SampleRange {
    start: u64,
    end: u64,
}
impl SampleRange {
    pub fn raw_start(self) -> u64 {
        self.start
    }
    pub fn raw_end(self) -> u64 {
        self.end
    }
    pub fn samples(self) -> u64 {
        self.end - self.start
    }
    pub fn to_raw(self, sample: u64) -> Option<u64> {
        (sample <= self.samples()).then(|| self.start + sample)
    }
    pub fn from_raw(self, sample: u64) -> Option<u64> {
        (self.start..=self.end)
            .contains(&sample)
            .then(|| sample - self.start)
    }
}
/// Flat 32-byte physical index entry; all positions count samples per channel.
#[derive(Clone, Copy, Debug)]
pub struct IndexEntry {
    byte_offset: u64,
    raw_position: u64,
    bytes: u32,
    header: [u8; 4],
    free: u32,
    reset: bool,
}
impl IndexEntry {
    pub fn byte_offset(self) -> u64 {
        self.byte_offset
    }
    pub fn raw_position(self) -> u64 {
        self.raw_position
    }
    pub fn bytes(self) -> usize {
        self.bytes as usize
    }
    pub fn header(self) -> FrameHeader {
        FrameHeader::parse(&self.header).unwrap()
    }
    pub fn discontinuity(self) -> bool {
        self.reset
    }
    fn free(self, limits: crate::Limits) -> Result<Option<FreeFormat>, Error> {
        if self.free == 0 {
            Ok(None)
        } else {
            FreeFormat::new(self.free as usize, limits).map(Some)
        }
    }
    fn payload_bytes(self) -> usize {
        self.bytes() - self.header().main_data_offset()
    }
}
pub struct FrameIndex {
    entries: Vec<IndexEntry>,
    raw_samples: u64,
    origin: u64,
    metadata: Metadata,
}
impl FrameIndex {
    pub fn frames(&self) -> &[IndexEntry] {
        &self.entries
    }
    pub fn raw_samples(&self) -> u64 {
        self.raw_samples
    }
    pub fn origin(&self) -> u64 {
        self.origin
    }
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    fn scan<R: Read + Seek>(mut source: R, options: StreamOptions) -> Result<(R, Self), Error> {
        let origin = source.stream_position()?;
        let mut packets = PacketReader::with_options(
            source,
            ReaderOptions {
                limits: options.decode.limits,
                truncated: options.truncated,
                free_format: options.free_format,
            },
        );
        let mut entries = Vec::new();
        let mut raw_samples = 0u64;
        while let Some(packet) = packets.next_packet()? {
            if entries.len() >= options.decode.limits.index_frames {
                return Err(Error::LimitExceeded("MP3 frame index"));
            }
            if !options.decode.allow_mpeg25 && packet.header.version() == crate::MpegVersion::Mpeg25
            {
                return Err(Error::Unsupported("MPEG-2.5 disabled"));
            }
            let reset = packet.skipped_bytes != 0 && !entries.is_empty();
            if reset && options.recovery == Recovery::Strict {
                return Err(Error::Invalid("lost stream synchronization"));
            }
            if let Err(error) = Frame::parse(
                packet.data,
                packet.free_format,
                options.decode.verify_crc,
                options.decode.limits,
            ) && !recoverable(&error, options.recovery)
            {
                return Err(error);
            }
            reserve(&mut entries, 1)?;
            entries.push(IndexEntry {
                byte_offset: origin
                    .checked_add(packet.byte_offset)
                    .ok_or(Error::LimitExceeded("index byte position"))?,
                raw_position: raw_samples,
                bytes: u32::try_from(packet.data.len())
                    .map_err(|_| Error::LimitExceeded("index frame bytes"))?,
                header: packet.header.to_bytes(),
                free: packet.free_format.map_or(0, |f| f.unpadded_bytes() as u32),
                reset,
            });
            raw_samples = raw_samples
                .checked_add(packet.header.samples() as u64)
                .ok_or(Error::LimitExceeded("index sample position"))?;
        }
        if entries.is_empty() {
            return Err(Error::Invalid("no Layer III frames"));
        }
        let (source, metadata) = packets.into_scanned();
        Ok((
            source,
            Self {
                entries,
                raw_samples,
                origin,
                metadata,
            },
        ))
    }
    fn range(&self, mode: SampleMode) -> Result<SampleRange, Error> {
        if mode == SampleMode::Raw {
            return Ok(SampleRange {
                start: 0,
                end: self.raw_samples,
            });
        }
        let first = self.entries[0].header();
        let tag = match self.metadata.vbr()? {
            Some(VbrHeader::Xing(tag)) => Some(tag),
            _ => None,
        };
        let start = if tag.is_some() {
            first.samples() as u64
        } else {
            0
        };
        let gapless = match mode {
            SampleMode::Trim(gapless) => Some(gapless),
            _ => tag.as_ref().and_then(|tag| tag.gapless),
        };
        let Some(gapless) = gapless else {
            return Ok(SampleRange {
                start,
                end: self.raw_samples,
            });
        };
        if mode == SampleMode::Gapless
            && let Some(frames) = tag.and_then(|tag| tag.frames)
            && frames as u64 != self.entries.len() as u64 - 1
        {
            return Err(Error::Invalid(
                "Xing frame count differs from physical index",
            ));
        }
        if self.entries.iter().any(|entry| {
            let h = entry.header();
            !h.compatible(first) || h.channels() != first.channels()
        }) {
            return Err(Error::Unsupported("gapless metadata across format changes"));
        }
        let samples = (self.raw_samples - start)
            .checked_sub(gapless.encoder_delay as u64 + gapless.encoder_padding as u64)
            .ok_or(Error::Invalid("gapless delay/padding exceeds audio length"))?;
        let start = start + gapless.encoder_delay as u64 + DECODER_DELAY;
        let end = start
            .checked_add(samples)
            .ok_or(Error::LimitExceeded("gapless sample position"))?;
        // A zero/short padding field occurs in real non-LAME files (e.g.
        // FFmpeg/Shine). Public file decoders stop at the last physical sample;
        // that field does not request synthesized samples after physical EOF.
        // Caller-supplied Trim can explicitly request reconstruction of the
        // finite DSP tail when its encoder timing is known independently.
        let end = if mode == SampleMode::Gapless {
            end.min(self.raw_samples)
        } else {
            end
        };
        Ok(SampleRange {
            start: start.min(end),
            end,
        })
    }
}
fn recoverable(error: &Error, recovery: Recovery) -> bool {
    recovery == Recovery::Conceal
        && matches!(
            error,
            Error::Invalid(_)
                | Error::Truncated
                | Error::CrcMismatch { .. }
                | Error::MissingReservoir { .. }
        )
}
pub struct FileFrame<'a> {
    pub header: FrameHeader,
    pub pcm: Pcm<'a>,
    pub position: u64,
    pub raw_position: u64,
    /// None for an end-of-stream DSP drain rather than a physical MP3 frame.
    pub byte_offset: Option<u64>,
    pub frame_index: Option<usize>,
    pub status: FrameStatus,
}
struct Ready {
    header: FrameHeader,
    first: usize,
    last: usize,
    position: u64,
    raw_position: u64,
    index: Option<usize>,
    status: FrameStatus,
}
/// Builds an exact compressed-frame index, without caching decoded PCM.
/// The source must remain unchanged after construction. Opening scans framing,
/// side information and optional CRC; entropy errors are checked when decoded.
pub struct IndexedDecoder<R> {
    source: R,
    index: FrameIndex,
    range: SampleRange,
    options: FileOptions,
    decoder: FrameDecoder,
    input: Vec<u8>,
    next: usize,
    position: u64,
    drained: bool,
    failed: bool,
}
impl<R: Read + Seek> IndexedDecoder<R> {
    pub fn new(source: R) -> Result<Self, Error> {
        Self::with_options(source, FileOptions::default())
    }
    pub fn with_options(source: R, options: FileOptions) -> Result<Self, Error> {
        let (source, index) = FrameIndex::scan(source, options.stream)?;
        let range = index.range(options.samples)?;
        let mut result = Self {
            source,
            index,
            range,
            options,
            decoder: FrameDecoder::with_options(options.stream.decode),
            input: Vec::new(),
            next: 0,
            position: 0,
            drained: false,
            failed: false,
        };
        result.seek_sample(0)?;
        Ok(result)
    }
    pub fn index(&self) -> &FrameIndex {
        &self.index
    }
    pub fn metadata(&self) -> &Metadata {
        &self.index.metadata
    }
    pub fn sample_range(&self) -> SampleRange {
        self.range
    }
    pub fn samples(&self) -> u64 {
        self.range.samples()
    }
    /// Position of the next sample returned, in the selected coordinate mode.
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn into_inner(self) -> R {
        self.source
    }
    fn load(&mut self, n: usize) -> Result<IndexEntry, Error> {
        let entry = self.index.entries[n];
        self.source.seek(SeekFrom::Start(entry.byte_offset))?;
        self.input.clear();
        reserve(&mut self.input, entry.bytes())?;
        self.input.resize(entry.bytes(), 0);
        self.source.read_exact(&mut self.input).map_err(|error| {
            if error.kind() == ErrorKind::UnexpectedEof {
                Error::Truncated
            } else {
                error.into()
            }
        })?;
        if self.input[..4] != entry.header {
            return Err(Error::Invalid("source header changed after indexing"));
        }
        Ok(entry)
    }
    fn decode(&mut self, n: usize) -> Result<FrameStatus, Error> {
        let entry = self.load(n)?;
        if entry.reset {
            self.decoder.reset();
        }
        if let Err(error) = self
            .decoder
            .decode(&self.input, entry.free(self.options.stream.decode.limits)?)
        {
            if !recoverable(&error, self.options.stream.recovery) {
                return Err(error);
            }
            self.decoder
                .conceal(&self.input[entry.header().main_data_offset()..])?;
            return Ok(if matches!(error, Error::MissingReservoir { .. }) {
                FrameStatus::MissingReservoir
            } else {
                FrameStatus::Corrupt
            });
        }
        Ok(FrameStatus::Decoded)
    }
    fn seek_inner(&mut self, sample: u64) -> Result<(), Error> {
        self.decoder.reset();
        self.drained = false;
        if sample == self.samples() {
            self.next = self.index.entries.len();
            self.position = sample;
            self.drained = true;
            return Ok(());
        }
        let raw = self.range.start + sample;
        let target = self
            .index
            .entries
            .partition_point(|e| e.raw_position <= raw)
            .saturating_sub(1);
        let mut start = target;
        let mut granules = 0;
        while start > 0 && !self.index.entries[start].reset && granules < 2 {
            start -= 1;
            granules += self.index.entries[start].header().granules();
        }
        let mut seed = start;
        let mut retained = 0usize;
        while seed > 0 && !self.index.entries[seed].reset && retained < 511 {
            seed -= 1;
            retained = retained.saturating_add(self.index.entries[seed].payload_bytes());
        }
        for n in seed..start {
            let entry = self.load(n)?;
            self.decoder
                .prime_reservoir(&self.input[entry.header().main_data_offset()..])?;
        }
        for n in start..target {
            self.decode(n)?;
        }
        self.next = target;
        self.position = sample;
        Ok(())
    }
    /// Exact sample seek, including EOF. Invalid positions leave state unchanged.
    /// A successful seek rebuilds reservoir and finite DSP history; it also
    /// recovers the decoder after a prior decoding error.
    pub fn seek_sample(&mut self, sample: u64) -> Result<(), Error> {
        if sample > self.samples() {
            return Err(Error::Invalid("seek exceeds sample count"));
        }
        match self.seek_inner(sample) {
            Ok(()) => {
                self.failed = false;
                Ok(())
            }
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
    fn advance(&mut self) -> Result<Option<Ready>, Error> {
        if self.position == self.samples() {
            return Ok(None);
        }
        let wanted = self.range.start + self.position;
        while self.next < self.index.entries.len() {
            let n = self.next;
            let entry = self.index.entries[n];
            let status = self.decode(n)?;
            self.next += 1;
            let header = entry.header();
            let raw = wanted.max(entry.raw_position);
            let end = self
                .range
                .end
                .min(entry.raw_position + header.samples() as u64);
            if raw < end {
                let first = (raw - entry.raw_position) as usize;
                let last = (end - entry.raw_position) as usize;
                let position = self.position;
                self.position += end - raw;
                return Ok(Some(Ready {
                    header,
                    first,
                    last,
                    position,
                    raw_position: raw,
                    index: Some(n),
                    status,
                }));
            }
        }
        if !self.drained && self.range.end > self.index.raw_samples {
            let header = self.index.entries.last().unwrap().header();
            self.decoder.drain(header)?;
            self.drained = true;
            let raw = wanted.max(self.index.raw_samples);
            let end = self
                .range
                .end
                .min(self.index.raw_samples.saturating_add(576));
            if raw < end {
                let position = self.position;
                self.position += end - raw;
                return Ok(Some(Ready {
                    header,
                    first: (raw - self.index.raw_samples) as usize,
                    last: (end - self.index.raw_samples) as usize,
                    position,
                    raw_position: raw,
                    index: None,
                    status: FrameStatus::Decoded,
                }));
            }
        }
        Err(Error::Invalid("sample range exceeds decoded output"))
    }
    pub fn next_frame(&mut self) -> Result<Option<FileFrame<'_>>, Error> {
        if self.failed {
            return Err(Error::Invalid("indexed decoder requires seek after error"));
        }
        match self.advance() {
            Ok(Some(out)) => {
                let pcm = if out.index.is_some() {
                    self.decoder.pcm(out.header)
                } else {
                    self.decoder.drained_pcm(out.header)
                };
                Ok(Some(FileFrame {
                    header: out.header,
                    pcm: pcm.slice(out.first..out.last).unwrap(),
                    position: out.position,
                    raw_position: out.raw_position,
                    byte_offset: out.index.map(|n| self.index.entries[n].byte_offset),
                    frame_index: out.index,
                    status: out.status,
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
