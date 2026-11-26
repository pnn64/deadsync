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
    pub(crate) fn decode(
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
            self.macroblock(
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
                    self.predict(
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

    fn macroblock(
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
            self.predict(frame, x, y, mode, predictors.motion)?;
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
                let samples = block::decode(
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
                *predictor = (vector + limit).rem_euclid(2 * limit) - limit;
            }
        }
        Ok(())
    }

    fn predict(
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
                frame.predict(
                    x,
                    y,
                    Prediction {
                        frame: reference,
                        motion: motion[direction],
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
