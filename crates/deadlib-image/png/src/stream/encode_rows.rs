use crate::{EncodeOptions, Error, Filter, Image, Samples, reserve, scanline, zlib};

/// Filter and compress an image into the stream that `zlib::compress` gives
/// for its whole filtered bytes, holding only rows and a compression window.
pub(crate) fn compress_image(
    image: &Image,
    options: &EncodeOptions,
    buffer: Vec<u8>,
) -> Result<Vec<u8>, Error> {
    let size = scanline::filtered_size(
        image.width,
        image.height,
        image.color_type,
        image.bit_depth,
        options.interlaced,
    )?;
    let mut compressor = zlib::Compressor::new(options.compression, size, buffer)?;
    if options.compression == zlib::Compression::Rle
        && !options.interlaced
        && options.filter == Filter::None
        && image.bit_depth == 8
        && let Samples::U8(pixels) = &image.samples
    {
        let stride = image.width as usize * image.color_type.channels();
        for row in pixels.chunks_exact(stride) {
            compressor.push_zeros(1);
            compressor.push(row);
        }
        return Ok(compressor.finish());
    }
    let mut rows = Rows::new(image, options.interlaced, options.filter)?;
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
    /// The filter the previous adaptive row chose, tried first.
    last_kind: u8,
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
            last_kind: 1,
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
            // The lowest score wins and ties keep the earlier filter. Trying
            // the previous row's choice first usually sets a tight limit, so
            // the other candidates stop early. A candidate before the current
            // best also wins a tie, so its limit is one higher.
            let mut best = u64::MAX;
            let mut best_kind = 5;
            let first = self.last_kind;
            for kind in core::iter::once(first).chain((1..5).filter(|&kind| kind != first)) {
                if self.index == 0 && matches!(kind, 2 | 4) {
                    continue;
                }
                let limit = if kind < best_kind {
                    best.saturating_add(1)
                } else {
                    best
                };
                if limit == 0 {
                    continue;
                }
                let score = if self.index == 0 {
                    scanline::filter_first::<true>(row, bpp, kind, &mut self.candidate[1..], limit)
                } else {
                    scanline::filter_into::<true>(
                        row,
                        previous,
                        bpp,
                        kind,
                        &mut self.candidate[1..],
                        limit,
                    )
                };
                if score < limit {
                    best = score;
                    best_kind = kind;
                    core::mem::swap(&mut self.candidate, &mut self.output);
                }
            }
            selected = best_kind;
            self.last_kind = best_kind;
            if scanline::score_at_most(row, best) {
                selected = 0;
                self.output[1..].copy_from_slice(row);
            }
        } else {
            if self.index == 0 {
                scanline::filter_first::<false>(row, bpp, selected, &mut self.output[1..], 0);
            } else {
                scanline::filter_into::<false>(
                    row,
                    previous,
                    bpp,
                    selected,
                    &mut self.output[1..],
                    0,
                );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColorType;

    /// Adaptive rows chosen by scoring every filter in full, in order.
    fn reference(pixels: &[u8], stride: usize) -> Vec<Vec<u8>> {
        let mut rows = Vec::new();
        for (index, row) in pixels.chunks(stride).enumerate() {
            let previous = &pixels[index.saturating_sub(1) * stride..][..stride];
            let mut out = vec![0; stride + 1];
            if index != 0 && row == previous {
                out[0] = if row.iter().all(|&byte| byte == 0) {
                    0
                } else {
                    2
                };
                rows.push(out);
                continue;
            }
            let (mut best, mut selected) = (u64::MAX, 0);
            let mut candidate = vec![0; stride];
            for kind in 1..5 {
                if index == 0 && matches!(kind, 2 | 4) {
                    continue;
                }
                let score = if index == 0 {
                    scanline::filter_first::<true>(row, 4, kind, &mut candidate, u64::MAX)
                } else {
                    scanline::filter_into::<true>(row, previous, 4, kind, &mut candidate, u64::MAX)
                };
                if score < best {
                    (best, selected) = (score, kind);
                    out[1..].copy_from_slice(&candidate);
                }
            }
            if scanline::score_at_most(row, best) {
                selected = 0;
                out[1..].copy_from_slice(row);
            }
            out[0] = selected;
            rows.push(out);
        }
        rows
    }

    #[test]
    fn adaptive_choices_match_full_in_order_scoring() {
        let (width, height) = (301, 64);
        let stride = width * 4;
        let mut state = 0x2545_f491_u32;
        let mut pixels: Vec<u8> = Vec::new();
        for y in 0..height {
            let start = pixels.len();
            for x in 0..stride {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let above = if y == 0 {
                    0
                } else {
                    pixels[start - stride + x]
                };
                pixels.push(match y % 8 {
                    0 => state as u8,
                    // Up wins: small residuals against the row above.
                    1 if x < 4 => 0,
                    1 => above.wrapping_add((x % 4) as u8),
                    // The row above shifted one pixel left makes every filter
                    // produce the same bytes, so Sub must win the tie even
                    // when the previous row's choice is tried first.
                    2 => pixels.get(start - stride + x + 4).copied().unwrap_or(7),
                    // Constant and zero rows tie Sub with Up and None.
                    3 => 0,
                    4 => 9,
                    5 => above,
                    6 => (x / 4 * 3) as u8,
                    _ => (state as u8) & 3,
                });
            }
        }
        let image = Image {
            width: width as u32,
            height: height as u32,
            color_type: ColorType::Rgba,
            bit_depth: 8,
            samples: Samples::U8(pixels.clone()),
        };
        let mut rows = Rows::new(&image, false, Filter::Adaptive).unwrap();
        for (index, expected) in reference(&pixels, stride).iter().enumerate() {
            let row = rows.next_row().unwrap().unwrap();
            assert_eq!(row.bytes, &expected[..], "row {index}");
        }
        assert!(rows.next_row().unwrap().is_none());
    }
}
