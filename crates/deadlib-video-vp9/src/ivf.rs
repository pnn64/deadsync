//! IVF container parser.
//!
//! IVF is a very simple raw-stream container commonly used for libvpx (VP8/VP9)
//! testing and distribution. The VP9 spec itself does not define a container
//! format, so this parser is implemented based on the publicly documented IVF
//! format spec (a 32-byte file header + a 12-byte header per frame). All
//! multi-byte integer values are little-endian.
//!
//! File header layout (32 bytes):
//!
//! | Offset | Size | Contents |
//! | --- | --- | --- |
//! | 0  | 4 | Signature `"DKIF"` |
//! | 4  | 2 | Version (should be 0) |
//! | 6  | 2 | Header length (bytes, usually 32) |
//! | 8  | 4 | Codec FourCC (`"VP90"` for VP9) |
//! | 12 | 2 | Width (pixels) |
//! | 14 | 2 | Height (pixels) |
//! | 16 | 4 | Timebase denominator |
//! | 20 | 4 | Timebase numerator |
//! | 24 | 4 | Frame count |
//! | 28 | 4 | Unused |
//!
//! Frame header layout (12 bytes, followed by frame data):
//!
//! | Offset | Size | Contents |
//! | --- | --- | --- |
//! | 0 | 4 | Size of the frame data (excluding this header) |
//! | 4 | 8 | 64-bit presentation timestamp |

use core::fmt;

/// Size of the IVF file header (bytes).
const IVF_FILE_HEADER_SIZE: usize = 32;
/// Size of the IVF frame header (bytes).
const IVF_FRAME_HEADER_SIZE: usize = 12;
/// The signature at the start of an IVF file.
const IVF_SIGNATURE: &[u8; 4] = b"DKIF";

/// Errors that can occur while parsing IVF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IvfError {
    /// The buffer is shorter than the file header.
    TooShortForFileHeader,
    /// The first 4 bytes are not `"DKIF"`.
    BadSignature,
    /// An invalid value, e.g. the header_length in the header is inconsistent with the actual buffer size.
    InvalidHeaderLength,
    /// Not enough bytes remain to read a frame header.
    TruncatedFrameHeader,
    /// The buffer does not contain as many bytes as the frame header claims for the data size.
    TruncatedFrameData,
}

impl fmt::Display for IvfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IvfError::TooShortForFileHeader => {
                f.write_str("IVF buffer shorter than 32-byte header")
            }
            IvfError::BadSignature => f.write_str("invalid IVF signature (expected \"DKIF\")"),
            IvfError::InvalidHeaderLength => f.write_str("invalid IVF header length"),
            IvfError::TruncatedFrameHeader => f.write_str("truncated IVF frame header"),
            IvfError::TruncatedFrameData => f.write_str("truncated IVF frame payload"),
        }
    }
}

impl core::error::Error for IvfError {}

/// Contents of the IVF file header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IvfHeader {
    /// Format version (usually 0).
    pub version: u16,
    /// Header length (bytes). Usually 32.
    pub header_length: u16,
    /// Codec FourCC (e.g. `[b'V', b'P', b'9', b'0']`).
    pub fourcc: [u8; 4],
    /// Frame width (pixels).
    pub width: u16,
    /// Frame height (pixels).
    pub height: u16,
    /// Timebase denominator.
    pub timebase_denominator: u32,
    /// Timebase numerator.
    pub timebase_numerator: u32,
    /// Number of frames contained in the file (self-reported by the encoder; may not match the actual count).
    pub frame_count: u32,
}

/// IVF data for a single frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IvfFrame<'a> {
    /// 64-bit presentation timestamp (in timebase units).
    pub timestamp: u64,
    /// Raw frame data (for VP9, the byte sequence of a frame or superframe).
    pub data: &'a [u8],
}

/// A reader that splits an IVF file into frames in order.
///
/// Designed around loading the entire buffer into memory up front and
/// borrowing a slice of it. Implements `Iterator`, returning the next frame
/// on each call to `next()`.
#[derive(Debug, Clone)]
pub struct IvfReader<'a> {
    header: IvfHeader,
    /// The remaining unread byte sequence (repeated frame header + data).
    remaining: &'a [u8],
}

impl<'a> IvfReader<'a> {
    /// Reads the IVF file header from the start of the buffer and constructs an `IvfReader`.
    pub fn new(buf: &'a [u8]) -> Result<Self, IvfError> {
        if buf.len() < IVF_FILE_HEADER_SIZE {
            return Err(IvfError::TooShortForFileHeader);
        }
        if &buf[0..4] != IVF_SIGNATURE {
            return Err(IvfError::BadSignature);
        }
        let version = read_u16_le(buf, 4);
        let header_length = read_u16_le(buf, 6);
        let fourcc = [buf[8], buf[9], buf[10], buf[11]];
        let width = read_u16_le(buf, 12);
        let height = read_u16_le(buf, 14);
        let timebase_denominator = read_u32_le(buf, 16);
        let timebase_numerator = read_u32_le(buf, 20);
        let frame_count = read_u32_le(buf, 24);

        // header_length indicates the actual size of the file header. If it is
        // less than 32 bytes, the start position of the following data would
        // be indeterminate, so treat it as an invalid value.
        let header_length_usize = header_length as usize;
        if header_length_usize < IVF_FILE_HEADER_SIZE || header_length_usize > buf.len() {
            return Err(IvfError::InvalidHeaderLength);
        }

        Ok(Self {
            header: IvfHeader {
                version,
                header_length,
                fourcc,
                width,
                height,
                timebase_denominator,
                timebase_numerator,
                frame_count,
            },
            remaining: &buf[header_length_usize..],
        })
    }

    /// Returns the parsed IVF file header.
    pub fn header(&self) -> &IvfHeader {
        &self.header
    }
}

impl<'a> Iterator for IvfReader<'a> {
    type Item = Result<IvfFrame<'a>, IvfError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        if self.remaining.len() < IVF_FRAME_HEADER_SIZE {
            // What remains is a partial chunk too short to read another frame header from.
            self.remaining = &[];
            return Some(Err(IvfError::TruncatedFrameHeader));
        }

        let frame_size = read_u32_le(self.remaining, 0) as usize;
        let timestamp = read_u64_le(self.remaining, 4);

        let data_start = IVF_FRAME_HEADER_SIZE;
        let Some(data_end) = data_start.checked_add(frame_size) else {
            self.remaining = &[];
            return Some(Err(IvfError::TruncatedFrameData));
        };
        if self.remaining.len() < data_end {
            self.remaining = &[];
            return Some(Err(IvfError::TruncatedFrameData));
        }

        let data = &self.remaining[data_start..data_end];
        self.remaining = &self.remaining[data_end..];

        Some(Ok(IvfFrame { timestamp, data }))
    }
}

fn read_u16_le(buf: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buf[offset], buf[offset + 1]])
}

fn read_u32_le(buf: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

fn read_u64_le(buf: &[u8], offset: usize) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&buf[offset..offset + 8]);
    u64::from_le_bytes(bytes)
}
