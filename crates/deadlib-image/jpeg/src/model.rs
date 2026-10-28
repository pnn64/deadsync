use crate::{Error, Result};

/// SOF process identifiers, including all JPEG-1 differential processes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Process {
    Baseline = 0xc0,
    ExtendedSequential = 0xc1,
    Progressive = 0xc2,
    Lossless = 0xc3,
    DifferentialSequential = 0xc5,
    DifferentialProgressive = 0xc6,
    DifferentialLossless = 0xc7,
    ArithmeticSequential = 0xc9,
    ArithmeticProgressive = 0xca,
    ArithmeticLossless = 0xcb,
    ArithmeticDifferentialSequential = 0xcd,
    ArithmeticDifferentialProgressive = 0xce,
    ArithmeticDifferentialLossless = 0xcf,
}

impl TryFrom<u8> for Process {
    type Error = Error;
    fn try_from(v: u8) -> Result<Self> {
        Ok(match v {
            0xc0 => Self::Baseline,
            0xc1 => Self::ExtendedSequential,
            0xc2 => Self::Progressive,
            0xc3 => Self::Lossless,
            0xc5 => Self::DifferentialSequential,
            0xc6 => Self::DifferentialProgressive,
            0xc7 => Self::DifferentialLossless,
            0xc9 => Self::ArithmeticSequential,
            0xca => Self::ArithmeticProgressive,
            0xcb => Self::ArithmeticLossless,
            0xcd => Self::ArithmeticDifferentialSequential,
            0xce => Self::ArithmeticDifferentialProgressive,
            0xcf => Self::ArithmeticDifferentialLossless,
            _ => return Err(Error::Invalid("SOF process marker")),
        })
    }
}

impl Process {
    pub const fn is_lossless(self) -> bool {
        matches!(self as u8 & 3, 3)
    }
    pub const fn is_progressive(self) -> bool {
        self as u8 & 3 == 2
    }
    pub const fn is_arithmetic(self) -> bool {
        self as u8 & 8 != 0
    }
    pub const fn is_differential(self) -> bool {
        self as u8 & 4 != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentSpec {
    pub id: u8,
    pub horizontal_sampling: u8,
    pub vertical_sampling: u8,
    pub quantization_table: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameHeader {
    pub process: Process,
    pub precision: u8,
    pub width: u16,
    /// Zero means that a DNL marker must provide the height.
    pub height: u16,
    pub components: Vec<ComponentSpec>,
}

impl FrameHeader {
    pub fn parse(marker: u8, data: &[u8]) -> Result<Self> {
        if data.len() < 6 {
            return Err(Error::Invalid("short SOF"));
        }
        let process = Process::try_from(marker)?;
        let precision = data[0];
        let height = u16::from_be_bytes([data[1], data[2]]);
        let width = u16::from_be_bytes([data[3], data[4]]);
        let count = data[5] as usize;
        if count == 0
            || data.len() != 6 + 3 * count
            || width == 0
            || (process.is_progressive() && count > 4)
        {
            return Err(Error::Invalid("SOF dimensions or component count"));
        }
        if !(if process == Process::Baseline {
            precision == 8
        } else if process.is_lossless() {
            (2..=16).contains(&precision)
        } else {
            precision == 8 || precision == 12
        }) {
            return Err(Error::Invalid("sample precision for process"));
        }
        let mut components = Vec::new();
        components
            .try_reserve_exact(count)
            .map_err(|_| Error::Allocation)?;
        for c in data[6..].chunks_exact(3) {
            let horizontal_sampling = c[1] >> 4;
            let vertical_sampling = c[1] & 15;
            if !(1..=4).contains(&horizontal_sampling)
                || !(1..=4).contains(&vertical_sampling)
                || c[2] > 3
                || (process.is_lossless() && c[2] != 0)
                || components.iter().any(|v: &ComponentSpec| v.id == c[0])
            {
                return Err(Error::Invalid("SOF component parameters"));
            }
            components.push(ComponentSpec {
                id: c[0],
                horizontal_sampling,
                vertical_sampling,
                quantization_table: c[2],
            });
        }
        Ok(Self {
            process,
            precision,
            width,
            height,
            components,
        })
    }
}

/// APPn or COM payload. The bytes are borrowed unchanged from the source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataSegment<'a> {
    pub marker: u8,
    pub data: &'a [u8],
    pub offset: usize,
    pub preceding_scans: usize,
}

/// Quantization values and coefficients use natural row-major frequency order.
pub const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];
