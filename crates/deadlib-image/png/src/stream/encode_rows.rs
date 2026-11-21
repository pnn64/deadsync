use crate::{EncodeOptions, Error, Filter, Image, Samples, reserve, scanline, zlib};

/// Filter and compress an image into the stream that `zlib::compress` gives
/// for its whole filtered bytes, holding only rows and a compression window.
pub(crate) fn compress_image(image: &Image, options: &EncodeOptions) -> Result<Vec<u8>, Error> {
    let size = scanline::filtered_size(
        image.width,
        image.height,
        image.color_type,
        image.bit_depth,
        options.interlaced,
    )?;
    let mut rows = Rows::new(image, options.interlaced, options.filter)?;
    let mut compressor = zlib::Compressor::new(options.compression, size)?;
    while let Some(row) = rows.next_row()? {
        if row.zero {
            compressor.push(&row.bytes[..1]);
            compressor.push_zeros(row.bytes.len() - 1);
        } else {
            compressor.push(row.bytes);
        }
    }
    Ok(compressor.finish())
}

/// A filter type followed by its filtered row.
pub(super) struct FilteredRow<'a> {
    pub(super) bytes: &'a [u8],
    /// Every byte after the filter type is known to be zero.
    pub(super) zero: bool,
}

pub(super) struct Rows<'a> {
    image: &'a Image,
    pixels: Option<&'a [u8]>,
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
    /// `output` holds only zeros after its filter type.
    zeroed: bool,
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
        let pixels = match &image.samples {
            Samples::U8(pixels) if image.bit_depth == 8 && !interlaced => Some(pixels.as_slice()),
            _ => None,
        };
        Ok(Self {
            image,
            pixels,
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
            zeroed: false,
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
                    if self.pixels.is_none() {
                        resize(&mut self.row, bytes)?;
                        if self.filter != Filter::Sub && self.height > 1 {
                            resize(&mut self.previous, bytes)?;
                        }
                    }
                    if self.filter == Filter::Adaptive {
                        resize(&mut self.candidate, bytes + 1)?;
                    }
                }
                resize(&mut self.output, bytes + 1)?;
                self.zeroed = false;
                return Ok(true);
            }
            self.pass += 1;
        }
        Ok(false)
    }
    pub(super) fn next_row(&mut self) -> Result<Option<FilteredRow<'_>>, Error> {
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
            self.zeroed = false;
            return Ok(Some(FilteredRow {
                bytes: &self.output,
                zero: false,
            }));
        }
        let (row, previous) = if let Some(pixels) = self.pixels {
            let stride = self.output.len() - 1;
            let start = y * stride;
            (
                &pixels[start..start + stride],
                &pixels[start.saturating_sub(stride)..start],
            )
        } else {
            scanline::pack_row(
                self.image,
                y,
                self.x as usize,
                self.dx as usize,
                self.width,
                &mut self.row,
            );
            (self.row.as_slice(), self.previous.as_slice())
        };
        let bpp = (self.image.color_type.channels() * self.image.bit_depth as usize).div_ceil(8);
        let mut selected = self.filter as u8;
        let zero = self.filter == Filter::Adaptive && self.index != 0 && row == previous;
        if zero {
            // Up is zero; None/Sub can tie it only for an all-zero row.
            selected = if row.iter().all(|&byte| byte == 0) {
                0
            } else {
                2
            };
            if !self.zeroed {
                self.output[1..].fill(0);
            }
        } else if self.filter == Filter::Adaptive {
            let mut best = u64::MAX;
            for kind in 1..5 {
                if self.index == 0 && matches!(kind, 2 | 4) {
                    continue;
                }
                let score = if self.index == 0 {
                    scanline::filter_first::<true>(row, bpp, kind, &mut self.candidate[1..])
                } else {
                    scanline::filter_into::<true>(
                        row,
                        previous,
                        bpp,
                        kind,
                        &mut self.candidate[1..],
                    )
                };
                if score < best {
                    best = score;
                    selected = kind;
                    core::mem::swap(&mut self.candidate, &mut self.output);
                    if best == 0 {
                        break;
                    }
                }
            }
            if scanline::score_at_most(row, best) {
                selected = 0;
                self.output[1..].copy_from_slice(row);
            }
        } else {
            if self.index == 0 {
                scanline::filter_first::<false>(row, bpp, selected, &mut self.output[1..]);
            } else {
                scanline::filter_into::<false>(row, previous, bpp, selected, &mut self.output[1..]);
            }
        }
        self.output[0] = selected;
        self.zeroed = zero;
        self.index += 1;
        if self.pixels.is_none() && self.filter != Filter::Sub && self.index < self.height {
            core::mem::swap(&mut self.row, &mut self.previous);
        }
        Ok(Some(FilteredRow {
            bytes: &self.output,
            zero,
        }))
    }
    pub(super) fn memory_bytes(&self) -> usize {
        self.row.capacity()
            + self.previous.capacity()
            + self.candidate.capacity()
            + self.output.capacity()
    }
}
