use crate::{
    BOS, CONTINUED, EOS, Error, PageHeader, UNKNOWN_GRANULE_POSITION, reserve, write_page,
};
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
    serial: u32,
    sequence: u32,
    lacing: [u8; 255],
    segments: usize,
    body: Vec<u8>,
    granule: u64,
    last_granule: u64,
    continued: bool,
    packet_open: bool,
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
        Self {
            inner,
            serial,
            sequence: 0,
            lacing: [0; 255],
            segments: 0,
            body: Vec::new(),
            granule: UNKNOWN_GRANULE_POSITION,
            last_granule: 0,
            continued: false,
            packet_open: false,
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
        let mut position = 0;
        loop {
            if self.segments == 255 {
                self.emit(false, &[])?;
            }
            if self.segments == 0 {
                self.continued = self.packet_open;
            }
            let len = (data.len() - position).min((255 - self.segments) * 255);
            let payload = &data[position..position + len];
            let mut tail = len;
            if len >= 255 {
                let full = len / 255;
                self.lacing[self.segments..self.segments + full].fill(255);
                self.segments += full;
                tail -= full * 255;
            }
            position += len;
            self.packet_open = self.segments == 255;
            if !self.packet_open {
                self.lacing[self.segments] = tail as u8;
                self.segments += 1;
                self.granule = granule;
                self.last_granule = granule;
            }
            let flush = self.packet_open
                || first_packet
                || eos
                || self.body.len() + len >= self.options.target_page_body;
            let page_eos = eos && !self.packet_open;
            if self.body.is_empty() && flush {
                self.emit(page_eos, payload)?;
            } else {
                reserve(&mut self.body, len)?;
                self.body.extend_from_slice(payload);
                if flush {
                    self.emit(page_eos, &[])?;
                }
            }
            if !self.packet_open {
                return Ok(());
            }
        }
    }
    fn emit(&mut self, eos: bool, direct: &[u8]) -> Result<(), Error> {
        let flags = if !self.started { BOS } else { 0 }
            | if self.continued { CONTINUED } else { 0 }
            | if eos { EOS } else { 0 };
        let granule = if self.segments == 0 && eos {
            self.last_granule
        } else {
            self.granule
        };
        write_page(
            &mut self.inner,
            PageHeader {
                flags,
                granule_position: granule,
                serial: self.serial,
                sequence: self.sequence,
            },
            &self.lacing[..self.segments],
            if self.body.is_empty() {
                direct
            } else {
                &self.body
            },
        )?;
        self.sequence = self.sequence.wrapping_add(1);
        self.started = true;
        self.ended = eos;
        self.segments = 0;
        self.body.clear();
        self.granule = UNKNOWN_GRANULE_POSITION;
        self.continued = self.packet_open;
        Ok(())
    }
    /// Force a page boundary without ending the logical stream.
    pub fn flush_page(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if self.segments == 0 {
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
