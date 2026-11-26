use alloc::{vec, vec::Vec};

use crate::headers::{PictureHeader, Sequence};
use crate::PictureKind;

/// An 8-bit sample plane with padded rows.
#[derive(Debug)]
pub struct Plane {
    width: usize,
    height: usize,
    stride: usize,
    samples: Vec<u8>,
}

impl Plane {
    fn new(width: usize, height: usize, block_size: usize, rows: usize) -> Self {
        let stride = width.next_multiple_of(block_size);
        Self {
            width,
            height,
            stride,
            samples: vec![0; stride * rows],
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

    pub(crate) fn block(&mut self, x: usize, y: usize, block: &[i16; 64], intra: bool) {
        for (row, coeff) in block.as_chunks::<8>().0.iter().enumerate() {
            let start = (y + row) * self.stride + x;
            for (pixel, &residual) in self.samples[start..start + 8].iter_mut().zip(coeff) {
                let base = if intra { 0 } else { i32::from(*pixel) };
                *pixel = (base + i32::from(residual)).clamp(0, 255) as u8;
            }
        }
    }

    fn sample(&self, x: i32, y: i32) -> i32 {
        let x = x.clamp(0, self.stride as i32 - 1) as usize;
        let y = y.clamp(0, (self.samples.len() / self.stride) as i32 - 1) as usize;
        i32::from(self.samples[y * self.stride + x])
    }

    fn predict(&mut self, x: usize, y: usize, reference: Prediction<'_>, plane: usize) {
        let source = &reference.frame.planes[plane];
        let (size, motion) = if plane == 0 {
            (16, reference.motion)
        } else {
            (8, [reference.motion[0] / 2, reference.motion[1] / 2])
        };
        let source_x = x as i32 + (motion[0] >> 1);
        let source_y = y as i32 + (motion[1] >> 1);
        let subpixel = (motion[0] & 1, motion[1] & 1);
        for row in 0..size {
            let start = (y + row) * self.stride + x;
            let sy = source_y + row as i32;
            for (col, pixel) in self.samples[start..start + size].iter_mut().enumerate() {
                let sx = source_x + col as i32;
                let a = source.sample(sx, sy);
                let prediction = match subpixel {
                    (0, 0) => a,
                    (1, 0) => (a + source.sample(sx + 1, sy) + 1) >> 1,
                    (0, 1) => (a + source.sample(sx, sy + 1) + 1) >> 1,
                    _ => {
                        (a + source.sample(sx + 1, sy)
                            + source.sample(sx, sy + 1)
                            + source.sample(sx + 1, sy + 1)
                            + 2)
                            >> 2
                    }
                } as u8;
                *pixel = if reference.average {
                    (u16::from(prediction) + u16::from(*pixel)).div_ceil(2) as u8
                } else {
                    prediction
                };
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Prediction<'a> {
    pub(crate) frame: &'a Frame,
    pub(crate) motion: [i32; 2],
    pub(crate) average: bool,
}

/// A decoded progressive YCbCr 4:2:0 frame.
#[derive(Debug)]
pub struct Frame {
    pub(crate) planes: [Plane; 3],
    frame_rate: (u32, u32),
    temporal_reference: u16,
    kind: PictureKind,
}

impl Frame {
    pub(crate) fn new(sequence: &Sequence, picture: &PictureHeader) -> Self {
        let width = sequence.width;
        let height = sequence.height;
        let chroma_width = width.div_ceil(2);
        let chroma_height = height.div_ceil(2);
        let macroblock_rows = sequence.macroblock_rows();
        Self {
            planes: [
                Plane::new(width, height, 16, 16 * macroblock_rows),
                Plane::new(chroma_width, chroma_height, 8, 8 * macroblock_rows),
                Plane::new(chroma_width, chroma_height, 8, 8 * macroblock_rows),
            ],
            frame_rate: sequence.frame_rate,
            temporal_reference: picture.temporal_reference,
            kind: picture.kind,
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

    /// The nominal frame rate as a numerator and denominator.
    pub fn frame_rate(&self) -> (u32, u32) {
        self.frame_rate
    }

    /// The 10-bit temporal reference from the picture header.
    pub fn temporal_reference(&self) -> u16 {
        self.temporal_reference
    }

    pub fn picture_kind(&self) -> PictureKind {
        self.kind
    }

    pub(crate) fn predict(&mut self, x: usize, y: usize, reference: Prediction<'_>) {
        self.planes[0].predict(x, y, reference, 0);
        self.planes[1].predict(x / 2, y / 2, reference, 1);
        self.planes[2].predict(x / 2, y / 2, reference, 2);
    }

    pub(crate) fn matches(&self, sequence: &Sequence) -> bool {
        self.width() == sequence.width
            && self.height() == sequence.height
            && self.planes[0].samples.len()
                == self.planes[0].stride * 16 * sequence.macroblock_rows()
    }
}
