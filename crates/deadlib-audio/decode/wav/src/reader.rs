use crate::{
    ChunkInfo, Container, Error, Format, Limits, SampleFormat, Stream, reserve, timeline::Timeline,
};
use std::io::{Read, Seek, SeekFrom};

/// File-backed reader: scans headers once, skips audio with Seek, retains only
/// format bytes and metadata descriptors. No audio-size-dependent allocation.
pub struct Reader<R> {
    inner: R,
    base: u64,
    end: u64,
    container: Container,
    format: Vec<u8>,
    chunks: Vec<ChunkInfo>,
    data: ChunkInfo,
    fact: Option<u64>,
    ds64_sample_count: Option<u64>,
    pos: u64,
    failed: bool,
    timeline: Option<Timeline>,
    hold: Vec<u8>,
    hold_at: u64,
}
impl<R: Read + Seek> Reader<R> {
    pub fn new(inner: R) -> Result<Self, Error> {
        Self::with_limits(inner, Limits::default())
    }
    pub fn with_limits(mut inner: R, limits: Limits) -> Result<Self, Error> {
        let base = inner.stream_position()?;
        let physical_end = inner.seek(SeekFrom::End(0))?;
        inner.seek(SeekFrom::Start(base))?;
        let mut s = Stream::with_limits(inner, limits)?;
        let end = base
            .checked_add(s.form_bytes())
            .ok_or(Error::Invalid("RIFF offset overflow"))?;
        if end > physical_end {
            return Err(Error::Truncated);
        }
        let container = s.container();
        let ds64_sample_count = s.ds64().map(|d| d.sample_count);
        let endian = container.endian();
        let mut format = Vec::new();
        let mut chunks = Vec::new();
        let mut data = None;
        let mut fact = None;
        let mut timeline = None;
        while let Some(c) = s.next_chunk_seek()? {
            reserve(&mut chunks, 1)?;
            chunks.push(c);
            match &c.id {
                b"fmt " => {
                    if !format.is_empty() {
                        return Err(Error::Invalid("duplicate format chunk"));
                    }
                    if c.size > limits.format_bytes as u64 {
                        return Err(Error::Limit("format size"));
                    }
                    reserve(&mut format, c.size as usize)?;
                    format.resize(c.size as usize, 0);
                    s.read_chunk(&mut format)?;
                    Format::parse(&format, endian)?;
                }
                b"data" => {
                    if data.replace(c).is_some() {
                        return Err(Error::Unsupported("multiple data chunks"));
                    }
                }
                b"LIST" if c.size >= 4 => {
                    let mut kind = [0; 4];
                    s.read_chunk(&mut kind)?;
                    if kind == *b"wavl" {
                        if data.is_some() {
                            return Err(Error::Invalid("multiple WAVE data sections"));
                        }
                        let mut index = Timeline::default();
                        s.enter_list_payload(c)?;
                        while let Some(child) = s.next_chunk_seek()? {
                            let silence = if child.id == *b"slnt" {
                                if child.size != 4 {
                                    return Err(Error::Invalid("slnt payload size"));
                                }
                                let mut b = [0; 4];
                                s.read_chunk(&mut b)?;
                                u64::from(endian.u32(&b))
                            } else {
                                0
                            };
                            index.push(child, silence)?;
                        }
                        s.leave_list_payload()?;
                        timeline = Some(index);
                        data = Some(c);
                    }
                }
                b"fact" => {
                    if fact.is_some() {
                        return Err(Error::Invalid("duplicate fact chunk"));
                    }
                    if c.size < 4 {
                        return Err(Error::Truncated);
                    }
                    let mut b = [0; 4];
                    s.read_chunk(&mut b)?;
                    let n = endian.u32(&b);
                    fact = Some(s.container().fact_frames(n, ds64_sample_count));
                }
                _ => {}
            }
        }
        let f = Format::parse(&format, endian)?;
        let data = data.ok_or(Error::Invalid("missing data chunk"))?;
        let mut hold = Vec::new();
        if let Some(index) = &mut timeline {
            index.initialize(f, fact)?;
            if let Some(align) = index.align {
                reserve(&mut hold, usize::from(align))?;
                hold.resize(
                    usize::from(align),
                    if f.sample_format()? == SampleFormat::U8 {
                        128
                    } else {
                        0
                    },
                );
            }
        } else if matches!(f.encoding, crate::Encoding::Pcm | crate::Encoding::Float)
            && !data.size.is_multiple_of(u64::from(f.block_align))
        {
            return Err(Error::Invalid("partial PCM frame"));
        }
        let mut inner = s.into_inner();
        inner.seek(SeekFrom::Start(base + data.offset))?;
        Ok(Self {
            inner,
            base,
            end,
            container,
            format,
            chunks,
            data,
            fact,
            ds64_sample_count,
            pos: 0,
            failed: false,
            timeline,
            hold,
            hold_at: 0,
        })
    }
    pub fn container(&self) -> Container {
        self.container
    }
    pub fn format(&self) -> Format<'_> {
        Format::parse(&self.format, self.container.endian()).unwrap()
    }
    pub fn chunks(&self) -> &[ChunkInfo] {
        &self.chunks
    }
    pub fn fact(&self) -> Option<u64> {
        self.fact
    }
    pub fn ds64_sample_count(&self) -> Option<u64> {
        self.ds64_sample_count
    }
    /// A single data chunk. Segmented wavl audio returns None.
    pub fn data_chunk(&self) -> Option<ChunkInfo> {
        self.timeline.is_none().then_some(self.data)
    }
    pub fn audio_chunk(&self) -> ChunkInfo {
        self.data
    }
    pub fn wavl_chunks(&self) -> Option<impl Iterator<Item = ChunkInfo> + '_> {
        self.timeline
            .as_ref()
            .map(|t| t.entries.iter().map(|e| e.chunk))
    }
    pub fn into_inner(self) -> R {
        self.inner
    }
    pub fn position(&self) -> u64 {
        self.pos
    }
    pub fn frames(&self) -> Result<u64, Error> {
        let f = self.format();
        f.sample_format()?;
        Ok(self
            .timeline
            .as_ref()
            .map_or(self.data.size / u64::from(f.block_align), |t| t.frames))
    }
    pub fn seek_frame(&mut self, frame: u64) -> Result<(), Error> {
        if frame > self.frames()? {
            return Err(Error::Invalid("frame position"));
        }
        self.failed = true;
        if let Some(index) = &self.timeline {
            if let Some(entry) = index.at(frame) {
                if entry.chunk.id == *b"data" {
                    self.inner.seek(SeekFrom::Start(
                        self.base
                            + entry.chunk.offset
                            + (frame - entry.start) * u64::from(self.format().block_align),
                    ))?;
                } else {
                    self.load_hold(entry.hold)?;
                }
            } else {
                self.inner.seek(SeekFrom::Start(
                    self.base + self.data.offset + self.data.size,
                ))?;
            }
        } else {
            let offset = frame * u64::from(self.format().block_align);
            self.inner
                .seek(SeekFrom::Start(self.base + self.data.offset + offset))?;
        }
        self.pos = frame;
        self.failed = false;
        Ok(())
    }
    /// Raw container bytes in complete frames, returned as a frame count.
    pub fn read_frames(&mut self, out: &mut [u8]) -> Result<usize, Error> {
        if self.failed {
            return Err(Error::Invalid(
                "reader requires a successful seek after I/O error",
            ));
        }
        let align = usize::from(self.format().block_align);
        let frames = (self.frames()? - self.pos).min((out.len() / align) as u64) as usize;
        self.failed = true;
        if self.timeline.is_some() {
            let mut done = 0;
            while done < frames {
                let entry = self
                    .timeline
                    .as_ref()
                    .unwrap()
                    .at(self.pos)
                    .ok_or(Error::Invalid("wavl frame index"))?;
                let skip = self.pos - entry.start;
                let n = (entry.frames - skip).min((frames - done) as u64) as usize;
                let dst = &mut out[done * align..(done + n) * align];
                if entry.chunk.id == *b"data" {
                    let offset = entry.chunk.offset + skip * align as u64;
                    self.inner.seek(SeekFrom::Start(self.base + offset))?;
                    self.inner.read_exact(dst)?;
                    self.hold.copy_from_slice(&dst[dst.len() - align..]);
                    self.hold_at = offset + dst.len() as u64 - align as u64;
                } else {
                    self.load_hold(entry.hold)?;
                    crate::timeline::repeat(dst, &self.hold);
                }
                done += n;
                self.pos += n as u64;
            }
            self.failed = false;
            return Ok(frames);
        }
        self.inner.read_exact(&mut out[..frames * align])?;
        self.pos += frames as u64;
        self.failed = false;
        Ok(frames)
    }
    pub fn read_i16(&mut self, out: &mut [i16]) -> Result<usize, Error> {
        crate::native_io::NativeIo::new(self.format())?.read_i16(out, |b| self.read_frames(b))
    }
    pub fn read_i32(&mut self, out: &mut [i32]) -> Result<usize, Error> {
        crate::native_io::NativeIo::new(self.format())?.read_i32(out, |b| self.read_frames(b))
    }
    /// Explicit integer expansion in container scale, using caller storage.
    pub fn read_integers(&mut self, out: &mut [i32]) -> Result<usize, Error> {
        crate::native_io::NativeIo::new(self.format())?.read_integers(out, |b| self.read_frames(b))
    }
    pub fn read_f32(&mut self, out: &mut [f32]) -> Result<usize, Error> {
        crate::native_io::NativeIo::new(self.format())?.read_f32(out, |b| self.read_frames(b))
    }
    pub fn read_f64(&mut self, out: &mut [f64]) -> Result<usize, Error> {
        crate::native_io::NativeIo::new(self.format())?.read_f64(out, |b| self.read_frames(b))
    }
    /// Read a retained metadata payload into a caller buffer, preserving audio position.
    pub fn read_metadata(&mut self, c: ChunkInfo, out: &mut [u8]) -> Result<usize, Error> {
        if !self.chunks.contains(&c)
            && !self
                .timeline
                .as_ref()
                .is_some_and(|t| t.entries.iter().any(|e| e.chunk == c))
        {
            return Err(Error::Invalid("unknown chunk descriptor"));
        }
        let absolute = self.base + c.offset;
        if c.size > self.end - absolute {
            return Err(Error::Invalid("metadata boundary"));
        }
        let n = c.size.min(out.len() as u64) as usize;
        self.failed = true;
        let restore = self.inner.stream_position()?;
        self.inner.seek(SeekFrom::Start(absolute))?;
        self.inner.read_exact(&mut out[..n])?;
        self.inner.seek(SeekFrom::Start(restore))?;
        self.failed = false;
        Ok(n)
    }
    fn load_hold(&mut self, offset: u64) -> Result<(), Error> {
        if offset != self.hold_at {
            if offset == 0 {
                let byte = if self.format().sample_format()? == SampleFormat::U8 {
                    128
                } else {
                    0
                };
                self.hold.fill(byte);
            } else {
                self.inner.seek(SeekFrom::Start(self.base + offset))?;
                self.inner.read_exact(&mut self.hold)?;
            }
            self.hold_at = offset;
        }
        Ok(())
    }
    // The compressed wrapper addresses bytes independently of native PCM pos.
    // Keep native reads poisoned until a native seek, if this Reader is reused.
    pub(crate) fn seek_encoded(&mut self, offset: u64) -> Result<(), Error> {
        if self.timeline.is_some() {
            return Err(Error::Unsupported("segmented encoded data"));
        }
        if offset > self.data.size {
            return Err(Error::Invalid("encoded data offset"));
        }
        self.failed = true;
        self.inner
            .seek(SeekFrom::Start(self.base + self.data.offset + offset))?;
        Ok(())
    }
    pub(crate) fn read_encoded(&mut self, offset: u64, out: &mut [u8]) -> Result<usize, Error> {
        self.seek_encoded(offset)?;
        let n = (self.data.size - offset).min(out.len() as u64) as usize;
        self.inner.read_exact(&mut out[..n])?;
        Ok(n)
    }
}
