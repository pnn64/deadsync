//! Native Vorbis I raw synthesis and Ogg streaming; see COVERAGE.md for status.
//! Raw packet APIs have no dependency on a container or playback environment.
#![forbid(unsafe_code)]

pub mod bitpack;
pub mod codebook;
mod decoder;
mod encoder;
mod floor;
mod floor_table;
mod headers;
mod ogg;
mod residue;
mod seek;
mod setup;
mod transform;

pub use decoder::{DecodeOptions, PacketDecoder, PcmBlock};
pub use encoder::{EncodeOptions, EncodedPacket, OggEncoder, PacketEncoder};
pub use headers::{Channel, CommentField, Comments, Identification, Metadata, channel_order};
pub use ogg::{
    OggDecoder, OggEvent, OggOptions, OggPcm, OggReader, OggStreamDecoder, StreamEnd, StreamId,
    StreamInfo, decode_reader,
};
pub use seek::{OggSeekReader, SeekOptions, SeekStream, VorbisIndex};
pub use setup::{
    AudioHeader, Floor, Floor0, Floor1, FloorClass, Mapping, Mode, Residue, Setup, Submap,
};
pub use transform::{Imdct, Mdct};

/// Scalar placement for type-two partitions that are not channel aligned.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Residue2Layout {
    /// Vorbis I section 8: deinterleave the exact flattened scalar range.
    #[default]
    Interleaved,
    /// Match libvorbis 1.3.7's decodevv_add: round both partition endpoints
    /// down to channel groups and start at the first channel of each group.
    /// This differs from the published interleave rule for unaligned ranges.
    Libvorbis,
}

/// Floor-zero amplitude reads, Bark-grid rounding and partial-LSP concealment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Floor0Behavior {
    /// Published unsigned amplitude field (0..=63 bits) and f64 Bark equation.
    #[default]
    Vorbis,
    /// Pinned libvorbis 1.3.7 behavior: signed 32-bit amplitude read, EOF for
    /// larger widths, f32 Bark intermediates and the observed x86 width-32
    /// normalization. Incomplete LSP words conceal the floor without sticky EOF.
    /// Use `allow_nonfinite` to retain its IEEE results.
    Libvorbis,
}

/// Floor-one guards and nominal end-of-packet reconstruction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Floor1Behavior {
    /// Clamp reconstructed values to the published range after prediction.
    #[default]
    Vorbis,
    /// Match libvorbis 1.3.7: mask reconstructed values to 15 bits, guard
    /// multiplied rendering ordinates at 0..=255, and retain its EOF behavior.
    Libvorbis,
}

/// Codebook wire-value interpretation and packet-availability checks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CodebookBehavior {
    /// Published mantissa * 2^(exponent - 788), retaining the full field range
    /// and checking coded arrays against the exact available bit count.
    #[default]
    Vorbis,
    /// Match libvorbis 1.3.7: clamp the unbiased exponent to -63..=63 and
    /// check whole remaining bytes before reading length/quantized arrays.
    Libvorbis,
}

/// Residue setup acceptance and Huffman end-of-packet handling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ResidueBehavior {
    /// Require ordered ranges and whole VQ vectors per partition.
    #[default]
    Vorbis,
    /// Accept libvorbis's partial vectors and empty inverted ranges. Incomplete
    /// Huffman words consume available bits without introducing sticky EOF.
    Libvorbis,
}

/// Header parsing policy, shared by complete setup and standalone codebooks.
#[derive(Clone, Copy, Debug, Default)]
pub struct ParseOptions {
    pub limits: Limits,
    /// Accept zero-active-entry books and their libvorbis consumer behavior.
    /// Other incomplete/overspecified Huffman trees remain invalid.
    pub allow_empty_codebooks: bool,
    /// Wire-value and array-availability interpretation for every codebook.
    pub codebook_behavior: CodebookBehavior,
    /// Retained by Setup for subsequent residue reconstruction.
    pub residue_behavior: ResidueBehavior,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    EndOfPacket,
    Invalid(&'static str),
    LimitExceeded(&'static str),
    Allocation,
    Failed,
    Container(deadlib_audio_decode_ogg::Error),
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EndOfPacket => f.write_str("Vorbis end of packet"),
            Self::Invalid(reason) => write!(f, "invalid Vorbis packet: {reason}"),
            Self::LimitExceeded(reason) => write!(f, "Vorbis resource limit: {reason}"),
            Self::Allocation => f.write_str("Vorbis allocation failed"),
            Self::Failed => f.write_str("Vorbis stream state is terminal after an earlier error"),
            Self::Container(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for Error {}
impl From<deadlib_audio_decode_ogg::Error> for Error {
    fn from(error: deadlib_audio_decode_ogg::Error) -> Self {
        Self::Container(error)
    }
}

/// Configurable application policies, not Vorbis I format restrictions.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_packet_bytes: usize,
    pub max_comment_bytes: usize,
    pub max_comments: usize,
    /// Bound `ilog(dimensions) + ilog(entries)` before reading lengths or
    /// allocating codebook storage. The default 24 matches libvorbis's guard.
    /// Set to 40 to permit the full wire-field extent, subject to other limits.
    pub max_codebook_extent_bits: u8,
    pub max_codebook_entries: usize,
    pub max_codebook_values: usize,
    pub max_huffman_nodes: usize,
    pub max_setup_memory: usize,
    /// Owned decode buffer/table elements, separately from setup storage.
    pub max_decode_memory: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_packet_bytes: 64 << 20,
            max_comment_bytes: 32 << 20,
            max_comments: 1 << 20,
            max_codebook_extent_bits: 24,
            max_codebook_entries: 1 << 20,
            max_codebook_values: 16 << 20,
            max_huffman_nodes: 2 << 20,
            max_setup_memory: 128 << 20,
            max_decode_memory: 128 << 20,
        }
    }
}
pub(crate) fn reserve<T>(v: &mut Vec<T>, additional: usize) -> Result<(), Error> {
    v.try_reserve(additional).map_err(|_| Error::Allocation)
}
pub(crate) struct Budget {
    remaining: usize,
}
impl Budget {
    pub(crate) fn new(limits: &Limits) -> Self {
        Self {
            remaining: limits.max_setup_memory,
        }
    }
    pub(crate) fn charge(&mut self, bytes: usize) -> Result<(), Error> {
        self.remaining = self
            .remaining
            .checked_sub(bytes)
            .ok_or(Error::LimitExceeded("setup memory"))?;
        Ok(())
    }
}
