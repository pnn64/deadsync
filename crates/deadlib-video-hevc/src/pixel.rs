//! Generic pixel type abstraction for multi-bit-depth HEVC decoding.
//!
//! The `Pixel` trait abstracts over `u8` (8-bit) and `u16` (10/12-bit) sample
//! storage. Functions that operate on pixel planes are generic over `P: Pixel`,
//! avoiding the need for runtime enum dispatch or always-u16 memory overhead.
//!
//! Bit depth (8, 10, 12) is a *runtime* parameter passed alongside `P` — the
//! trait only handles the *storage type*. A `u16` pixel could represent 10-bit
//! or 12-bit data depending on `bit_depth`.

/// Pixel data that can be either 8-bit or 10/12-bit.
///
/// This enum is used by `Frame` and `DecodedPicture` to store pixel planes
/// without fixing the bit depth at compile time. The decoder creates the
/// appropriate variant based on the SPS `bit_depth_luma` / `bit_depth_chroma`.
#[derive(Debug, Clone)]
pub enum PixelData {
    /// 8-bit samples, one byte per pixel.
    U8(Vec<u8>),
    /// 10-bit or 12-bit samples, two bytes per pixel (values in `0..2^bit_depth - 1`).
    U16(Vec<u16>),
}

impl PixelData {
    /// Access the data as `&[u8]`, returning `None` if this is a `U16` variant.
    pub fn as_u8(&self) -> Option<&[u8]> {
        match self {
            PixelData::U8(v) => Some(v),
            PixelData::U16(_) => None,
        }
    }

    /// Access the data as `&[u16]`, returning `None` if this is a `U8` variant.
    pub fn as_u16(&self) -> Option<&[u16]> {
        match self {
            PixelData::U16(v) => Some(v),
            PixelData::U8(_) => None,
        }
    }

    /// Number of pixel samples (not bytes) in this plane.
    pub fn len(&self) -> usize {
        match self {
            PixelData::U8(v) => v.len(),
            PixelData::U16(v) => v.len(),
        }
    }

    /// Returns `true` if the plane contains no samples.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Trait for pixel sample types. Implemented for `u8` and `u16`.
pub trait Pixel: Copy + Clone + Default + Send + Sync + Sized + 'static {
    /// Convert from a signed 32-bit computation result, clamping to the
    /// valid range `[0, max_val]` where `max_val = (1 << bit_depth) - 1`.
    fn from_i32_clamped(val: i32, bit_depth: u8) -> Self;

    /// Widen to i32 for arithmetic.
    fn to_i32(self) -> i32;

    /// Create a zero value.
    fn zero() -> Self;

    /// Storage width of the sample type in bits (8 for `u8`, 16 for `u16`).
    /// Lets hot loops pick a narrower accumulator at compile time.
    const BITS: u32;

    /// Extract a typed slice from a `PixelData` enum.
    ///
    /// Returns the inner `&[Self]` if the variant matches `Self`, panics otherwise.
    /// This is safe because the decoder guarantees that all pictures in a sequence
    /// have the same bit depth, so the variant always matches.
    fn extract_slice(data: &PixelData) -> &[Self];

    /// Extract a mutable typed slice from a `PixelData` enum.
    fn extract_slice_mut(data: &mut PixelData) -> &mut [Self];

    /// Wrap a `Vec<Self>` into the matching `PixelData` variant.
    fn wrap_vec(v: Vec<Self>) -> PixelData;

    /// A decoded picture's planes in this sample type.
    fn picture_planes(p: &crate::dpb::PicturePlanes) -> &[crate::shared::SharedTable<Self>; 3];

    /// Wrap planes of this sample type.
    fn wrap_planes(p: [crate::shared::SharedTable<Self>; 3]) -> crate::dpb::PicturePlanes;
}

impl Pixel for u8 {
    const BITS: u32 = 8;

    #[inline(always)]
    fn from_i32_clamped(val: i32, _bit_depth: u8) -> Self {
        val.clamp(0, 255) as u8
    }

    #[inline(always)]
    fn to_i32(self) -> i32 {
        self as i32
    }

    #[inline(always)]
    fn zero() -> Self {
        0
    }

    #[inline(always)]
    fn extract_slice(data: &PixelData) -> &[Self] {
        match data {
            PixelData::U8(v) => v,
            PixelData::U16(_) => panic!("PixelData::U16 accessed as u8"),
        }
    }

    #[inline(always)]
    fn extract_slice_mut(data: &mut PixelData) -> &mut [Self] {
        match data {
            PixelData::U8(v) => v,
            PixelData::U16(_) => panic!("PixelData::U16 accessed as u8"),
        }
    }

    #[inline(always)]
    fn wrap_vec(v: Vec<Self>) -> PixelData {
        PixelData::U8(v)
    }

    #[inline(always)]
    fn picture_planes(p: &crate::dpb::PicturePlanes) -> &[crate::shared::SharedTable<Self>; 3] {
        match p {
            crate::dpb::PicturePlanes::U8(planes) => planes,
            crate::dpb::PicturePlanes::U16(_) => panic!("picture planes accessed as u8"),
        }
    }

    #[inline(always)]
    fn wrap_planes(p: [crate::shared::SharedTable<Self>; 3]) -> crate::dpb::PicturePlanes {
        crate::dpb::PicturePlanes::U8(p)
    }
}

impl Pixel for u16 {
    const BITS: u32 = 16;

    #[inline(always)]
    fn from_i32_clamped(val: i32, bit_depth: u8) -> Self {
        let max = (1i32 << bit_depth) - 1;
        val.clamp(0, max) as u16
    }

    #[inline(always)]
    fn to_i32(self) -> i32 {
        self as i32
    }

    #[inline(always)]
    fn zero() -> Self {
        0
    }

    #[inline(always)]
    fn extract_slice(data: &PixelData) -> &[Self] {
        match data {
            PixelData::U16(v) => v,
            PixelData::U8(_) => panic!("PixelData::U8 accessed as u16"),
        }
    }

    #[inline(always)]
    fn extract_slice_mut(data: &mut PixelData) -> &mut [Self] {
        match data {
            PixelData::U16(v) => v,
            PixelData::U8(_) => panic!("PixelData::U8 accessed as u16"),
        }
    }

    #[inline(always)]
    fn wrap_vec(v: Vec<Self>) -> PixelData {
        PixelData::U16(v)
    }

    #[inline(always)]
    fn picture_planes(p: &crate::dpb::PicturePlanes) -> &[crate::shared::SharedTable<Self>; 3] {
        match p {
            crate::dpb::PicturePlanes::U16(planes) => planes,
            crate::dpb::PicturePlanes::U8(_) => panic!("picture planes accessed as u16"),
        }
    }

    #[inline(always)]
    fn wrap_planes(p: [crate::shared::SharedTable<Self>; 3]) -> crate::dpb::PicturePlanes {
        crate::dpb::PicturePlanes::U16(p)
    }
}

/// Maximum pixel value for a given bit depth: `(1 << bit_depth) - 1`.
#[inline(always)]
pub fn max_pixel_val(bit_depth: u8) -> i32 {
    (1i32 << bit_depth) - 1
}

/// Default (mid-grey) reference sample for intra prediction: `1 << (bit_depth - 1)`.
#[inline(always)]
pub fn default_ref_sample(bit_depth: u8) -> i32 {
    1i32 << (bit_depth - 1)
}

/// MC luma filter shift: `14 - bit_depth` for HEVC (spec 8.5.3.2.2.1).
/// 8-bit: 6, 10-bit: 4, 12-bit: 2.
#[inline(always)]
pub fn mc_shift(bit_depth: u8) -> u8 {
    14 - bit_depth
}

/// MC luma filter rounding offset: `1 << (mc_shift - 1)`.
#[inline(always)]
pub fn mc_offset(bit_depth: u8) -> i32 {
    1i32 << (mc_shift(bit_depth) - 1)
}

/// Bi-prediction combining shift: `14 - bit_depth + 1`.
/// 8-bit: 7, 10-bit: 5, 12-bit: 3.
#[inline(always)]
pub fn bipred_shift(bit_depth: u8) -> u8 {
    14 - bit_depth + 1
}

/// Bi-prediction rounding offset: `1 << (bipred_shift - 1)`.
#[inline(always)]
pub fn bipred_offset(bit_depth: u8) -> i32 {
    1i32 << (bipred_shift(bit_depth) - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u8_pixel_clamp() {
        assert_eq!(u8::from_i32_clamped(128, 8), 128u8);
        assert_eq!(u8::from_i32_clamped(-5, 8), 0u8);
        assert_eq!(u8::from_i32_clamped(300, 8), 255u8);
    }

    #[test]
    fn u16_pixel_clamp_10bit() {
        assert_eq!(u16::from_i32_clamped(512, 10), 512u16);
        assert_eq!(u16::from_i32_clamped(-5, 10), 0u16);
        assert_eq!(u16::from_i32_clamped(2000, 10), 1023u16);
    }

    #[test]
    fn u16_pixel_clamp_12bit() {
        assert_eq!(u16::from_i32_clamped(2048, 12), 2048u16);
        assert_eq!(u16::from_i32_clamped(5000, 12), 4095u16);
    }

    #[test]
    fn mc_shift_values() {
        assert_eq!(mc_shift(8), 6);
        assert_eq!(mc_shift(10), 4);
        assert_eq!(mc_shift(12), 2);
    }

    #[test]
    fn mc_offset_values() {
        assert_eq!(mc_offset(8), 32);
        assert_eq!(mc_offset(10), 8);
        assert_eq!(mc_offset(12), 2);
    }

    #[test]
    fn bipred_shift_values() {
        assert_eq!(bipred_shift(8), 7);
        assert_eq!(bipred_shift(10), 5);
        assert_eq!(bipred_shift(12), 3);
    }

    #[test]
    fn default_ref_sample_values() {
        assert_eq!(default_ref_sample(8), 128);
        assert_eq!(default_ref_sample(10), 512);
        assert_eq!(default_ref_sample(12), 2048);
    }

    #[test]
    fn max_pixel_values() {
        assert_eq!(max_pixel_val(8), 255);
        assert_eq!(max_pixel_val(10), 1023);
        assert_eq!(max_pixel_val(12), 4095);
    }
}
