use crate::codec::{Header, be16, be32};
mod language;
mod profile;
/// IANA snapshot used for encoder-side BCP47 subtag registration checks.
/// Decoders retain well-formed, unregistered tags as unknown raw metadata.
pub const LANGUAGE_SUBTAG_REGISTRY_DATE: &str = language::REGISTRY_DATE;

pub(crate) fn is_known(kind: &[u8; 4]) -> bool {
    matches!(
        kind,
        b"gAMA"
            | b"cHRM"
            | b"sBIT"
            | b"sRGB"
            | b"cICP"
            | b"mDCV"
            | b"cLLI"
            | b"iCCP"
            | b"bKGD"
            | b"hIST"
            | b"pHYs"
            | b"sPLT"
            | b"eXIf"
            | b"tIME"
            | b"tEXt"
            | b"zTXt"
            | b"iTXt"
    )
}
use crate::{AncillaryChunk, ChunkPosition, ColorType, Error, Limits, Png, zlib};
use std::collections::HashSet;

/// Native integer metadata values. Fixed-point integers retain the exact wire
/// precision. Latin-1 keywords/text remain bytes; iTXt uses Unicode strings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Metadata {
    Gamma(u32),
    Chromaticities([u32; 8]),
    SignificantBits(Vec<u8>),
    Srgb(u8),
    IccProfile {
        name: Vec<u8>,
        profile: Vec<u8>,
    },
    Cicp([u8; 4]),
    MasteringDisplay {
        chromaticities: [u16; 8],
        luminance: [u32; 2],
    },
    ContentLight([u32; 2]),
    BackgroundGray(u16),
    BackgroundRgb([u16; 3]),
    BackgroundIndex(u8),
    Histogram(Vec<u16>),
    PhysicalDimensions {
        x: u32,
        y: u32,
        unit: u8,
    },
    SuggestedPalette {
        name: Vec<u8>,
        depth: u8,
        entries: Vec<[u16; 5]>,
    },
    Exif(Vec<u8>),
    /// Year, month, day, hour, minute, second (60 permits a leap second).
    Time([u16; 6]),
    Text {
        keyword: Vec<u8>,
        text: Vec<u8>,
        compressed: bool,
    },
    InternationalText {
        keyword: Vec<u8>,
        language: String,
        translated_keyword: String,
        text: String,
        compressed: bool,
    },
    Unknown {
        kind: [u8; 4],
        data: Vec<u8>,
    },
}

fn keyword(value: &[u8]) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > 79
        || value.first() == Some(&32)
        || value.last() == Some(&32)
        || value.windows(2).any(|v| v == b"  ")
        || !value.iter().all(|&b| (32..=126).contains(&b) || b >= 161)
    {
        return Err(Error::Invalid("metadata keyword"));
    }
    Ok(())
}
fn nul<'a>(bytes: &mut &'a [u8]) -> Result<&'a [u8], Error> {
    let n = bytes
        .iter()
        .position(|&b| b == 0)
        .ok_or(Error::Invalid("missing metadata separator"))?;
    let value = &bytes[..n];
    *bytes = &bytes[n + 1..];
    Ok(value)
}
fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Result<&'a [u8], Error> {
    let value = bytes.get(..n).ok_or(Error::Invalid("metadata length"))?;
    *bytes = &bytes[n..];
    Ok(value)
}
fn text_valid(bytes: &[u8]) -> Result<(), Error> {
    if bytes.contains(&0) {
        Err(Error::Invalid("null in metadata text"))
    } else {
        Ok(())
    }
}
fn inflate(bytes: &[u8], remaining: &mut usize) -> Result<Vec<u8>, Error> {
    let out = zlib::decompress(bytes, *remaining)?;
    *remaining -= out.len();
    Ok(out)
}

// WHATWG UTF-8 decode consumes exactly one leading BOM and replaces malformed
// sequences. Raw ancillary bytes remain available unchanged in Png::ancillary.
fn utf8_decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes)).into_owned()
}

pub(crate) fn encoding_data<'a>(
    c: &'a AncillaryChunk,
    image: &crate::Image,
) -> std::borrow::Cow<'a, [u8]> {
    if &c.kind == b"bKGD" && image.color_type != ColorType::Indexed && image.bit_depth < 16 {
        let mask = ((1u32 << image.bit_depth) - 1) as u16;
        std::borrow::Cow::Owned(
            c.data
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|&v| (u16::from_be_bytes(v) & mask).to_be_bytes())
                .collect(),
        )
    } else {
        std::borrow::Cow::Borrowed(&c.data)
    }
}

fn parse(
    c: &AncillaryChunk,
    h: Header,
    palette: usize,
    remaining: &mut usize,
    encoding: bool,
) -> Result<Metadata, Error> {
    let mut d = c.data.as_slice();
    let invalid = || Error::Invalid("metadata length or value");
    let four_byte_values: &[u8] = match &c.kind {
        b"gAMA" | b"cHRM" | b"cLLI" => d,
        b"pHYs" if d.len() == 9 => &d[..8],
        b"mDCV" if d.len() == 24 => &d[16..],
        _ => &[],
    };
    if four_byte_values
        .as_chunks::<4>()
        .0
        .iter()
        .any(|v| be32(v) > 0x7fff_ffff)
    {
        return Err(Error::Invalid("metadata integer exceeds 31 bits"));
    }
    Ok(match &c.kind {
        b"gAMA" if d.len() == 4 && be32(d) != 0 => Metadata::Gamma(be32(d)),
        b"cHRM" if d.len() == 32 => {
            Metadata::Chromaticities(core::array::from_fn(|n| be32(&d[n * 4..])))
        }
        b"sBIT" => {
            let n = if h.color == ColorType::Indexed {
                3
            } else {
                h.color.channels()
            };
            let max = if h.color == ColorType::Indexed {
                8
            } else {
                h.depth
            };
            if d.len() != n || d.iter().any(|&b| b == 0 || b > max) {
                return Err(invalid());
            }
            Metadata::SignificantBits(d.to_vec())
        }
        b"sRGB" if d.len() == 1 && d[0] <= 3 => Metadata::Srgb(d[0]),
        b"cICP" if d.len() == 4 && d[2] == 0 && d[3] <= 1 => Metadata::Cicp(d.try_into().unwrap()),
        b"mDCV" if d.len() == 24 => Metadata::MasteringDisplay {
            chromaticities: core::array::from_fn(|n| be16(&d[n * 2..])),
            luminance: [be32(&d[16..]), be32(&d[20..])],
        },
        b"cLLI" if d.len() == 8 => Metadata::ContentLight([be32(d), be32(&d[4..])]),
        b"iCCP" => {
            let name = nul(&mut d)?;
            keyword(name)?;
            if take(&mut d, 1)?[0] != 0 {
                return Err(Error::Invalid("ICC compression method"));
            }
            let bytes = inflate(d, remaining)?;
            match profile::icc(&bytes, Some(h.color)) {
                Ok(()) => Metadata::IccProfile {
                    name: name.to_vec(),
                    profile: bytes,
                },
                Err(Error::Invalid(_)) if !encoding => Metadata::Unknown {
                    kind: c.kind,
                    data: c.data.clone(),
                },
                Err(error) => return Err(error),
            }
        }
        b"bKGD" => {
            let n = match h.color {
                ColorType::Indexed => 1,
                ColorType::Grayscale | ColorType::GrayscaleAlpha => 2,
                _ => 6,
            };
            if d.len() != n {
                return Err(invalid());
            }
            if h.color == ColorType::Indexed {
                if d[0] as usize >= palette {
                    return Err(Error::Invalid("background palette index"));
                }
                Metadata::BackgroundIndex(d[0])
            } else {
                let mask = ((1u32 << h.depth) - 1) as u16;
                if n == 2 {
                    Metadata::BackgroundGray(be16(d) & mask)
                } else {
                    Metadata::BackgroundRgb(core::array::from_fn(|n| be16(&d[n * 2..]) & mask))
                }
            }
        }
        b"hIST" if palette != 0 && d.len() == palette * 2 => Metadata::Histogram(
            d.as_chunks::<2>()
                .0
                .iter()
                .map(|&v| u16::from_be_bytes(v))
                .collect(),
        ),
        b"pHYs" if d.len() == 9 && d[8] <= 1 => Metadata::PhysicalDimensions {
            x: be32(d),
            y: be32(&d[4..]),
            unit: d[8],
        },
        b"sPLT" => {
            let name = nul(&mut d)?;
            keyword(name)?;
            let depth = take(&mut d, 1)?[0];
            let stride = match depth {
                8 => 6,
                16 => 10,
                _ => return Err(invalid()),
            };
            if !d.len().is_multiple_of(stride) {
                return Err(invalid());
            }
            let entries: Vec<[u16; 5]> = d
                .chunks(stride)
                .map(|v| {
                    if depth == 8 {
                        [
                            u16::from(v[0]),
                            u16::from(v[1]),
                            u16::from(v[2]),
                            u16::from(v[3]),
                            be16(&v[4..]),
                        ]
                    } else {
                        core::array::from_fn(|n| be16(&v[n * 2..]))
                    }
                })
                .collect();
            if entries.windows(2).any(|v| v[0][4] < v[1][4]) {
                return Err(Error::Invalid("sPLT frequency order"));
            }
            Metadata::SuggestedPalette {
                name: name.to_vec(),
                depth,
                entries,
            }
        }
        b"eXIf" => match profile::exif(d) {
            Ok(()) => Metadata::Exif(d.to_vec()),
            Err(_) if !encoding => Metadata::Unknown {
                kind: c.kind,
                data: d.to_vec(),
            },
            Err(error) => return Err(error),
        },
        b"tIME"
            if d.len() == 7
                && (1..=12).contains(&d[2])
                && (1..=31).contains(&d[3])
                && d[4] < 24
                && d[5] < 60
                && d[6] <= 60 =>
        {
            Metadata::Time([
                be16(d),
                d[2] as u16,
                d[3] as u16,
                d[4] as u16,
                d[5] as u16,
                d[6] as u16,
            ])
        }
        b"tEXt" | b"zTXt" => {
            let key = nul(&mut d)?;
            keyword(key)?;
            let compressed = &c.kind == b"zTXt";
            let text = if compressed {
                if take(&mut d, 1)?[0] != 0 {
                    return Err(Error::Invalid("text compression method"));
                }
                inflate(d, remaining)?
            } else {
                d.to_vec()
            };
            text_valid(&text)?;
            Metadata::Text {
                keyword: key.to_vec(),
                text,
                compressed,
            }
        }
        b"iTXt" => {
            let key = nul(&mut d)?;
            keyword(key)?;
            let flags = take(&mut d, 2)?;
            if flags[0] > 1 || ((flags[0] == 1 || encoding) && flags[1] != 0) {
                return Err(Error::Invalid("iTXt compression"));
            }
            let language = nul(&mut d)?;
            if !language::registered(language) {
                if encoding {
                    return Err(Error::Invalid("invalid or unregistered iTXt language tag"));
                }
                if !language::well_formed(language) {
                    return Err(Error::Invalid("iTXt language tag"));
                }
                // PNG 13.2 permits an ancillary field we cannot interpret to
                // be handled as an unknown chunk. Preserve it for future
                // registry versions without interpreting an invalid tag.
                return Ok(Metadata::Unknown {
                    kind: c.kind,
                    data: c.data.clone(),
                });
            }
            let translated = nul(&mut d)?;
            let text = if flags[0] == 1 {
                inflate(d, remaining)?
            } else {
                d.to_vec()
            };
            text_valid(&text)?;
            if encoding
                && (std::str::from_utf8(translated).is_err() || std::str::from_utf8(&text).is_err())
            {
                return Err(Error::Invalid("iTXt UTF-8"));
            }
            Metadata::InternationalText {
                keyword: key.to_vec(),
                language: String::from_utf8_lossy(language).into_owned(),
                translated_keyword: utf8_decode(translated),
                text: utf8_decode(&text),
                compressed: flags[0] == 1,
            }
        }
        b"gAMA" | b"cHRM" | b"sRGB" | b"cICP" | b"mDCV" | b"cLLI" | b"hIST" | b"pHYs" | b"tIME" => {
            return Err(invalid());
        }
        _ => Metadata::Unknown {
            kind: c.kind,
            data: d.to_vec(),
        },
    })
}

pub(crate) fn validate(
    chunks: &[AncillaryChunk],
    h: Header,
    palette: usize,
    limits: Limits,
) -> Result<Vec<Metadata>, Error> {
    validate_impl(chunks, h, palette, limits, false)
}

pub(crate) fn validate_encoding(
    chunks: &[AncillaryChunk],
    h: Header,
    palette: usize,
    limits: Limits,
) -> Result<Vec<Metadata>, Error> {
    validate_impl(chunks, h, palette, limits, true)
}

fn validate_impl(
    chunks: &[AncillaryChunk],
    h: Header,
    palette: usize,
    limits: Limits,
    encoding: bool,
) -> Result<Vec<Metadata>, Error> {
    let mut validator = Validator::new(limits.max_metadata_bytes);
    validator.encoding = encoding;
    let mut out = Vec::new();
    for c in chunks {
        out.push(validator.check(c, h, palette)?);
    }
    validator.finish()?;
    Ok(out)
}

pub(crate) struct Validator {
    unique: HashSet<[u8; 4]>,
    names: HashSet<Vec<u8>>,
    remaining: usize,
    encoding: bool,
    srgb_compatible: bool,
}
impl Validator {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            unique: HashSet::new(),
            names: HashSet::new(),
            remaining: limit,
            encoding: false,
            srgb_compatible: true,
        }
    }
    pub(crate) fn remaining(&self) -> usize {
        self.remaining
    }
    pub(crate) fn has(&self, kind: &[u8; 4]) -> bool {
        self.unique.contains(kind)
    }
    pub(crate) fn check(
        &mut self,
        c: &AncillaryChunk,
        h: Header,
        palette: usize,
    ) -> Result<Metadata, Error> {
        self.remaining = self
            .remaining
            .checked_sub(c.data.len())
            .ok_or(Error::LimitExceeded)?;
        let early = matches!(
            &c.kind,
            b"cHRM" | b"gAMA" | b"iCCP" | b"sBIT" | b"sRGB" | b"cICP" | b"mDCV" | b"cLLI"
        );
        let before_data =
            early || matches!(&c.kind, b"bKGD" | b"hIST" | b"pHYs" | b"sPLT" | b"eXIf");
        if (early && c.position != ChunkPosition::BeforePalette)
            || (before_data && c.position == ChunkPosition::AfterData)
        {
            return Err(Error::Invalid("ancillary chunk ordering"));
        }
        if matches!(&c.kind, b"bKGD" | b"hIST")
            && palette != 0
            && c.position == ChunkPosition::BeforePalette
        {
            return Err(Error::Invalid("ancillary chunk must follow PLTE"));
        }
        if (early || matches!(&c.kind, b"bKGD" | b"hIST" | b"pHYs" | b"eXIf" | b"tIME"))
            && !self.unique.insert(c.kind)
        {
            return Err(Error::Invalid("duplicate ancillary chunk"));
        }
        let value = parse(c, h, palette, &mut self.remaining, self.encoding)?;
        // PNG 11.3.2.5, Table 17: optional fallback gAMA/cHRM accompanying
        // sRGB must describe sRGB, even if a still higher color chunk exists.
        match &value {
            Metadata::Gamma(v) if *v != 45455 => self.srgb_compatible = false,
            Metadata::Chromaticities(v)
                if *v != [31270, 32900, 64000, 33000, 30000, 60000, 15000, 6000] =>
            {
                self.srgb_compatible = false
            }
            _ => (),
        }
        if let Metadata::SuggestedPalette { name, .. } = &value
            && !self.names.insert(name.clone())
        {
            return Err(Error::Invalid("duplicate suggested palette name"));
        }
        Ok(value)
    }
    pub(crate) fn finish(&self) -> Result<(), Error> {
        if self.encoding && self.unique.contains(b"sRGB") && !self.srgb_compatible {
            return Err(Error::Invalid("gAMA or cHRM does not match sRGB"));
        }
        if self.unique.contains(b"mDCV") && !self.unique.contains(b"cICP") {
            return Err(Error::Invalid("mDCV requires cICP"));
        }
        Ok(())
    }
}

impl Png {
    pub fn metadata(&self, limits: Limits) -> Result<Vec<Metadata>, Error> {
        self.image
            .validate()
            .map_err(|_| Error::Invalid("image samples"))?;
        validate(
            &self.ancillary,
            Header {
                width: self.image.width,
                height: self.image.height,
                color: self.image.color_type,
                depth: self.image.bit_depth,
                interlaced: self.interlaced,
            },
            self.palette.len(),
            limits,
        )
    }
    /// The highest-precedence supported color-description chunk (PNG 3, 4.7).
    /// Invalid/unsupported ICC containers are skipped. Returns None on metadata
    /// errors or default resource limits; use the fallible variant to distinguish them.
    pub fn color_description(&self) -> Option<&AncillaryChunk> {
        self.color_description_with_limits(Limits::default())
            .ok()
            .flatten()
    }
    /// Validate candidates in precedence order, without applying color transforms
    /// or inflating metadata below the first supported description. Limit errors
    /// are returned, so an unexamined high-precedence profile is never silently
    /// replaced by a lower-precedence description.
    pub fn color_description_with_limits(
        &self,
        limits: Limits,
    ) -> Result<Option<&AncillaryChunk>, Error> {
        if !self.image.color_type.allows_depth(self.image.bit_depth) {
            return Err(Error::Invalid("image format"));
        }
        let h = Header {
            width: self.image.width,
            height: self.image.height,
            color: self.image.color_type,
            depth: self.image.bit_depth,
            interlaced: self.interlaced,
        };
        let mut remaining = limits.max_metadata_bytes;
        for kind in [b"cICP", b"iCCP", b"sRGB", b"gAMA", b"cHRM"] {
            if let Some(chunk) = self.ancillary.iter().find(|c| &c.kind == kind) {
                remaining = remaining
                    .checked_sub(chunk.data.len())
                    .ok_or(Error::LimitExceeded)?;
                if !matches!(
                    parse(chunk, h, self.palette.len(), &mut remaining, false)?,
                    Metadata::Unknown { .. }
                ) {
                    return Ok(Some(chunk));
                }
            }
        }
        Ok(None)
    }
}

impl Metadata {
    /// Serialize a metadata value. Image-dependent constraints and placement are
    /// checked by `encode`; raw integer values avoid floating-point rounding.
    pub fn to_chunk(
        &self,
        position: ChunkPosition,
        compression: zlib::Compression,
    ) -> Result<AncillaryChunk, Error> {
        let mut data = Vec::new();
        let kind = match self {
            Self::Gamma(v) => {
                data.extend_from_slice(&v.to_be_bytes());
                *b"gAMA"
            }
            Self::Chromaticities(v) => {
                for n in v {
                    data.extend_from_slice(&n.to_be_bytes());
                }
                *b"cHRM"
            }
            Self::SignificantBits(v) => {
                data.extend_from_slice(v);
                *b"sBIT"
            }
            Self::Srgb(v) => {
                data.push(*v);
                *b"sRGB"
            }
            Self::Cicp(v) => {
                data.extend_from_slice(v);
                *b"cICP"
            }
            Self::MasteringDisplay {
                chromaticities,
                luminance,
            } => {
                for n in chromaticities {
                    data.extend_from_slice(&n.to_be_bytes());
                }
                for n in luminance {
                    data.extend_from_slice(&n.to_be_bytes());
                }
                *b"mDCV"
            }
            Self::ContentLight(v) => {
                for n in v {
                    data.extend_from_slice(&n.to_be_bytes());
                }
                *b"cLLI"
            }
            Self::IccProfile { name, profile } => {
                keyword(name)?;
                profile::icc(profile, None)?;
                data.extend_from_slice(name);
                data.extend_from_slice(&[0, 0]);
                data.extend_from_slice(&zlib::compress(profile, compression)?);
                *b"iCCP"
            }
            Self::BackgroundGray(v) => {
                data.extend_from_slice(&v.to_be_bytes());
                *b"bKGD"
            }
            Self::BackgroundRgb(v) => {
                for n in v {
                    data.extend_from_slice(&n.to_be_bytes());
                }
                *b"bKGD"
            }
            Self::BackgroundIndex(v) => {
                data.push(*v);
                *b"bKGD"
            }
            Self::Histogram(v) => {
                for n in v {
                    data.extend_from_slice(&n.to_be_bytes());
                }
                *b"hIST"
            }
            Self::PhysicalDimensions { x, y, unit } => {
                data.extend_from_slice(&x.to_be_bytes());
                data.extend_from_slice(&y.to_be_bytes());
                data.push(*unit);
                *b"pHYs"
            }
            Self::SuggestedPalette {
                name,
                depth,
                entries,
            } => {
                keyword(name)?;
                data.extend_from_slice(name);
                data.extend_from_slice(&[0, *depth]);
                for entry in entries {
                    for (n, v) in entry.iter().enumerate() {
                        if *depth == 8 && n < 4 {
                            if *v > 255 {
                                return Err(Error::Invalid("sPLT sample range"));
                            }
                            data.push(*v as u8);
                        } else {
                            data.extend_from_slice(&v.to_be_bytes());
                        }
                    }
                }
                *b"sPLT"
            }
            Self::Exif(v) => {
                profile::exif(v)?;
                data.extend_from_slice(v);
                *b"eXIf"
            }
            Self::Time(v) => {
                data.extend_from_slice(&v[0].to_be_bytes());
                for &n in &v[1..] {
                    data.push(u8::try_from(n).map_err(|_| Error::Invalid("time component"))?);
                }
                *b"tIME"
            }
            Self::Text {
                keyword: key,
                text,
                compressed,
            } => {
                keyword(key)?;
                text_valid(text)?;
                data.extend_from_slice(key);
                data.push(0);
                if *compressed {
                    data.push(0);
                    data.extend_from_slice(&zlib::compress(text, compression)?);
                    *b"zTXt"
                } else {
                    data.extend_from_slice(text);
                    *b"tEXt"
                }
            }
            Self::InternationalText {
                keyword: key,
                language,
                translated_keyword,
                text,
                compressed,
            } => {
                keyword(key)?;
                text_valid(language.as_bytes())?;
                if !language::registered(language.as_bytes()) {
                    return Err(Error::Invalid("iTXt language tag"));
                }
                text_valid(translated_keyword.as_bytes())?;
                text_valid(text.as_bytes())?;
                data.extend_from_slice(key);
                data.extend_from_slice(&[0, u8::from(*compressed), 0]);
                data.extend_from_slice(language.as_bytes());
                data.push(0);
                data.extend_from_slice(translated_keyword.as_bytes());
                data.push(0);
                if *compressed {
                    data.extend_from_slice(&zlib::compress(text.as_bytes(), compression)?);
                } else {
                    data.extend_from_slice(text.as_bytes());
                }
                *b"iTXt"
            }
            Self::Unknown { kind, data: bytes } => {
                data.extend_from_slice(bytes);
                *kind
            }
        };
        Ok(AncillaryChunk {
            kind,
            data,
            position,
        })
    }
}
