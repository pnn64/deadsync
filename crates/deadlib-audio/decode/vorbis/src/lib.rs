//! Native Vorbis I raw synthesis and Ogg streaming; see COVERAGE.md for status.
//! Raw packet APIs have no dependency on a container or playback environment.
#![forbid(unsafe_code)]

pub mod bitpack;
pub mod codebook;
mod decoder;
mod floor;
mod floor_table;
mod headers;
mod ogg;
mod residue;
mod seek;
mod setup;
mod transform;

pub use decoder::{PacketDecoder, PcmBlock};
pub use headers::{Channel, CommentField, Comments, Identification, Metadata, channel_order};
pub use ogg::{
    OggDecoder, OggEvent, OggOptions, OggPcm, OggReader, OggStreamDecoder, StreamEnd, StreamId,
    StreamInfo, decode_reader,
};
pub use seek::{OggSeekReader, SeekOptions, SeekStream, VorbisIndex};
pub use setup::{
    AudioHeader, Floor, Floor0, Floor1, FloorClass, Mapping, Mode, Residue, Setup, Submap,
};
pub use transform::Imdct;

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
