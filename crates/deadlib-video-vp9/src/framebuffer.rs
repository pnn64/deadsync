//! Working planes and reference-picture sample storage.

use alloc::{vec, vec::Vec};

use core::fmt;

/// A reconstructed plane with enough precision for all VP9 profiles.
#[derive(Clone)]
pub struct Plane {
    pub width: usize,
    pub height: usize,
    /// Row stride in samples, which can exceed the visible width after cropping.
    stride: usize,
    /// Absolute pixel column of this buffer's first column (0 for a whole-frame plane).
    pub x0: usize,
    pixels: RefPlaneData,
}

impl fmt::Debug for Plane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Plane")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("stride", &self.stride)
            .field("x0", &self.x0)
            .finish_non_exhaustive()
    }
}

impl Plane {
    pub fn new(width: usize, height: usize, byte: bool) -> Self {
        Self {
            width,
            height,
            stride: width,
            x0: 0,
            pixels: if byte {
                RefPlaneData::U8(vec![0; width * height])
            } else {
                RefPlaneData::U16(vec![0; width * height])
            },
        }
    }

    /// Row stride in samples, including any pixels beyond the visible width.
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    #[inline]
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// Backing allocation geometry `(stride, len)` for pool lookup and deduplication.
    #[inline]
    pub(crate) fn backing_dims(&self) -> (usize, usize) {
        (self.stride, self.pixels.len())
    }

    /// Allocated sample capacity.
    #[inline]
    pub(crate) fn capacity(&self) -> usize {
        self.pixels.capacity()
    }

    /// Crops this plane's logical `(width, height)` in-place without reallocating or copying
    /// the backing buffer (for zero-copy DPB storage in `RefFrameData`).
    pub(crate) fn crop_in_place(&mut self, crop_width: usize, crop_height: usize) {
        debug_assert_eq!(self.x0, 0, "crop_in_place requires a whole-frame plane");
        debug_assert!(crop_width <= self.stride);
        debug_assert!(crop_height * self.stride <= self.pixels.len());
        self.width = crop_width;
        self.height = crop_height;
    }

    /// Restores a pooled plane back to full `(width, height)` with `x0 == 0` if its backing
    /// allocation matches `(stride == width && data.len() == width * height)`.
    pub(crate) fn try_reuse_as(&mut self, width: usize, height: usize) -> bool {
        let data = &self.pixels;
        if self.stride == width && data.len() == width * height {
            self.width = width;
            self.height = height;
            self.x0 = 0;
            true
        } else {
            false
        }
    }

    /// Reuses a pooled whole-frame working plane for `(width, height)` when its backing capacity
    /// fits within `[width * height, max_keep_elems]`, avoiding reallocations across multi-layer
    /// SVC while releasing oversized buffers after a permanent down-resize.
    pub(crate) fn try_reuse_working(
        &mut self,
        width: usize,
        height: usize,
        max_keep_elems: usize,
    ) -> bool {
        let data = &mut self.pixels;
        let need = width * height;
        if data.capacity() >= need && data.capacity() <= max_keep_elems.max(need) {
            self.width = width;
            self.height = height;
            self.stride = width;
            self.x0 = 0;
            data.resize(need);
            true
        } else {
            false
        }
    }

    #[inline]
    pub(crate) fn is_byte(&self) -> bool {
        matches!(self.pixels, RefPlaneData::U8(_))
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    pub(crate) fn byte_ptr(&mut self) -> Option<*mut u8> {
        match &mut self.pixels {
            RefPlaneData::U8(samples) => Some(samples.as_mut_ptr()),
            RefPlaneData::U16(_) => None,
        }
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    pub(crate) fn word_ptr(&mut self) -> Option<*mut u16> {
        match &mut self.pixels {
            RefPlaneData::U8(_) => None,
            RefPlaneData::U16(samples) => Some(samples.as_mut_ptr()),
        }
    }

    pub(crate) fn update_row(
        &mut self,
        x: usize,
        y: usize,
        width: usize,
        mut update: impl FnMut(usize, u16) -> u16,
    ) {
        let start = y * self.stride + (x - self.x0);
        let interval = start..start + width;
        match &mut self.pixels {
            RefPlaneData::U8(samples) => {
                for (i, sample) in samples[interval].iter_mut().enumerate() {
                    *sample = update(i, u16::from(*sample)) as u8;
                }
            }
            RefPlaneData::U16(samples) => {
                for (i, sample) in samples[interval].iter_mut().enumerate() {
                    *sample = update(i, *sample);
                }
            }
        }
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_word_block(
        &mut self,
        [x, y]: [usize; 2],
        [w, h]: [usize; 2],
        read: bool,
        scratch: &mut [u16],
        filter: impl FnOnce(*mut u16, usize, usize, usize) -> bool,
    ) -> bool {
        debug_assert!(x >= self.x0 && x - self.x0 + w <= self.width && y + h <= self.height);
        let x = x - self.x0;
        match &mut self.pixels {
            RefPlaneData::U16(samples) => filter(samples.as_mut_ptr(), self.stride, x, y),
            RefPlaneData::U8(samples) => {
                let block = &mut scratch[..w * h];
                if read {
                    for (row, out) in block.chunks_exact_mut(w).enumerate() {
                        let start = (y + row) * self.stride + x;
                        for (out, &sample) in out.iter_mut().zip(&samples[start..start + w]) {
                            *out = u16::from(sample);
                        }
                    }
                }
                if !filter(block.as_mut_ptr(), w, 0, 0) {
                    return false;
                }
                for (row, input) in block.chunks_exact(w).enumerate() {
                    let start = (y + row) * self.stride + x;
                    // AVX2 is checked by the caller. Both row slices cover w samples.
                    unsafe {
                        crate::simd::narrow_u16_to_u8_avx2(
                            input,
                            samples[start..start + w].as_mut_ptr(),
                        );
                    }
                }
                true
            }
        }
    }

    pub(crate) fn copy_byte_block(
        &mut self,
        [x, y]: [usize; 2],
        [w, h]: [usize; 2],
        source: &[u8],
        source_stride: usize,
        average: bool,
    ) -> bool {
        let RefPlaneData::U8(samples) = &mut self.pixels else {
            return false;
        };
        let x = x - self.x0;
        for row in 0..h {
            let start = (y + row) * self.stride + x;
            let dst = &mut samples[start..start + w];
            let src = &source[row * source_stride..][..w];
            if average {
                for (dst, &src) in dst.iter_mut().zip(src) {
                    *dst = ((u16::from(*dst) + u16::from(src) + 1) >> 1) as u8;
                }
            } else {
                dst.copy_from_slice(src);
            }
        }
        true
    }

    /// Unchecked pixel read for hot decode/prediction/loop-filter paths.
    ///
    /// # Safety
    /// Caller must ensure `x >= self.x0`, `x - self.x0 < self.width`, and `y < self.height`.
    #[inline]
    pub(crate) unsafe fn get_unchecked(&self, x: usize, y: usize) -> u16 {
        debug_assert!(
            x >= self.x0 && x - self.x0 < self.width && y < self.height,
            "Plane::get_unchecked out of range"
        );
        // SAFETY: caller guarantees `(x - self.x0, y)` lies strictly within `[0..width) x [0..height)`.
        let offset = y * self.stride + (x - self.x0);
        match &self.pixels {
            RefPlaneData::U8(samples) => u16::from(*samples.get_unchecked(offset)),
            RefPlaneData::U16(samples) => *samples.get_unchecked(offset),
        }
    }

    /// Unchecked pixel write for hot decode/prediction/loop-filter paths.
    ///
    /// # Safety
    /// Caller must ensure `x >= self.x0`, `x - self.x0 < self.width`, and `y < self.height`.
    #[inline]
    pub(crate) unsafe fn set_unchecked(&mut self, x: usize, y: usize, v: u16) {
        debug_assert!(
            x >= self.x0 && x - self.x0 < self.width && y < self.height,
            "Plane::set_unchecked out of range"
        );
        let offset = y * self.stride + (x - self.x0);
        // SAFETY: caller guarantees `(x - self.x0, y)` lies strictly within `[0..width) x [0..height)`.
        match &mut self.pixels {
            RefPlaneData::U8(samples) => *samples.get_unchecked_mut(offset) = v as u8,
            RefPlaneData::U16(samples) => *samples.get_unchecked_mut(offset) = v,
        }
    }

    pub(crate) fn copy_cropped_u8(&self, width: usize, height: usize, out: &mut Vec<u8>) {
        let RefPlaneData::U8(samples) = &self.pixels else {
            unreachable!()
        };
        out.clear();
        if self.stride == width {
            out.extend_from_slice(&samples[..width * height]);
        } else {
            out.reserve(width * height);
            for row in samples.chunks_exact(self.stride).take(height) {
                out.extend_from_slice(&row[..width]);
            }
        }
    }
}

/// Borrowed slice of a reference plane's samples (`u8` for 8-bit DPB, `u16` for 10/12-bit DPB),
/// letting non-hot scalar and scaled/border inter-prediction paths stay non-generic.
#[derive(Clone, Copy)]
pub(crate) enum RefSlice<'a> {
    U8(&'a [u8]),
    U16(&'a [u16]),
}

impl RefSlice<'_> {
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    #[inline]
    pub(crate) fn len(self) -> usize {
        match self {
            RefSlice::U8(s) => s.len(),
            RefSlice::U16(s) => s.len(),
        }
    }

    #[inline]
    pub(crate) fn get_i32(self, idx: usize) -> i32 {
        match self {
            RefSlice::U8(s) => s[idx] as i32,
            RefSlice::U16(s) => s[idx] as i32,
        }
    }
}

/// Backing pixel storage for a cropped DPB reference plane ([`RefPlane`]).
///
/// 8-bit streams (profiles 0/1) store packed `u8` samples (halving DPB footprint and doubling
/// cache density during motion compensation); 10/12-bit streams (profiles 2/3) keep `u16`.
#[derive(Debug, Clone)]
enum RefPlaneData {
    U8(Vec<u8>),
    U16(Vec<u16>),
}

impl RefPlaneData {
    fn len(&self) -> usize {
        match self {
            Self::U8(data) => data.len(),
            Self::U16(data) => data.len(),
        }
    }

    fn capacity(&self) -> usize {
        match self {
            Self::U8(data) => data.capacity(),
            Self::U16(data) => data.capacity(),
        }
    }

    fn resize(&mut self, len: usize) {
        match self {
            Self::U8(data) => data.resize(len, 0),
            Self::U16(data) => data.resize(len, 0),
        }
    }
}

/// One cropped reference plane stored inside [`crate::dpb::RefFrameData`].
#[derive(Debug, Clone)]
pub struct RefPlane {
    pub width: usize,
    pub height: usize,
    stride: usize,
    data: RefPlaneData,
}

impl RefPlane {
    /// Returns the visible rows for an 8-bit plane.
    pub fn rows_u8(&self) -> Option<impl ExactSizeIterator<Item = &[u8]>> {
        let RefPlaneData::U8(pixels) = &self.data else {
            return None;
        };
        Some(
            pixels
                .chunks_exact(self.stride)
                .take(self.height)
                .map(|row| &row[..self.width]),
        )
    }

    /// Returns the visible rows for a 10- or 12-bit plane.
    pub fn rows_u16(&self) -> Option<impl ExactSizeIterator<Item = &[u16]>> {
        let RefPlaneData::U16(pixels) = &self.data else {
            return None;
        };
        Some(
            pixels
                .chunks_exact(self.stride)
                .take(self.height)
                .map(|row| &row[..self.width]),
        )
    }

    /// Wraps a tightly packed `width x height` 8-bit reference plane (`stride == width`).
    pub(crate) fn from_u8_cropped(width: usize, height: usize, data: Vec<u8>) -> Self {
        debug_assert_eq!(data.len(), width * height);
        Self {
            width,
            height,
            stride: width,
            data: RefPlaneData::U8(data),
        }
    }

    /// Moves a cropped-in-place 10/12-bit [`Plane`] into a `RefPlane` with zero copy.
    pub(crate) fn from_u16_plane(plane: Plane) -> Self {
        debug_assert_eq!(plane.x0, 0);
        Self {
            width: plane.width,
            height: plane.height,
            stride: plane.stride,
            data: plane.pixels,
        }
    }

    #[inline]
    pub fn stride(&self) -> usize {
        self.stride
    }

    #[inline]
    pub(crate) fn as_slice_ref(&self) -> RefSlice<'_> {
        match &self.data {
            RefPlaneData::U8(v) => RefSlice::U8(v),
            RefPlaneData::U16(v) => RefSlice::U16(v),
        }
    }

    /// Backing geometry `(stride, len)` when `U16` (for `plane_pool` matching).
    #[inline]
    pub(crate) fn u16_backing_dims(&self) -> Option<(usize, usize)> {
        match &self.data {
            RefPlaneData::U16(v) => Some((self.stride, v.len())),
            RefPlaneData::U8(_) => None,
        }
    }

    /// Cropped sample count `width * height` when `U8` (for `ref_u8_pool` matching).
    #[inline]
    pub(crate) fn u8_len(&self) -> Option<usize> {
        match &self.data {
            RefPlaneData::U8(v) => Some(v.len()),
            RefPlaneData::U16(_) => None,
        }
    }

    /// Unwraps the owned backing buffer for caller-managed DPB eviction pooling (`Ok(Vec<u8>)`
    /// for 8-bit, `Err(Plane)` for 10/12-bit).
    pub(crate) fn into_backing(self) -> Result<Vec<u8>, Plane> {
        match self.data {
            RefPlaneData::U8(v) => Ok(v),
            RefPlaneData::U16(data) => Err(Plane {
                width: self.width,
                height: self.height,
                stride: self.stride,
                x0: 0,
                pixels: RefPlaneData::U16(data),
            }),
        }
    }

    /// Copies out display-cropped [`crate::PlaneData`] (used by `show_existing_frame`).
    pub(crate) fn to_plane_data(&self) -> crate::PlaneData {
        match &self.data {
            RefPlaneData::U8(v) => {
                debug_assert_eq!(self.stride, self.width);
                crate::PlaneData::U8(v.clone())
            }
            RefPlaneData::U16(v) => {
                if self.width == self.stride {
                    crate::PlaneData::U16(v[..self.width * self.height].to_vec())
                } else {
                    let mut out = Vec::with_capacity(self.width * self.height);
                    for y in 0..self.height {
                        let start = y * self.stride;
                        out.extend_from_slice(&v[start..start + self.width]);
                    }
                    crate::PlaneData::U16(out)
                }
            }
        }
    }
}
