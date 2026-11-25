use crate::{ColorType, Error, Image, Samples, checksum::crc32, reserve, scanline, zlib};

pub const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_metadata_bytes: usize,
    pub max_chunks: usize,
    pub max_frames: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 256 << 20,
            max_output_bytes: 512 << 20,
            max_metadata_bytes: 16 << 20,
            max_chunks: 1_000_000,
            max_frames: 10_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DecodeOptions {
    pub limits: Limits,
    pub palette_policy: PalettePolicy,
    pub animation_policy: AnimationPolicy,
    pub metadata_policy: MetadataPolicy,
}

/// Handling of optional metadata faults, independently of image/animation data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MetadataPolicy {
    /// Discard corrupt, malformed, misplaced or duplicate optional metadata.
    /// Accept Exif after IDAT and keep the first CRC-valid ICC occurrence.
    /// Resource limits, critical chunks, tRNS and APNG checks remain enforced.
    #[default]
    Recover,
    /// Reject metadata CRC, ordering, multiplicity and parsing errors.
    /// Unsupported ICC/Exif containers still retain their raw representation.
    Strict,
}

impl MetadataPolicy {
    pub(crate) fn discard_bad_crc(self, kind: &[u8; 4]) -> bool {
        self == Self::Recover
            && kind[0] & 32 != 0
            && !matches!(kind, b"tRNS" | b"acTL" | b"fcTL" | b"fdAT")
    }
}

/// Handling of animation faults when the static PNG can still be verified.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AnimationPolicy {
    /// Report the first error and stop decoding.
    #[default]
    Strict,
    /// Report an animation error, discard animation frames and decode the
    /// static image. PNG framing, static data and resource errors remain fatal;
    /// metadata uses `MetadataPolicy`. See `Png::animation_error` and
    /// `DecodeEvent::AnimationError`.
    RecoverDefault,
}

/// Handling of pixel indices outside PLTE. Native samples always retain their
/// original indices; expansion and APNG rendering use opaque black for them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PalettePolicy {
    /// PNG Third Edition 13.1 recovery, without changing samples or the palette.
    #[default]
    RecoverBlack,
    /// Report an error instead of accepting an out-of-range palette index.
    Strict,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Filter {
    None = 0,
    Sub = 1,
    Up = 2,
    Average = 3,
    Paeth = 4,
    #[default]
    Adaptive = 5,
}

#[derive(Clone, Copy, Debug)]
pub struct EncodeOptions {
    pub interlaced: bool,
    pub filter: Filter,
    pub compression: zlib::Compression,
    pub idat_chunk_size: usize,
}
impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            interlaced: false,
            filter: Filter::Adaptive,
            compression: zlib::Compression::Fast,
            idat_chunk_size: 64 << 10,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Transparency {
    Grayscale(u16),
    Rgb([u16; 3]),
    Palette(Vec<u8>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkPosition {
    BeforePalette,
    BeforeData,
    AfterData,
}

/// Retained ancillary data in its original wire representation. Compressed
/// metadata uses independent zlib streams; typed accessors are added separately.
/// Names are case-sensitive. Decoding retains unknown ancillary chunks even
/// when their third letter sets the reserved bit (PNG Third Edition 13.5).
/// Both encoders require alphabetic names with a lowercase first and uppercase
/// third letter. Reencoding replaces IDAT, so unknown chunks with an uppercase
/// fourth letter are omitted; safe-to-copy chunks retain their placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AncillaryChunk {
    pub kind: [u8; 4],
    pub data: Vec<u8>,
    pub position: ChunkPosition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Png {
    pub image: Image,
    pub interlaced: bool,
    pub palette: Vec<[u8; 3]>,
    pub transparency: Option<Transparency>,
    pub ancillary: Vec<AncillaryChunk>,
    pub animation: Option<crate::Animation>,
    /// First recovered animation fault. The animation is discarded while the
    /// static image and remaining PNG checks are completed. This diagnostic
    /// is not serialized by either encoder.
    pub animation_error: Option<Error>,
}
impl Png {
    pub fn new(image: Image) -> Self {
        Self {
            image,
            interlaced: false,
            palette: Vec::new(),
            transparency: None,
            ancillary: Vec::new(),
            animation: None,
            animation_error: None,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Header {
    pub width: u32,
    pub height: u32,
    pub color: ColorType,
    pub depth: u8,
    pub interlaced: bool,
}
impl Header {
    pub(crate) fn parse(data: &[u8]) -> Result<Self, Error> {
        if data.len() != 13 {
            return Err(Error::Invalid("IHDR length"));
        }
        let width = be32(&data[..4]);
        let height = be32(&data[4..8]);
        let color = ColorType::try_from(data[9]).map_err(|_| Error::Invalid("color type"))?;
        if width == 0 || height == 0 || width > 0x7fff_ffff || height > 0x7fff_ffff {
            return Err(Error::Invalid("image dimensions"));
        }
        if !color.allows_depth(data[8]) || data[10] != 0 || data[11] != 0 || data[12] > 1 {
            return Err(Error::Invalid("IHDR format"));
        }
        Ok(Self {
            width,
            height,
            color,
            depth: data[8],
            interlaced: data[12] != 0,
        })
    }
    pub(crate) fn wire(self) -> [u8; 13] {
        let mut data = [0; 13];
        data[..4].copy_from_slice(&self.width.to_be_bytes());
        data[4..8].copy_from_slice(&self.height.to_be_bytes());
        data[8] = self.depth;
        data[9] = self.color as u8;
        data[12] = u8::from(self.interlaced);
        data
    }
    pub(crate) fn sample_bytes(self) -> Result<usize, Error> {
        (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|n| n.checked_mul(self.color.channels()))
            .and_then(|n| n.checked_mul(if self.depth == 16 { 2 } else { 1 }))
            .ok_or(Error::LimitExceeded)
    }
}
pub(crate) fn be32(data: &[u8]) -> u32 {
    u32::from_be_bytes(data[..4].try_into().unwrap())
}
pub(crate) fn be16(data: &[u8]) -> u16 {
    u16::from_be_bytes(data[..2].try_into().unwrap())
}

#[inline]
pub(crate) fn chunk_type_valid(kind: [u8; 4]) -> bool {
    let bytes = u32::from_ne_bytes(kind);
    let lower = bytes | 0x2020_2020;
    // For ASCII lanes neither addition carries into the next byte. The high
    // bit marks >= 'a' after adding 31 and > 'z' after adding 5.
    let at_least_a = lower.wrapping_add(0x1f1f_1f1f);
    let after_z = lower.wrapping_add(0x0505_0505);
    (bytes | !at_least_a | after_z) & 0x8080_8080 == 0
}

fn chunk<'a>(input: &mut &'a [u8]) -> Result<([u8; 4], &'a [u8], bool), Error> {
    if input.len() < 12 {
        return Err(Error::Truncated);
    }
    let len = be32(input) as usize;
    if len > 0x7fff_ffff {
        return Err(Error::Invalid("chunk length exceeds 31 bits"));
    }
    let end = len.checked_add(12).ok_or(Error::LimitExceeded)?;
    if end > input.len() {
        return Err(Error::Truncated);
    }
    let kind: [u8; 4] = input[4..8].try_into().unwrap();
    if !chunk_type_valid(kind) {
        return Err(Error::Invalid("chunk type"));
    }
    let trailer = end - 4;
    let data = &input[8..trailer];
    let checksum_valid = crc32(&[&input[4..trailer]]) == be32(&input[trailer..end]);
    *input = &input[end..];
    Ok((kind, data, checksum_valid))
}

pub fn decode(input: &[u8]) -> Result<Png, Error> {
    decode_with_options(input, &DecodeOptions::default())
}

pub fn decode_with_options(mut input: &[u8], options: &DecodeOptions) -> Result<Png, Error> {
    let limits = options.limits;
    if input.len() > limits.max_input_bytes {
        return Err(Error::LimitExceeded);
    }
    if options.animation_policy == AnimationPolicy::RecoverDefault {
        return crate::decode_reader(input, options);
    }
    if input.len() < 8 {
        return Err(Error::Truncated);
    }
    if &input[..8] != SIGNATURE {
        return Err(Error::Invalid("signature"));
    }
    input = &input[8..];
    let (kind, data, checksum_valid) = chunk(&mut input)?;
    if !checksum_valid {
        return Err(Error::Checksum);
    }
    if &kind != b"IHDR" {
        return Err(Error::Invalid("IHDR must be first"));
    }
    let header = Header::parse(data)?;
    let mut animation_decoder = crate::apng::Decoder::new(header, *options)?;
    let expected = scanline::filtered_size(
        header.width,
        header.height,
        header.color,
        header.depth,
        header.interlaced,
    )?;
    if expected > limits.max_output_bytes {
        return Err(Error::LimitExceeded);
    }
    let mut palette = Vec::new();
    let mut transparency = None;
    let mut ancillary = Vec::new();
    let mut compressed_start = &[][..];
    let mut compressed_wire_len = 0;
    let mut compressed_bytes = 0;
    let mut metadata_bytes = 0usize;
    let mut discarded_metadata_bytes = 0usize;
    let mut seen_data = false;
    let mut ended_data = false;
    let mut count = 1usize;
    loop {
        count = count.checked_add(1).ok_or(Error::LimitExceeded)?;
        if count > limits.max_chunks {
            return Err(Error::LimitExceeded);
        }
        let chunk_start = input;
        let (kind, data, checksum_valid) = chunk(&mut input)?;
        if seen_data && &kind != b"IDAT" {
            ended_data = true;
        }
        if !checksum_valid {
            if !options.metadata_policy.discard_bad_crc(&kind) {
                return Err(Error::Checksum);
            }
            metadata_bytes = metadata_bytes
                .checked_add(data.len())
                .ok_or(Error::LimitExceeded)?;
            if metadata_bytes > limits.max_metadata_bytes {
                return Err(Error::LimitExceeded);
            }
            discarded_metadata_bytes += data.len();
            continue;
        }
        match &kind {
            b"acTL" | b"fcTL" | b"fdAT" => {
                animation_decoder.chunk(&kind, data, chunk_start, seen_data, palette.len())?
            }
            b"IHDR" => return Err(Error::Invalid("duplicate IHDR")),
            b"PLTE" => {
                if seen_data || !palette.is_empty() || transparency.is_some() {
                    return Err(Error::Invalid("PLTE ordering or duplication"));
                }
                if matches!(
                    header.color,
                    ColorType::Grayscale | ColorType::GrayscaleAlpha
                ) || data.is_empty()
                    || data.len() % 3 != 0
                    || data.len() > 768
                {
                    return Err(Error::Invalid("PLTE contents"));
                }
                if header.color == ColorType::Indexed && data.len() / 3 > 1usize << header.depth {
                    return Err(Error::Invalid("palette exceeds bit depth"));
                }
                palette.extend_from_slice(data.as_chunks::<3>().0);
            }
            b"tRNS" => {
                if seen_data || transparency.is_some() {
                    return Err(Error::Invalid("tRNS ordering or duplication"));
                }
                let value = match header.color {
                    ColorType::Grayscale if data.len() == 2 => Transparency::Grayscale(be16(data)),
                    ColorType::Rgb if data.len() == 6 => {
                        Transparency::Rgb([be16(data), be16(&data[2..]), be16(&data[4..])])
                    }
                    ColorType::Indexed if !palette.is_empty() && data.len() <= palette.len() => {
                        Transparency::Palette(data.to_vec())
                    }
                    _ => return Err(Error::Invalid("tRNS contents")),
                };
                transparency = Some(value);
            }
            b"IDAT" => {
                if ended_data {
                    return Err(Error::Invalid("nonconsecutive IDAT"));
                }
                if !seen_data {
                    animation_decoder.before_default()?;
                    compressed_start = chunk_start;
                }
                if header.color == ColorType::Indexed && palette.is_empty() {
                    return Err(Error::Invalid("missing PLTE"));
                }
                seen_data = true;
                compressed_wire_len = compressed_start.len() - input.len();
                compressed_bytes += data.len();
            }
            b"IEND" => {
                if !data.is_empty() || !seen_data {
                    return Err(Error::Invalid("IEND contents or missing IDAT"));
                }
                if !input.is_empty() {
                    return Err(Error::Invalid("data after IEND"));
                }
                break;
            }
            _ if kind[0] & 32 == 0 => return Err(Error::UnsupportedCriticalChunk(kind)),
            _ => {
                metadata_bytes = metadata_bytes
                    .checked_add(data.len())
                    .ok_or(Error::LimitExceeded)?;
                if metadata_bytes > limits.max_metadata_bytes {
                    return Err(Error::LimitExceeded);
                }
                let position = if seen_data {
                    ChunkPosition::AfterData
                } else if palette.is_empty() {
                    ChunkPosition::BeforePalette
                } else {
                    ChunkPosition::BeforeData
                };
                reserve(&mut ancillary, 1)?;
                ancillary.push(AncillaryChunk {
                    kind,
                    data: data.to_vec(),
                    position,
                });
            }
        }
    }
    crate::metadata::retain_valid(
        &mut ancillary,
        header,
        palette.len(),
        limits.max_metadata_bytes - discarded_metadata_bytes,
        options.metadata_policy,
    )?;
    // PNG Third Edition 11.2.3 recommends ignoring unused bytes in final IDAT.
    let raw = zlib::decompress_png_chunks(
        &compressed_start[..compressed_wire_len],
        compressed_bytes,
        expected,
    )?;
    if raw.len() != expected {
        return Err(Error::Invalid("image data length"));
    }
    let image = scanline::decode(
        raw,
        header.width,
        header.height,
        header.color,
        header.depth,
        header.interlaced,
        limits.max_output_bytes,
    )?;
    let animation = animation_decoder.finish(palette.len())?;
    let png = Png {
        image,
        interlaced: header.interlaced,
        palette,
        transparency,
        ancillary,
        animation,
        animation_error: None,
    };
    validate_pixels(&png, options.palette_policy)?;
    Ok(png)
}

pub(crate) fn validate_pixels(png: &Png, policy: PalettePolicy) -> Result<(), Error> {
    if png.image.color_type == ColorType::Indexed {
        let Samples::U8(samples) = &png.image.samples else {
            return Err(Error::Invalid("indexed sample storage"));
        };
        if png.palette.is_empty()
            || png.palette.len() > 1usize << png.image.bit_depth
            || (policy == PalettePolicy::Strict
                && indices_outside_palette(samples, png.palette.len(), png.image.bit_depth))
        {
            return Err(Error::Invalid("palette index outside PLTE"));
        }
    }
    Ok(())
}

// Samples have already been unpacked or validated against the bit depth.
pub(crate) fn indices_outside_palette(samples: &[u8], palette: usize, depth: u8) -> bool {
    if palette >= 1usize << depth {
        return false;
    }
    let threshold = palette as u8;
    let (words, tail) = samples.as_chunks::<8>();
    const HIGH: u64 = 0x8080_8080_8080_8080;
    let bias = u64::from_ne_bytes([128 - (threshold & 127); 8]);
    let lower_half = u64::from_ne_bytes([!(threshold & 128); 8]);
    words.iter().any(|&bytes| {
        let word = u64::from_ne_bytes(bytes);
        // Low seven bits plus bias fit in each byte. The sum's high bit
        // reports a low-part match without carrying into adjacent bytes.
        let carry = (word & !HIGH) + bias;
        ((word & carry) | ((word | carry) & lower_half)) & HIGH != 0
    }) || tail.iter().any(|&value| value >= threshold)
}

pub(crate) fn write_chunk<const N: usize>(
    output: &mut Vec<u8>,
    kind: &[u8; 4],
    parts: [&[u8]; N],
) -> Result<(), Error> {
    let len = parts
        .iter()
        .try_fold(0usize, |len, data| len.checked_add(data.len()))
        .ok_or(Error::LimitExceeded)?;
    if len > 0x7fff_ffff {
        return Err(Error::LimitExceeded);
    }
    reserve(output, len + 12)?;
    output.extend_from_slice(&(len as u32).to_be_bytes());
    let start = output.len();
    output.extend_from_slice(kind);
    for part in parts {
        output.extend_from_slice(part);
    }
    output.extend_from_slice(&crc32(&[&output[start..]]).to_be_bytes());
    Ok(())
}

pub(crate) fn reserve_chunk_run(
    output: &mut Vec<u8>,
    payload: usize,
    chunks: usize,
    framing: usize,
) -> Result<(), Error> {
    if chunks > 1 {
        // Include room for the final IEND.
        let additional = chunks
            .checked_mul(framing)
            .and_then(|headers| headers.checked_add(payload))
            .and_then(|bytes| bytes.checked_add(12))
            .ok_or(Error::Allocation)?;
        reserve(output, additional)?;
    }
    Ok(())
}

fn write_ancillary(out: &mut Vec<u8>, png: &Png, position: ChunkPosition) -> Result<(), Error> {
    for c in png.ancillary.iter().filter(|c| c.position == position) {
        if ancillary_writable(c)? {
            write_chunk(
                out,
                &c.kind,
                [&crate::metadata::encoding_data(c, &png.image)],
            )?;
        }
    }
    Ok(())
}

pub(crate) fn ancillary_writable(c: &AncillaryChunk) -> Result<bool, Error> {
    if c.kind[0] & 32 == 0
        || c.kind[2] & 32 != 0
        || !chunk_type_valid(c.kind)
        || matches!(&c.kind, b"tRNS" | b"acTL" | b"fcTL" | b"fdAT")
    {
        return Err(Error::Invalid("ancillary chunk type"));
    }
    // Reencoding changes critical IDAT data (PNG 3, 14.2).
    Ok(c.kind[3] & 32 != 0 || crate::metadata::is_known(&c.kind))
}

pub(crate) fn validate_encoding(png: &Png, options: &EncodeOptions) -> Result<Header, Error> {
    png.image
        .validate()
        .map_err(|_| Error::Invalid("image samples"))?;
    if png.image.width > 0x7fff_ffff
        || png.image.height > 0x7fff_ffff
        || options.idat_chunk_size == 0
        || options.idat_chunk_size > 0x7fff_ffff
    {
        return Err(Error::Invalid("encoding dimensions or chunk size"));
    }
    validate_pixels(png, PalettePolicy::Strict)?;
    if png.palette.len() > 256
        || (!png.palette.is_empty()
            && matches!(
                png.image.color_type,
                ColorType::Grayscale | ColorType::GrayscaleAlpha
            ))
    {
        return Err(Error::Invalid("palette for color type"));
    }
    let header = Header {
        width: png.image.width,
        height: png.image.height,
        color: png.image.color_type,
        depth: png.image.bit_depth,
        interlaced: options.interlaced,
    };
    crate::metadata::validate_encoding(png, header)?;
    crate::apng::validate(png, header, PalettePolicy::Strict)?;
    match &png.transparency {
        None => (),
        Some(Transparency::Grayscale(_)) if header.color == ColorType::Grayscale => (),
        Some(Transparency::Rgb(_)) if header.color == ColorType::Rgb => (),
        Some(Transparency::Palette(v))
            if header.color == ColorType::Indexed && v.len() <= png.palette.len() => {}
        _ => return Err(Error::Invalid("transparency for color type")),
    }
    for c in &png.ancillary {
        ancillary_writable(c)?;
    }
    Ok(header)
}

pub(crate) fn write_transparency(
    png: &Png,
    write: impl FnOnce(&[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let mask = ((1u32 << png.image.bit_depth) - 1) as u16;
    match &png.transparency {
        Some(Transparency::Grayscale(v)) => write(&(v & mask).to_be_bytes()),
        Some(Transparency::Rgb(v)) => write(v.map(|v| (v & mask).to_be_bytes()).as_flattened()),
        Some(Transparency::Palette(v)) => write(v),
        None => Ok(()),
    }
}

pub fn encode(png: &Png, options: &EncodeOptions) -> Result<Vec<u8>, Error> {
    let header = validate_encoding(png, options)?;
    let mut out = SIGNATURE.to_vec();
    write_chunk(&mut out, b"IHDR", [&header.wire()])?;
    write_ancillary(&mut out, png, ChunkPosition::BeforePalette)?;
    if !png.palette.is_empty() {
        write_chunk(&mut out, b"PLTE", [png.palette.as_flattened()])?;
    }
    write_transparency(png, |data| write_chunk(&mut out, b"tRNS", [data]))?;
    write_ancillary(&mut out, png, ChunkPosition::BeforeData)?;
    crate::apng::write_control(&mut out, png)?;
    let mut compressor = crate::zlib::Compressor::new(options.compression);
    let mut rows = crate::stream::Rows::new(&png.image, options.interlaced, options.filter)?;
    let compressed =
        crate::stream::compress_image(&png.image, options, &mut compressor, &mut rows)?;
    let chunks = compressed.chunks(options.idat_chunk_size);
    reserve_chunk_run(&mut out, compressed.len(), chunks.len(), 12)?;
    for part in chunks {
        write_chunk(&mut out, b"IDAT", [part])?;
    }
    crate::apng::write_frames(&mut out, png, options, &mut compressor, &mut rows)?;
    write_ancillary(&mut out, png, ChunkPosition::AfterData)?;
    write_chunk(&mut out, b"IEND", [])?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{chunk_type_valid, indices_outside_palette};

    #[test]
    fn chunk_type_word_check_matches_ascii_in_every_lane() {
        for lane in 0..4 {
            for byte in 0..=u8::MAX {
                let mut kind = *b"ruSt";
                kind[lane] = byte;
                assert_eq!(chunk_type_valid(kind), byte.is_ascii_alphabetic());
            }
        }
    }

    #[test]
    fn palette_word_check_matches_scalar_in_every_lane_and_tail() {
        for depth in [1, 2, 4, 8] {
            let range = 1usize << depth;
            for palette in 0..=range {
                for value in 0..range {
                    let mut backing = [palette.saturating_sub(1) as u8; 18];
                    let samples = &mut backing[1..];
                    for lane in 0..samples.len() {
                        let previous = samples[lane];
                        samples[lane] = value as u8;
                        assert_eq!(
                            indices_outside_palette(samples, palette, depth),
                            samples.iter().any(|&sample| usize::from(sample) >= palette),
                            "depth {depth}, palette {palette}, value {value}, lane {lane}"
                        );
                        samples[lane] = previous;
                    }
                }
            }
        }
    }
}
