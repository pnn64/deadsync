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

    pub(crate) fn write<const INTRA: bool>(&mut self, [x, y]: [usize; 2], block: &[i16; 64]) {
        #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
        x86::write::<INTRA>(&mut self.samples[y * self.stride + x..], self.stride, block);
        #[cfg(not(all(target_arch = "x86_64", target_feature = "sse2")))]
        for (row, values) in block.as_chunks::<8>().0.iter().enumerate() {
            let start = (y + row) * self.stride + x;
            for (pixel, &value) in self.samples[start..start + 8].iter_mut().zip(values) {
                let base = if INTRA { 0 } else { i32::from(*pixel) };
                *pixel = (base + i32::from(value)).clamp(0, 255) as u8;
            }
        }
    }

    pub(crate) fn average<const N: usize>(&mut self, other: &Self, [x, y]: [usize; 2]) {
        for row in y..y + N {
            let start = row * self.stride + x;
            for (dst, &src) in self.samples[start..start + N]
                .iter_mut()
                .zip(&other.samples[start..start + N])
            {
                *dst = (u16::from(*dst) + u16::from(src)).div_ceil(2) as u8;
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
        let left = if x < 0 {
            (x.unsigned_abs() as usize).min(span)
        } else {
            0
        };
        let start = x.clamp(0, self.width as i32 - 1) as usize;
        let count = (span - left).min(self.width - start);
        for row in 0..rows {
            let sy = (y + row as i32).clamp(0, self.height as i32 - 1) as usize;
            let src = &self.samples[sy * self.stride..][..self.width];
            let dst = &mut tile[row * span..][..span];
            if count == span {
                dst.copy_from_slice(&src[start..start + span]);
                continue;
            }
            dst[..left].fill(src[0]);
            dst[left..left + count].copy_from_slice(&src[start..start + count]);
            dst[left + count..].fill(src[self.width - 1]);
        }
    }
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[allow(unsafe_code)]
mod x86 {
    use core::arch::x86_64::*;

    #[inline]
    pub(super) fn write<const INTRA: bool>(dst: &mut [u8], stride: usize, block: &[i16; 64]) {
        assert!(dst.len() >= 8 && (dst.len() - 8) / 7 >= stride);
        // Each source row contains eight i16s. The destination check covers
        // eight bytes in every output row, including the last.
        unsafe {
            for (y, row) in block.as_chunks::<8>().0.iter().enumerate() {
                let pixels = dst.as_mut_ptr().add(y * stride).cast();
                let mut samples = _mm_loadu_si128(row.as_ptr().cast());
                if !INTRA {
                    let predicted = _mm_unpacklo_epi8(_mm_loadl_epi64(pixels), _mm_setzero_si128());
                    samples = _mm_adds_epi16(samples, predicted);
                }
                _mm_storel_epi64(pixels, _mm_packus_epi16(samples, samples));
            }
        }
    }
}

/// A decoded progressive picture with 8-bit Y, Cb and Cr planes.
#[derive(Debug)]
pub struct Frame {
    pub(crate) planes: [Plane; 3],
    kind: PictureKind,
    pub(crate) presentation_time: Option<(i64, i64)>,
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
            presentation_time: None,
        }
    }

    pub(crate) fn reuse(mut self, width: usize, height: usize, kind: PictureKind) -> Self {
        if (self.width(), self.height()) != (width, height) {
            return Self::new(width, height, kind);
        }
        // Reconstruction writes every macroblock, including skipped ones.
        self.kind = kind;
        self.presentation_time = None;
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

    pub fn picture_kind(&self) -> PictureKind {
        self.kind
    }

    /// Presentation time and its clock frequency as signaled by MPEG-4 Visual.
    pub fn presentation_time(&self) -> Option<(i64, i64)> {
        self.presentation_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_extension_matches_individual_sample_clamping() {
        let mut plane = Plane::new(9, 7, 8);
        for (i, sample) in plane.samples.iter_mut().enumerate() {
            *sample = (i * 73 + i / 3) as u8;
        }
        for x in -18..=18 {
            for y in -18..=18 {
                for (span, rows) in [(8, 8), (9, 8), (16, 17), (17, 17)] {
                    let mut tile = [0; 289];
                    plane.extend_edge([x, y], span, rows, &mut tile);
                    for row in 0..rows {
                        for column in 0..span {
                            let sx = (x + column as i32).clamp(0, 8) as usize;
                            let sy = (y + row as i32).clamp(0, 6) as usize;
                            assert_eq!(
                                tile[row * span + column],
                                plane.samples[sy * plane.stride + sx]
                            );
                        }
                    }
                    assert!(tile[span * rows..].iter().all(|&v| v == 0));
                }
            }
        }
    }
}
