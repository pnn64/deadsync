use crate::bitstream::Bits;
use crate::{Error, PictureKind, Result};

pub(crate) const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

pub(crate) const ALTERNATE: [usize; 64] = [
    0, 8, 16, 24, 1, 9, 2, 10, 17, 25, 32, 40, 48, 56, 57, 49, 41, 33, 26, 18, 3, 11, 4, 12, 19,
    27, 34, 42, 50, 58, 35, 43, 51, 59, 20, 28, 5, 13, 6, 14, 21, 29, 36, 44, 52, 60, 37, 45, 53,
    61, 22, 30, 7, 15, 23, 31, 38, 46, 54, 62, 39, 47, 55, 63,
];

const INTRA: [u8; 64] = [
    8, 16, 19, 22, 26, 27, 29, 34, 16, 16, 22, 24, 27, 29, 34, 37, 19, 22, 26, 27, 29, 34, 34, 38,
    22, 22, 26, 27, 29, 34, 37, 40, 22, 26, 27, 29, 32, 35, 40, 48, 26, 27, 29, 32, 35, 40, 48, 58,
    26, 27, 29, 34, 38, 46, 56, 69, 27, 29, 35, 38, 46, 56, 69, 83,
];

pub(crate) struct Sequence {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) frame_rate: (u32, u32),
    pub(crate) matrices: [[u8; 64]; 4],
    pub(crate) progressive: bool,
    pub(crate) extended: bool,
    aspect: u32,
}

impl Sequence {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        let mut bits = Bits::new(bytes);
        let width = bits.read(12)? as usize;
        let height = bits.read(12)? as usize;
        let aspect = bits.read(4)?;
        if !(1..=14).contains(&aspect) {
            return Err(Error::Invalid("aspect ratio code"));
        }
        let frame_rate = match bits.read(4)? {
            1 => (24_000, 1001),
            2 => (24, 1),
            3 => (25, 1),
            4 => (30_000, 1001),
            5 => (30, 1),
            6 => (50, 1),
            7 => (60_000, 1001),
            8 => (60, 1),
            _ => return Err(Error::Invalid("frame rate code")),
        };
        bits.skip(18)?;
        bits.marker()?;
        bits.skip(11)?;
        let intra = read_matrix(&mut bits)?.unwrap_or(INTRA);
        let inter = read_matrix(&mut bits)?.unwrap_or([16; 64]);
        Ok(Self {
            width,
            height,
            frame_rate,
            matrices: [intra, inter, intra, inter],
            progressive: true,
            extended: false,
            aspect,
        })
    }

    pub(crate) fn extend(&mut self, bits: &mut Bits<'_>) -> Result<()> {
        if self.extended {
            return Err(Error::Invalid("duplicate sequence extension"));
        }
        if self.aspect > 4 {
            return Err(Error::Invalid("MPEG-2 aspect ratio code"));
        }
        bits.skip(8)?; // Profile and level.
        self.progressive = bits.flag()?;
        if bits.read(2)? != 1 {
            return Err(Error::Unsupported("chroma format other than 4:2:0"));
        }
        self.width |= (bits.read(2)? as usize) << 12;
        self.height |= (bits.read(2)? as usize) << 12;
        self.check_dimensions()?;
        bits.skip(12)?;
        bits.marker()?;
        bits.skip(9)?; // VBV extension and low-delay flag.
        self.frame_rate.0 *= bits.read(2)? + 1;
        self.frame_rate.1 *= bits.read(5)? + 1;
        self.extended = true;
        Ok(())
    }

    pub(crate) fn check_dimensions(&self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(Error::Invalid("zero picture dimension"));
        }
        if self.width > 4096 || self.height > 2800 {
            return Err(Error::Unsupported("picture dimensions above 4096 × 2800"));
        }
        Ok(())
    }

    pub(crate) fn macroblock_rows(&self) -> usize {
        if self.progressive {
            self.height.div_ceil(16)
        } else {
            2 * self.height.div_ceil(32)
        }
    }

    pub(crate) fn update_matrices(&mut self, bits: &mut Bits<'_>) -> Result<()> {
        let [intra, inter, chroma_intra, chroma_inter] = [
            read_matrix(bits)?,
            read_matrix(bits)?,
            read_matrix(bits)?,
            read_matrix(bits)?,
        ];
        if let Some(matrix) = intra {
            self.matrices[0] = matrix;
            self.matrices[2] = matrix;
        }
        if let Some(matrix) = inter {
            self.matrices[1] = matrix;
            self.matrices[3] = matrix;
        }
        if let Some(matrix) = chroma_intra {
            self.matrices[2] = matrix;
        }
        if let Some(matrix) = chroma_inter {
            self.matrices[3] = matrix;
        }
        Ok(())
    }
}

fn read_matrix(bits: &mut Bits<'_>) -> Result<Option<[u8; 64]>> {
    if !bits.flag()? {
        return Ok(None);
    }
    let mut matrix = [0; 64];
    for position in ZIGZAG {
        let value = bits.read(8)? as u8;
        if value == 0 {
            return Err(Error::Invalid("zero quantization matrix weight"));
        }
        matrix[position] = value;
    }
    Ok(Some(matrix))
}

pub(crate) struct PictureHeader {
    pub(crate) temporal_reference: u16,
    pub(crate) kind: PictureKind,
}

impl PictureHeader {
    pub(crate) fn parse(bytes: &[u8], mpeg2: bool) -> Result<(Self, Option<Coding>)> {
        let mut bits = Bits::new(bytes);
        let temporal_reference = bits.read(10)? as u16;
        let kind = match bits.read(3)? {
            1 => PictureKind::I,
            2 => PictureKind::P,
            3 => PictureKind::B,
            _ => return Err(Error::Invalid("picture coding type")),
        };
        bits.skip(16)?;
        let directions = match kind {
            PictureKind::I => 0,
            PictureKind::P => 1,
            PictureKind::B => 2,
        };
        let coding = if mpeg2 {
            bits.skip(4 * directions)?;
            None
        } else {
            Some(Coding::mpeg1(&mut bits, usize::from(directions))?)
        };
        while bits.flag()? {
            bits.skip(8)?;
        }
        let picture = Self {
            temporal_reference,
            kind,
        };
        Ok((picture, coding))
    }
}

pub(crate) struct Coding {
    pub(crate) f_code: [[u8; 2]; 2],
    pub(crate) full_pel: [bool; 2],
    pub(crate) dc_precision: u8,
    pub(crate) concealment_vectors: bool,
    pub(crate) nonlinear_scale: bool,
    pub(crate) intra_vlc: bool,
    pub(crate) alternate_scan: bool,
}

impl Coding {
    fn mpeg1(bits: &mut Bits<'_>, directions: usize) -> Result<Self> {
        let mut f_code = [[0; 2]; 2];
        let mut full_pel = [false; 2];
        for direction in 0..directions {
            full_pel[direction] = bits.flag()?;
            let code = bits.read(3)? as u8;
            if code == 0 {
                return Err(Error::Invalid("motion vector f_code"));
            }
            f_code[direction] = [code; 2];
        }
        Ok(Self {
            f_code,
            full_pel,
            dc_precision: 0,
            concealment_vectors: false,
            nonlinear_scale: false,
            intra_vlc: false,
            alternate_scan: false,
        })
    }

    pub(crate) fn parse(bits: &mut Bits<'_>) -> Result<Self> {
        let f_code = [
            [bits.read(4)? as u8, bits.read(4)? as u8],
            [bits.read(4)? as u8, bits.read(4)? as u8],
        ];
        let dc_precision = bits.read(2)? as u8;
        if bits.read(2)? != 3 {
            return Err(Error::Unsupported("field pictures"));
        }
        bits.skip(1)?; // Top field first.
        if !bits.flag()? {
            return Err(Error::Unsupported("field motion or field DCT"));
        }
        let coding = Self {
            f_code,
            full_pel: [false; 2],
            dc_precision,
            concealment_vectors: bits.flag()?,
            nonlinear_scale: bits.flag()?,
            intra_vlc: bits.flag()?,
            alternate_scan: bits.flag()?,
        };
        bits.skip(2)?; // Repeat first field and chroma 4:2:0 type.
        if !bits.flag()? {
            return Err(Error::Unsupported("interlaced pictures"));
        }
        if bits.flag()? {
            bits.skip(20)?; // Composite display metadata.
        }
        Ok(coding)
    }

    pub(crate) fn quantizer(&self, bits: &mut Bits<'_>) -> Result<i32> {
        const NONLINEAR: [i32; 32] = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 18, 20, 22, 24, 28, 32, 36, 40, 44, 48, 52,
            56, 64, 72, 80, 88, 96, 104, 112,
        ];
        let code = bits.read(5)? as usize;
        if code == 0 {
            return Err(Error::Invalid("zero quantizer scale"));
        }
        Ok(if self.nonlinear_scale {
            NONLINEAR[code]
        } else {
            2 * code as i32
        })
    }
}
