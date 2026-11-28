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
    next_motion: Vec<Colocated>,
    backward_prediction: Option<Frame>,
}

#[derive(Clone, Copy, Default)]
struct Colocated {
    vectors: [Vector; 4],
    four_vectors: bool,
    skipped: bool,
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
    let Workspace {
        motion,
        predictors,
        next_motion,
        ..
    } = workspace;
    motion.reset(columns, rows, 1);
    next_motion.clear();
    if !layer.low_delay {
        next_motion.resize(columns * rows, Colocated::default());
    }
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
                if let Some(next) = next_motion.get_mut(address) {
                    *next = Colocated {
                        vectors,
                        four_vectors: mb.four_vectors,
                        skipped: mb.skipped,
                    };
                }
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

pub(crate) fn decode_bidirectional(
    bits: &mut Bits<'_>,
    layer: Layer,
    mut picture: Picture,
    references: [&Frame; 2],
    span: [i64; 2],
    spare: Option<Frame>,
    workspace: &mut Workspace,
) -> Result<Frame> {
    let columns = layer.width.div_ceil(16);
    let rows = layer.height.div_ceil(16);
    if workspace.next_motion.len() != columns * rows {
        return Err(Error::MissingReference);
    }
    let frame_for = |spare: Option<Frame>| match spare {
        Some(frame) => frame.reuse(layer.width, layer.height, PictureKind::B),
        None => Frame::new(layer.width, layer.height, PictureKind::B),
    };
    let mut frame = frame_for(spare);
    let mut backward = frame_for(workspace.backward_prediction.take());
    for y in 0..rows {
        let mut predictors = [[0; 2]; 2];
        for x in 0..columns {
            let address = y * columns + x;
            if layer.resync
                && address != 0
                && picture.packet(bits, layer, address, columns * rows)?
            {
                predictors = [[0; 2]; 2];
            }
            let position = [x, y];
            let next = workspace.next_motion[address];
            if next.skipped {
                predict(
                    &mut frame,
                    references[0],
                    position,
                    [[0; 2]; 4],
                    false,
                    false,
                );
                continue;
            }
            let direct_skip = bits.flag()?;
            let (mode, pattern) = if direct_skip {
                (0, 0)
            } else {
                let no_pattern = bits.flag()?;
                let mut mode = 0;
                while !bits.flag()? {
                    mode += 1;
                    if mode == 4 {
                        return Err(Error::Invalid("bidirectional macroblock mode"));
                    }
                }
                let pattern = if no_pattern { 0 } else { bits.read(6)? };
                if mode != 0 && pattern != 0 && bits.flag()? {
                    let delta = if bits.flag()? { 2 } else { -2 };
                    picture.quantizer = (picture.quantizer + delta).clamp(1, 31);
                }
                (mode, pattern)
            };
            let mut vectors = [[[0; 2]; 4]; 2];
            if mode == 0 {
                let mut delta = [0; 2];
                if !direct_skip {
                    delta = [motion_delta(bits, 0)?, motion_delta(bits, 0)?];
                }
                let [offset, distance] = span;
                for (i, vector) in next.vectors.into_iter().enumerate() {
                    for axis in 0..2 {
                        let motion = i64::from(vector[axis]);
                        vectors[0][i][axis] = (motion * offset / distance) as i32 + delta[axis];
                        vectors[1][i][axis] = if delta[axis] == 0 {
                            (motion * (offset - distance) / distance) as i32
                        } else {
                            vectors[0][i][axis] - vector[axis]
                        };
                    }
                }
            } else {
                for (direction, code) in [picture.f_code, picture.b_code].into_iter().enumerate() {
                    if mode == 1 || mode == 3 - direction {
                        let predictor = &mut predictors[direction];
                        *predictor = [
                            motion_component(bits, predictor[0], code)?,
                            motion_component(bits, predictor[1], code)?,
                        ];
                        vectors[direction] = [*predictor; 4];
                    }
                }
            }
            let four = mode == 0 && next.four_vectors;
            if mode == 2 {
                predict(&mut frame, references[1], position, vectors[1], four, false);
            } else {
                predict(&mut frame, references[0], position, vectors[0], four, false);
                if mode != 3 {
                    predict(
                        &mut backward,
                        references[1],
                        position,
                        vectors[1],
                        four,
                        false,
                    );
                    frame.planes[0].average::<16>(&backward.planes[0], [16 * x, 16 * y]);
                    for plane in 1..3 {
                        frame.planes[plane].average::<8>(&backward.planes[plane], [8 * x, 8 * y]);
                    }
                }
            }
            for (i, (plane, bx, by)) in block_positions(x, y).into_iter().enumerate() {
                if pattern & (32 >> i) != 0 {
                    let samples = block::inter::<false>(bits, picture.quantizer)?;
                    frame.planes[plane].write::<false>([8 * bx, 8 * by], &samples);
                }
            }
        }
    }
    workspace.backward_prediction = Some(backward);
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

const MOTION_BITS: u8 = 10;
const MOTION_SIZE: usize = 1 << MOTION_BITS;

// Include the sign and residual of common motion codes in one lookup.
static SHORT_MOTION: [[u16; MOTION_SIZE]; 7] = {
    let mut table = [[0; MOTION_SIZE]; 7];
    let mut shift = 0;
    while shift < table.len() {
        let mut prefix = 0;
        while prefix < MOTION_SIZE {
            let entry = tables::MOTION[prefix << (12 - MOTION_BITS)];
            let length = (entry & 31) as usize;
            let magnitude = (entry >> 5) as i32;
            if length != 0 {
                if magnitude == 0 {
                    table[shift][prefix] = length as u16;
                } else {
                    let total = length + 1 + shift;
                    if total <= MOTION_BITS as usize {
                        let suffix = prefix >> (MOTION_BITS as usize - total);
                        let residual = suffix & ((1 << shift) - 1);
                        let delta = ((magnitude - 1) << shift) + residual as i32 + 1;
                        let signed = if suffix & (1 << shift) == 0 {
                            delta
                        } else {
                            -delta
                        };
                        table[shift][prefix] = ((signed as u16) << 4) | total as u16;
                    }
                }
            }
            prefix += 1;
        }
        shift += 1;
    }
    table
};

pub(crate) fn motion_delta(bits: &mut Bits<'_>, shift: u8) -> Result<i32> {
    let entry = SHORT_MOTION[shift as usize][bits.peek(MOTION_BITS) as usize];
    if entry != 0 {
        bits.skip((entry & 15) as u8)?;
        return Ok(i32::from((entry as i16) >> 4));
    }
    motion_delta_long(bits, shift)
}

#[cold]
fn motion_delta_long(bits: &mut Bits<'_>, shift: u8) -> Result<i32> {
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
    // The wrap interval is a power of two for every legal f_code.
    Ok(((vector + limit) & (2 * limit - 1)) - limit)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_motion_codes_match_the_long_decoder() {
        for shift in 0..=6 {
            for prefix in 0..=u16::MAX {
                let [a, b] = prefix.to_be_bytes();
                let bytes = [a, b, 0xa5];
                for len in 1..=3 {
                    let mut expected = Bits::new(&bytes[..len]);
                    let mut actual = expected;
                    let result = motion_delta_long(&mut expected, shift);
                    assert_eq!(motion_delta(&mut actual, shift), result);
                    if result.is_ok() {
                        assert_eq!(actual.peek(24), expected.peek(24));
                        assert_eq!(actual.alignment_stuffing(), expected.alignment_stuffing());
                    }
                }
            }
        }
    }

    #[test]
    fn motion_components_wrap_both_boundaries_for_every_code() {
        for code in 1..=7 {
            let limit = 32 << (code - 1);
            // Magnitude one with positive/negative sign and zero residual.
            let mut positive = Bits::new(&[0b0100_0000, 0]);
            let mut negative = Bits::new(&[0b0110_0000, 0]);
            assert_eq!(motion_component(&mut positive, limit - 1, code), Ok(-limit));
            assert_eq!(motion_component(&mut negative, -limit, code), Ok(limit - 1));
            let mut zero = Bits::new(&[0x80]);
            assert_eq!(motion_component(&mut zero, -limit, code), Ok(-limit));
        }
    }
}
