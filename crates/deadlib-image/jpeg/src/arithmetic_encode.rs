//! Native arithmetic encoding, derived from T.81 D.1 and F.1.4.
use crate::arithmetic::{PROBABILITIES, Statistics};
use crate::output::Output;
use crate::{Error, Result, ZIGZAG};

/// Arithmetic conditioning parameters written for each encoder table.
#[derive(Clone, Copy, Debug)]
pub struct ArithmeticConditioning {
    /// Lower DC conditioning exponent L, in 0..=U.
    pub dc_lower: u8,
    /// Upper DC conditioning exponent U, in L..=15.
    pub dc_upper: u8,
    /// AC magnitude-model boundary Kx, in 1..=63.
    pub ac_split: u8,
}
impl Default for ArithmeticConditioning {
    fn default() -> Self {
        Self {
            dc_lower: 0,
            dc_upper: 1,
            ac_split: 5,
        }
    }
}
impl ArithmeticConditioning {
    pub(crate) fn validate(self) -> Result<()> {
        if self.dc_lower > self.dc_upper || self.dc_upper > 15 || !(1..=63).contains(&self.ac_split)
        {
            return Err(Error::Invalid("encoder arithmetic conditioning"));
        }
        Ok(())
    }
}

/// Carry resolution retains one byte and a count of pending FFs. Zero runs
/// remain pending so Flush can discard trailing zero bytes without rewriting
/// output or allocating an intermediate entropy buffer.
pub(crate) struct Coder<'a> {
    output: &'a mut Output,
    a: u32,
    c: u32,
    count: u8,
    pending: Option<u8>,
    stack: usize,
    zeros: usize,
}

impl Statistics {
    #[allow(clippy::too_many_arguments)]
    pub fn encode_dc(
        &mut self,
        coder: &mut Coder<'_>,
        table: usize,
        value: i32,
        predictor: &mut i32,
        previous_class: &mut usize,
        conditioning: ArithmeticConditioning,
    ) -> Result<()> {
        let difference = value - *predictor;
        // F.1.4 defines the maximum-precision model for both 8/12-bit images.
        if i16::try_from(difference).is_err() {
            return Err(Error::Invalid("encoder arithmetic DC magnitude"));
        }
        let state = &mut self.dc[table];
        let context = *previous_class;
        coder.encode(&mut state[context], u8::from(difference != 0))?;
        *predictor = value;
        if difference == 0 {
            *previous_class = 0;
            return Ok(());
        }
        let sign = u8::from(difference < 0);
        coder.encode(&mut state[context + 1], sign)?;
        let magnitude = difference.unsigned_abs() - 1;
        coder.encode(
            &mut state[context + 2 + sign as usize],
            u8::from(magnitude != 0),
        )?;
        if magnitude != 0 {
            let mut bound = 1;
            let mut category = 20;
            loop {
                let more = magnitude >= bound * 2;
                coder.encode(&mut state[category], u8::from(more))?;
                if !more {
                    break;
                }
                bound <<= 1;
                category += 1;
            }
            let mut bit = bound >> 1;
            while bit != 0 {
                coder.encode(&mut state[category + 14], u8::from(magnitude & bit != 0))?;
                bit >>= 1;
            }
        }
        let absolute = magnitude + 1;
        let lower = conditioning.dc_lower;
        *previous_class = if absolute <= if lower == 0 { 0 } else { 1 << (lower - 1) } {
            0
        } else if absolute <= 1 << conditioning.dc_upper {
            4 + 4 * sign as usize
        } else {
            12 + 4 * sign as usize
        };
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode_ac(
        &mut self,
        coder: &mut Coder<'_>,
        table: usize,
        block: &[i16; 64],
        conditioning: ArithmeticConditioning,
        ss: usize,
        se: usize,
        al: u8,
    ) -> Result<()> {
        let state = &mut self.ac[table];
        let magnitude = |k: usize| i32::from(block[ZIGZAG[k]]).unsigned_abs() >> al;
        let last = (ss..=se).rfind(|&k| magnitude(k) != 0).unwrap_or(ss - 1);
        let mut k = ss;
        while k <= se {
            let mut base = 3 * (k - 1);
            coder.encode(&mut state[base], u8::from(k > last))?;
            if k > last {
                break;
            }
            while magnitude(k) == 0 {
                coder.encode(&mut state[base + 1], 0)?;
                k += 1;
                base += 3;
            }
            coder.encode(&mut state[base + 1], 1)?;
            let value = i32::from(block[ZIGZAG[k]]);
            let absolute = magnitude(k);
            coder.fixed(u8::from(value < 0))?;
            let magnitude = absolute - 1;
            coder.encode(&mut state[base + 2], u8::from(magnitude != 0))?;
            if magnitude != 0 {
                let mut bound = 1;
                let mut category = base + 2;
                coder.encode(&mut state[category], u8::from(magnitude >= 2))?;
                if magnitude >= 2 {
                    bound = 2;
                    category = if k <= conditioning.ac_split as usize {
                        189
                    } else {
                        217
                    };
                    loop {
                        let more = magnitude >= bound * 2;
                        coder.encode(&mut state[category], u8::from(more))?;
                        if !more {
                            break;
                        }
                        bound <<= 1;
                        category += 1;
                    }
                }
                let mut bit = bound >> 1;
                while bit != 0 {
                    coder.encode(&mut state[category + 14], u8::from(magnitude & bit != 0))?;
                    bit >>= 1;
                }
            }
            k += 1;
        }
        Ok(())
    }

    pub fn encode_ac_refine(
        &mut self,
        coder: &mut Coder<'_>,
        table: usize,
        block: &[i16; 64],
        scan: crate::progressive::ProgressiveScan,
    ) -> Result<()> {
        let magnitude = |k: usize| i32::from(block[ZIGZAG[k]]).unsigned_abs();
        let last = (scan.ss..=scan.se)
            .rfind(|&k| magnitude(k) >> scan.al != 0)
            .unwrap_or(scan.ss - 1);
        let history = (scan.ss..=scan.se)
            .rfind(|&k| magnitude(k) >> scan.ah != 0)
            .map_or(scan.ss, |k| k + 1);
        let state = &mut self.ac[table];
        let mut k = scan.ss;
        while k <= scan.se {
            let mut base = 3 * (k - 1);
            if k >= history {
                coder.encode(&mut state[base], u8::from(k > last))?;
                if k > last {
                    break;
                }
            }
            loop {
                let value = magnitude(k) >> scan.al;
                if value > 1 {
                    coder.encode(&mut state[base + 2], (value & 1) as u8)?;
                    break;
                }
                coder.encode(&mut state[base + 1], u8::from(value != 0))?;
                if value != 0 {
                    coder.fixed(u8::from(block[ZIGZAG[k]] < 0))?;
                    break;
                }
                k += 1;
                base += 3;
            }
            k += 1;
        }
        Ok(())
    }
}
impl<'a> Coder<'a> {
    pub fn new(output: &'a mut Output) -> Self {
        Self {
            output,
            a: 65536,
            c: 0,
            count: 11,
            pending: None,
            stack: 0,
            zeros: 0,
        }
    }
    #[inline]
    pub fn encode(&mut self, state: &mut u8, decision: u8) -> Result<()> {
        let p = PROBABILITIES[(*state & 127) as usize];
        let qe = u32::from(p.qe);
        let lps = decision != *state >> 7;
        self.a -= qe;
        if lps {
            if self.a >= qe {
                self.c += self.a;
                self.a = qe;
            }
            *state = (*state & 128) ^ p.lps;
        } else {
            if self.a >= 0x8000 {
                return Ok(());
            }
            if self.a < qe {
                self.c += self.a;
                self.a = qe;
            }
            *state = (*state & 128) | p.mps;
        }
        while self.a < 0x8000 {
            self.a <<= 1;
            self.c <<= 1;
            self.count -= 1;
            if self.count == 0 {
                self.byte_out()?;
                self.count = 8;
            }
        }
        Ok(())
    }
    pub fn fixed(&mut self, decision: u8) -> Result<()> {
        self.encode(&mut 0, decision)
    }

    fn emit(&mut self, byte: u8) -> Result<()> {
        if byte == 0 {
            self.zeros = self
                .zeros
                .checked_add(1)
                .ok_or(Error::LimitExceeded("encoded bytes"))?;
            return Ok(());
        }
        let zeros = [0; 64];
        while self.zeros != 0 {
            let n = self.zeros.min(zeros.len());
            self.output.raw(&zeros[..n])?;
            self.zeros -= n;
        }
        if byte == 255 {
            self.output.raw(&[255, 0])
        } else {
            self.output.raw(&[byte])
        }
    }
    fn byte_out(&mut self) -> Result<()> {
        let value = self.c >> 19;
        if value > 255 {
            if let Some(previous) = self.pending.take() {
                self.emit(
                    previous
                        .checked_add(1)
                        .ok_or(Error::Invalid("arithmetic encoder carry"))?,
                )?;
            }
            while self.stack != 0 {
                self.emit(0)?;
                self.stack -= 1;
            }
            self.pending = Some(value as u8);
        } else if value == 255 {
            self.stack = self
                .stack
                .checked_add(1)
                .ok_or(Error::LimitExceeded("encoded bytes"))?;
        } else {
            if let Some(previous) = self.pending.take() {
                self.emit(previous)?;
            }
            while self.stack != 0 {
                self.emit(255)?;
                self.stack -= 1;
            }
            self.pending = Some(value as u8);
        }
        self.c &= 0x7ffff;
        Ok(())
    }
    pub fn finish(&mut self) -> Result<()> {
        let mut final_code = (self.c + self.a - 1) & 0xffff0000;
        if final_code < self.c {
            final_code += 0x8000;
        }
        self.c = final_code << self.count;
        self.byte_out()?;
        self.c <<= 8;
        self.byte_out()?;
        if let Some(previous) = self.pending.take() {
            self.emit(previous)?;
        }
        while self.stack != 0 {
            self.emit(255)?;
            self.stack -= 1;
        }
        self.zeros = 0; // D.1.8: trailing unstuffed zeros are supplied by the decoder.
        Ok(())
    }
    pub fn restart(&mut self, index: u8) -> Result<()> {
        self.finish()?;
        self.output.marker(0xd0 + index)?;
        self.a = 65536;
        self.c = 0;
        self.count = 11;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arithmetic::Arithmetic;
    #[test]
    fn t81_k4_encoding_golden_sequence() {
        let decisions = [
            0, 2, 0, 0x51, 0, 0, 0, 0xc0, 3, 0x52, 0x87, 0x2a, 0xaa, 0xaa, 0xaa, 0xaa, 0x82, 0xc0,
            0x20, 0, 0xfc, 0xd7, 0x9e, 0xf6, 0x74, 0xea, 0xab, 0xf7, 0x69, 0x7e, 0xe7, 0x4c,
        ];
        let expected = [
            0x65, 0x5b, 0x51, 0x44, 0xf7, 0x96, 0x9d, 0x51, 0x78, 0x55, 0xbf, 0xff, 0, 0xfc, 0x51,
            0x84, 0xc7, 0xce, 0xf9, 0x39, 0, 0x28, 0x7d, 0x46, 0x70, 0x8e, 0xcb, 0xc0, 0xf6,
        ];
        for limit in [expected.len(), usize::MAX] {
            let mut output = Output::new(limit);
            let mut encoder = Coder::new(&mut output);
            let mut state = 0;
            for byte in decisions {
                for bit in (0..8).rev() {
                    encoder.encode(&mut state, (byte >> bit) & 1).unwrap();
                }
            }
            encoder.finish().unwrap();
            assert_eq!(output.bytes, expected);
        }
    }
    #[test]
    fn adaptive_fixed_decisions_carries_and_restart_resets() {
        let mut bits = Vec::new();
        let mut random = 0xa7c1_0359u32;
        for i in 0..200000 {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            bits.push(match i / 10000 % 4 {
                0 => 0,
                1 => 1,
                _ => (random >> 31) as u8,
            });
        }
        let mut output = Output::new(100000);
        let mut encoder = Coder::new(&mut output);
        let mut states = [0; 16];
        for (i, &bit) in bits.iter().enumerate() {
            if i % 17 == 0 {
                encoder.fixed(bit).unwrap();
            } else {
                encoder.encode(&mut states[i % 16], bit).unwrap();
            }
            if i == 99999 {
                encoder.restart(0).unwrap();
                states.fill(0);
            }
        }
        encoder.finish().unwrap();
        assert!(output.bytes.windows(2).any(|w| w == [255, 0]));
        let mut decoder = Arithmetic::new(&output.bytes).unwrap();
        states.fill(0);
        for (i, &bit) in bits.iter().enumerate() {
            let value = if i % 17 == 0 {
                decoder.fixed().unwrap()
            } else {
                decoder.decode(&mut states[i % 16]).unwrap()
            };
            assert_eq!(value, bit, "decision {i}");
            if i == 99999 {
                decoder.restart(0).unwrap();
                states.fill(0);
            }
        }
        decoder.finish().unwrap();
    }
}
