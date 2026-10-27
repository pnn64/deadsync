//! Native framing, streaming, whole-stream integrity, and sample-accurate seeks.
use crate::{
    Error, FrameDecoder, FrameHeader, Limits, Metadata, Pcm, StreamInfo, checksum::Md5, reserve,
};
use std::io::Read;

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
        Ok(Self {
            source,
            metadata,
            options,
            decoder: FrameDecoder::new(Some(info))?,
            sequence: Sequence::new(info, options),
            buffer: Vec::new(),
            consumed: 0,
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
        if self.consumed != 0 {
            self.buffer.copy_within(self.consumed.., 0);
            self.buffer.truncate(self.buffer.len() - self.consumed);
            self.consumed = 0;
        }
        loop {
            if self.buffer.is_empty() && self.eof {
                self.sequence.finish()?;
                self.done = true;
                return Ok(None);
            }
            match self.decoder.decode(&self.buffer) {
                Ok(frame) => {
                    let start = self.sequence.accept(frame.header, frame.pcm)?;
                    self.consumed = frame.bytes;
                    return Ok(Some((frame.header, start)));
                }
                Err(Error::Truncated) if !self.eof => (),
                Err(error) => return Err(error),
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

fn read_exact<R: Read>(source: &mut R, out: &mut [u8]) -> Result<(), Error> {
    source.read_exact(out).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            Error::Truncated
        } else {
            e.into()
        }
    })
}
