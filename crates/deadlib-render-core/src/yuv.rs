//! Packed YUV420 images and borrowed texture uploads.

/// Levels and color coefficients used to sample YUV planes as RGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct YuvConversion {
    pub levels: [f32; 4],
    pub coeffs: [f32; 4],
}

impl YuvConversion {
    pub const BT709_LIMITED: Self = Self::limited([1.5748, -0.187_324, -0.468_124, 1.8556]);

    #[must_use]
    pub const fn limited(coeffs: [f32; 4]) -> Self {
        Self {
            levels: [255.0 / 219.0, -16.0 / 219.0, 255.0 / 224.0, -128.0 / 224.0],
            coeffs,
        }
    }

    #[must_use]
    pub const fn full(coeffs: [f32; 4]) -> Self {
        Self {
            levels: [1.0, 0.0, 1.0, -128.0 / 255.0],
            coeffs,
        }
    }
}

/// One opaque planar YUV420 frame backed by one reusable buffer.
///
/// Planes are tightly packed in Y, U, V order. Dimensions are always non-zero
/// and even, allowing every graphics backend to upload the planes directly to
/// ordinary 8-bit textures without repacking rows. The source color-space and
/// range conversion travels with the pixels for the renderer shader.
pub struct Yuv420Image {
    width: u32,
    height: u32,
    raw: Vec<u8>,
    conversion: YuvConversion,
}

impl Yuv420Image {
    #[must_use]
    pub fn from_raw(width: u32, height: u32, raw: Vec<u8>) -> Option<Self> {
        Self::from_raw_with_conversion(width, height, raw, YuvConversion::BT709_LIMITED)
    }

    #[must_use]
    pub fn from_raw_with_conversion(
        width: u32,
        height: u32,
        raw: Vec<u8>,
        conversion: YuvConversion,
    ) -> Option<Self> {
        (Self::packed_byte_len(width, height) == Some(raw.len())).then_some(Self {
            width,
            height,
            raw,
            conversion,
        })
    }

    /// Buffer size for a tightly packed frame with nonzero, even dimensions.
    #[inline]
    #[must_use]
    pub fn packed_byte_len(width: u32, height: u32) -> Option<usize> {
        if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
            return None;
        }
        let luma = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?;
        luma.checked_add(luma / 2)
    }

    #[inline(always)]
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[inline(always)]
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[inline(always)]
    #[must_use]
    pub fn as_raw(&self) -> &[u8] {
        &self.raw
    }

    #[must_use]
    pub fn planes(&self) -> (&[u8], &[u8], &[u8]) {
        let y_len = self.width as usize * self.height as usize;
        let chroma_len = y_len / 4;
        let (y, chroma) = self.raw.split_at(y_len);
        let (u, v) = chroma.split_at(chroma_len);
        (y, u, v)
    }

    #[inline(always)]
    #[must_use]
    pub const fn conversion(&self) -> YuvConversion {
        self.conversion
    }

    /// Borrow the packed planes and conversion for a renderer upload.
    #[inline]
    #[must_use]
    pub fn as_upload(&self) -> Yuv420Upload<'_> {
        let (y, u, v) = self.planes();
        Yuv420Upload {
            width: self.width,
            height: self.height,
            y,
            u,
            v,
            levels: self.conversion.levels,
            coeffs: self.conversion.coeffs,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn into_raw(self) -> Vec<u8> {
        self.raw
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Yuv420Upload<'a> {
    pub width: u32,
    pub height: u32,
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub levels: [f32; 4],
    pub coeffs: [f32; 4],
}

impl Yuv420Upload<'_> {
    #[inline]
    #[must_use]
    pub const fn is_valid(self) -> bool {
        let Some(luma_len) = (self.width as usize).checked_mul(self.height as usize) else {
            return false;
        };
        let chroma_len = luma_len / 4;
        self.width != 0
            && self.height != 0
            && self.width.is_multiple_of(2)
            && self.height.is_multiple_of(2)
            && self.y.len() == luma_len
            && self.u.len() == chroma_len
            && self.v.len() == chroma_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yuv420_image_exposes_tightly_packed_planes() {
        let raw: Vec<u8> = (0..12).collect();
        let source_buffer = raw.as_ptr();
        let conversion = YuvConversion::full([1.402, -0.344_136, -0.714_136, 1.772]);
        let image = Yuv420Image::from_raw_with_conversion(4, 2, raw, conversion).unwrap();
        let upload = image.as_upload();
        assert!(upload.is_valid());
        assert_eq!(upload.y.as_ptr(), source_buffer);
        assert_eq!(upload.y, &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(upload.u, &[8, 9]);
        assert_eq!(upload.v, &[10, 11]);
        assert_eq!(upload.levels, conversion.levels);
        assert_eq!(upload.coeffs, conversion.coeffs);
        let recycled = image.into_raw();
        assert_eq!(recycled.as_ptr(), source_buffer);
    }

    #[test]
    fn yuv420_image_rejects_invalid_dimensions_and_buffer_lengths() {
        assert!(Yuv420Image::from_raw(3, 2, vec![0; 9]).is_none());
        assert!(Yuv420Image::from_raw(0, 2, Vec::new()).is_none());
        assert!(Yuv420Image::from_raw(2, 2, vec![0; 5]).is_none());
        assert!(Yuv420Image::from_raw(u32::MAX - 1, u32::MAX - 1, Vec::new()).is_none());
    }
}
