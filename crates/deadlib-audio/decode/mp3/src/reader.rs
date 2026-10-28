//! Bounded, read-backed Layer III packet framing. Read boundaries are immaterial.
use crate::{
    Error, Frame, FrameHeader, FreeFormat, Limits, reserve,
    tags::{Id3Tag, Id3v1, VbrHeader},
};
use std::io::{ErrorKind, Read};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TruncatedPolicy {
    #[default]
    Error,
    /// Discard an incomplete final audio frame. Metadata remains checked.
    Discard,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ReaderOptions {
    pub limits: Limits,
    pub truncated: TruncatedPolicy,
    /// A known unpadded free-format size permits one- or two-frame streams.
    pub free_format: Option<FreeFormat>,
}
pub struct Id3Block {
    pub byte_offset: u64,
    data: Vec<u8>,
}
impl Id3Block {
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
#[derive(Default)]
pub struct Metadata {
    id3v2: Vec<Id3Block>,
    id3v1: Option<[u8; 128]>,
    first_frame: Option<(FrameHeader, Vec<u8>)>,
    bytes: usize,
}
impl Metadata {
    pub fn id3v2(&self) -> &[Id3Block] {
        &self.id3v2
    }
    pub fn id3v1(&self) -> Option<Id3v1<'_>> {
        Id3v1::parse(self.id3v1.as_ref()?)
    }
    pub fn vbr(&self) -> Result<Option<VbrHeader<'_>>, Error> {
        match &self.first_frame {
            Some((header, data)) => VbrHeader::parse(*header, data),
            None => Ok(None),
        }
    }
    pub fn first_header(&self) -> Option<FrameHeader> {
        self.first_frame.as_ref().map(|(h, _)| *h)
    }
}
/// Complete physical packet. Header framing is checked; side information,
/// main data and protection CRC are checked by `FrameDecoder`.
pub struct Packet<'a> {
    pub header: FrameHeader,
    pub data: &'a [u8],
    pub byte_offset: u64,
    /// Junk bytes skipped since the preceding packet, excluding metadata.
    pub skipped_bytes: u64,
    pub free_format: Option<FreeFormat>,
}
struct Prepared {
    start: usize,
    count: usize,
    header: FrameHeader,
    free_format: Option<FreeFormat>,
    byte_offset: u64,
    skipped_bytes: u64,
}
pub struct PacketReader<R> {
    source: R,
    options: ReaderOptions,
    buffer: Vec<u8>,
    head: usize,
    position: u64,
    eof: bool,
    done: bool,
    failed: bool,
    locked: bool,
    packets: u64,
    skipped: usize,
    skipped_total: u64,
    discarded_tail: usize,
    free: Option<(FrameHeader, FreeFormat)>,
    unresolved_free: bool,
    metadata: Metadata,
}
impl<R: Read> PacketReader<R> {
    pub fn new(source: R) -> Self {
        Self::with_options(source, ReaderOptions::default())
    }
    pub fn with_options(source: R, options: ReaderOptions) -> Self {
        Self {
            source,
            options,
            buffer: Vec::new(),
            head: 0,
            position: 0,
            eof: false,
            done: false,
            failed: false,
            locked: false,
            packets: 0,
            skipped: 0,
            skipped_total: 0,
            discarded_tail: 0,
            free: None,
            unresolved_free: false,
            metadata: Metadata::default(),
        }
    }
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    pub fn byte_position(&self) -> u64 {
        self.position
    }
    pub fn skipped_bytes(&self) -> u64 {
        self.skipped_total
    }
    pub fn discarded_tail_bytes(&self) -> usize {
        self.discarded_tail
    }
    pub fn get_ref(&self) -> &R {
        &self.source
    }
    pub(crate) fn into_scanned(self) -> (R, Metadata) {
        (self.source, self.metadata)
    }
    /// Returns the source (which may have read ahead) and its unread lookahead.
    pub fn into_parts(mut self) -> (R, Vec<u8>) {
        self.buffer.copy_within(self.head.., 0);
        self.buffer.truncate(self.buffer.len() - self.head);
        (self.source, self.buffer)
    }
    pub fn reset(&mut self, source: R) {
        self.source = source;
        self.buffer.clear();
        self.head = 0;
        self.position = 0;
        self.eof = false;
        self.done = false;
        self.failed = false;
        self.locked = false;
        self.packets = 0;
        self.skipped = 0;
        self.skipped_total = 0;
        self.discarded_tail = 0;
        self.free = None;
        self.unresolved_free = false;
        self.metadata = Metadata::default();
    }
    fn available(&self) -> &[u8] {
        &self.buffer[self.head..]
    }
    fn consume(&mut self, count: usize) -> Result<(), Error> {
        self.position = self
            .position
            .checked_add(count as u64)
            .ok_or(Error::LimitExceeded("stream byte position"))?;
        self.head += count;
        Ok(())
    }
    fn ensure(&mut self, count: usize) -> Result<bool, Error> {
        let max = self
            .options
            .limits
            .frame_bytes
            .checked_mul(2)
            .and_then(|n| n.checked_add(4))
            .ok_or(Error::LimitExceeded("stream buffer"))?
            .max(128);
        if count > max {
            return Err(Error::LimitExceeded("stream buffer"));
        }
        while self.available().len() < count && !self.eof {
            if self.head != 0 {
                self.buffer.copy_within(self.head.., 0);
                self.buffer.truncate(self.buffer.len() - self.head);
                self.head = 0;
            }
            let start = self.buffer.len();
            let extra = 8192.min(max - start);
            reserve(&mut self.buffer, extra)?;
            self.buffer.resize(start + extra, 0);
            let read = loop {
                match self.source.read(&mut self.buffer[start..]) {
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    result => break result,
                }
            };
            match read {
                Ok(n) => {
                    self.buffer.truncate(start + n);
                    self.eof = n == 0;
                }
                Err(error) => {
                    self.buffer.truncate(start);
                    return Err(error.into());
                }
            }
        }
        Ok(self.available().len() >= count)
    }
    fn junk(&mut self, count: usize) -> Result<(), Error> {
        self.skipped = self
            .skipped
            .checked_add(count)
            .ok_or(Error::LimitExceeded("MPEG synchronization"))?;
        if self.skipped > self.options.limits.sync_bytes {
            return Err(Error::LimitExceeded("MPEG synchronization"));
        }
        self.skipped_total = self
            .skipped_total
            .checked_add(count as u64)
            .ok_or(Error::LimitExceeded("stream byte position"))?;
        self.locked = false;
        self.consume(count)
    }
    fn read_id3(&mut self) -> Result<(), Error> {
        if !self.ensure(10)? {
            return Err(Error::Truncated);
        }
        let count = Id3Tag::encoded_len(self.available(), self.options.limits)?;
        let total = self
            .metadata
            .bytes
            .checked_add(count)
            .ok_or(Error::LimitExceeded("stream metadata"))?;
        if total > self.options.limits.metadata_bytes {
            return Err(Error::LimitExceeded("stream metadata"));
        }
        let offset = self.position;
        let mut data = Vec::new();
        data.try_reserve_exact(count)
            .map_err(|_| Error::Allocation)?;
        data.resize(count, 0);
        let cached = count.min(self.available().len());
        data[..cached].copy_from_slice(&self.available()[..cached]);
        self.consume(cached)?;
        let mut read = cached;
        while read < count {
            match self.source.read(&mut data[read..]) {
                Ok(0) => return Err(Error::Truncated),
                Ok(n) => {
                    read += n;
                    self.position = self
                        .position
                        .checked_add(n as u64)
                        .ok_or(Error::LimitExceeded("stream byte position"))?;
                }
                Err(error) if error.kind() == ErrorKind::Interrupted => (),
                Err(error) => return Err(error.into()),
            }
        }
        Id3Tag::parse(&data, self.options.limits)?;
        reserve(&mut self.metadata.id3v2, 1)?;
        self.metadata.id3v2.push(Id3Block {
            byte_offset: offset,
            data,
        });
        self.metadata.bytes = total;
        Ok(())
    }
    fn incomplete_audio(&mut self) -> Result<Option<Prepared>, Error> {
        match self.options.truncated {
            TruncatedPolicy::Error => Err(Error::Truncated),
            TruncatedPolicy::Discard => {
                self.discarded_tail = self.available().len();
                self.consume(self.discarded_tail)?;
                Ok(None)
            }
        }
    }
    fn prepare(&mut self) -> Result<Option<Prepared>, Error> {
        loop {
            if !self.ensure(4)? {
                let bytes = self.available();
                if bytes.starts_with(b"ID3") {
                    return Err(Error::Truncated);
                }
                if bytes.first() == Some(&0xff) && (bytes.len() == 1 || bytes[1] & 0xe0 == 0xe0) {
                    return self.incomplete_audio();
                }
                if self.unresolved_free {
                    return Err(Error::Unsupported(
                        "free-format size requires three headers",
                    ));
                }
                self.junk(bytes.len())?;
                return Ok(None);
            }
            if self.available().starts_with(b"ID3") {
                self.read_id3()?;
                continue;
            }
            if self.packets != 0 && self.available().starts_with(b"TAG") {
                if !self.ensure(128)? {
                    return Err(Error::Truncated);
                }
                let bytes: [u8; 128] = self.available()[..128].try_into().unwrap();
                let total = self
                    .metadata
                    .bytes
                    .checked_add(128)
                    .ok_or(Error::LimitExceeded("stream metadata"))?;
                if total > self.options.limits.metadata_bytes {
                    return Err(Error::LimitExceeded("stream metadata"));
                }
                self.metadata.id3v1 = Some(bytes);
                self.metadata.bytes = total;
                self.consume(128)?;
                continue;
            }
            let header = match FrameHeader::parse(self.available()) {
                Ok(h) => h,
                Err(_) => {
                    self.junk(1)?;
                    continue;
                }
            };
            let free = if header.bitrate().is_none() {
                if let Some(size) = self.options.free_format {
                    Some(size)
                } else if let Some((_, size)) = self.free.filter(|(h, _)| h.compatible(header)) {
                    Some(size)
                } else {
                    let max = self
                        .options
                        .limits
                        .frame_bytes
                        .checked_mul(2)
                        .and_then(|n| n.checked_add(4))
                        .ok_or(Error::LimitExceeded("free-format probe"))?;
                    let mut size = FreeFormat::infer(self.available(), self.options.limits);
                    while size.is_err() && !self.eof {
                        let next = (self.available().len() + 8192).min(max);
                        if next <= self.available().len() {
                            break;
                        }
                        self.ensure(next)?;
                        size = FreeFormat::infer(self.available(), self.options.limits);
                    }
                    match size {
                        Ok(size) => {
                            self.free = Some((header, size));
                            Some(size)
                        }
                        Err(_) if self.locked => {
                            return Err(Error::Unsupported(
                                "free-format size requires three headers",
                            ));
                        }
                        Err(_) => {
                            self.unresolved_free = true;
                            self.junk(1)?;
                            continue;
                        }
                    }
                }
            } else {
                None
            };
            let count = header
                .frame_bytes()
                .unwrap_or_else(|| free.unwrap().unpadded_bytes() + header.padding());
            if count < header.main_data_offset() {
                self.junk(1)?;
                continue;
            }
            if count > self.options.limits.frame_bytes {
                return Err(Error::LimitExceeded("MP3 frame"));
            }
            if !self.ensure(count)? {
                // A false leading header may claim more bytes than the entire
                // remaining stream while a later complete frame is present.
                let later = !self.locked
                    && self
                        .available()
                        .windows(4)
                        .enumerate()
                        .skip(1)
                        .any(|(n, bytes)| {
                            FrameHeader::parse(bytes).is_ok_and(|h| {
                                h.frame_bytes().is_some_and(|len| {
                                    len >= h.main_data_offset()
                                        && len <= self.options.limits.frame_bytes
                                        && len <= self.available().len() - n
                                })
                            })
                        });
                if later {
                    self.junk(1)?;
                    continue;
                }
                return self.incomplete_audio();
            }
            if !self.locked {
                self.ensure(count + 4)?;
                if !self.eof {
                    let next = FrameHeader::parse(&self.available()[count..]);
                    let trailer = self.available()[count..].starts_with(b"ID3")
                        || self.available()[count..].starts_with(b"TAG");
                    if !trailer
                        && !next.is_ok_and(|h| {
                            h.compatible(header)
                                && h.bitrate().is_none() == header.bitrate().is_none()
                        })
                        && Frame::parse(
                            &self.available()[..count],
                            free,
                            false,
                            self.options.limits,
                        )
                        .is_err()
                    {
                        self.junk(1)?;
                        continue;
                    }
                }
            }
            if self.metadata.first_frame.is_none() {
                let mut data = Vec::new();
                reserve(&mut data, count)?;
                data.extend_from_slice(&self.available()[..count]);
                self.metadata.first_frame = Some((header, data));
            }
            let start = self.head;
            let position = self.position;
            let skipped = self.skipped as u64;
            self.consume(count)?;
            self.skipped = 0;
            self.unresolved_free = false;
            self.locked = true;
            self.packets = self
                .packets
                .checked_add(1)
                .ok_or(Error::LimitExceeded("stream frame count"))?;
            return Ok(Some(Prepared {
                start,
                count,
                header,
                free_format: free,
                byte_offset: position,
                skipped_bytes: skipped,
            }));
        }
    }
    pub fn next_packet(&mut self) -> Result<Option<Packet<'_>>, Error> {
        if self.failed {
            return Err(Error::Invalid("packet reader requires reset after error"));
        }
        if self.done {
            return Ok(None);
        }
        match self.prepare() {
            Ok(Some(Prepared {
                start,
                count,
                header,
                free_format,
                byte_offset,
                skipped_bytes,
            })) => Ok(Some(Packet {
                header,
                data: &self.buffer[start..start + count],
                byte_offset,
                skipped_bytes,
                free_format,
            })),
            Ok(None) => {
                self.done = true;
                Ok(None)
            }
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
}
