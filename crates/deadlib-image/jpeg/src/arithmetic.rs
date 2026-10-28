//! Native JPEG binary arithmetic decoding, T.81 Annex D.
use crate::progressive::ProgressiveScan;
use crate::{Error, Result, ZIGZAG};

#[derive(Clone, Copy)]
pub(crate) struct Probability {
    pub qe: u16,
    pub lps: u8,
    pub mps: u8,
}
const fn p(qe: u16, lps: u8, mps: u8, switch: bool) -> Probability {
    Probability {
        qe,
        lps: lps | if switch { 128 } else { 0 },
        mps,
    }
}
// T.81 Table D.3, independently transcribed from the rendered standard.
// The high LPS-transition bit toggles the stored most-probable-symbol sense.
pub(crate) const PROBABILITIES: [Probability; 113] = [
    p(0x5a1d, 1, 1, true),
    p(0x2586, 14, 2, false),
    p(0x1114, 16, 3, false),
    p(0x080b, 18, 4, false),
    p(0x03d8, 20, 5, false),
    p(0x01da, 23, 6, false),
    p(0x00e5, 25, 7, false),
    p(0x006f, 28, 8, false),
    p(0x0036, 30, 9, false),
    p(0x001a, 33, 10, false),
    p(0x000d, 35, 11, false),
    p(0x0006, 9, 12, false),
    p(0x0003, 10, 13, false),
    p(0x0001, 12, 13, false),
    p(0x5a7f, 15, 15, true),
    p(0x3f25, 36, 16, false),
    p(0x2cf2, 38, 17, false),
    p(0x207c, 39, 18, false),
    p(0x17b9, 40, 19, false),
    p(0x1182, 42, 20, false),
    p(0x0cef, 43, 21, false),
    p(0x09a1, 45, 22, false),
    p(0x072f, 46, 23, false),
    p(0x055c, 48, 24, false),
    p(0x0406, 49, 25, false),
    p(0x0303, 51, 26, false),
    p(0x0240, 52, 27, false),
    p(0x01b1, 54, 28, false),
    p(0x0144, 56, 29, false),
    p(0x00f5, 57, 30, false),
    p(0x00b7, 59, 31, false),
    p(0x008a, 60, 32, false),
    p(0x0068, 62, 33, false),
    p(0x004e, 63, 34, false),
    p(0x003b, 32, 35, false),
    p(0x002c, 33, 9, false),
    p(0x5ae1, 37, 37, true),
    p(0x484c, 64, 38, false),
    p(0x3a0d, 65, 39, false),
    p(0x2ef1, 67, 40, false),
    p(0x261f, 68, 41, false),
    p(0x1f33, 69, 42, false),
    p(0x19a8, 70, 43, false),
    p(0x1518, 72, 44, false),
    p(0x1177, 73, 45, false),
    p(0x0e74, 74, 46, false),
    p(0x0bfb, 75, 47, false),
    p(0x09f8, 77, 48, false),
    p(0x0861, 78, 49, false),
    p(0x0706, 79, 50, false),
    p(0x05cd, 48, 51, false),
    p(0x04de, 50, 52, false),
    p(0x040f, 50, 53, false),
    p(0x0363, 51, 54, false),
    p(0x02d4, 52, 55, false),
    p(0x025c, 53, 56, false),
    p(0x01f8, 54, 57, false),
    p(0x01a4, 55, 58, false),
    p(0x0160, 56, 59, false),
    p(0x0125, 57, 60, false),
    p(0x00f6, 58, 61, false),
    p(0x00cb, 59, 62, false),
    p(0x00ab, 61, 63, false),
    p(0x008f, 61, 32, false),
    p(0x5b12, 65, 65, true),
    p(0x4d04, 80, 66, false),
    p(0x412c, 81, 67, false),
    p(0x37d8, 82, 68, false),
    p(0x2fe8, 83, 69, false),
    p(0x293c, 84, 70, false),
    p(0x2379, 86, 71, false),
    p(0x1edf, 87, 72, false),
    p(0x1aa9, 87, 73, false),
    p(0x174e, 72, 74, false),
    p(0x1424, 72, 75, false),
    p(0x119c, 74, 76, false),
    p(0x0f6b, 74, 77, false),
    p(0x0d51, 75, 78, false),
    p(0x0bb6, 77, 79, false),
    p(0x0a40, 77, 48, false),
    p(0x5832, 80, 81, true),
    p(0x4d1c, 88, 82, false),
    p(0x438e, 89, 83, false),
    p(0x3bdd, 90, 84, false),
    p(0x34ee, 91, 85, false),
    p(0x2eae, 92, 86, false),
    p(0x299a, 93, 87, false),
    p(0x2516, 86, 71, false),
    p(0x5570, 88, 89, true),
    p(0x4ca9, 95, 90, false),
    p(0x44d9, 96, 91, false),
    p(0x3e22, 97, 92, false),
    p(0x3824, 99, 93, false),
    p(0x32b4, 99, 94, false),
    p(0x2e17, 93, 86, false),
    p(0x56a8, 95, 96, true),
    p(0x4f46, 101, 97, false),
    p(0x47e5, 102, 98, false),
    p(0x41cf, 103, 99, false),
    p(0x3c3d, 104, 100, false),
    p(0x375e, 99, 93, false),
    p(0x5231, 105, 102, false),
    p(0x4c0f, 106, 103, false),
    p(0x4639, 107, 104, false),
    p(0x415e, 103, 99, false),
    p(0x5627, 105, 106, true),
    p(0x50e7, 108, 107, false),
    p(0x4b85, 109, 103, false),
    p(0x5597, 110, 109, false),
    p(0x504f, 111, 107, false),
    p(0x5a10, 110, 111, true),
    p(0x5522, 112, 109, false),
    p(0x59eb, 112, 111, true),
];

/// C contains the concatenated 16-bit Cx and C-low registers of D.2.3.
pub(crate) struct Arithmetic<'a> {
    input: &'a [u8],
    position: usize,
    a: u32,
    c: u32,
    count: u8,
}
impl<'a> Arithmetic<'a> {
    pub fn new(input: &'a [u8]) -> Result<Self> {
        let mut result = Self {
            input,
            position: 0,
            a: 65536,
            c: 0,
            count: 0,
        };
        result.initialize()?;
        Ok(result)
    }
    fn initialize(&mut self) -> Result<()> {
        self.a = 65536;
        self.count = 0;
        self.c = (self.byte()? as u32) << 24;
        self.c |= (self.byte()? as u32) << 16;
        Ok(())
    }
    fn byte(&mut self) -> Result<u8> {
        let Some(&byte) = self.input.get(self.position) else {
            return Ok(0);
        };
        if byte != 255 {
            self.position += 1;
            return Ok(byte);
        }
        match self.input.get(self.position + 1) {
            Some(0) => {
                self.position += 2;
                Ok(255)
            }
            Some(_) => Ok(0), // D.2.6: a marker causes zero-padding until the interval ends.
            None => Err(Error::Truncated),
        }
    }
    #[inline]
    pub fn decode(&mut self, state: &mut u8) -> Result<u8> {
        let probability = PROBABILITIES[(*state & 127) as usize];
        let qe = u32::from(probability.qe);
        let mps = *state >> 7;
        self.a -= qe;
        let lps;
        if self.c >> 16 >= self.a {
            self.c -= self.a << 16;
            lps = self.a >= qe;
            self.a = qe;
        } else if self.a < 0x8000 {
            lps = self.a < qe;
        } else {
            return Ok(mps);
        }
        *state = if lps {
            (*state & 128) ^ probability.lps
        } else {
            (*state & 128) | probability.mps
        };
        while self.a < 0x8000 {
            if self.count == 0 {
                self.c |= (self.byte()? as u32) << 8;
                self.count = 8;
            }
            self.a <<= 1;
            self.c <<= 1;
            self.count -= 1;
        }
        Ok(mps ^ u8::from(lps))
    }
    pub fn fixed(&mut self) -> Result<u8> {
        self.decode(&mut 0)
    }
    fn seek_marker(&mut self) -> Result<Option<u8>> {
        while let Some(&byte) = self.input.get(self.position) {
            if byte != 255 {
                self.position += 1;
                continue;
            }
            self.position += 1;
            if self.input.get(self.position) == Some(&0) {
                self.position += 1;
                continue;
            }
            while self.input.get(self.position) == Some(&255) {
                self.position += 1;
            }
            let marker = *self.input.get(self.position).ok_or(Error::Truncated)?;
            self.position += 1;
            return Ok(Some(marker));
        }
        Ok(None)
    }
    pub fn restart(&mut self, expected: u8) -> Result<()> {
        if self.seek_marker()? != Some(0xd0 + expected) {
            return Err(Error::Invalid("arithmetic restart sequence"));
        }
        self.initialize()
    }
    pub fn finish(mut self) -> Result<()> {
        if self.seek_marker()?.is_some() {
            return Err(Error::Invalid("unexpected arithmetic restart"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Conditioning {
    pub dc_lower: [u8; 4],
    pub dc_upper: [u8; 4],
    pub ac_split: [u8; 4],
}
impl Default for Conditioning {
    fn default() -> Self {
        Self {
            dc_lower: [0; 4],
            dc_upper: [1; 4],
            ac_split: [5; 4],
        }
    }
}
impl Conditioning {
    pub fn parse(&mut self, data: &[u8]) -> Result<()> {
        if data.is_empty() || !data.len().is_multiple_of(2) {
            return Err(Error::Invalid("DAC length"));
        }
        for &[selector, value] in data.as_chunks::<2>().0 {
            let class = selector >> 4;
            let id = (selector & 15) as usize;
            if class > 1 || id > 3 {
                return Err(Error::Invalid("DAC selector"));
            }
            if class == 0 {
                let lower = value & 15;
                let upper = value >> 4;
                if lower > upper {
                    return Err(Error::Invalid("DAC DC bounds"));
                }
                self.dc_lower[id] = lower;
                self.dc_upper[id] = upper;
            } else {
                if !(1..=63).contains(&value) {
                    return Err(Error::Invalid("DAC AC split"));
                }
                self.ac_split[id] = value;
            }
        }
        Ok(())
    }
}

pub(crate) struct Statistics {
    pub dc: [[u8; 49]; 4],
    pub ac: [[u8; 245]; 4],
}
impl Default for Statistics {
    fn default() -> Self {
        Self {
            dc: [[0; 49]; 4],
            ac: [[0; 245]; 4],
        }
    }
}
impl Statistics {
    pub fn reset(&mut self) {
        self.dc.fill([0; 49]);
        self.ac.fill([0; 245]);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn sequential(
        &mut self,
        block: &mut [i16; 64],
        coder: &mut Arithmetic<'_>,
        dc: usize,
        ac: usize,
        previous_class: &mut usize,
        predictor: &mut i32,
        conditioning: Conditioning,
    ) -> Result<()> {
        self.dc_initial(block, coder, dc, previous_class, predictor, conditioning)?;
        self.ac_initial(block, coder, ac, conditioning, 1, 63, 0)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn progressive(
        &mut self,
        scan: ProgressiveScan,
        block: &mut [i16; 64],
        coder: &mut Arithmetic<'_>,
        dc: usize,
        ac: usize,
        previous_class: &mut usize,
        predictor: &mut i32,
        conditioning: Conditioning,
    ) -> Result<()> {
        if scan.ss == 0 {
            if scan.ah == 0 {
                self.dc_initial(block, coder, dc, previous_class, predictor, conditioning)?;
                block[0] = i16::try_from(*predictor << scan.al)
                    .map_err(|_| Error::Invalid("arithmetic progressive DC range"))?;
            } else {
                // DC point transforms preserve two's-complement low bits.
                block[0] |= i16::from(coder.fixed()?) << scan.al;
            }
        } else if scan.ah == 0 {
            self.ac_initial(block, coder, ac, conditioning, scan.ss, scan.se, scan.al)?;
        } else {
            self.ac_refine(block, coder, ac, scan)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn dc_initial(
        &mut self,
        block: &mut [i16; 64],
        coder: &mut Arithmetic<'_>,
        dc: usize,
        previous_class: &mut usize,
        predictor: &mut i32,
        conditioning: Conditioning,
    ) -> Result<()> {
        let state = &mut self.dc[dc];
        let context = *previous_class;
        if coder.decode(&mut state[context])? != 0 {
            let sign = coder.decode(&mut state[context + 1])?;
            let mut magnitude = 0u32;
            if coder.decode(&mut state[context + 2 + sign as usize])? != 0 {
                let mut bound = 1u32;
                let mut category = 20usize;
                while coder.decode(&mut state[category])? != 0 {
                    bound <<= 1;
                    category += 1;
                    if bound >= 32768 || category > 34 {
                        return Err(Error::Invalid("arithmetic DC magnitude"));
                    }
                }
                magnitude = bound;
                let mut bit = bound >> 1;
                while bit > 0 {
                    if coder.decode(&mut state[category + 14])? != 0 {
                        magnitude |= bit;
                    }
                    bit >>= 1;
                }
            }
            magnitude += 1;
            if magnitude > 32768 || (magnitude == 32768 && sign == 0) {
                return Err(Error::Invalid("arithmetic DC magnitude"));
            }
            let lower = conditioning.dc_lower[dc];
            let upper = conditioning.dc_upper[dc];
            *previous_class = if magnitude <= if lower == 0 { 0 } else { 1 << (lower - 1) } {
                0
            } else if magnitude <= 1 << upper {
                4 + 4 * sign as usize
            } else {
                12 + 4 * sign as usize
            };
            *predictor += if sign == 0 {
                magnitude as i32
            } else {
                -(magnitude as i32)
            };
        } else {
            *previous_class = 0;
        }
        block[0] = i16::try_from(*predictor)
            .map_err(|_| Error::Invalid("arithmetic DC coefficient range"))?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn ac_initial(
        &mut self,
        block: &mut [i16; 64],
        coder: &mut Arithmetic<'_>,
        ac: usize,
        conditioning: Conditioning,
        ss: usize,
        se: usize,
        al: u8,
    ) -> Result<()> {
        let state = &mut self.ac[ac];
        let mut k = ss;
        while k <= se {
            let mut base = 3 * (k - 1);
            if coder.decode(&mut state[base])? != 0 {
                break;
            }
            while coder.decode(&mut state[base + 1])? == 0 {
                k += 1;
                base += 3;
                if k > se {
                    return Err(Error::Invalid("arithmetic AC zero run"));
                }
            }
            let sign = coder.fixed()?;
            let mut magnitude = 0u32;
            if coder.decode(&mut state[base + 2])? != 0 {
                let mut bound = 1u32;
                let mut category = base + 2;
                if coder.decode(&mut state[category])? != 0 {
                    bound = 2;
                    category = if k <= conditioning.ac_split[ac] as usize {
                        189
                    } else {
                        217
                    };
                    while coder.decode(&mut state[category])? != 0 {
                        bound <<= 1;
                        category += 1;
                        if bound >= 32768 {
                            return Err(Error::Invalid("arithmetic AC magnitude"));
                        }
                    }
                }
                magnitude = bound;
                let mut bit = bound >> 1;
                while bit > 0 {
                    if coder.decode(&mut state[category + 14])? != 0 {
                        magnitude |= bit;
                    }
                    bit >>= 1;
                }
            }
            magnitude += 1;
            if magnitude > 32768 {
                return Err(Error::Invalid("arithmetic AC magnitude"));
            }
            let value = if sign == 0 {
                magnitude as i32
            } else {
                -(magnitude as i32)
            };
            block[ZIGZAG[k]] = i16::try_from(value << al)
                .map_err(|_| Error::Invalid("arithmetic AC coefficient range"))?;
            k += 1;
        }
        Ok(())
    }

    fn ac_refine(
        &mut self,
        block: &mut [i16; 64],
        coder: &mut Arithmetic<'_>,
        ac: usize,
        scan: ProgressiveScan,
    ) -> Result<()> {
        let state = &mut self.ac[ac];
        let bit = 1i16 << scan.al;
        // G.1.3.3: before the previous end-of-band, EOB decisions are bypassed.
        // History is local to this block/band, rather than an inter-block EOB run.
        let eob_history = (scan.ss..=scan.se)
            .rfind(|&k| block[ZIGZAG[k]] != 0)
            .map_or(scan.ss, |k| k + 1);
        let mut k = scan.ss;
        while k <= scan.se {
            let mut base = 3 * (k - 1);
            if k >= eob_history && coder.decode(&mut state[base])? != 0 {
                break;
            }
            loop {
                let value = &mut block[ZIGZAG[k]];
                if *value != 0 {
                    if coder.decode(&mut state[base + 2])? != 0 {
                        *value = value
                            .checked_add(if *value > 0 { bit } else { -bit })
                            .ok_or(Error::Invalid("arithmetic AC refinement range"))?;
                    }
                    break;
                }
                if coder.decode(&mut state[base + 1])? != 0 {
                    *value = if coder.fixed()? == 0 { bit } else { -bit };
                    break;
                }
                k += 1;
                base += 3;
                if k > scan.se {
                    return Err(Error::Invalid("arithmetic refinement zero run"));
                }
            }
            k += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn t81_k4_arithmetic_golden_sequence() {
        let coded = [
            0x65, 0x5b, 0x51, 0x44, 0xf7, 0x96, 0x9d, 0x51, 0x78, 0x55, 0xbf, 0xff, 0, 0xfc, 0x51,
            0x84, 0xc7, 0xce, 0xf9, 0x39, 0, 0x28, 0x7d, 0x46, 0x70, 0x8e, 0xcb, 0xc0, 0xf6,
        ];
        let expected = [
            0x00, 0x02, 0x00, 0x51, 0, 0, 0, 0xc0, 0x03, 0x52, 0x87, 0x2a, 0xaa, 0xaa, 0xaa, 0xaa,
            0x82, 0xc0, 0x20, 0, 0xfc, 0xd7, 0x9e, 0xf6, 0x74, 0xea, 0xab, 0xf7, 0x69, 0x7e, 0xe7,
            0x4c,
        ];
        let mut decoder = Arithmetic::new(&coded).unwrap();
        let mut state = 0;
        for (byte, &value) in expected.iter().enumerate() {
            for bit in (0..8).rev() {
                assert_eq!(
                    decoder.decode(&mut state).unwrap(),
                    (value >> bit) & 1,
                    "event {}",
                    byte * 8 + 7 - bit
                );
            }
        }
        decoder.finish().unwrap();
    }

    #[test]
    fn probability_transitions_and_conditioning_bounds() {
        for p in PROBABILITIES {
            assert!((1..0x8000).contains(&p.qe));
            assert!((p.lps & 127) < 113 && p.mps < 113);
        }
        let mut tables = Conditioning::default();
        tables
            .parse(&[0, 0, 1, 0x73, 2, 0xff, 3, 0x10, 0x10, 1, 0x13, 63])
            .unwrap();
        assert_eq!(tables.dc_lower, [0, 3, 15, 0]);
        assert_eq!(tables.dc_upper, [0, 7, 15, 1]);
        assert_eq!(tables.ac_split, [1, 5, 5, 63]);
        for data in [
            &[][..],
            &[0],
            &[0, 0, 1],
            &[4, 0],
            &[0x20, 1],
            &[0, 0x01],
            &[0x10, 0],
            &[0x13, 64],
        ] {
            assert!(tables.parse(data).is_err(), "{data:?}");
        }
    }

    #[test]
    fn zero_padding_stuffing_and_restart_register_reset() {
        let mut zero = Arithmetic::new(&[]).unwrap();
        let mut marker = Arithmetic::new(&[255, 0xd0, 0, 0]).unwrap();
        for _ in 0..256 {
            assert_eq!(zero.fixed().unwrap(), marker.fixed().unwrap());
        }
        marker.restart(0).unwrap();
        let mut fresh = Arithmetic::new(&[0, 0]).unwrap();
        for _ in 0..256 {
            assert_eq!(marker.fixed().unwrap(), fresh.fixed().unwrap());
        }
        marker.finish().unwrap();
        assert!(Arithmetic::new(&[255]).is_err());
        assert!(Arithmetic::new(&[255, 0xd1]).unwrap().restart(0).is_err());
        assert!(Arithmetic::new(&[255, 0xd0]).unwrap().finish().is_err());
        let mut stuffed = Arithmetic::new(&[255, 0, 7]).unwrap();
        assert_eq!(stuffed.c, 0xff07_0000);
        assert_eq!(stuffed.byte().unwrap(), 0);
    }
}
