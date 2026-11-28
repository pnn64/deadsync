use crate::bitstream::{vlc_table, Bits};
use crate::frame::Prediction;
use crate::headers::{Coding, Sequence};
use crate::{block, tables, Error, Frame, PictureKind, Result, Unit};

const INTRA: u8 = 1;
const PATTERN: u8 = 2;
const QUANTIZER: u8 = 4;
const FORWARD: u8 = 8;
const BACKWARD: u8 = 16;
const ZERO_MOTION: u8 = 32;

static I_VLC: [u16; 4] = vlc_table(&[(1, 1), (1, 2)]);
static I_MODES: [u8; 2] = [INTRA, QUANTIZER | INTRA];
static P_VLC: [u16; 64] = vlc_table(&[(3, 5), (1, 2), (1, 3), (1, 1), (1, 6), (1, 5), (2, 5)]);
static P_MODES: [u8; 7] = [
    INTRA,
    PATTERN | FORWARD | ZERO_MOTION,
    FORWARD,
    FORWARD | PATTERN,
    QUANTIZER | INTRA,
    QUANTIZER | PATTERN | FORWARD | ZERO_MOTION,
    QUANTIZER | FORWARD | PATTERN,
];
static B_VLC: [u16; 64] = vlc_table(&[
    (3, 5),
    (2, 3),
    (3, 3),
    (2, 4),
    (3, 4),
    (2, 2),
    (3, 2),
    (1, 6),
    (2, 6),
    (3, 6),
    (2, 5),
]);
static B_MODES: [u8; 11] = [
    INTRA,
    BACKWARD,
    BACKWARD | PATTERN,
    FORWARD,
    FORWARD | PATTERN,
    FORWARD | BACKWARD,
    FORWARD | BACKWARD | PATTERN,
    QUANTIZER | INTRA,
    QUANTIZER | BACKWARD | PATTERN,
    QUANTIZER | FORWARD | PATTERN,
    QUANTIZER | FORWARD | BACKWARD | PATTERN,
];

pub(crate) struct Slice<'a> {
    pub(crate) sequence: &'a Sequence,
    pub(crate) coding: &'a Coding,
    pub(crate) references: [Option<&'a Frame>; 2],
}

struct Predictors {
    dc: [i32; 3],
    motion: [[i32; 2]; 2],
    last_mode: u8,
}

impl Slice<'_> {
    pub(crate) fn decode<const MPEG2: bool>(
        &self,
        unit: Unit<'_>,
        frame: &mut Frame,
        next_macroblock: &mut usize,
    ) -> Result<()> {
        let mut bits = Bits::new(unit.payload);
        let mut quantizer = self.coding.quantizer(&mut bits)?;
        while bits.flag()? {
            bits.skip(8)?;
        }

        let columns = self.sequence.width.div_ceil(16);
        let count = columns * self.sequence.macroblock_rows();
        let row = usize::from(unit.code - 1);
        if row >= self.sequence.macroblock_rows() {
            return Err(Error::Invalid("slice row exceeds picture"));
        }
        let first = address_increment(&mut bits, columns)?;
        let mut address = row * columns + first - 1;
        if first > columns || address != *next_macroblock {
            return Err(Error::Invalid(
                "slice start does not match picture coverage",
            ));
        }
        let mut predictors = Predictors {
            dc: self.initial_dc(),
            motion: [[0; 2]; 2],
            last_mode: 0,
        };

        loop {
            if address >= count {
                return Err(Error::Invalid("macroblock address exceeds picture"));
            }
            self.macroblock::<MPEG2>(
                &mut bits,
                frame,
                (address % columns * 16, address / columns * 16),
                &mut quantizer,
                &mut predictors,
            )?;
            address += 1;
            if bits.at_padding() {
                *next_macroblock = address;
                return Ok(());
            }

            let target = address + address_increment(&mut bits, count - address)? - 1;
            if target >= count {
                return Err(Error::Invalid("macroblock address exceeds picture"));
            }
            if target > address {
                let mode = match frame.picture_kind() {
                    PictureKind::P => {
                        predictors.motion[0] = [0; 2];
                        FORWARD
                    }
                    PictureKind::B if predictors.last_mode & INTRA == 0 => predictors.last_mode,
                    _ => return Err(Error::Invalid("skipped macroblock after intra coding")),
                };
                predictors.dc = self.initial_dc();
                for skipped in address..target {
                    self.predict::<MPEG2>(
                        frame,
                        skipped % columns * 16,
                        skipped / columns * 16,
                        mode,
                        predictors.motion,
                    )?;
                }
            }
            address = target;
        }
    }

    fn initial_dc(&self) -> [i32; 3] {
        [128 << self.coding.dc_precision; 3]
    }

    fn macroblock<const MPEG2: bool>(
        &self,
        bits: &mut Bits<'_>,
        frame: &mut Frame,
        (x, y): (usize, usize),
        quantizer: &mut i32,
        predictors: &mut Predictors,
    ) -> Result<()> {
        let mode = match frame.picture_kind() {
            PictureKind::I => I_MODES[bits.vlc(&I_VLC)?],
            PictureKind::P => P_MODES[bits.vlc(&P_VLC)?],
            PictureKind::B => B_MODES[bits.vlc(&B_VLC)?],
        };
        let intra = mode & INTRA != 0;
        if mode & QUANTIZER != 0 {
            *quantizer = self.coding.quantizer(bits)?;
        }

        if intra {
            if self.coding.concealment_vectors {
                self.read_motion(bits, &mut predictors.motion[0], 0)?;
                bits.marker()?;
            } else {
                predictors.motion = [[0; 2]; 2];
            }
        } else {
            predictors.dc = self.initial_dc();
            if mode & ZERO_MOTION != 0 {
                predictors.motion[0] = [0; 2];
            } else {
                for (direction, flag) in [FORWARD, BACKWARD].into_iter().enumerate() {
                    if mode & flag != 0 {
                        self.read_motion(bits, &mut predictors.motion[direction], direction)?;
                    }
                }
            }
            self.predict::<MPEG2>(frame, x, y, mode, predictors.motion)?;
        }
        predictors.last_mode = mode;

        let pattern = if intra {
            63
        } else if mode & PATTERN != 0 {
            let pattern = bits.vlc(&tables::PATTERN)?;
            if pattern == 0 {
                return Err(Error::Invalid("zero coded block pattern"));
            }
            pattern
        } else {
            0
        };
        let positions = [
            (0, x, y),
            (0, x + 8, y),
            (0, x, y + 8),
            (0, x + 8, y + 8),
            (1, x / 2, y / 2),
            (2, x / 2, y / 2),
        ];
        for (index, (plane, x, y)) in positions.into_iter().enumerate() {
            if pattern & (32 >> index) != 0 {
                let samples = block::decode::<MPEG2>(
                    bits,
                    self.coding,
                    self.sequence,
                    &mut predictors.dc,
                    plane,
                    *quantizer,
                    intra,
                )?;
                frame.planes[plane].block(x, y, &samples, intra);
            }
        }
        Ok(())
    }

    fn read_motion(
        &self,
        bits: &mut Bits<'_>,
        motion: &mut [i32; 2],
        direction: usize,
    ) -> Result<()> {
        for (predictor, code) in motion.iter_mut().zip(self.coding.f_code[direction]) {
            if !(1..=9).contains(&code) {
                return Err(Error::Invalid("motion vector f_code"));
            }
            let magnitude = bits.vlc(&tables::MOTION)? as i32;
            if magnitude != 0 {
                let negative = bits.flag()?;
                let residual = bits.read(code - 1)? as i32;
                let delta = ((magnitude - 1) << (code - 1)) + residual + 1;
                let limit = 16 << (code - 1);
                let vector = *predictor + if negative { -delta } else { delta };
                // The validated f_code makes the wrap interval a power of two.
                *predictor = ((vector + limit) & (2 * limit - 1)) - limit;
            }
        }
        Ok(())
    }

    fn predict<const MPEG2: bool>(
        &self,
        frame: &mut Frame,
        x: usize,
        y: usize,
        mode: u8,
        motion: [[i32; 2]; 2],
    ) -> Result<()> {
        let mut average = false;
        for (direction, flag) in [FORWARD, BACKWARD].into_iter().enumerate() {
            if mode & flag != 0 {
                let reference = self.references[direction].ok_or(Error::MissingReference)?;
                // MPEG-1 keeps the predictor in its coded units.
                let mut motion = motion[direction];
                if !MPEG2 && self.coding.full_pel[direction] {
                    motion = motion.map(|component| component * 2);
                }
                frame.predict(
                    x,
                    y,
                    Prediction {
                        frame: reference,
                        motion,
                        average,
                    },
                );
                average = true;
            }
        }
        Ok(())
    }
}

fn address_increment(bits: &mut Bits<'_>, limit: usize) -> Result<usize> {
    let mut increment = 0usize;
    loop {
        match bits.vlc(&tables::ADDRESS)? {
            symbol @ 0..=32 => return Ok(increment + symbol + 1),
            33 => {
                increment += 33;
                if increment >= limit {
                    return Err(Error::Invalid(
                        "macroblock address increment exceeds picture",
                    ));
                }
            }
            _ => {} // Macroblock stuffing.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coding(code: u8) -> Coding {
        Coding {
            f_code: [[code; 2]; 2],
            full_pel: [false; 2],
            dc_precision: 0,
            concealment_vectors: false,
            nonlinear_scale: false,
            intra_vlc: false,
            alternate_scan: false,
        }
    }

    #[test]
    fn motion_codes_wrap_at_each_supported_range() {
        let sequence = Sequence::parse(&[0x01, 0x00, 0x10, 0x13, 0, 0, 0x20, 0]).unwrap();
        for code in 1..=9 {
            let coding = coding(code);
            let slice = Slice {
                sequence: &sequence,
                coding: &coding,
                references: [None; 2],
            };
            let limit: i32 = 16 << (code - 1);
            for predictor in [-limit, -limit + 1, -1, 0, 1, limit - 2, limit - 1] {
                for (magnitude, prefix, width) in [(1, 1u32, 2), (16, 12, 10)] {
                    for negative in [false, true] {
                        for residual in [0, (1 << (code - 1)) - 1] {
                            // Encode one nonzero component, followed by the one-bit zero code.
                            let encoded = (prefix << (code + 1))
                                | (u32::from(negative) << code)
                                | ((residual as u32) << 1)
                                | 1;
                            let bytes = (encoded << (32 - width - code - 1)).to_be_bytes();
                            let delta = ((magnitude - 1) << (code - 1)) + residual + 1;
                            let vector = predictor + if negative { -delta } else { delta };
                            let expected = (vector + limit).rem_euclid(2 * limit) - limit;
                            for direction in 0..2 {
                                let mut motion = [predictor; 2];
                                slice
                                    .read_motion(&mut Bits::new(&bytes), &mut motion, direction)
                                    .unwrap();
                                assert_eq!(motion, [expected, predictor]);
                            }
                        }
                    }
                }
                let mut motion = [predictor; 2];
                slice
                    .read_motion(&mut Bits::new(&[0xc0]), &mut motion, 0)
                    .unwrap();
                assert_eq!(motion, [predictor; 2]);
            }
        }
    }

    #[test]
    fn invalid_and_truncated_motion_leave_the_predictor_unchanged() {
        let sequence = Sequence::parse(&[0x01, 0x00, 0x10, 0x13, 0, 0, 0x20, 0]).unwrap();
        for (code, bytes, expected) in [
            (0, [0xc0], Error::Invalid("motion vector f_code")),
            (10, [0xc0], Error::Invalid("motion vector f_code")),
            (9, [0x40], Error::Truncated),
        ] {
            let coding = coding(code);
            let slice = Slice {
                sequence: &sequence,
                coding: &coding,
                references: [None; 2],
            };
            let mut motion = [-1, 1];
            assert_eq!(
                slice.read_motion(&mut Bits::new(&bytes), &mut motion, 0),
                Err(expected)
            );
            assert_eq!(motion, [-1, 1]);
        }
    }
}
