//! Native framing, streaming, whole-stream integrity, and sample-accurate seeks.
use crate::{
    Error, FrameDecoder, FrameHeader, Limits, Metadata, Pcm, StreamInfo, checksum::Md5, reserve,
};
use std::io::{Read, Seek, SeekFrom};

#[derive(Clone, Copy, Debug)]
pub struct DecodeOptions {
    pub limits: Limits,
    pub verify_md5: bool,
    /// Accept pre-2007 variable blocks detected through STREAMINFO (RFC B.1).
    pub allow_legacy_variable_blocks: bool,
}
impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            verify_md5: true,
            allow_legacy_variable_blocks: true,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Integrity {
    /// No complete sequential pass has finished yet.
    Pending,
    /// STREAMINFO had an MD5 and it matched a full sequential pass.
    Verified,
    /// Frame CRCs and sample counts were checked; no MD5 was supplied or requested.
    Unavailable,
    /// Seeking prevented whole-stream MD5 verification. Frame CRCs remain checked.
    Partial,
}
#[derive(Debug)]
pub struct StreamFrame<'a> {
    pub start_sample: u64,
    pub header: FrameHeader,
    pub pcm: Pcm<'a>,
}

#[derive(Clone)]
struct Sequence {
    info: StreamInfo,
    frames: u64,
    samples: u64,
    variable: Option<bool>,
    fixed_size: usize,
    short: bool,
    digest: Option<Md5>,
    integrity: Integrity,
    legacy_variable: bool,
}
impl Sequence {
    fn new(info: StreamInfo, options: DecodeOptions) -> Self {
        Self {
            info,
            frames: 0,
            samples: 0,
            variable: None,
            fixed_size: 0,
            short: false,
            digest: (options.verify_md5 && info.md5 != [0; 16]).then(Md5::new),
            integrity: Integrity::Pending,
            legacy_variable: options.allow_legacy_variable_blocks
                && info.min_block_size != info.max_block_size,
        }
    }
    fn accept(&mut self, header: FrameHeader, pcm: Pcm<'_>) -> Result<u64, Error> {
        if self.short {
            return Err(Error::Invalid("short block before final frame"));
        }
        if self
            .variable
            .is_some_and(|v| v != header.variable_block_size)
        {
            return Err(Error::Invalid("blocking strategy changed"));
        }
        let variable = header.variable_block_size || self.legacy_variable;
        let expected = if variable { self.samples } else { self.frames };
        if header.number != expected {
            return Err(Error::Invalid("non-contiguous frame numbering"));
        }
        if header.block_size > self.info.max_block_size as usize {
            return Err(Error::Invalid("block exceeds STREAMINFO maximum"));
        }
        if self.frames == 0 {
            self.fixed_size = header.block_size;
        }
        if !variable && header.block_size > self.fixed_size {
            return Err(Error::Invalid("fixed block size increased"));
        }
        let end = self.samples + header.block_size as u64;
        if self.info.total_samples != 0 && end > self.info.total_samples {
            return Err(Error::Invalid("samples exceed STREAMINFO total"));
        }
        self.short = header.block_size < self.info.min_block_size as usize
            || (!variable && header.block_size < self.fixed_size);
        self.variable = Some(header.variable_block_size);
        if let Some(digest) = &mut self.digest {
            pcm.update_md5(digest, header.bits_per_sample);
        }
        let start = self.samples;
        self.samples = end;
        self.frames += 1;
        Ok(start)
    }
    fn finish(&mut self) -> Result<(), Error> {
        if self.info.total_samples != 0 && self.samples != self.info.total_samples {
            return Err(Error::Invalid("STREAMINFO sample count mismatch"));
        }
        if let Some(digest) = self.digest.take() {
            if digest.finalize() != self.info.md5 {
                return Err(Error::Checksum("PCM MD5"));
            }
            self.integrity = Integrity::Verified;
        } else if self.integrity == Integrity::Pending {
            self.integrity = Integrity::Unavailable;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Landmark {
    offset: usize,
    sample: u64,
    frame: u64,
    fixed_size: usize,
}

/// Zero-copy native input with a lazy frame index. Seeking discovers only frames
/// up to the target; subsequent seeks reuse those landmarks and decode one frame.
pub struct Decoder<'a> {
    data: &'a [u8],
    metadata: Metadata<'a>,
    options: DecodeOptions,
    decoder: FrameDecoder,
    sequence: Sequence,
    offset: usize,
    index: Vec<Landmark>,
    skip: usize,
    done: bool,
    failed: bool,
}
impl<'a> Decoder<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, Error> {
        Self::with_options(data, DecodeOptions::default())
    }
    pub fn with_options(data: &'a [u8], options: DecodeOptions) -> Result<Self, Error> {
        let metadata = Metadata::parse(data, options.limits)?;
        let info = metadata.stream_info;
        let offset = metadata.audio_offset;
        Ok(Self {
            data,
            metadata,
            options,
            decoder: FrameDecoder::new(Some(info))?,
            sequence: Sequence::new(info, options),
            offset,
            index: Vec::new(),
            skip: 0,
            done: false,
            failed: false,
        })
    }
    pub fn metadata(&self) -> &Metadata<'a> {
        &self.metadata
    }
    pub fn integrity(&self) -> Integrity {
        self.sequence.integrity
    }
    pub fn position(&self) -> u64 {
        self.sequence.samples + self.skip as u64
    }
    pub fn read_next(&mut self) -> Result<Option<StreamFrame<'_>>, Error> {
        if self.failed {
            return Err(Error::Invalid("decoder is terminal after error"));
        }
        match self.next_inner() {
            Ok(Some((header, start, skip))) => Ok(Some(StreamFrame {
                start_sample: start + skip as u64,
                header,
                pcm: self.decoder.pcm(header).trim_start(skip),
            })),
            Ok(None) => Ok(None),
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
    fn next_inner(&mut self) -> Result<Option<(FrameHeader, u64, usize)>, Error> {
        if self.done {
            return Ok(None);
        }
        if self.offset == self.data.len() {
            self.sequence.finish()?;
            self.done = true;
            return Ok(None);
        }
        let available = &self.data[self.offset..];
        let frame = self
            .decoder
            .decode(&available[..available.len().min(self.options.limits.frame_bytes)])
            .map_err(|e| {
                if e == Error::Truncated && available.len() >= self.options.limits.frame_bytes {
                    Error::LimitExceeded("frame bytes")
                } else {
                    e
                }
            })?;
        let header = frame.header;
        let start = self.sequence.accept(header, frame.pcm)?;
        if self.index.last().is_none_or(|p| p.offset < self.offset) {
            reserve(&mut self.index, 1)?;
            self.index.push(Landmark {
                offset: self.offset,
                sample: start,
                frame: self.sequence.frames - 1,
                fixed_size: self.sequence.fixed_size,
            });
        }
        self.offset += frame.bytes;
        let skip = std::mem::take(&mut self.skip);
        Ok(Some((header, start, skip)))
    }
    /// Next returned PCM starts exactly at `sample`. Seeking to EOF is allowed.
    /// A seek to zero restarts MD5 verification; other seeks mark it partial.
    pub fn seek(&mut self, sample: u64) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("decoder is terminal after error"));
        }
        if self.sequence.info.total_samples != 0 && sample > self.sequence.info.total_samples {
            return Err(Error::Invalid("seek beyond end"));
        }
        let point = self.index.partition_point(|p| p.sample <= sample);
        let landmark = if point == 0 {
            Landmark {
                offset: self.metadata.audio_offset,
                sample: 0,
                frame: 0,
                fixed_size: 0,
            }
        } else {
            self.index[point - 1]
        };
        self.sequence = Sequence::new(self.metadata.stream_info, self.options);
        self.sequence.samples = landmark.sample;
        self.sequence.frames = landmark.frame;
        self.sequence.fixed_size = landmark.fixed_size;
        if sample != 0 {
            self.sequence.digest = None;
            self.sequence.integrity = Integrity::Partial;
        }
        self.offset = landmark.offset;
        self.done = false;
        self.skip = 0;
        while self.offset < self.data.len() {
            let header =
                FrameHeader::parse(&self.data[self.offset..], Some(&self.metadata.stream_info))?;
            if sample < self.sequence.samples + header.block_size as u64 {
                self.skip = (sample - self.sequence.samples) as usize;
                return Ok(());
            }
            self.read_next()?;
        }
        if self.sequence.samples != sample {
            return Err(Error::Invalid("seek beyond end"));
        }
        self.sequence.finish()?;
        self.done = true;
        Ok(())
    }
}

/// Bounded synchronous streaming reader. Wrap files/sockets in `BufReader` to
/// amortize metadata reads. PCM remains borrowed until the next call.
pub struct Reader<R> {
    source: R,
    metadata: Vec<u8>,
    options: DecodeOptions,
    decoder: FrameDecoder,
    sequence: Sequence,
    buffer: Vec<u8>,
    consumed: usize,
    offset: u64,
    eof: bool,
    done: bool,
    failed: bool,
}
impl<R: Read> Reader<R> {
    pub fn new(source: R) -> Result<Self, Error> {
        Self::with_options(source, DecodeOptions::default())
    }
    pub fn with_options(mut source: R, options: DecodeOptions) -> Result<Self, Error> {
        let mut signature = [0; 4];
        read_exact(&mut source, &mut signature)?;
        if &signature != b"fLaC" {
            return Err(Error::Invalid("native signature"));
        }
        let mut metadata = signature.to_vec();
        let mut count = 0;
        loop {
            if count >= options.limits.metadata_blocks {
                return Err(Error::LimitExceeded("metadata blocks"));
            }
            let mut head = [0; 4];
            read_exact(&mut source, &mut head)?;
            let len = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
            if metadata.len() - 4 + len + 4 > options.limits.metadata_bytes {
                return Err(Error::LimitExceeded("metadata bytes"));
            }
            reserve(&mut metadata, len + 4)?;
            metadata.extend_from_slice(&head);
            let start = metadata.len();
            metadata.resize(start + len, 0);
            read_exact(&mut source, &mut metadata[start..])?;
            count += 1;
            if head[0] & 128 != 0 {
                break;
            }
        }
        let info = Metadata::parse(&metadata, options.limits)?.stream_info;
        let offset = metadata.len() as u64;
        Ok(Self {
            source,
            metadata,
            options,
            decoder: FrameDecoder::new(Some(info))?,
            sequence: Sequence::new(info, options),
            buffer: Vec::new(),
            consumed: 0,
            offset,
            eof: false,
            done: false,
            failed: false,
        })
    }
    pub fn stream_info(&self) -> StreamInfo {
        self.sequence.info
    }
    pub fn metadata(&self) -> Result<Metadata<'_>, Error> {
        Metadata::parse(&self.metadata, self.options.limits)
    }
    pub fn integrity(&self) -> Integrity {
        self.sequence.integrity
    }
    pub fn position(&self) -> u64 {
        self.sequence.samples
    }
    pub fn into_inner(self) -> R {
        self.source
    }
    pub fn read_next(&mut self) -> Result<Option<StreamFrame<'_>>, Error> {
        if self.failed {
            return Err(Error::Invalid("reader is terminal after error"));
        }
        match self.next_inner() {
            Ok(Some((header, start))) => Ok(Some(StreamFrame {
                start_sample: start,
                header,
                pcm: self.decoder.pcm(header),
            })),
            Ok(None) => Ok(None),
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
    fn next_inner(&mut self) -> Result<Option<(FrameHeader, u64)>, Error> {
        if self.done {
            return Ok(None);
        }
        if self.consumed == self.buffer.len() {
            self.buffer.clear();
            self.consumed = 0;
        }
        loop {
            if self.buffer.is_empty() && self.eof {
                self.sequence.finish()?;
                self.done = true;
                return Ok(None);
            }
            match self.decoder.decode_buffer(&self.buffer[self.consumed..]) {
                Ok(frame) => {
                    let start = self.sequence.accept(frame.header, frame.pcm)?;
                    self.consumed += frame.bytes;
                    self.offset += frame.bytes as u64;
                    return Ok(Some((frame.header, start)));
                }
                Err(Error::Truncated) if !self.eof => (),
                Err(error) => return Err(error),
            }
            // Compact only when more input is needed, rather than copying the
            // remaining buffer after every small frame. Checkpoints are bit
            // offsets within the unchanged current-frame prefix.
            if self.consumed != 0 {
                self.buffer.copy_within(self.consumed.., 0);
                self.buffer.truncate(self.buffer.len() - self.consumed);
                self.consumed = 0;
            }
            // Doubling avoids quadratic restart work on very large frames. A
            // short Read (including one-byte readers) is accumulated before retry.
            let target = self
                .buffer
                .len()
                .saturating_mul(2)
                .max(8192)
                .min(self.options.limits.frame_bytes);
            if target <= self.buffer.len() {
                return Err(Error::LimitExceeded("frame bytes"));
            }
            let additional = target - self.buffer.len();
            reserve(&mut self.buffer, additional)?;
            let mut scratch = [0u8; 8192];
            while self.buffer.len() < target {
                let n = (target - self.buffer.len()).min(scratch.len());
                match self.source.read(&mut scratch[..n]) {
                    Ok(0) => {
                        self.eof = true;
                        break;
                    }
                    Ok(n) => self.buffer.extend_from_slice(&scratch[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SeekOptions {
    pub decode: DecodeOptions,
    pub max_index_frames: usize,
    /// Treat SEEKTABLE entries as untrusted hints, checking their target frames.
    pub use_seektable: bool,
}
impl Default for SeekOptions {
    fn default() -> Self {
        Self {
            decode: DecodeOptions::default(),
            max_index_frames: 4 << 20,
            use_seektable: true,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct SeekStats {
    pub table_hits: u64,
    pub table_rejections: u64,
}
#[derive(Clone, Copy)]
struct FileLandmark {
    offset: u64,
    sample: u64,
    number: u64,
}
struct Hint {
    point: crate::metadata::SeekPoint,
    rejected: bool,
}

/// Native file-backed exact seeking with a lazy frame index and validated
/// SEEKTABLE hints. Construction reads metadata only, at the source's current
/// position. Buffers survive repeated seeks. The byte source must not change.
pub struct SeekReader<R> {
    reader: Reader<R>,
    origin: u64,
    options: SeekOptions,
    hints: Vec<Hint>,
    index: Vec<FileLandmark>,
    basis: Option<FrameHeader>,
    pending: Option<(FrameHeader, u64, usize)>,
    stats: SeekStats,
    failed: bool,
}
impl<R: Read + Seek> SeekReader<R> {
    pub fn new(source: R) -> Result<Self, Error> {
        Self::with_options(source, SeekOptions::default())
    }
    pub fn with_options(mut source: R, options: SeekOptions) -> Result<Self, Error> {
        let origin = source.stream_position()?;
        let reader = Reader::with_options(source, options.decode)?;
        let mut hints = Vec::new();
        if options.use_seektable {
            for block in &reader.metadata()?.blocks {
                if block.kind == 3 {
                    for point in crate::metadata::SeekTable::parse(block.data)?.points() {
                        if point.sample == u64::MAX {
                            break;
                        }
                        reserve(&mut hints, 1)?;
                        hints.push(Hint {
                            point,
                            rejected: false,
                        });
                    }
                }
            }
        }
        Ok(Self {
            reader,
            origin,
            options,
            hints,
            index: Vec::new(),
            basis: None,
            pending: None,
            stats: SeekStats::default(),
            failed: false,
        })
    }
    pub fn stream_info(&self) -> StreamInfo {
        self.reader.stream_info()
    }
    pub fn metadata(&self) -> Result<Metadata<'_>, Error> {
        self.reader.metadata()
    }
    pub fn integrity(&self) -> Integrity {
        self.reader.integrity()
    }
    pub fn position(&self) -> u64 {
        self.pending
            .map_or(self.reader.position(), |(_, start, skip)| {
                start + skip as u64
            })
    }
    pub fn index_len(&self) -> usize {
        self.index.len()
    }
    pub fn seek_stats(&self) -> SeekStats {
        self.stats
    }
    pub fn into_inner(self) -> R {
        self.reader.into_inner()
    }
    fn remember(&mut self, offset: u64, header: FrameHeader, sample: u64) -> Result<(), Error> {
        if sample == 0 {
            self.basis = Some(header);
        }
        match self.index.binary_search_by_key(&sample, |f| f.sample) {
            Ok(i) => {
                if self.index[i].offset != offset || self.index[i].number != header.number {
                    return Err(Error::Invalid("indexed native frame changed"));
                }
            }
            Err(i) => {
                if self.index.len() >= self.options.max_index_frames {
                    return Err(Error::LimitExceeded("indexed native frames"));
                }
                reserve(&mut self.index, 1)?;
                self.index.insert(
                    i,
                    FileLandmark {
                        offset,
                        sample,
                        number: header.number,
                    },
                );
            }
        }
        Ok(())
    }
    fn next_frame(&mut self) -> Result<Option<(FrameHeader, u64)>, Error> {
        let offset = self.reader.offset;
        let result = self.reader.next_inner()?;
        if let Some((header, sample)) = result {
            self.remember(offset, header, sample)?;
        }
        Ok(result)
    }
    pub fn read_next(&mut self) -> Result<Option<StreamFrame<'_>>, Error> {
        if self.failed {
            return Err(Error::Invalid("native seek reader is terminal after error"));
        }
        let result = match self.pending.take() {
            Some(frame) => Ok(Some(frame)),
            None => self
                .next_frame()
                .map(|f| f.map(|(header, start)| (header, start, 0))),
        };
        match result {
            Ok(Some((header, start, skip))) => Ok(Some(StreamFrame {
                start_sample: start + skip as u64,
                header,
                pcm: self.reader.decoder.pcm(header).trim_start(skip),
            })),
            Ok(None) => Ok(None),
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
    fn restore(&mut self, point: FileLandmark, partial: bool) -> Result<(), Error> {
        let absolute = self
            .origin
            .checked_add(point.offset)
            .ok_or(Error::Invalid("seek offset overflow"))?;
        if self.reader.source.seek(SeekFrom::Start(absolute))? != absolute {
            return Err(Error::Io("seek returned a different position".into()));
        }
        self.reader.sequence = Sequence::new(self.reader.stream_info(), self.options.decode);
        if let Some(basis) = self.basis {
            self.reader.sequence.variable = Some(basis.variable_block_size);
            self.reader.sequence.fixed_size = basis.block_size;
            self.reader.sequence.frames =
                if basis.variable_block_size || self.reader.sequence.legacy_variable {
                    u64::from(point.sample != 0)
                } else {
                    point.number
                };
        }
        self.reader.sequence.samples = point.sample;
        if partial {
            self.reader.sequence.digest = None;
            self.reader.sequence.integrity = Integrity::Partial;
        }
        self.reader.offset = point.offset;
        self.reader.buffer.clear();
        self.reader.decoder.clear_partial();
        self.reader.consumed = 0;
        self.reader.eof = false;
        self.reader.done = false;
        self.reader.failed = false;
        self.pending = None;
        Ok(())
    }
    /// Next PCM begins exactly at `sample`. Zero restarts whole-stream integrity;
    /// nonzero seeks verify the selected frame and mark integrity partial. Bad
    /// SEEKTABLE targets are discarded and a validated earlier frame is used.
    /// A successful explicit seek recovers the reader after a previous error.
    pub fn seek(&mut self, sample: u64) -> Result<(), Error> {
        if self.stream_info().total_samples != 0 && sample > self.stream_info().total_samples {
            return Err(Error::Invalid("seek beyond end"));
        }
        self.failed = true;
        let result = self.seek_inner(sample);
        if result.is_ok() {
            self.failed = false;
        }
        result
    }
    fn seek_inner(&mut self, sample: u64) -> Result<(), Error> {
        let beginning = FileLandmark {
            offset: self.reader.metadata.len() as u64,
            sample: 0,
            number: 0,
        };
        if sample == 0 {
            self.restore(beginning, false)?;
            return Ok(());
        }
        // Establish the blocking strategy and nominal fixed size from the first
        // CRC-validated frame, never from a potentially stale table entry.
        if self.basis.is_none() {
            self.restore(beginning, true)?;
            if self.next_frame()?.is_none() {
                return Err(Error::Invalid("seek beyond end"));
            }
        }
        let i = self.index.partition_point(|p| p.sample <= sample);
        let landmark = if i == 0 { beginning } else { self.index[i - 1] };
        let hint = self.hints.partition_point(|h| h.point.sample <= sample);
        let hint = hint
            .checked_sub(1)
            .filter(|&i| !self.hints[i].rejected && self.hints[i].point.sample > landmark.sample);
        if let Some(i) = hint {
            let point = self.hints[i].point;
            let basis = self.basis.unwrap();
            let variable = basis.variable_block_size || self.reader.sequence.legacy_variable;
            let checked = (|| {
                if point.samples == 0
                    || (!variable && !point.sample.is_multiple_of(basis.block_size as u64))
                {
                    return Err(Error::Invalid("SEEKTABLE sample alignment"));
                }
                let offset = beginning
                    .offset
                    .checked_add(point.offset)
                    .ok_or(Error::Invalid("SEEKTABLE offset overflow"))?;
                let number = if variable {
                    point.sample
                } else {
                    point.sample / basis.block_size as u64
                };
                self.restore(
                    FileLandmark {
                        offset,
                        sample: point.sample,
                        number,
                    },
                    true,
                )?;
                let (header, start) = self.reader.next_inner()?.ok_or(Error::Truncated)?;
                if header.block_size != point.samples as usize {
                    return Err(Error::Invalid("SEEKTABLE block size"));
                }
                self.remember(offset, header, start)?;
                Ok((header, start))
            })();
            match checked {
                Ok((header, start)) => {
                    self.stats.table_hits += 1;
                    if sample < self.reader.position() {
                        self.pending = Some((header, start, (sample - start) as usize));
                        return Ok(());
                    }
                    return self.scan_to(sample);
                }
                Err(error @ (Error::Io(_) | Error::Allocation | Error::LimitExceeded(_))) => {
                    return Err(error);
                }
                Err(_) => {
                    self.hints[i].rejected = true;
                    self.stats.table_rejections += 1;
                }
            }
        }
        self.restore(landmark, true)?;
        self.scan_to(sample)
    }
    fn scan_to(&mut self, sample: u64) -> Result<(), Error> {
        while let Some((header, start)) = self.next_frame()? {
            if sample < self.reader.position() {
                self.pending = Some((header, start, (sample - start) as usize));
                return Ok(());
            }
        }
        if self.reader.position() != sample {
            return Err(Error::Invalid("seek beyond end"));
        }
        Ok(())
    }
}

fn read_exact<R: Read>(source: &mut R, out: &mut [u8]) -> Result<(), Error> {
    source.read_exact(out).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            Error::Truncated
        } else {
            e.into()
        }
    })
}
