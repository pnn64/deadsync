//! Bounded forward-only WAVE data with native caller-buffer frame access.
use crate::{
    ChunkInfo, Container, Encoding, Error, Format, Limits, Stream, native_io::NativeIo, reserve,
};
use std::io::{self, Read};

/// A metadata callback can read only this payload, never its padding or the next
/// header. Unread bytes are automatically drained after the callback returns.
pub struct ChunkPayload<'a, R> {
    stream: &'a mut Stream<R>,
    remaining: u64,
    prefix: [u8; 4],
    prefix_position: usize,
    prefix_len: usize,
}
impl<R> ChunkPayload<'_, R> {
    pub fn remaining(&self) -> u64 {
        self.remaining
    }
}
impl<'a, R> ChunkPayload<'a, R> {
    fn new(stream: &'a mut Stream<R>, remaining: u64, prefix: Option<[u8; 4]>) -> Self {
        Self {
            stream,
            remaining,
            prefix: prefix.unwrap_or_default(),
            prefix_position: 0,
            prefix_len: if prefix.is_some() { 4 } else { 0 },
        }
    }
}
impl<R: Read> Read for ChunkPayload<'_, R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if self.prefix_position < self.prefix_len {
            let n = out.len().min(self.prefix_len - self.prefix_position);
            out[..n].copy_from_slice(&self.prefix[self.prefix_position..self.prefix_position + n]);
            self.prefix_position += n;
            self.remaining -= n as u64;
            return Ok(n);
        }
        let n = self.stream.read_chunk(out).map_err(|e| match e {
            Error::Io(e) => e,
            Error::Truncated => io::ErrorKind::UnexpectedEof.into(),
            e => io::Error::new(io::ErrorKind::InvalidData, e),
        })?;
        self.remaining -= n as u64;
        Ok(n)
    }
}

#[derive(Clone, Copy)]
enum WavlState {
    Next,
    Data(u64),
    Silence(u64),
    End,
}
struct WavlForward {
    hold: Vec<u8>,
    seen: u64,
    position: u64,
    state: WavlState,
}

/// Opening reads through the first data header, without buffering audio. fmt
/// must precede data. Call finish to validate the remaining form and padding;
/// EOF from a data read alone does not validate trailing chunks.
pub struct ForwardReader<R> {
    stream: Stream<R>,
    format: Vec<u8>,
    data: ChunkInfo,
    fact: Option<u64>,
    bytes_read: u64,
    wavl: Option<WavlForward>,
    failed: bool,
}
impl<R: Read> ForwardReader<R> {
    pub fn new(inner: R) -> Result<Self, Error> {
        Self::with_limits(inner, Limits::default())
    }
    pub fn with_limits(inner: R, limits: Limits) -> Result<Self, Error> {
        Self::with_metadata(inner, limits, |_, _| Ok(()))
    }
    /// Visit leading ancillary payloads without retaining their descriptors or
    /// allocating their data. fmt/fact/ds64 are handled internally. A callback
    /// error aborts opening; use Stream directly for arbitrary demux policies.
    pub fn with_metadata(
        inner: R,
        limits: Limits,
        mut visit: impl FnMut(ChunkInfo, &mut ChunkPayload<'_, R>) -> Result<(), Error>,
    ) -> Result<Self, Error> {
        let mut stream = Stream::with_limits(inner, limits)?;
        let mut format = Vec::new();
        let mut fact = None;
        let (data, wavl) = loop {
            let chunk = stream
                .next_chunk()?
                .ok_or(Error::Invalid("missing data chunk"))?;
            match &chunk.id {
                b"fmt " => {
                    if !format.is_empty() {
                        return Err(Error::Invalid("duplicate format chunk"));
                    }
                    if chunk.size > limits.format_bytes as u64 {
                        return Err(Error::Limit("format size"));
                    }
                    reserve(&mut format, chunk.size as usize)?;
                    format.resize(chunk.size as usize, 0);
                    stream.read_chunk(&mut format)?;
                    Format::parse(&format, stream.container().endian())?;
                }
                b"fact" => {
                    if fact.is_some() {
                        return Err(Error::Invalid("duplicate fact chunk"));
                    }
                    fact = Some(read_fact(&mut stream, chunk)?);
                }
                b"data" => {
                    if format.is_empty() {
                        return Err(Error::Unsupported("forward-only data before format"));
                    }
                    break (chunk, None);
                }
                b"LIST" if chunk.size >= 4 => {
                    let mut kind = [0; 4];
                    stream.read_chunk(&mut kind)?;
                    if kind == *b"wavl" {
                        if format.is_empty() {
                            return Err(Error::Unsupported("forward-only data before format"));
                        }
                        let f = Format::parse(&format, stream.container().endian())?;
                        let mut hold = Vec::new();
                        if matches!(f.encoding, Encoding::Pcm | Encoding::Float) {
                            reserve(&mut hold, usize::from(f.block_align))?;
                            hold.resize(
                                usize::from(f.block_align),
                                if f.sample_format()? == crate::SampleFormat::U8 {
                                    128
                                } else {
                                    0
                                },
                            );
                        }
                        stream.enter_list_payload(chunk)?;
                        break (
                            chunk,
                            Some(WavlForward {
                                hold,
                                seen: 0,
                                position: 0,
                                state: WavlState::Next,
                            }),
                        );
                    }
                    visit(
                        chunk,
                        &mut ChunkPayload::new(&mut stream, chunk.size, Some(kind)),
                    )?;
                }
                _ => visit(chunk, &mut ChunkPayload::new(&mut stream, chunk.size, None))?,
            }
        };
        let f = Format::parse(&format, stream.container().endian())?;
        if wavl.is_none()
            && matches!(f.encoding, Encoding::Pcm | Encoding::Float)
            && !data.size.is_multiple_of(u64::from(f.block_align))
        {
            return Err(Error::Invalid("partial PCM frame"));
        }
        Ok(Self {
            stream,
            format,
            data,
            fact,
            bytes_read: 0,
            wavl,
            failed: false,
        })
    }
    pub fn container(&self) -> Container {
        self.stream.container()
    }
    pub fn format(&self) -> Format<'_> {
        Format::parse(&self.format, self.container().endian()).unwrap()
    }
    /// fact encountered before data. A trailing fact is checked during finish.
    pub fn fact(&self) -> Option<u64> {
        self.fact
    }
    /// Raw ds64 field. Only an RF64 fact sentinel selects this sample count.
    /// BW64 uses ignored dummy words here, preserved for inspection only.
    pub fn ds64_sample_count(&self) -> Option<u64> {
        self.stream.ds64().map(|d| d.sample_count)
    }
    pub fn data_chunk(&self) -> Option<ChunkInfo> {
        self.wavl.is_none().then_some(self.data)
    }
    pub fn audio_chunk(&self) -> ChunkInfo {
        self.data
    }
    pub fn data_bytes_read(&self) -> u64 {
        self.bytes_read
    }
    pub fn frames(&self) -> Result<u64, Error> {
        let f = self.format();
        f.sample_format()?;
        if let Some(w) = &self.wavl {
            return self
                .fact
                .or_else(|| matches!(w.state, WavlState::End).then_some(w.seen))
                .ok_or(Error::Unsupported(
                    "wavl duration is unknown before trailing fact or end",
                ));
        }
        Ok(self.data.size / u64::from(f.block_align))
    }
    pub fn position(&self) -> Result<u64, Error> {
        let f = self.format();
        f.sample_format()?;
        if let Some(w) = &self.wavl {
            return Ok(w.position);
        }
        if !self.bytes_read.is_multiple_of(u64::from(f.block_align)) {
            return Err(Error::Invalid(
                "native stream position is not frame aligned",
            ));
        }
        Ok(self.bytes_read / u64::from(f.block_align))
    }
    /// Raw encoded bytes for any supported or opaque descriptor. Mixing raw
    /// reads with native frame calls requires a frame-aligned current position.
    pub fn read_data(&mut self, out: &mut [u8]) -> Result<usize, Error> {
        if self.wavl.is_some() {
            return Err(Error::Unsupported("wavl data is not contiguous"));
        }
        let n = self.stream.read_chunk(out)?;
        self.bytes_read += n as u64;
        Ok(n)
    }
    pub fn read_frames(&mut self, out: &mut [u8]) -> Result<usize, Error> {
        if self.wavl.is_some() {
            return self.read_frames_with_metadata(out, |_, _| Ok(()));
        }
        let frames = self.frames()?;
        let pos = self.position()?;
        let align = usize::from(self.format().block_align);
        let n = (frames - pos).min((out.len() / align) as u64) as usize;
        Ok(self.read_data(&mut out[..n * align])? / align)
    }
    /// Visit ancillary chunks encountered between wavl audio segments. Ordinary
    /// contiguous data reads have no interleaved metadata to visit.
    pub fn read_frames_with_metadata(
        &mut self,
        out: &mut [u8],
        mut visit: impl FnMut(ChunkInfo, &mut ChunkPayload<'_, R>) -> Result<(), Error>,
    ) -> Result<usize, Error> {
        if self.wavl.is_none() {
            return self.read_frames(out);
        }
        if self.failed {
            return Err(Error::Invalid("forward reader unusable after error"));
        }
        self.format().sample_format()?;
        let align = usize::from(self.format().block_align);
        let capacity = out.len() / align;
        self.failed = true;
        let result = (|| {
            let w = self.wavl.as_mut().unwrap();
            let mut done = 0;
            while done < capacity {
                match w.state {
                    WavlState::End => break,
                    WavlState::Data(0) | WavlState::Silence(0) => w.state = WavlState::Next,
                    WavlState::Next => {
                        let Some(c) = self.stream.next_chunk()? else {
                            self.stream.leave_list_payload()?;
                            w.state = WavlState::End;
                            if self.fact.is_some_and(|n| n != w.seen) {
                                return Err(Error::Invalid("wavl fact sample count mismatch"));
                            }
                            break;
                        };
                        if let Some((silence, frames)) =
                            wavl_entry(&mut self.stream, c, Some(align))?
                        {
                            w.seen = w
                                .seen
                                .checked_add(frames)
                                .ok_or(Error::Limit("wavl frame count overflow"))?;
                            if self.fact.is_some_and(|n| w.seen > n) {
                                return Err(Error::Invalid("wavl fact sample count mismatch"));
                            }
                            w.state = if silence {
                                WavlState::Silence(frames)
                            } else {
                                WavlState::Data(frames)
                            };
                        } else {
                            visit(c, &mut ChunkPayload::new(&mut self.stream, c.size, None))?;
                        }
                    }
                    WavlState::Data(left) => {
                        let n = left.min((capacity - done) as u64) as usize;
                        let dst = &mut out[done * align..(done + n) * align];
                        self.stream.read_chunk(dst)?;
                        w.hold.copy_from_slice(&dst[dst.len() - align..]);
                        self.bytes_read += dst.len() as u64;
                        w.position += n as u64;
                        done += n;
                        w.state = WavlState::Data(left - n as u64);
                    }
                    WavlState::Silence(left) => {
                        let n = left.min((capacity - done) as u64) as usize;
                        crate::timeline::repeat(
                            &mut out[done * align..(done + n) * align],
                            &w.hold,
                        );
                        w.position += n as u64;
                        done += n;
                        w.state = WavlState::Silence(left - n as u64);
                    }
                }
            }
            Ok(done)
        })();
        if result.is_ok() {
            self.failed = false;
        }
        result
    }
    pub fn read_i16(&mut self, out: &mut [i16]) -> Result<usize, Error> {
        NativeIo::new(self.format())?.read_i16(out, |b| self.read_frames(b))
    }
    pub fn read_i32(&mut self, out: &mut [i32]) -> Result<usize, Error> {
        NativeIo::new(self.format())?.read_i32(out, |b| self.read_frames(b))
    }
    pub fn read_integers(&mut self, out: &mut [i32]) -> Result<usize, Error> {
        NativeIo::new(self.format())?.read_integers(out, |b| self.read_frames(b))
    }
    pub fn read_f32(&mut self, out: &mut [f32]) -> Result<usize, Error> {
        NativeIo::new(self.format())?.read_f32(out, |b| self.read_frames(b))
    }
    pub fn read_f64(&mut self, out: &mut [f64]) -> Result<usize, Error> {
        NativeIo::new(self.format())?.read_f64(out, |b| self.read_frames(b))
    }
    pub fn into_inner(self) -> R {
        self.stream.into_inner()
    }
    /// Drain unread audio and validate all trailing headers/padding. Does not
    /// consume bytes after this form. Unread compressed blocks are not decoded.
    pub fn finish(self) -> Result<R, Error> {
        self.finish_with_metadata(|_, _| Ok(()))
    }
    pub fn finish_with_metadata(
        self,
        visit: impl FnMut(ChunkInfo, &mut ChunkPayload<'_, R>) -> Result<(), Error>,
    ) -> Result<R, Error> {
        self.finish_checked(None, visit)
    }
    pub(crate) fn finish_checked(
        mut self,
        frames: Option<u64>,
        mut visit: impl FnMut(ChunkInfo, &mut ChunkPayload<'_, R>) -> Result<(), Error>,
    ) -> Result<R, Error> {
        if self.failed {
            return Err(Error::Invalid("forward reader unusable after error"));
        }
        if let Some(w) = &mut self.wavl
            && !matches!(w.state, WavlState::End)
        {
            let align = (!w.hold.is_empty()).then_some(w.hold.len());
            while let Some(c) = self.stream.next_chunk()? {
                if let Some((_, frames)) = wavl_entry(&mut self.stream, c, align)? {
                    w.seen = w
                        .seen
                        .checked_add(frames)
                        .ok_or(Error::Limit("wavl frame count overflow"))?;
                } else {
                    visit(c, &mut ChunkPayload::new(&mut self.stream, c.size, None))?;
                }
            }
            self.stream.leave_list_payload()?;
            w.state = WavlState::End;
        }
        while let Some(chunk) = self.stream.next_chunk()? {
            match &chunk.id {
                b"fmt " => return Err(Error::Invalid("duplicate format chunk")),
                b"data" => return Err(Error::Unsupported("multiple data chunks")),
                b"fact" => {
                    if self.fact.is_some() {
                        return Err(Error::Invalid("duplicate fact chunk"));
                    }
                    self.fact = Some(read_fact(&mut self.stream, chunk)?);
                }
                b"LIST" if chunk.size >= 4 => {
                    let mut kind = [0; 4];
                    self.stream.read_chunk(&mut kind)?;
                    if kind == *b"wavl" {
                        return Err(Error::Invalid("multiple WAVE data sections"));
                    }
                    visit(
                        chunk,
                        &mut ChunkPayload::new(&mut self.stream, chunk.size, Some(kind)),
                    )?;
                }
                _ => visit(
                    chunk,
                    &mut ChunkPayload::new(&mut self.stream, chunk.size, None),
                )?,
            }
        }
        if let Some(w) = &self.wavl {
            let fact = self
                .fact
                .ok_or(Error::Invalid("wavl requires fact sample count"))?;
            if !w.hold.is_empty() && fact != w.seen {
                return Err(Error::Invalid("wavl fact sample count mismatch"));
            }
        }
        if let Some(frames) = frames {
            let actual = self
                .fact
                .or(self
                    .ds64_sample_count()
                    .filter(|_| self.container() == Container::Rf64))
                .ok_or(Error::Invalid("ADPCM requires fact sample count"))?;
            if actual != frames {
                return Err(Error::Invalid("forward ADPCM duration mismatch"));
            }
        }
        Ok(self.stream.into_inner())
    }
}
fn wavl_entry<R: Read>(
    stream: &mut Stream<R>,
    c: ChunkInfo,
    align: Option<usize>,
) -> Result<Option<(bool, u64)>, Error> {
    match &c.id {
        b"data" => {
            if let Some(align) = align {
                if !c.size.is_multiple_of(align as u64) {
                    return Err(Error::Invalid("partial PCM frame"));
                }
                return Ok(Some((false, c.size / align as u64)));
            }
            Ok(Some((false, 0)))
        }
        b"slnt" => {
            if c.size != 4 {
                return Err(Error::Invalid("slnt payload size"));
            }
            let mut b = [0; 4];
            stream.read_chunk(&mut b)?;
            Ok(Some((true, u64::from(stream.container().endian().u32(&b)))))
        }
        _ => Ok(None),
    }
}
fn read_fact<R: Read>(stream: &mut Stream<R>, chunk: ChunkInfo) -> Result<u64, Error> {
    if chunk.size < 4 {
        return Err(Error::Truncated);
    }
    let mut bytes = [0; 4];
    stream.read_chunk(&mut bytes)?;
    let frames = stream.container().endian().u32(&bytes);
    Ok(stream
        .container()
        .fact_frames(frames, stream.ds64().map(|d| d.sample_count)))
}
