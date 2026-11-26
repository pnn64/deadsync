use alloc::{vec, vec::Vec};

use crate::PictureKind;

/// One visible component of a YCbCr 4:2:0 picture.
#[derive(Debug)]
pub struct Plane {
    width: usize,
    height: usize,
    stride: usize,
    samples: Vec<u8>,
}

impl Plane {
    fn new(width: usize, height: usize, alignment: usize) -> Self {
        let stride = width.next_multiple_of(alignment);
        Self {
            width,
            height,
            stride,
            samples: vec![0; stride * height.next_multiple_of(alignment)],
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn rows(&self) -> impl ExactSizeIterator<Item = &[u8]> + '_ {
        self.samples
            .chunks_exact(self.stride)
            .take(self.height)
            .map(|row| &row[..self.width])
    }

    pub(crate) fn write(&mut self, [x, y]: [usize; 2], block: &[i16; 64], intra: bool) {
        for (row, values) in block.as_chunks::<8>().0.iter().enumerate() {
            let start = (y + row) * self.stride + x;
            for (pixel, &value) in self.samples[start..start + 8].iter_mut().zip(values) {
                let base = if intra { 0 } else { i32::from(*pixel) };
                *pixel = (base + i32::from(value)).clamp(0, 255) as u8;
            }
        }
    }

    pub(crate) fn predict<const W: usize>(
        &mut self,
        source: &Self,
        [x, y]: [usize; 2],
        [dx, dy]: [i32; 2],
        truncate: bool,
    ) {
        let origin_x = x as i32 + (dx >> 1);
        let origin_y = y as i32 + (dy >> 1);
        let fraction = (((dy & 1) << 1) | (dx & 1)) as usize;
        let span = W + (fraction & 1);
        let rows = W + (fraction >> 1);
        let dst = &mut self.samples[y * self.stride + x..];
        if origin_x >= 0
            && origin_y >= 0
            && origin_x as usize + span <= source.width
            && origin_y as usize + rows <= source.height
        {
            let src = &source.samples[origin_y as usize * source.stride + origin_x as usize..];
            crate::motion::predict::<W>(dst, self.stride, src, source.stride, fraction, truncate);
        } else {
            let mut tile = [0; 17 * 17];
            source.extend_edge([origin_x, origin_y], span, rows, &mut tile);
            crate::motion::predict::<W>(dst, self.stride, &tile, span, fraction, truncate);
        }
    }

    #[cold]
    fn extend_edge(&self, [x, y]: [i32; 2], span: usize, rows: usize, tile: &mut [u8]) {
        for row in 0..rows {
            let sy = (y + row as i32).clamp(0, self.height as i32 - 1) as usize;
            let src = &self.samples[sy * self.stride..][..self.width];
            let dst = &mut tile[row * span..][..span];
            for (column, sample) in dst.iter_mut().enumerate() {
                let sx = (x + column as i32).clamp(0, self.width as i32 - 1) as usize;
                *sample = src[sx];
            }
        }
    }
}

/// A decoded progressive picture with 8-bit Y, Cb and Cr planes.
#[derive(Debug)]
pub struct Frame {
    pub(crate) planes: [Plane; 3],
    kind: PictureKind,
}

impl Frame {
    pub(crate) fn new(width: usize, height: usize, kind: PictureKind) -> Self {
        let [cw, ch] = [width.div_ceil(2), height.div_ceil(2)];
        Self {
            planes: [
                Plane::new(width, height, 16),
                Plane::new(cw, ch, 8),
                Plane::new(cw, ch, 8),
            ],
            kind,
        }
    }

    pub fn width(&self) -> usize {
        self.planes[0].width
    }

    pub fn height(&self) -> usize {
        self.planes[0].height
    }

    /// The Y, Cb and Cr planes, in that order.
    pub fn planes(&self) -> &[Plane; 3] {
        &self.planes
    }

    pub fn picture_kind(&self) -> PictureKind {
        self.kind
    }
}
