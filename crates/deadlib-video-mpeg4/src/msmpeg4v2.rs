use alloc::sync::Arc;

use crate::{
    bitstream::{vlc_table, Bits},
    block,
    picture::{self, Grid, Vector},
    tables, Error, Frame, PictureKind, Result,
};

static INTRA_CHROMA_PATTERN: [u16; 8] = vlc_table(&[(1, 1), (0, 3), (1, 3), (1, 2)]);
static PREDICTED_MB: [u16; 128] = vlc_table(&[
    (1, 1),
    (0, 2),
    (3, 3),
    (9, 5),
    (5, 4),
    (33, 7),
    (32, 7),
    (17, 6),
]);

/// A serial decoder for Microsoft MPEG-4 v2 (MP42) picture packets.
pub struct MsMpeg4v2Decoder {
    width: usize,
    height: usize,
    slice_rows: usize,
    reference: Option<Arc<Frame>>,
    spare: Option<Frame>,
    motion: Grid<Vector>,
    predictors: [Grid<block::Predictor>; 3],
}

impl MsMpeg4v2Decoder {
    /// Creates a decoder with the dimensions signaled by the container.
    pub fn new(width: usize, height: usize) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(Error::Invalid("MP42 picture dimensions"));
        }
        if width > 4096 || height > 4096 {
            return Err(Error::Unsupported("MP42 picture dimensions"));
        }
        Ok(Self {
            width,
            height,
            slice_rows: height.div_ceil(16),
            reference: None,
            spare: None,
            motion: Grid::default(),
            predictors: Default::default(),
        })
    }

    /// Decodes a complete MP42 picture packet and retains it for P pictures.
    pub fn push_packet(&mut self, packet: &[u8]) -> Result<Arc<Frame>> {
        let columns = self.width.div_ceil(16);
        let rows = self.height.div_ceil(16);
        if packet.len() < (columns * rows).div_ceil(8) {
            return Err(Error::Truncated);
        }
        let mut bits = Bits::new(packet);
        let header = Header::parse(&mut bits, rows, self.slice_rows)?;
        let reference = match header.kind {
            PictureKind::I => None,
            PictureKind::P => Some(self.reference.as_deref().ok_or(Error::MissingReference)?),
        };
        let mut frame = match self.spare.take() {
            Some(frame) => frame.reuse(self.width, self.height, header.kind),
            None => Frame::new(self.width, self.height, header.kind),
        };
        self.motion.reset(columns, rows, 0);
        for (grid, shift) in self.predictors.iter_mut().zip([1, 0, 0]) {
            grid.reset(columns, rows, shift);
        }

        for y in 0..rows {
            let first_slice_row = y % header.slice_rows == 0;
            if first_slice_row {
                for grid in &mut self.predictors {
                    grid.start_packet(0, y);
                }
            }
            for x in 0..columns {
                let mb = Macroblock::parse(&mut bits, &header)?;
                if !mb.intra {
                    let reference = reference.ok_or(Error::MissingReference)?;
                    let vector = if mb.skipped {
                        [0; 2]
                    } else {
                        let predicted = predict_motion(&self.motion, x, y, first_slice_row);
                        [
                            motion_component(&mut bits, predicted[0])?,
                            motion_component(&mut bits, predicted[1])?,
                        ]
                    };
                    self.motion.set(x, y, vector);
                    picture::predict(&mut frame, reference, [x, y], [vector; 4], false, false);
                }
                for (i, (plane, bx, by)) in picture::block_positions(x, y).into_iter().enumerate() {
                    let coded = mb.pattern & (32 >> i) != 0;
                    if mb.intra {
                        let grid = &mut self.predictors[plane];
                        let (samples, prediction) = block::intra::<true>(
                            &mut bits,
                            coded,
                            true,
                            mb.predict_ac,
                            header.quantizer,
                            plane,
                            grid.left_diagonal_above(bx, by),
                        )?;
                        grid.set(bx, by, prediction);
                        frame.planes[plane].write::<true>([8 * bx, 8 * by], &samples);
                    } else if coded {
                        let samples = block::inter::<true>(&mut bits, header.quantizer)?;
                        frame.planes[plane].write::<false>([8 * bx, 8 * by], &samples);
                    }
                }
            }
        }

        let frame = Arc::new(frame);
        let retired = self.reference.replace(Arc::clone(&frame));
        self.spare = retired.and_then(|frame| Arc::try_unwrap(frame).ok());
        self.slice_rows = header.slice_rows;
        Ok(frame)
    }
}

struct Header {
    kind: PictureKind,
    quantizer: i32,
    slice_rows: usize,
    use_skip: bool,
}

impl Header {
    fn parse(bits: &mut Bits<'_>, rows: usize, previous_slice_rows: usize) -> Result<Self> {
        let kind = match bits.read(2)? {
            0 => PictureKind::I,
            1 => PictureKind::P,
            _ => return Err(Error::Invalid("MP42 picture type")),
        };
        let quantizer = bits.read(5)? as i32;
        if quantizer == 0 {
            return Err(Error::Invalid("MP42 quantizer"));
        }
        let (slice_rows, use_skip) = if kind == PictureKind::I {
            let code = bits.read(5)? as usize;
            if !(0x17..=0x16 + rows).contains(&code) {
                return Err(Error::Invalid("MP42 slice count"));
            }
            (rows / (code - 0x16), false)
        } else {
            (previous_slice_rows, bits.flag()?)
        };
        Ok(Self {
            kind,
            quantizer,
            slice_rows,
            use_skip,
        })
    }
}

struct Macroblock {
    intra: bool,
    predict_ac: bool,
    pattern: usize,
    skipped: bool,
}

impl Macroblock {
    fn parse(bits: &mut Bits<'_>, header: &Header) -> Result<Self> {
        if header.use_skip && bits.flag()? {
            return Ok(Self {
                intra: false,
                predict_ac: false,
                pattern: 0,
                skipped: true,
            });
        }
        let (intra, chroma) = if header.kind == PictureKind::I {
            (true, bits.vlc(&INTRA_CHROMA_PATTERN)?)
        } else {
            let code = bits.vlc(&PREDICTED_MB)?;
            (code >= 4, code & 3)
        };
        let predict_ac = intra && bits.flag()?;
        let mut luma = bits.vlc(&tables::LUMA_PATTERN)?;
        if !intra && chroma != 3 {
            luma ^= 15;
        }
        Ok(Self {
            intra,
            predict_ac,
            pattern: (luma << 2) | chroma,
            skipped: false,
        })
    }
}

fn predict_motion(grid: &Grid<Vector>, x: usize, y: usize, first_slice_row: bool) -> Vector {
    let (x, y) = (x as i32, y as i32);
    let left = grid.get(x - 1, y);
    if first_slice_row {
        left
    } else {
        picture::median(left, grid.get(x, y - 1), grid.get(x + 1, y - 1))
    }
}

fn motion_component(bits: &mut Bits<'_>, predictor: i32) -> Result<i32> {
    let value = predictor + picture::motion_delta(bits, 0)?;
    Ok(match value {
        ..=-64 => value + 64,
        64.. => value - 64,
        _ => value,
    })
}
