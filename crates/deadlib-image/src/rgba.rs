use std::ops::{Deref, Index, IndexMut};

/// One straight-alpha RGBA8 pixel in upload order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rgba(pub [u8; 4]);

impl Index<usize> for Rgba {
    type Output = u8;

    fn index(&self, index: usize) -> &u8 {
        &self.0[index]
    }
}

impl IndexMut<usize> for Rgba {
    fn index_mut(&mut self, index: usize) -> &mut u8 {
        &mut self.0[index]
    }
}

/// A tightly packed RGBA8 raster shared by decoders and render backends.
#[derive(Debug, Eq, PartialEq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

impl Clone for RgbaImage {
    fn clone(&self) -> Self {
        Self {
            width: self.width,
            height: self.height,
            bytes: self.bytes.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.bytes.clone_from(&source.bytes);
        self.width = source.width;
        self.height = source.height;
    }
}

impl RgbaImage {
    fn pixel_count(width: u32, height: u32) -> Option<usize> {
        (width as usize).checked_mul(height as usize)
    }

    fn byte_count(width: u32, height: u32) -> Option<usize> {
        Self::pixel_count(width, height)?.checked_mul(4)
    }

    pub fn from_raw(width: u32, height: u32, bytes: Vec<u8>) -> Option<Self> {
        (Self::byte_count(width, height)? <= bytes.len()).then_some(Self {
            width,
            height,
            bytes,
        })
    }

    pub fn new(width: u32, height: u32) -> Self {
        let length = Self::byte_count(width, height).expect("RGBA raster dimensions overflow");
        Self {
            width,
            height,
            bytes: vec![0; length],
        }
    }

    pub fn from_pixel(width: u32, height: u32, pixel: Rgba) -> Self {
        let length = Self::byte_count(width, height).expect("RGBA raster dimensions overflow");
        let mut bytes = Vec::with_capacity(length);
        for _ in 0..length / 4 {
            bytes.extend_from_slice(&pixel.0);
        }
        Self {
            width,
            height,
            bytes,
        }
    }

    pub fn from_fn(width: u32, height: u32, mut pixel: impl FnMut(u32, u32) -> Rgba) -> Self {
        let length = Self::byte_count(width, height).expect("RGBA raster dimensions overflow");
        let mut bytes = Vec::with_capacity(length);
        for y in 0..height {
            for x in 0..width {
                bytes.extend_from_slice(&pixel(x, y).0);
            }
        }
        Self {
            width,
            height,
            bytes,
        }
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn as_raw(&self) -> &Vec<u8> {
        &self.bytes
    }

    pub fn into_raw(self) -> Vec<u8> {
        self.bytes
    }

    fn pixel_index(&self, x: u32, y: u32) -> usize {
        assert!(
            x < self.width && y < self.height,
            "RGBA pixel outside raster"
        );
        y as usize * self.width as usize + x as usize
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Rgba {
        Rgba(self.bytes.as_chunks::<4>().0[self.pixel_index(x, y)])
    }

    pub fn put_pixel(&mut self, x: u32, y: u32, pixel: Rgba) {
        let index = self.pixel_index(x, y);
        self.bytes.as_chunks_mut::<4>().0[index] = pixel.0;
    }

    pub fn pixels(&self) -> impl ExactSizeIterator<Item = Rgba> + DoubleEndedIterator + '_ {
        let count = self.width as usize * self.height as usize;
        self.bytes.as_chunks::<4>().0[..count]
            .iter()
            .copied()
            .map(Rgba)
    }

    pub fn enumerate_pixels(&self) -> impl ExactSizeIterator<Item = (u32, u32, Rgba)> + '_ {
        self.pixels().enumerate().map(|(index, pixel)| {
            let width = self.width as usize;
            ((index % width) as u32, (index / width) as u32, pixel)
        })
    }
}

impl AsRef<[u8]> for RgbaImage {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl Deref for RgbaImage {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}

impl AsMut<[u8]> for RgbaImage {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}
