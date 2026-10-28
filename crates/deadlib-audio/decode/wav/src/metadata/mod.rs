//! Explicit, allocation-free metadata views and caller-buffer serialization.
//! Container opening does not parse these payloads. Original bytes, encodings,
//! reserved fields and unknown chunks remain available through Chunk/Wave.
mod bext;
mod bw64;
mod markers;
mod riff;
use crate::{Chunk, Endian, Error, Limits};
pub use bext::{Bext, BextSpec, Loudness};
pub use bw64::{
    AlignmentPoint, AudioId, Bxml, Chna, SubXmlChunk, Sxml, write_bxml, write_chna, write_sxml,
};
pub use markers::{Marker, Markers, write_markers};
pub use riff::{
    AssociatedText, CuePoint, Cues, List, ListEntries, ListEntry, SampleLoop, Sampler,
    SamplerHeader, write_associated_text, write_cues, write_list, write_region_text, write_sampler,
};

#[derive(Clone, Copy, Debug)]
pub enum Metadata<'a> {
    List(List<'a>),
    Cues(Cues<'a>),
    Sampler(Sampler<'a>),
    Bext(Bext<'a>),
    Markers(Markers<'a>),
    Chna(Chna<'a>),
    Axml(&'a [u8]),
    Bxml(Bxml<'a>),
    Sxml(Sxml<'a>),
    Opaque(Chunk<'a>),
}
impl<'a> Metadata<'a> {
    pub fn raw(self) -> &'a [u8] {
        match self {
            Self::List(v) => v.raw(),
            Self::Cues(v) => v.raw(),
            Self::Sampler(v) => v.raw(),
            Self::Bext(v) => v.raw(),
            Self::Markers(v) => v.raw(),
            Self::Chna(v) => v.raw(),
            Self::Axml(v) => v,
            Self::Bxml(v) => v.raw(),
            Self::Sxml(v) => v.raw(),
            Self::Opaque(v) => v.data,
        }
    }
}
impl<'a> Chunk<'a> {
    /// Explicit metadata parsing; structural validation does not interpret XML,
    /// expand compressed XML, resolve ADM references or apply sampler loops.
    pub fn metadata(self, endian: Endian, limits: Limits) -> Result<Metadata<'a>, Error> {
        if endian == Endian::Big
            && [*b"bext", *b"chna", *b"r64m", *b"bxml", *b"sxml"].contains(&self.id)
        {
            return Err(Error::Unsupported(
                "little-endian broadcast metadata in RIFX",
            ));
        }
        Ok(match &self.id {
            b"LIST" => Metadata::List(List::parse(self.data, endian, limits)?),
            b"cue " => Metadata::Cues(Cues::parse(self.data, endian, limits)?),
            b"smpl" => Metadata::Sampler(Sampler::parse(self.data, endian, limits)?),
            b"bext" => Metadata::Bext(Bext::parse(self.data)?),
            b"r64m" => Metadata::Markers(Markers::parse(self.data, limits)?),
            b"chna" => Metadata::Chna(Chna::parse(self.data, limits)?),
            b"axml" => Metadata::Axml(self.data),
            b"bxml" => Metadata::Bxml(Bxml::parse(self.data)?),
            b"sxml" => Metadata::Sxml(Sxml::parse(self.data, limits)?),
            _ => Metadata::Opaque(self),
        })
    }
}

pub fn nul_terminated(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}
pub(crate) fn count(n: usize, limits: Limits) -> Result<(), Error> {
    if n > limits.chunks {
        Err(Error::Limit("metadata entry count"))
    } else {
        Ok(())
    }
}
pub(crate) fn length(header: usize, n: usize, stride: usize, tail: usize) -> Result<usize, Error> {
    n.checked_mul(stride)
        .and_then(|v| header.checked_add(v))
        .and_then(|v| v.checked_add(tail))
        .ok_or(Error::Limit("metadata size overflow"))
}
pub(crate) fn output(out: &mut [u8], len: usize) -> Result<&mut [u8], Error> {
    out.get_mut(..len)
        .ok_or(Error::Limit("metadata output capacity"))
}
