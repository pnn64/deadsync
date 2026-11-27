//! Read-only output backed by the decoder's reference-picture buffers.

use std::{fmt, sync::Arc};

use crate::{
    DecodeError, Frame, PixelData,
    dpb::{DecodedPicture, PicturePlanes},
    sps::Sps,
};

#[derive(Clone, Copy)]
pub(crate) struct PictureLayout {
    width: u32,
    height: u32,
    left: usize,
    top: usize,
    bit_depth: u8,
    chroma_format_idc: u32,
}

impl PictureLayout {
    pub(crate) fn from_sps(sps: &Sps) -> Result<Self, DecodeError> {
        let width = sps.cropped_width();
        let height = sps.cropped_height();
        let left = sps.conf_win_left_offset.checked_mul(sps.sub_width_c());
        let top = sps.conf_win_top_offset.checked_mul(sps.sub_height_c());
        match (left, top) {
            (Some(left), Some(top))
                if left
                    .checked_add(width)
                    .is_some_and(|end| end <= sps.pic_width_in_luma_samples)
                    && top
                        .checked_add(height)
                        .is_some_and(|end| end <= sps.pic_height_in_luma_samples) =>
            {
                Ok(Self {
                    width,
                    height,
                    left: left as usize,
                    top: top as usize,
                    bit_depth: sps.bit_depth_luma,
                    chroma_format_idc: sps.chroma_array_type(),
                })
            }
            _ => Err(DecodeError::InvalidSyntax(
                "conformance window exceeds picture dimensions",
            )),
        }
    }
}

/// A reconstructed picture that shares its samples with the reference buffer.
///
/// It can be retained after decoding later packets. The visible rows exclude
/// stride padding and conformance-window cropping.
#[derive(Clone)]
pub struct Picture {
    reference: Arc<DecodedPicture>,
    layout: PictureLayout,
}

impl fmt::Debug for Picture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Picture")
            .field("width", &self.width())
            .field("height", &self.height())
            .field("bit_depth", &self.bit_depth())
            .field("pic_order_cnt", &self.pic_order_cnt())
            .finish_non_exhaustive()
    }
}

impl Picture {
    pub(crate) fn new(reference: Arc<DecodedPicture>, layout: PictureLayout) -> Self {
        Self { reference, layout }
    }

    pub fn width(&self) -> u32 {
        self.layout.width
    }

    pub fn height(&self) -> u32 {
        self.layout.height
    }

    pub fn bit_depth(&self) -> u8 {
        self.layout.bit_depth
    }

    pub fn chroma_format_idc(&self) -> u32 {
        self.layout.chroma_format_idc
    }

    pub fn pic_order_cnt(&self) -> i32 {
        self.reference.poc
    }

    /// Returns the Y, Cb and Cr planes, in that order.
    pub fn planes(&self) -> [PicturePlane<'_>; 3] {
        let reference = &self.reference;
        debug_assert!(reference.progress.is_complete());
        core::array::from_fn(|i| {
            let (pitch, shift_x, shift_y) = if i == 0 {
                (reference.y_stride, 0, 0)
            } else {
                (
                    reference.uv_stride,
                    reference.chroma_shift_w,
                    reference.chroma_shift_h,
                )
            };
            let row = PlaneLayout {
                pitch,
                x: self.layout.left >> shift_x,
                y: self.layout.top >> shift_y,
                width: self.width() as usize >> shift_x,
                height: self.height() as usize >> shift_y,
            };
            let samples = match &reference.planes {
                PicturePlanes::U8(planes) => Samples::Byte(&planes[i]),
                PicturePlanes::U16(planes) => Samples::Word(&planes[i]),
            };
            PicturePlane { samples, row }
        })
    }

    /// Copies the visible samples into tightly packed, owned planes.
    pub fn to_owned_frame(&self) -> Frame {
        let [y, u, v] = self.planes().map(PicturePlane::copy_samples);
        Frame {
            y,
            u,
            v,
            width: self.width(),
            height: self.height(),
            pic_order_cnt: self.pic_order_cnt(),
            bit_depth: self.bit_depth(),
            chroma_format_idc: self.chroma_format_idc(),
        }
    }

    #[cfg(test)]
    pub(crate) fn empty_for_test(poc: i32) -> Self {
        let empty = || PixelData::U8(Vec::new());
        Self::new(
            Arc::new(DecodedPicture::new(empty(), empty(), empty(), 0, 0, poc)),
            PictureLayout {
                width: 0,
                height: 0,
                left: 0,
                top: 0,
                bit_depth: 8,
                chroma_format_idc: 1,
            },
        )
    }
}

#[derive(Clone, Copy)]
struct PlaneLayout {
    pitch: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl PlaneLayout {
    fn rows<T>(self, samples: &[T]) -> impl ExactSizeIterator<Item = &[T]> {
        (self.y..self.y + self.height).map(move |y| {
            let start = y * self.pitch + self.x;
            &samples[start..start + self.width]
        })
    }

    fn copy<T: Copy>(self, samples: &[T]) -> Vec<T> {
        if self.x == 0 && self.pitch == self.width {
            let start = self.y * self.pitch;
            return samples[start..start + self.height * self.width].to_vec();
        }
        let mut packed = Vec::with_capacity(self.width * self.height);
        for row in self.rows(samples) {
            packed.extend_from_slice(row);
        }
        packed
    }
}

enum Samples<'a> {
    Byte(&'a [u8]),
    Word(&'a [u16]),
}

/// A borrowed view of the visible samples in one plane.
pub struct PicturePlane<'a> {
    samples: Samples<'a>,
    row: PlaneLayout,
}

impl<'a> PicturePlane<'a> {
    /// Visible rows if the plane uses byte samples.
    pub fn rows_u8(&self) -> Option<impl ExactSizeIterator<Item = &'a [u8]>> {
        match self.samples {
            Samples::Byte(samples) => Some(self.row.rows(samples)),
            Samples::Word(_) => None,
        }
    }

    /// Visible rows if the plane uses 16-bit sample storage.
    pub fn rows_u16(&self) -> Option<impl ExactSizeIterator<Item = &'a [u16]>> {
        match self.samples {
            Samples::Word(samples) => Some(self.row.rows(samples)),
            Samples::Byte(_) => None,
        }
    }

    fn copy_samples(self) -> PixelData {
        match self.samples {
            Samples::Byte(samples) => PixelData::U8(self.row.copy(samples)),
            Samples::Word(samples) => PixelData::U16(self.row.copy(samples)),
        }
    }
}
