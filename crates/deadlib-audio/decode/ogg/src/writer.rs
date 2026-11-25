use crate::page::{FIXED_HEADER_SIZE, MAX_HEADER_SIZE, write_page_inner};
use crate::{BOS, CONTINUED, EOS, Error, UNKNOWN_GRANULE_POSITION, reserve};
use std::io::Write;

#[derive(Clone, Copy, Debug)]
pub struct WriteOptions {
    /// Flush after a completed packet reaches this body size. Pages may be
    /// larger when a packet spans pages; the hard limit is 255 segments.
    pub target_page_body: usize,
}
impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            target_page_body: 8192,
        }
    }
}

/// Streaming paginator for one logical stream. Multiple instances can feed
/// page buffers into a physical multiplexer in codec-defined timestamp order.
/// The first packet is always isolated on a BOS page, as mapping rules require.
pub struct PacketWriter<W> {
    inner: W,
    sequence: u32,
    serial_crc: (u32, u32),
    header: [u8; MAX_HEADER_SIZE],
    header_len: usize,
    body: Vec<u8>,
    granule: u64,
    last_granule: u64,
    continued: bool,
    started: bool,
    ended: bool,
    failed: bool,
    options: WriteOptions,
}
impl<W: Write> PacketWriter<W> {
    pub fn new(inner: W, serial: u32) -> Self {
        Self::with_options(inner, serial, WriteOptions::default())
    }
    pub fn with_options(inner: W, serial: u32, options: WriteOptions) -> Self {
        let mut header = [0; MAX_HEADER_SIZE];
        header[..4].copy_from_slice(b"OggS");
        header[14..18].copy_from_slice(&serial.to_le_bytes());
        Self {
            inner,
            sequence: 0,
            serial_crc: crate::crc::serial_crc(serial),
            header,
            header_len: FIXED_HEADER_SIZE,
            body: Vec::new(),
            granule: UNKNOWN_GRANULE_POSITION,
            last_granule: 0,
            continued: false,
            started: false,
            ended: false,
            failed: false,
            options,
        }
    }
    pub fn get_ref(&self) -> &W {
        &self.inner
    }
    /// Does not implicitly finish or flush the underlying writer.
    pub fn into_inner(self) -> W {
        self.inner
    }

    pub fn write_packet(&mut self, data: &[u8], granule: u64, eos: bool) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if self.ended {
            return Err(Error::Invalid("packet after EOS"));
        }
        if granule == UNKNOWN_GRANULE_POSITION {
            return Err(Error::Invalid("packet requires a known granule"));
        }
        if !self.started && data.len() >= 255 * 255 {
            return Err(Error::Invalid("initial header must fit a single page"));
        }
        let result = self.packet_inner(data, granule, eos);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn packet_inner(&mut self, data: &[u8], granule: u64, eos: bool) -> Result<(), Error> {
        let first_packet = !self.started;
        if self.header_len == MAX_HEADER_SIZE {
            self.emit(false, &[])?;
        }
        let available = (MAX_HEADER_SIZE - self.header_len) * 255;
        let remaining = if data.len() >= available {
            self.write_span_pages(data, available)?
        } else {
            data
        };
        let len = remaining.len();
        let mut tail = len;
        if len >= 255 {
            let full = len / 255;
            self.header[self.header_len..self.header_len + full].fill(255);
            self.header_len += full;
            tail -= full * 255;
        }
        self.header[self.header_len] = tail as u8;
        self.header_len += 1;
        self.granule = granule;
        self.last_granule = granule;

        let flush = first_packet || eos || self.body.len() + len >= self.options.target_page_body;
        if self.body.is_empty() && flush {
            return self.emit(eos, remaining);
        }
        self.append_body(remaining, data.len())?;
        if flush {
            self.emit(eos, &[])?;
        }
        Ok(())
    }
    #[inline(never)]
    fn write_span_pages<'a>(
        &mut self,
        data: &'a [u8],
        mut available: usize,
    ) -> Result<&'a [u8], Error> {
        let mut remaining = data;
        while remaining.len() >= available {
            let (payload, rest) = remaining.split_at(available);
            self.header[self.header_len..].fill(255);
            self.header_len = MAX_HEADER_SIZE;
            if self.body.is_empty() {
                self.emit(false, payload)?;
            } else {
                if payload.len() > self.body.capacity() - self.body.len() {
                    grow_body(&mut self.body, payload.len())?;
                }
                self.body.extend_from_slice(payload);
                self.emit(false, &[])?;
            }
            remaining = rest;
            available = 255 * 255;
        }
        Ok(remaining)
    }
    fn append_body(&mut self, data: &[u8], packet_len: usize) -> Result<(), Error> {
        let len = data.len();
        if len > self.body.capacity() - self.body.len() {
            // Reserve around the soft target for packets that fit a page.
            let additional = if self.body.capacity() == 0 && packet_len < 255 * 255 {
                self.options
                    .target_page_body
                    .saturating_add(len)
                    .min(255 * 255)
            } else {
                len
            };
            reserve(&mut self.body, additional)?;
        }
        self.body.extend_from_slice(data);
        Ok(())
    }
    fn emit(&mut self, eos: bool, direct: &[u8]) -> Result<(), Error> {
        let flags = if !self.started { BOS } else { 0 }
            | if self.continued { CONTINUED } else { 0 }
            | if eos { EOS } else { 0 };
        let granule = if self.header_len == FIXED_HEADER_SIZE && eos {
            self.last_granule
        } else {
            self.granule
        };
        let header = &mut self.header[..self.header_len];
        header[5] = flags;
        header[6..14].copy_from_slice(&granule.to_le_bytes());
        header[18..22].copy_from_slice(&self.sequence.to_le_bytes());
        header[26] = (self.header_len - FIXED_HEADER_SIZE) as u8;
        write_page_inner::<_, true>(
            &mut self.inner,
            header,
            if self.body.is_empty() {
                direct
            } else {
                &self.body
            },
            self.serial_crc,
        )?;
        self.sequence = self.sequence.wrapping_add(1);
        self.started = true;
        self.ended = eos;
        self.continued = header[FIXED_HEADER_SIZE..].last() == Some(&255);
        self.header_len = FIXED_HEADER_SIZE;
        self.body.clear();
        self.granule = UNKNOWN_GRANULE_POSITION;
        Ok(())
    }
    /// Force a page boundary without ending the logical stream.
    pub fn flush_page(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if self.header_len == FIXED_HEADER_SIZE {
            return Ok(());
        }
        let result = self.emit(false, &[]);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    /// Set EOS on the buffered page, or emit a legal nil EOS page if all
    /// packets have already been flushed. Does not call `Write::flush`.
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if self.ended {
            return Ok(());
        }
        if !self.started {
            return Err(Error::Invalid("logical stream has no initial packet"));
        }
        let result = self.emit(true, &[]);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}

#[cold]
fn grow_body(body: &mut Vec<u8>, additional: usize) -> Result<(), Error> {
    // Lacing bounds any buffered body at one full page.
    let max = 255 * 255;
    if body.capacity() > max / 2 && body.try_reserve_exact(max - body.len()).is_ok() {
        return Ok(());
    }
    reserve(body, additional)
}
