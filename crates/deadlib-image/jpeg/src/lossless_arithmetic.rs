//! Annex H.1.2.3: 25 two-dimensional contexts and 158 probability bins.
use crate::arithmetic::Arithmetic;
use crate::arithmetic_encode::Coder;
use crate::output::Sink;
use crate::{Error, Result};

pub(crate) struct Statistics {
    bins: [[u8; 158]; 4],
}
impl Default for Statistics {
    fn default() -> Self {
        Self {
            bins: [[0; 158]; 4],
        }
    }
}
impl Statistics {
    pub fn reset(&mut self) {
        self.bins.fill([0; 158]);
    }
    pub fn decode(
        &mut self,
        coder: &mut Arithmetic<'_>,
        table: usize,
        left: usize,
        above: usize,
        lower: u8,
        upper: u8,
    ) -> Result<(i32, u8)> {
        let bins = &mut self.bins[table];
        let context = 20 * left + 4 * above;
        if coder.decode(&mut bins[context])? == 0 {
            return Ok((0, 0));
        }
        let sign = coder.decode(&mut bins[context + 1])?;
        let mut magnitude = 0;
        if coder.decode(&mut bins[context + 2 + sign as usize])? != 0 {
            let mut bound = 1;
            let mut index = if above < 3 { 100 } else { 129 };
            while coder.decode(&mut bins[index])? != 0 {
                bound <<= 1;
                index += 1;
                if bound >= 32768 {
                    return Err(Error::Invalid("lossless arithmetic magnitude"));
                }
            }
            magnitude = bound;
            let mut bit = bound >> 1;
            while bit != 0 {
                if coder.decode(&mut bins[index + 14])? != 0 {
                    magnitude |= bit;
                }
                bit >>= 1;
            }
        }
        magnitude += 1;
        let difference = if sign == 0 { magnitude } else { -magnitude };
        Ok((difference, classify(difference, lower, upper)))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn encode<S: Sink>(
        &mut self,
        coder: &mut Coder<'_, S>,
        table: usize,
        left: usize,
        above: usize,
        difference: i32,
        lower: u8,
        upper: u8,
    ) -> Result<u8> {
        if !(-32768..=32768).contains(&difference) {
            return Err(Error::Invalid("lossless arithmetic magnitude"));
        }
        let bins = &mut self.bins[table];
        let context = 20 * left + 4 * above;
        coder.encode(&mut bins[context], u8::from(difference != 0))?;
        if difference == 0 {
            return Ok(0);
        }
        let sign = u8::from(difference < 0);
        coder.encode(&mut bins[context + 1], sign)?;
        let magnitude = difference.unsigned_abs() - 1;
        coder.encode(
            &mut bins[context + 2 + sign as usize],
            u8::from(magnitude != 0),
        )?;
        if magnitude != 0 {
            let mut bound = 1;
            let mut index = if above < 3 { 100 } else { 129 };
            loop {
                let more = magnitude >= bound * 2;
                coder.encode(&mut bins[index], u8::from(more))?;
                if !more {
                    break;
                }
                bound <<= 1;
                index += 1;
            }
            let mut bit = bound >> 1;
            while bit != 0 {
                coder.encode(&mut bins[index + 14], u8::from(magnitude & bit != 0))?;
                bit >>= 1;
            }
        }
        Ok(classify(difference, lower, upper))
    }
}

fn classify(difference: i32, lower: u8, upper: u8) -> u8 {
    let magnitude = difference.unsigned_abs();
    if magnitude <= if lower == 0 { 0 } else { 1 << (lower - 1) } {
        0
    } else if magnitude <= 1 << upper {
        1 + u8::from(difference < 0)
    } else {
        3 + u8::from(difference < 0)
    }
}

/// One row of difference classes per vertical sampling phase. Updating a column
/// consumes its old above-class before replacing it with the current class.
/// This also works for MCU-local vertical runs, without an image-sized grid.
pub(crate) struct History {
    data: Vec<u8>,
    planes: [(usize, usize, usize); 4],
}
impl History {
    pub fn bytes(shapes: &[(usize, usize)]) -> Result<usize> {
        shapes
            .iter()
            .try_fold(0usize, |total, &(width, rows)| {
                width.checked_mul(rows).and_then(|n| total.checked_add(n))
            })
            .ok_or(Error::LimitExceeded("lossless arithmetic history"))
    }
    pub fn new(shapes: &[(usize, usize)], available: usize) -> Result<Self> {
        let length = Self::bytes(shapes)?;
        if length > available {
            return Err(Error::LimitExceeded("lossless arithmetic history"));
        }
        let mut data = Vec::new();
        data.try_reserve_exact(length)
            .map_err(|_| Error::Allocation)?;
        data.resize(length, 0);
        let mut planes = [(0, 0, 0); 4];
        let mut offset = 0;
        for (i, &(width, rows)) in shapes.iter().enumerate() {
            planes[i] = (offset, width, rows);
            offset += width * rows;
        }
        Ok(Self { data, planes })
    }
    pub fn context(&self, component: usize, x: usize, y: usize, first_row: bool) -> (usize, usize) {
        let (offset, width, rows) = self.planes[component];
        let left = if x == 0 {
            0
        } else {
            self.data[offset + (y % rows) * width + x - 1] as usize
        };
        let above = if first_row {
            0
        } else {
            self.data[offset + ((y + rows - 1) % rows) * width + x] as usize
        };
        (left, above)
    }
    pub fn set(&mut self, component: usize, x: usize, y: usize, class: u8) {
        let (offset, width, rows) = self.planes[component];
        self.data[offset + (y % rows) * width + x] = class;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Output;
    #[test]
    fn difference_contexts_and_full_modulo_magnitudes() {
        assert_eq!(
            [0, 1, -1, 2, -2, 3, -3].map(|d| classify(d, 0, 1)),
            [0, 1, 2, 1, 2, 3, 4]
        );
        assert_eq!(
            [0, 1, -4, 5, -128, 129, -129].map(|d| classify(d, 3, 7)),
            [0, 0, 0, 1, 2, 3, 4]
        );
        for (lower, upper) in [(0, 1), (0, 0), (3, 7), (15, 15)] {
            let mut output = Output::new(1_000_000);
            let mut stats = Statistics::default();
            let mut coder = Coder::new(&mut output);
            for left in 0..5 {
                for above in 0..5 {
                    for difference in [
                        0, 1, -1, 2, -2, 3, -3, 128, -128, 32767, -32767, 32768, -32768,
                    ] {
                        assert_eq!(
                            stats
                                .encode(&mut coder, 3, left, above, difference, lower, upper)
                                .unwrap(),
                            classify(difference, lower, upper)
                        );
                    }
                }
            }
            coder.finish().unwrap();
            let mut stats = Statistics::default();
            let mut coder = Arithmetic::new(&output.bytes).unwrap();
            for left in 0..5 {
                for above in 0..5 {
                    for difference in [
                        0, 1, -1, 2, -2, 3, -3, 128, -128, 32767, -32767, 32768, -32768,
                    ] {
                        assert_eq!(
                            stats
                                .decode(&mut coder, 3, left, above, lower, upper)
                                .unwrap(),
                            (difference, classify(difference, lower, upper))
                        );
                    }
                }
            }
            coder.finish().unwrap();
        }
    }
}
