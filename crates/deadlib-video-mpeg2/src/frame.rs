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
                let base = if intra { 0 } else { i16::from(*pixel) };
                *pixel = residual.saturating_add(base).clamp(0, 255) as u8;
            }
        }
    }

    fn predict<const W: usize>(
        &mut self,
        x: usize,
        y: usize,
        source: &Self,
        motion: [i32; 2],
        average: bool,
    ) {
        let source_x = x as i32 + (motion[0] >> 1);
        let source_y = y as i32 + (motion[1] >> 1);
        let fraction = ((motion[1] & 1) << 1 | (motion[0] & 1)) as usize;
        let dst = &mut self.samples[y * self.stride + x..];
        let span = W + (fraction & 1);
        let rows = W + (fraction >> 1);
        if source_x >= 0
            && source_y >= 0
            && source_x as usize + span <= source.stride
            && (source_y as usize + rows) * source.stride <= source.samples.len()
        {
            let src = &source.samples[source_y as usize * source.stride + source_x as usize..];
            crate::motion::predict::<W>(dst, self.stride, src, source.stride, fraction, average);
        } else {
            let height = source.samples.len() / source.stride;
            let mut tile = [0; 17 * 17];
            source.extend_edge(source_x, source_y, span, rows, height, &mut tile);
            crate::motion::predict::<W>(dst, self.stride, &tile, span, fraction, average);
        }
    }

    #[cold]
    fn extend_edge(
        &self,
        x: i32,
        y: i32,
        span: usize,
        rows: usize,
        height: usize,
        tile: &mut [u8],
    ) {
        for row in 0..rows {
            let source_y = (y + row as i32).clamp(0, height as i32 - 1) as usize;
            let source_row = &self.samples[source_y * self.stride..][..self.stride];
            let target_row = &mut tile[row * span..][..span];
            for (col, sample) in target_row.iter_mut().enumerate() {
                let source_x = (x + col as i32).clamp(0, self.stride as i32 - 1) as usize;
                *sample = source_row[source_x];
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
    mpeg2: bool,
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
            mpeg2: sequence.extended,
        }
    }

    pub(crate) fn reuse(mut self, sequence: &Sequence, picture: &PictureHeader) -> Self {
        if !self.matches(sequence) {
            return Self::new(sequence, picture);
        }
        // Complete picture coverage ensures every sample is overwritten.
        self.frame_rate = sequence.frame_rate;
        self.temporal_reference = picture.temporal_reference;
        self.kind = picture.kind;
        self
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

    pub(crate) fn copy_samples(&mut self, reference: &Self) {
        for (plane, source) in self.planes.iter_mut().zip(reference.planes()) {
            plane.samples.copy_from_slice(&source.samples);
        }
    }

    pub(crate) fn predict(&mut self, x: usize, y: usize, reference: Prediction<'_>) {
        let [dx, dy] = reference.motion;
        let source = &reference.frame.planes;
        self.planes[0].predict::<16>(x, y, &source[0], [dx, dy], reference.average);
        for plane in 1..=2 {
            self.planes[plane].predict::<8>(
                x / 2,
                y / 2,
                &source[plane],
                [dx / 2, dy / 2],
                reference.average,
            );
        }
    }

    pub(crate) fn matches(&self, sequence: &Sequence) -> bool {
        self.mpeg2 == sequence.extended
            && self.width() == sequence.width
            && self.height() == sequence.height
            && self.planes[0].samples.len()
                == self.planes[0].stride * 16 * sequence.macroblock_rows()
    }
}

#[cfg(test)]
mod tests {
    use super::Plane;

    #[test]
    fn prediction_uses_padded_dimensions_at_every_edge() {
        fn check<const W: usize>(width: usize, height: usize, rows: usize) {
            let mut source = Plane::new(width, height, W, rows);
            for (index, pixel) in source.samples.iter_mut().enumerate() {
                *pixel = (index * 37 + (index / source.stride) * 17) as u8;
            }
            let mut destination = Plane::new(width, height, W, rows);
            for x in [0, source.stride - W] {
                for y in [0, rows - W] {
                    for dx in [-4096i32, -33, -3, -1, 0, 1, 2, 31, 4095] {
                        for dy in [-4096i32, -33, -3, -1, 0, 1, 2, 31, 4095] {
                            for average in [false, true] {
                                destination.samples.fill(211);
                                let mut expected = destination.samples.clone();
                                for row in 0..W {
                                    for col in 0..W {
                                        let tap = |ox: i32, oy: i32| {
                                            let sx = (x as i32 + (dx >> 1) + col as i32 + ox)
                                                .clamp(0, source.stride as i32 - 1);
                                            let sy = (y as i32 + (dy >> 1) + row as i32 + oy)
                                                .clamp(0, rows as i32 - 1);
                                            u16::from(
                                                source.samples
                                                    [sy as usize * source.stride + sx as usize],
                                            )
                                        };
                                        let predicted = match ((dy & 1) << 1) | (dx & 1) {
                                            0 => tap(0, 0),
                                            1 => (tap(0, 0) + tap(1, 0) + 1) >> 1,
                                            2 => (tap(0, 0) + tap(0, 1) + 1) >> 1,
                                            _ => {
                                                (tap(0, 0) + tap(1, 0) + tap(0, 1) + tap(1, 1) + 2)
                                                    >> 2
                                            }
                                        };
                                        expected[(y + row) * destination.stride + x + col] =
                                            if average {
                                                ((predicted + 211 + 1) >> 1) as u8
                                            } else {
                                                predicted as u8
                                            };
                                    }
                                }
                                destination.predict::<W>(x, y, &source, [dx, dy], average);
                                assert_eq!(
                                    destination.samples, expected,
                                    "{W}, {x}, {y}, {dx}, {dy}, {average}"
                                );
                            }
                        }
                    }
                }
            }
        }
        // Non-progressive sequence padding can extend beyond the visible height.
        check::<16>(19, 34, 64);
        check::<8>(10, 17, 32);
    }

    #[test]
    fn block_clips_wide_residuals_for_intra_and_prediction() {
        let mut plane = Plane::new(8, 8, 8, 8);
        let residuals = [i16::MIN, -257, -255, -1, 0, 1, 255, i16::MAX];
        for base in 0..=u8::MAX {
            let block = core::array::from_fn(|i| residuals[i % 8]);
            for intra in [false, true] {
                plane.samples.fill(base);
                plane.block(0, 0, &block, intra);
                for (&pixel, residual) in plane.samples.iter().zip(block) {
                    let prediction = if intra { 0 } else { i32::from(base) };
                    assert_eq!(
                        pixel,
                        (prediction + i32::from(residual)).clamp(0, 255) as u8
                    );
                }
            }
        }
    }
}
