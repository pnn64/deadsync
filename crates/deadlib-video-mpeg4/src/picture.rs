use alloc::vec::Vec;

use crate::{
    bitstream::Bits,
    block,
    headers::{Layer, Picture},
    tables, Error, Frame, PictureKind, Result,
};

pub(crate) type Vector = [i32; 2];

#[derive(Default)]
pub(crate) struct Grid<T> {
    width: usize,
    height: usize,
    cells: Vec<T>,
    shift: u8,
    packet: (usize, usize),
}

impl<T: Copy + Default> Grid<T> {
    pub(crate) fn reset(&mut self, columns: usize, rows: usize, shift: u8) {
        self.width = columns << shift;
        self.height = rows << shift;
        self.shift = shift;
        self.packet = (0, 0);
        self.cells.clear();
        self.cells.resize(self.width * self.height, T::default());
    }

    pub(crate) fn start_packet(&mut self, x: usize, y: usize) {
        self.packet = (y, x);
    }

    pub(crate) fn get(&self, x: i32, y: i32) -> T {
        let (x, y) = (x as usize, y as usize);
        if x < self.width && y < self.height && (y >> self.shift, x >> self.shift) >= self.packet {
            self.cells[y * self.width + x]
        } else {
            T::default()
        }
    }

    pub(crate) fn set(&mut self, x: usize, y: usize, value: T) {
        self.cells[y * self.width + x] = value;
    }

    pub(crate) fn left_diagonal_above(&self, x: usize, y: usize) -> [T; 3] {
        let (x, y) = (x as i32, y as i32);
        [
            self.get(x - 1, y),
            self.get(x - 1, y - 1),
            self.get(x, y - 1),
        ]
    }
}

#[derive(Default)]
pub(crate) struct Workspace {
    motion: Grid<Vector>,
    predictors: [Grid<block::Predictor>; 3],
}

struct Macroblock {
    intra: bool,
    four_vectors: bool,
    predict_ac: bool,
    separate_dc: bool,
    pattern: usize,
    skipped: bool,
}

impl Macroblock {
    fn parse(bits: &mut Bits<'_>, picture: &mut Picture) -> Result<Self> {
        let predicted = picture.kind == PictureKind::P;
        let code = loop {
            if predicted && bits.flag()? {
                return Ok(Self {
                    intra: false,
                    four_vectors: false,
                    predict_ac: false,
                    separate_dc: false,
                    pattern: 0,
                    skipped: true,
                });
            }
            let code = if predicted {
                bits.vlc(&tables::INTER_MB)?
            } else {
                bits.vlc(&tables::INTRA_MB)?
            };
            if code != if predicted { 20 } else { 8 } {
                break code;
            }
        };
        let intra = !predicted || code & 4 != 0;
        let predict_ac = intra && bits.flag()?;
        let mut luma = bits.vlc(&tables::LUMA_PATTERN)?;
        if !intra {
            luma ^= 15;
        }
        let separate_dc = picture.quantizer < picture.dc_threshold;
        let changes_quantizer = code & if predicted { 8 } else { 4 } != 0;
        if changes_quantizer {
            let delta = [-1, -2, 1, 2][bits.read(2)? as usize];
            picture.quantizer = (picture.quantizer + delta).clamp(1, 31);
        }
        Ok(Self {
            intra,
            four_vectors: predicted && code & 16 != 0,
            predict_ac,
            separate_dc,
            pattern: (luma << 2) | (code & 3),
            skipped: false,
        })
    }
}

pub(crate) fn decode(
    bits: &mut Bits<'_>,
    layer: Layer,
    mut picture: Picture,
    reference: Option<&Frame>,
    spare: Option<Frame>,
    workspace: &mut Workspace,
) -> Result<Frame> {
    if picture.kind == PictureKind::P && reference.is_none() {
        return Err(Error::MissingReference);
    }
    let columns = layer.width.div_ceil(16);
    let rows = layer.height.div_ceil(16);
    let mut frame = match spare {
        Some(frame) => frame.reuse(layer.width, layer.height, picture.kind),
        None => Frame::new(layer.width, layer.height, picture.kind),
    };
    let Workspace { motion, predictors } = workspace;
    motion.reset(columns, rows, 1);
    for (grid, shift) in predictors.iter_mut().zip([1, 0, 0]) {
        grid.reset(columns, rows, shift);
    }

    for y in 0..rows {
        for x in 0..columns {
            let address = y * columns + x;
            if layer.resync
                && address != 0
                && picture.packet(bits, layer, address, columns * rows)?
            {
                motion.start_packet(x, y);
                for grid in predictors.iter_mut() {
                    grid.start_packet(x, y);
                }
            }
            let mb = Macroblock::parse(bits, &mut picture)?;
            if !mb.intra {
                let reference = reference.ok_or(Error::MissingReference)?;
                let vectors = motion_vectors(bits, motion, [2 * x, 2 * y], &mb, picture.f_code)?;
                predict(
                    &mut frame,
                    reference,
                    [x, y],
                    vectors,
                    mb.four_vectors,
                    picture.truncate,
                );
            }
            for (i, (plane, bx, by)) in block_positions(x, y).into_iter().enumerate() {
                let coded = mb.pattern & (32 >> i) != 0;
                if mb.intra {
                    let grid = &mut predictors[plane];
                    let (samples, prediction) = block::intra::<false>(
                        bits,
                        coded,
                        mb.separate_dc,
                        mb.predict_ac,
                        picture.quantizer,
                        plane,
                        grid.left_diagonal_above(bx, by),
                    )?;
                    grid.set(bx, by, prediction);
                    frame.planes[plane].write::<true>([8 * bx, 8 * by], &samples);
                } else if coded {
                    let samples = block::inter::<false>(bits, picture.quantizer)?;
                    frame.planes[plane].write::<false>([8 * bx, 8 * by], &samples);
                }
            }
        }
    }
    Ok(frame)
}

pub(crate) fn block_positions(x: usize, y: usize) -> [(usize, usize, usize); 6] {
    [
        (0, 2 * x, 2 * y),
        (0, 2 * x + 1, 2 * y),
        (0, 2 * x, 2 * y + 1),
        (0, 2 * x + 1, 2 * y + 1),
        (1, x, y),
        (2, x, y),
    ]
}

fn motion_vectors(
    bits: &mut Bits<'_>,
    grid: &mut Grid<Vector>,
    [x, y]: [usize; 2],
    mb: &Macroblock,
    code: u8,
) -> Result<[Vector; 4]> {
    let mut out = [[0; 2]; 4];
    for (index, vector) in out.iter_mut().enumerate() {
        let [x, y] = [x + index % 2, y + index / 2];
        if !mb.skipped {
            *vector = if index == 0 || mb.four_vectors {
                let predicted = median_vector(grid, x as i32, y as i32, index);
                [
                    motion_component(bits, predicted[0], code)?,
                    motion_component(bits, predicted[1], code)?,
                ]
            } else {
                grid.get((x & !1) as i32, (y & !1) as i32)
            };
        }
        grid.set(x, y, *vector);
    }
    Ok(out)
}

fn median_vector(grid: &Grid<Vector>, x: i32, y: i32, index: usize) -> Vector {
    let left = grid.get(x - 1, y);
    let (row, column) = ((y >> 1) as usize, (x >> 1) as usize);
    let (start_row, start_column) = grid.packet;
    let top = grid.get(x, y - 1);
    let diagonal = grid.get(x + [2, 1, 1, -1][index], y - 1);
    // Until the top row is available, the left predictor takes precedence.
    if index < 2 && (row == start_row || (row == start_row + 1 && column < start_column)) {
        if column + 1 != start_column {
            return left;
        }
        if index == 0 && column == 0 {
            return diagonal;
        }
    }
    median(left, top, diagonal)
}

pub(crate) fn median(left: Vector, top: Vector, diagonal: Vector) -> Vector {
    core::array::from_fn(|i| left[i].clamp(top[i].min(diagonal[i]), top[i].max(diagonal[i])))
}

pub(crate) fn motion_delta(bits: &mut Bits<'_>, shift: u8) -> Result<i32> {
    let magnitude = bits.vlc(&tables::MOTION)? as i32;
    if magnitude == 0 {
        return Ok(0);
    }
    let negative = bits.flag()?;
    let delta = ((magnitude - 1) << shift) + bits.read(shift)? as i32 + 1;
    Ok(if negative { -delta } else { delta })
}

fn motion_component(bits: &mut Bits<'_>, predictor: i32, code: u8) -> Result<i32> {
    let shift = code - 1;
    let delta = motion_delta(bits, shift)?;
    if delta == 0 {
        return Ok(predictor);
    }
    let limit = 32 << shift;
    let vector = predictor + delta;
    Ok((vector + limit).rem_euclid(2 * limit) - limit)
}

pub(crate) fn predict(
    frame: &mut Frame,
    reference: &Frame,
    [x, y]: [usize; 2],
    vectors: [Vector; 4],
    four: bool,
    truncate: bool,
) {
    let chroma = if four {
        for (index, vector) in vectors.into_iter().enumerate() {
            let origin = [16 * x + 8 * (index % 2), 16 * y + 8 * (index / 2)];
            frame.planes[0].predict::<8>(&reference.planes[0], origin, vector, truncate);
        }
        core::array::from_fn(|axis| {
            let value: i32 = vectors.iter().map(|v| v[axis]).sum();
            (value >> 3) + i32::from(matches!(value & 15, 3..=7 | 14..=15))
        })
    } else {
        frame.planes[0].predict::<16>(&reference.planes[0], [16 * x, 16 * y], vectors[0], truncate);
        vectors[0].map(|component| (component >> 1) | (component & 1))
    };
    for (plane, previous) in frame.planes[1..].iter_mut().zip(&reference.planes[1..]) {
        plane.predict::<8>(previous, [8 * x, 8 * y], chroma, truncate);
    }
}
