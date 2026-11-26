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

    fn sample(&self, x: i32, y: i32) -> u16 {
        let x = x.clamp(0, self.width as i32 - 1) as usize;
        let y = y.clamp(0, self.height as i32 - 1) as usize;
        u16::from(self.samples[y * self.stride + x])
    }

    pub(crate) fn predict(
        &mut self,
        source: &Self,
        [x, y]: [usize; 2],
        size: usize,
        [dx, dy]: [i32; 2],
        truncate: bool,
    ) {
        let origin_x = x as i32 + (dx >> 1);
        let origin_y = y as i32 + (dy >> 1);
        let fraction = (dx & 1, dy & 1);
        let bias = u16::from(!truncate);
        for row in 0..size {
            let start = (y + row) * self.stride + x;
            let sy = origin_y + row as i32;
            for (col, pixel) in self.samples[start..start + size].iter_mut().enumerate() {
                let sx = origin_x + col as i32;
                let a = source.sample(sx, sy);
                let value = match fraction {
                    (0, 0) => a,
                    (1, 0) => (a + source.sample(sx + 1, sy) + bias) >> 1,
                    (0, 1) => (a + source.sample(sx, sy + 1) + bias) >> 1,
                    _ => {
                        (a + source.sample(sx, sy + 1)
                            + source.sample(sx + 1, sy)
                            + source.sample(sx + 1, sy + 1)
                            + 1
                            + bias)
                            >> 2
                    }
                };
                *pixel = value as u8;
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
