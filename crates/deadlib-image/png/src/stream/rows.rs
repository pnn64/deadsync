use super::{ImageKind, Row, RowSamples};
use crate::{
    ColorType, Error, Image, PalettePolicy, Samples,
    codec::Header,
    reserve, scanline,
    zlib::{Inflater, StreamStatus},
};

fn unpack_row(depth: u8, row: &[u8], out: &mut [u8], step: usize) {
    match depth {
        1 => scanline::unpack::<1>(row, out, step),
        2 => scanline::unpack::<2>(row, out, step),
        4 => scanline::unpack::<4>(row, out, step),
        _ => unreachable!("validated packed PNG depth"),
    }
}

#[derive(Default)]
pub(super) struct RowBuffers {
    history: Vec<u8>,
    row: Vec<u8>,
    previous: Vec<u8>,
}

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
    pub(super) fn new(
        header: Header,
        kind: ImageKind,
        limit: usize,
        buffers: RowBuffers,
    ) -> Result<Self, Error> {
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
        let RowBuffers {
            history,
            mut row,
            mut previous,
        } = buffers;
        // Only reuse plain rows of the same length. A different first-pass
        // capacity would change the stream's reported peak storage.
        if header.interlaced || row.len().checked_mul(header.height as usize) != Some(expected) {
            row = Vec::new();
            previous = Vec::new();
        } else if header.height == 1 {
            previous = Vec::new();
        }
        let mut rows = Self {
            header,
            kind,
            inflater: Inflater::with_history(expected, history)?,
            row,
            previous,
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
                self.row.resize(bytes, 0);
                if self.height > 1 {
                    if bytes > self.previous.len() {
                        let additional = bytes - self.previous.len();
                        reserve(&mut self.previous, additional)?;
                    }
                    self.previous.resize(bytes, 0);
                }
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
            self.row_index += 1;
            if self.row_index == self.height {
                self.pass += 1;
                self.start_pass()?;
            } else {
                core::mem::swap(&mut self.row, &mut self.previous);
            }
        }
        Ok(())
    }
    /// Consume compressed bytes until one complete row is ready or input ends.
    pub(super) fn feed(
        &mut self,
        input: &[u8],
        palette: usize,
        policy: PalettePolicy,
        materialize_samples: bool,
    ) -> Result<(usize, bool), Error> {
        debug_assert!(!self.ready);
        let output = if self.complete {
            &mut [][..]
        } else {
            &mut self.row[self.have..]
        };
        let progress = self.inflater.process(input, output)?;
        self.have += progress.written;
        if !self.complete && self.have == self.row.len() {
            let filter = self.row[0];
            if filter != 0 {
                let bpp = (self.header.color.channels() * self.header.depth as usize).div_ceil(8);
                if self.row_index == 0 {
                    scanline::unfilter_first(&mut self.row[1..], bpp, filter)?;
                } else {
                    scanline::unfilter(&mut self.row[1..], &self.previous[1..], bpp, filter)?;
                }
            }
            let depth = self.header.depth;
            let check_indices = self.header.color == ColorType::Indexed
                && policy == PalettePolicy::Strict
                && palette < 1usize << depth;
            if depth == 16 && materialize_samples {
                let count = self.width * self.header.color.channels();
                self.native16.clear();
                reserve(&mut self.native16, count)?;
                self.native16.extend(
                    self.row[1..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .copied()
                        .map(u16::from_be_bytes),
                );
            } else if depth < 8 && (materialize_samples || check_indices) {
                let additional = self.width.saturating_sub(self.native8.len());
                reserve(&mut self.native8, additional)?;
                self.native8.resize(self.width, 0);
                unpack_row(depth, &self.row[1..], &mut self.native8, 1);
            }
            if check_indices {
                let values = if depth == 8 {
                    &self.row[1..]
                } else {
                    &self.native8
                };
                if crate::codec::indices_outside_palette(values, palette, depth) {
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
    pub(super) fn reader_copies_raw(&self) -> bool {
        self.header.depth == 16 || (self.header.depth < 8 && self.native8.is_empty())
    }
    pub(super) fn copy_raw_into(&self, image: &mut Image) -> Result<(), Error> {
        if let Samples::U8(out) = &mut image.samples {
            debug_assert!(self.header.depth < 8 && self.native8.is_empty());
            let width = self.header.width as usize;
            let y = self.y as usize + self.row_index * self.dy as usize;
            let start = y * width + self.x as usize;
            let end = (y + 1) * width;
            if !self.header.interlaced {
                out.resize(end, 0);
            }
            unpack_row(
                self.header.depth,
                &self.row[1..],
                &mut out[start..end],
                self.dx as usize,
            );
            return Ok(());
        }
        let Samples::U16(out) = &mut image.samples else {
            return Err(Error::Invalid("row sample representation"));
        };
        let words = self.row[1..].as_chunks::<2>().0;
        if !self.header.interlaced {
            out.extend(words.iter().copied().map(u16::from_be_bytes));
            return Ok(());
        }
        let channels = self.header.color.channels();
        let y = self.y as usize + self.row_index * self.dy as usize;
        let start = (y * self.header.width as usize + self.x as usize) * channels;
        let out = &mut out[start..];
        let step = self.dx as usize;
        if step == 1 {
            for (out, &word) in out.iter_mut().zip(words) {
                *out = u16::from_be_bytes(word);
            }
        } else {
            match channels {
                1 => scanline::scatter_words::<1>(words, out, step),
                2 => scanline::scatter_words::<2>(words, out, step),
                3 => scanline::scatter_words::<3>(words, out, step),
                _ => scanline::scatter_words::<4>(words, out, step),
            }
        }
        Ok(())
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
    pub(super) fn finish(self) -> Result<(ImageKind, RowBuffers), Error> {
        debug_assert!(!self.ready);
        self.inflater.finish()?;
        if !self.complete {
            return Err(Error::Invalid("image data length"));
        }
        let mut buffers = RowBuffers {
            history: self.inflater.into_history(),
            ..RowBuffers::default()
        };
        if !self.header.interlaced {
            buffers.row = self.row;
            buffers.previous = self.previous;
        }
        Ok((self.kind, buffers))
    }
    pub(super) fn buffers_can_grow(&self) -> bool {
        // A pass can grow the filter rows at setup and the native rows when
        // its first row finishes. Later rows only overwrite or swap buffers.
        self.row_index == 0
    }
    pub(super) fn memory_bytes(&self) -> usize {
        self.inflater.working_memory_bytes()
            + self.row.capacity()
            + self.previous.capacity()
            + self.native8.capacity()
            + self.native16.capacity() * 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PalettePolicy,
        zlib::{Compression, compress},
    };

    #[test]
    fn reused_row_storage_matches_fresh_rows_and_capacities() {
        let mut buffers = RowBuffers::default();
        let shapes = [
            (17, 3, 8, false),
            (17, 1, 8, false),
            (17, 5, 8, false),
            (29, 3, 8, false),
            (29, 3, 8, true),
            (29, 3, 8, false),
            (17, 3, 4, false),
            (18, 2, 4, false),
            (29, 3, 16, true),
        ];
        for (frame, (width, height, depth, interlaced)) in shapes.into_iter().enumerate() {
            let header = Header {
                width,
                height,
                color: ColorType::Grayscale,
                depth,
                interlaced,
            };
            let mut filtered = Vec::new();
            for &(x, y, dx, dy) in scanline::passes(interlaced) {
                let width = scanline::extent(width, x, dx);
                if width == 0 {
                    continue;
                }
                let bytes = scanline::row_bytes(width, 1, depth).unwrap();
                for row in 0..scanline::extent(height, y, dy) {
                    filtered.push(2);
                    filtered.resize(filtered.len() + bytes, (frame + row + 1) as u8);
                }
            }
            let encoded = compress(&filtered, Compression::Fast).unwrap();
            let kind = ImageKind::AnimationFrame(frame as u32);
            let mut reused = ImageRows::new(header, kind, 65536, buffers).unwrap();
            let mut fresh = ImageRows::new(header, kind, 65536, RowBuffers::default()).unwrap();
            let mut remaining = encoded.as_slice();
            loop {
                let before = reused.memory_bytes();
                reused.advance().unwrap();
                fresh.advance().unwrap();
                assert_eq!(reused.memory_bytes(), fresh.memory_bytes());
                let (used, ready) = reused
                    .feed(remaining, 0, PalettePolicy::Strict, true)
                    .unwrap();
                assert_eq!(
                    (used, ready),
                    fresh
                        .feed(remaining, 0, PalettePolicy::Strict, true)
                        .unwrap()
                );
                assert_eq!(reused.memory_bytes(), fresh.memory_bytes());
                if !reused.buffers_can_grow() {
                    assert_eq!(reused.memory_bytes(), before);
                }
                remaining = &remaining[used..];
                if ready {
                    assert_eq!(reused.row(), fresh.row());
                } else {
                    assert!(reused.is_done());
                    assert!(fresh.is_done());
                    assert!(remaining.is_empty());
                    break;
                }
            }
            buffers = reused.finish().unwrap().1;
        }
    }
}
