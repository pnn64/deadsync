use super::{ImageKind, Row, RowSamples};
use crate::{
    ColorType, Error,
    codec::Header,
    reserve, scanline,
    zlib::{Inflater, StreamStatus},
};

pub(super) struct ImageRows {
    header: Header,
    kind: ImageKind,
    inflater: Inflater,
    row: Vec<u8>,
    previous: Vec<u8>,
    native8: Vec<u8>,
    native16: Vec<u16>,
    pass: usize,
    row_index: usize,
    width: usize,
    height: usize,
    x: u32,
    y: u32,
    dx: u32,
    dy: u32,
    have: usize,
    ready: bool,
    complete: bool,
}
impl ImageRows {
    pub(super) fn new(header: Header, kind: ImageKind, limit: usize) -> Result<Self, Error> {
        let expected = scanline::filtered_size(
            header.width,
            header.height,
            header.color,
            header.depth,
            header.interlaced,
        )?;
        if expected > limit || header.sample_bytes()? > limit {
            return Err(Error::LimitExceeded);
        }
        let mut rows = Self {
            header,
            kind,
            inflater: Inflater::new(expected)?,
            row: Vec::new(),
            previous: Vec::new(),
            native8: Vec::new(),
            native16: Vec::new(),
            pass: 0,
            row_index: 0,
            width: 0,
            height: 0,
            x: 0,
            y: 0,
            dx: 0,
            dy: 0,
            have: 0,
            ready: false,
            complete: false,
        };
        rows.start_pass()?;
        Ok(rows)
    }
    fn start_pass(&mut self) -> Result<(), Error> {
        let passes = scanline::passes(self.header.interlaced);
        while self.pass < passes.len() {
            let (x, y, dx, dy) = passes[self.pass];
            self.width = scanline::extent(self.header.width, x, dx);
            self.height = scanline::extent(self.header.height, y, dy);
            if self.width != 0 && self.height != 0 {
                self.x = x;
                self.y = y;
                self.dx = dx;
                self.dy = dy;
                self.row_index = 0;
                let bytes = scanline::row_bytes(
                    self.width,
                    self.header.color.channels(),
                    self.header.depth,
                )?
                .checked_add(1)
                .ok_or(Error::LimitExceeded)?;
                if bytes > self.row.len() {
                    let additional = bytes - self.row.len();
                    reserve(&mut self.row, additional)?;
                }
                if bytes > self.previous.len() {
                    let additional = bytes - self.previous.len();
                    reserve(&mut self.previous, additional)?;
                }
                self.row.resize(bytes, 0);
                self.previous.resize(bytes, 0);
                self.previous.fill(0);
                return Ok(());
            }
            self.pass += 1;
        }
        self.complete = true;
        Ok(())
    }
    pub(super) fn advance(&mut self) -> Result<(), Error> {
        if self.ready {
            self.ready = false;
            self.have = 0;
            core::mem::swap(&mut self.row, &mut self.previous);
            self.row_index += 1;
            if self.row_index == self.height {
                self.pass += 1;
                self.start_pass()?;
            }
        }
        Ok(())
    }
    /// Consume compressed bytes until one complete row is ready or input ends.
    pub(super) fn feed(
        &mut self,
        input: &[u8],
        palette: usize,
        policy: crate::PalettePolicy,
    ) -> Result<(usize, bool), Error> {
        self.advance()?;
        let output = if self.complete {
            &mut [][..]
        } else {
            &mut self.row[self.have..]
        };
        let progress = self.inflater.process(input, output)?;
        self.have += progress.written;
        if !self.complete && self.have == self.row.len() {
            let bpp = (self.header.color.channels() * self.header.depth as usize).div_ceil(8);
            let filter = self.row[0];
            scanline::unfilter(&mut self.row[1..], &self.previous[1..], bpp, filter)?;
            let count = self.width * self.header.color.channels();
            if self.header.depth == 16 {
                self.native16.clear();
                reserve(&mut self.native16, count)?;
                self.native16.extend(
                    self.row[1..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|&b| u16::from_be_bytes(b)),
                );
            } else if self.header.depth < 8 {
                self.native8.clear();
                reserve(&mut self.native8, count)?;
                let depth = self.header.depth as usize;
                for n in 0..count {
                    self.native8.push(
                        (self.row[1 + n * depth / 8] >> (8 - depth - n * depth % 8))
                            & ((1 << depth) - 1),
                    );
                }
            }
            if self.header.color == ColorType::Indexed && policy == crate::PalettePolicy::Strict {
                let values = if self.header.depth == 8 {
                    &self.row[1..]
                } else {
                    &self.native8
                };
                if values.iter().any(|&v| v as usize >= palette) {
                    return Err(Error::Invalid("palette index outside PLTE"));
                }
            }
            self.ready = true;
            return Ok((progress.consumed, true));
        }
        if progress.status == StreamStatus::Done && !self.complete {
            return Err(Error::Invalid("image data length"));
        }
        if progress.status == StreamStatus::NeedOutput {
            return Err(Error::Invalid("extra image data"));
        }
        Ok((progress.consumed, false))
    }
    pub(super) fn row(&self) -> Row<'_> {
        let samples = if self.header.depth == 16 {
            RowSamples::U16(&self.native16)
        } else if self.header.depth == 8 {
            RowSamples::U8(&self.row[1..])
        } else {
            RowSamples::U8(&self.native8)
        };
        Row {
            image: self.kind,
            pass: if self.header.interlaced {
                (self.pass + 1) as u8
            } else {
                0
            },
            x: self.x,
            y: self.y + self.row_index as u32 * self.dy,
            x_step: self.dx,
            width: self.width as u32,
            samples,
        }
    }
    pub(super) fn is_done(&self) -> bool {
        self.inflater.is_done()
    }
    pub(super) fn finish(&mut self) -> Result<ImageKind, Error> {
        self.advance()?;
        self.inflater.finish()?;
        if !self.complete {
            return Err(Error::Invalid("image data length"));
        }
        Ok(self.kind)
    }
    pub(super) fn memory_bytes(&self) -> usize {
        self.inflater.working_memory_bytes()
            + self.row.capacity()
            + self.previous.capacity()
            + self.native8.capacity()
            + self.native16.capacity() * 2
    }
}
