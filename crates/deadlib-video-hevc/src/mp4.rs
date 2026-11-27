//! Indexed HEVC MP4 sample tables, adapted from the Deadlib VP9 reader.

use core::{fmt, ops::Range};

use crate::{DecodeError, Decoder, Frame, Picture};

/// Errors from the HEVC track reader or decoder.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Mp4Error {
    Truncated,
    Invalid(&'static str),
    Unsupported(&'static str),
    NoVideo,
    Decode(DecodeError),
}

impl fmt::Display for Mp4Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated MP4 data"),
            Self::Invalid(field) => write!(f, "invalid MP4 {field}"),
            Self::Unsupported(feature) => write!(f, "unsupported MP4 {feature}"),
            Self::NoVideo => f.write_str("no HEVC video in MP4"),
            Self::Decode(error) => error.fmt(f),
        }
    }
}

impl core::error::Error for Mp4Error {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}

type Result<T> = core::result::Result<T, Mp4Error>;

struct Mp4Box<'a> {
    kind: &'a [u8; 4],
    body: &'a [u8],
}

fn next_box<'a>(input: &mut &'a [u8]) -> Result<Option<Mp4Box<'a>>> {
    if input.is_empty() {
        return Ok(None);
    }
    let file = *input;
    let size = take::<4>(input)?;
    let kind = take::<4>(input)?;
    let size = match u32::from_be_bytes(*size) {
        0 => file.len(),
        1 => usize::try_from(u64::from_be_bytes(*take::<8>(input)?))
            .map_err(|_| Mp4Error::Truncated)?,
        size => size as usize,
    };
    let header_len = file.len() - input.len();
    let body_len = size
        .checked_sub(header_len)
        .ok_or(Mp4Error::Invalid("box size"))?;
    let (body, rest) = input
        .split_at_checked(body_len)
        .ok_or(Mp4Error::Truncated)?;
    *input = rest;
    Ok(Some(Mp4Box { kind, body }))
}

fn take<'a, const N: usize>(input: &mut &'a [u8]) -> Result<&'a [u8; N]> {
    let (front, rest) = input.split_first_chunk::<N>().ok_or(Mp4Error::Truncated)?;
    *input = rest;
    Ok(front)
}

fn word(input: &mut &[u8]) -> Result<u32> {
    Ok(u32::from_be_bytes(*take(input)?))
}

fn find_box<'a>(mut input: &'a [u8], kind: &[u8; 4]) -> Result<Option<&'a [u8]>> {
    while let Some(atom) = next_box(&mut input)? {
        if atom.kind == kind {
            return Ok(Some(atom.body));
        }
    }
    Ok(None)
}

fn full_box(input: &mut &[u8]) -> Result<()> {
    if word(input)? != 0 {
        return Err(Mp4Error::Unsupported("sample table version or flags"));
    }
    Ok(())
}

fn entries<const N: usize>(mut input: &[u8]) -> Result<&[[u8; N]]> {
    full_box(&mut input)?;
    let count = word(&mut input)? as usize;
    let (entries, tail) = input.as_chunks::<N>();
    if entries.len() != count || !tail.is_empty() {
        return Err(Mp4Error::Invalid("sample table length"));
    }
    Ok(entries)
}

struct SampleSizes<'a> {
    fixed: usize,
    count: usize,
    sizes: &'a [[u8; 4]],
}

impl<'a> SampleSizes<'a> {
    fn parse(mut input: &'a [u8]) -> Result<Self> {
        full_box(&mut input)?;
        let fixed = word(&mut input)? as usize;
        let count = word(&mut input)? as usize;
        let (sizes, tail) = input.as_chunks::<4>();
        let entries = if fixed == 0 { count } else { 0 };
        if sizes.len() != entries || !tail.is_empty() {
            return Err(Mp4Error::Invalid("sample size table"));
        }
        Ok(Self {
            fixed,
            count,
            sizes,
        })
    }

    fn get(&self, index: usize) -> Result<usize> {
        if index >= self.count {
            return Err(Mp4Error::Invalid("sample count"));
        }
        let size = self
            .sizes
            .get(index)
            .map_or(self.fixed, |size| u32::from_be_bytes(*size) as usize);
        if size == 0 {
            return Err(Mp4Error::Invalid("empty HEVC sample"));
        }
        Ok(size)
    }
}

enum Offsets<'a> {
    Narrow(&'a [[u8; 4]]),
    Wide(&'a [[u8; 8]]),
}

impl Offsets<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Narrow(values) => values.len(),
            Self::Wide(values) => values.len(),
        }
    }

    fn get(&self, index: usize) -> Result<usize> {
        let offset = match self {
            Self::Narrow(values) => u64::from(u32::from_be_bytes(values[index])),
            Self::Wide(values) => u64::from_be_bytes(values[index]),
        };
        usize::try_from(offset).map_err(|_| Mp4Error::Invalid("chunk offset"))
    }
}

fn required<'a>(input: &'a [u8], kind: &[u8; 4], field: &'static str) -> Result<&'a [u8]> {
    find_box(input, kind)?.ok_or(Mp4Error::Invalid(field))
}

fn decode_samples(
    file: &[u8],
    media: &[Range<usize>],
    table: &[u8],
    description_count: usize,
    push: &mut impl FnMut(usize, &[u8]) -> Result<()>,
) -> Result<()> {
    let chunk_map = entries::<12>(required(table, b"stsc", "sample-to-chunk table")?)?;
    let sizes = SampleSizes::parse(required(table, b"stsz", "sample size table")?)?;
    let offsets = if let Some(input) = find_box(table, b"stco")? {
        Offsets::Narrow(entries(input)?)
    } else {
        Offsets::Wide(entries(required(table, b"co64", "chunk offset table")?)?)
    };
    let mut chunk = 0;
    let mut sample = 0;
    for (i, entry) in chunk_map.iter().enumerate() {
        let mut entry = &entry[..];
        let first = word(&mut entry)? as usize;
        let per_chunk = word(&mut entry)? as usize;
        let description = word(&mut entry)? as usize;
        let end = if let Some(next) = chunk_map.get(i + 1) {
            (word(&mut &next[..])? as usize)
                .checked_sub(1)
                .ok_or(Mp4Error::Invalid("sample-to-chunk table"))?
        } else {
            offsets.len()
        };
        if first.checked_sub(1) != Some(chunk)
            || end <= chunk
            || end > offsets.len()
            || per_chunk == 0
        {
            return Err(Mp4Error::Invalid("sample-to-chunk table"));
        }
        if description == 0 || description > description_count {
            return Err(Mp4Error::Invalid("sample description index"));
        }
        for c in chunk..end {
            let mut start = offsets.get(c)?;
            let bound = media
                .iter()
                .find(|span| span.contains(&start))
                .ok_or(Mp4Error::Invalid("chunk outside media data"))?
                .end;
            for _ in 0..per_chunk {
                let end = start
                    .checked_add(sizes.get(sample)?)
                    .filter(|&end| end <= bound)
                    .ok_or(Mp4Error::Truncated)?;
                push(description - 1, &file[start..end])?;
                start = end;
                sample += 1;
            }
        }
        chunk = end;
    }
    if sample != sizes.count || chunk != offsets.len() {
        return Err(Mp4Error::Invalid("sample count"));
    }
    Ok(())
}

fn sample_descriptions(input: &[u8]) -> Result<Vec<Option<&[u8]>>> {
    let mut input = input;
    full_box(&mut input)?;
    let count = word(&mut input)? as usize;
    let mut descriptions = Vec::new();
    while let Some(entry) = next_box(&mut input)? {
        let config = if matches!(entry.kind, b"hvc1" | b"hev1") {
            // A VisualSampleEntry has 78 bytes before its child boxes.
            let children = entry.body.get(78..).ok_or(Mp4Error::Truncated)?;
            Some(required(children, b"hvcC", "HEVC configuration box")?)
        } else {
            None
        };
        descriptions.push(config);
    }
    if descriptions.len() != count {
        return Err(Mp4Error::Invalid("sample description count"));
    }
    Ok(descriptions)
}

/// Decodes the first HEVC video track in a complete indexed MP4.
///
/// Frames are emitted in display order. Audio, timestamps and edit lists
/// are ignored. HEVC configuration is read from the track's `hvcC` box.
pub fn decode_mp4(file: &[u8], mut output: impl FnMut(Frame)) -> Result<()> {
    decode_mp4_pictures(file, |picture| output(picture.to_owned_frame()))
}

/// Decodes the first indexed HEVC MP4 track and emits shared pictures in display order.
pub fn decode_mp4_pictures(file: &[u8], mut output: impl FnMut(Picture)) -> Result<()> {
    let mut top = file;
    let mut movie = None;
    let mut media = Vec::new();
    while let Some(atom) = next_box(&mut top)? {
        match atom.kind {
            b"moov" if movie.is_none() => movie = Some(atom.body),
            b"mdat" => {
                let end = file.len() - top.len();
                media.push(end - atom.body.len()..end);
            }
            b"moof" => return Err(Mp4Error::Unsupported("movie fragments")),
            _ => {}
        }
    }

    let mut tracks = movie.ok_or(Mp4Error::Invalid("movie box"))?;
    while let Some(track) = next_box(&mut tracks)? {
        if track.kind != b"trak" {
            continue;
        }
        let track_media = required(track.body, b"mdia", "track media")?;
        let handler = required(track_media, b"hdlr", "media handler")?;
        if handler.get(8..12).ok_or(Mp4Error::Truncated)? != b"vide" {
            continue;
        }
        let info = required(track_media, b"minf", "media information")?;
        let table = required(info, b"stbl", "sample table")?;
        let configs = sample_descriptions(required(table, b"stsd", "sample descriptions")?)?;
        if configs.iter().all(Option::is_none) {
            continue;
        }

        let mut decoder = Decoder::new().with_display_order();
        let mut active_description = None;
        let mut length_size = 0;
        decode_samples(file, &media, table, configs.len(), &mut |index, sample| {
            if active_description != Some(index) {
                let config = configs[index].ok_or(Mp4Error::Unsupported("sample description"))?;
                length_size = decoder
                    .push_decoder_config(config)
                    .map_err(Mp4Error::Decode)?;
                active_description = Some(index);
            }
            decoder
                .push_packet_pictures(sample, length_size, &mut output)
                .map_err(Mp4Error::Decode)
        })?;
        while let Some(frame) = decoder.flush_picture().map_err(Mp4Error::Decode)? {
            output(frame);
        }
        return Ok(());
    }
    Err(Mp4Error::NoVideo)
}
