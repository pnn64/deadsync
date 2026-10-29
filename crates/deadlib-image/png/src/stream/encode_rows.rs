use crate::{Error, Filter, Image, reserve, scanline};

pub(super) struct Rows<'a> {
    image: &'a Image,
    interlaced: bool,
    filter: Filter,
    pass: usize,
    index: usize,
    initialized: bool,
    width: usize,
    height: usize,
    x: u32,
    y: u32,
    dx: u32,
    dy: u32,
    row: Vec<u8>,
    previous: Vec<u8>,
    candidate: Vec<u8>,
    output: Vec<u8>,
}
fn resize(bytes: &mut Vec<u8>, length: usize) -> Result<(), Error> {
    let additional = length.saturating_sub(bytes.len());
    reserve(bytes, additional)?;
    bytes.resize(length, 0);
    Ok(())
}
impl<'a> Rows<'a> {
    pub(super) fn new(image: &'a Image, interlaced: bool, filter: Filter) -> Result<Self, Error> {
        scanline::filtered_size(
            image.width,
            image.height,
            image.color_type,
            image.bit_depth,
            interlaced,
        )?;
        Ok(Self {
            image,
            interlaced,
            filter,
            pass: 0,
            index: 0,
            initialized: false,
            width: 0,
            height: 0,
            x: 0,
            y: 0,
            dx: 0,
            dy: 0,
            row: Vec::new(),
            previous: Vec::new(),
            candidate: Vec::new(),
            output: Vec::new(),
        })
    }
    fn start_pass(&mut self) -> Result<bool, Error> {
        let passes = scanline::passes(self.interlaced);
        while self.pass < passes.len() {
            let (x, y, dx, dy) = passes[self.pass];
            self.width = scanline::extent(self.image.width, x, dx);
            self.height = scanline::extent(self.image.height, y, dy);
            if self.width != 0 && self.height != 0 {
                self.x = x;
                self.y = y;
                self.dx = dx;
                self.dy = dy;
                self.index = 0;
                self.initialized = true;
                let bytes = scanline::row_bytes(
                    self.width,
                    self.image.color_type.channels(),
                    self.image.bit_depth,
                )?;
                if self.filter != Filter::None {
                    resize(&mut self.row, bytes)?;
                    if self.filter != Filter::Sub && self.height > 1 {
                        resize(&mut self.previous, bytes)?;
                    }
                    if self.filter == Filter::Adaptive {
                        resize(&mut self.candidate, bytes + 1)?;
                    }
                }
                resize(&mut self.output, bytes + 1)?;
                return Ok(true);
            }
            self.pass += 1;
        }
        Ok(false)
    }
    pub(super) fn next_row(&mut self) -> Result<Option<&[u8]>, Error> {
        if self.initialized && self.index == self.height {
            self.pass += 1;
            self.initialized = false;
        }
        if !self.initialized && !self.start_pass()? {
            return Ok(None);
        }
        let y = self.y as usize + self.index * self.dy as usize;
        if self.filter == Filter::None {
            scanline::pack_row(
                self.image,
                y,
                self.x as usize,
                self.dx as usize,
                self.width,
                &mut self.output[1..],
            );
            self.output[0] = 0;
            self.index += 1;
            return Ok(Some(&self.output));
        }
        scanline::pack_row(
            self.image,
            y,
            self.x as usize,
            self.dx as usize,
            self.width,
            &mut self.row,
        );
        let bpp = (self.image.color_type.channels() * self.image.bit_depth as usize).div_ceil(8);
        let mut selected = self.filter as u8;
        if self.filter == Filter::Adaptive {
            let mut best = u64::MAX;
            for kind in 0..5 {
                if self.index == 0 && matches!(kind, 2 | 4) {
                    continue;
                }
                let score = if self.index == 0 {
                    scanline::filter_first::<true>(&self.row, bpp, kind, &mut self.candidate[1..])
                } else {
                    scanline::filter_into::<true>(
                        &self.row,
                        &self.previous,
                        bpp,
                        kind,
                        &mut self.candidate[1..],
                    )
                };
                if score < best {
                    best = score;
                    selected = kind;
                    core::mem::swap(&mut self.candidate, &mut self.output);
                }
            }
        } else {
            if self.index == 0 {
                scanline::filter_first::<false>(&self.row, bpp, selected, &mut self.output[1..]);
            } else {
                scanline::filter_into::<false>(
                    &self.row,
                    &self.previous,
                    bpp,
                    selected,
                    &mut self.output[1..],
                );
            }
        }
        self.output[0] = selected;
        self.index += 1;
        if self.filter != Filter::Sub && self.index < self.height {
            core::mem::swap(&mut self.row, &mut self.previous);
        }
        Ok(Some(&self.output))
    }
    pub(super) fn memory_bytes(&self) -> usize {
        self.row.capacity()
            + self.previous.capacity()
            + self.candidate.capacity()
            + self.output.capacity()
    }
}
