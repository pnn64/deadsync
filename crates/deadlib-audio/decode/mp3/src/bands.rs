//! Layer III scalefactor boundaries. MPEG-2 24 kHz follows Table B.2's 332,
//! correcting the 330 typo in the old ISO simulation decoder's numeric data.
use crate::Error;
#[rustfmt::skip]
static LONG: [[u16;23];7] = [
    [0,4,8,12,16,20,24,30,36,44,52,62,74,90,110,134,162,196,238,288,342,418,576],
    [0,4,8,12,16,20,24,30,36,42,50,60,72,88,106,128,156,190,230,276,330,384,576],
    [0,4,8,12,16,20,24,30,36,44,54,66,82,102,126,156,194,240,296,364,448,550,576],
    [0,6,12,18,24,30,36,44,54,66,80,96,116,140,168,200,238,284,336,396,464,522,576],
    [0,6,12,18,24,30,36,44,54,66,80,96,114,136,162,194,232,278,332,394,464,540,576],
    [0,6,12,18,24,30,36,44,54,66,80,96,116,140,168,200,238,284,336,396,464,522,576],
    [0,12,24,36,48,60,72,88,108,132,160,192,232,280,336,400,476,566,568,570,572,574,576],
];
#[rustfmt::skip]
static SHORT: [[u16;14];7] = [
    [0,4,8,12,16,22,30,40,52,66,84,106,136,192],
    [0,4,8,12,16,22,28,38,50,64,80,100,126,192],
    [0,4,8,12,16,22,30,42,58,78,104,138,180,192],
    [0,4,8,12,18,24,32,42,56,74,100,132,174,192],
    [0,4,8,12,18,26,36,48,62,80,104,136,180,192],
    [0,4,8,12,18,26,36,48,62,80,104,134,174,192],
    [0,8,16,24,36,52,72,96,124,160,162,164,166,192],
];
#[derive(Clone, Copy, Debug)]
pub struct Bands {
    long: &'static [u16; 23],
    short: &'static [u16; 14],
    long_mixed: usize,
    mixed_lines: usize,
}
impl Bands {
    pub fn for_rate(rate: u32) -> Result<Self, Error> {
        let (l, s) = match rate {
            44100 => (0, 0),
            48000 => (1, 1),
            32000 => (2, 2),
            22050 => (3, 3),
            24000 => (4, 4),
            16000 => (5, 5),
            11025 | 12000 => (3, 5),
            8000 => (6, 6),
            _ => return Err(Error::Invalid("Layer III sampling rate")),
        };
        Ok(Self {
            long: &LONG[l],
            short: &SHORT[s],
            long_mixed: if rate >= 32000 { 8 } else { 6 },
            mixed_lines: if rate == 8000 { 72 } else { 36 },
        })
    }
    pub fn long(self) -> &'static [u16; 23] {
        self.long
    }
    pub fn short(self) -> &'static [u16; 14] {
        self.short
    }
    pub fn mixed_long_bands(self) -> usize {
        self.long_mixed
    }
    pub fn mixed_lines(self) -> usize {
        self.mixed_lines
    }
    pub fn mixed_subbands(self) -> usize {
        self.mixed_lines / 18
    }
}
