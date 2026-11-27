use alloc::vec::Vec;
use core::{fmt, ops::Range};

use crate::{DecodeError, Decoder, Frame, Picture};

/// Errors from the VP9 track reader or decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
            Self::NoVideo => f.write_str("no VP9 video in MP4"),
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

fn sample_descriptions(mut input: &[u8]) -> Result<Vec<bool>> {
    full_box(&mut input)?;
    let count = word(&mut input)? as usize;
    let mut supported = Vec::new();
    while let Some(entry) = next_box(&mut input)? {
        supported.push(entry.kind == b"vp09");
    }
    if supported.len() != count {
        return Err(Mp4Error::Invalid("sample description count"));
    }
    Ok(supported)
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
            return Err(Mp4Error::Invalid("empty VP9 sample"));
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
    descriptions: &[bool],
    push: &mut impl FnMut(&[u8]) -> Result<()>,
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
        if description.checked_sub(1).and_then(|i| descriptions.get(i)) != Some(&true) {
            return Err(Mp4Error::Unsupported("sample description"));
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
                push(&file[start..end])?;
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

#[derive(Clone, Copy)]
struct SampleDefaults {
    description: usize,
    size: usize,
}

fn fragment_defaults(movie: &[u8], track_id: u32) -> Result<SampleDefaults> {
    let mut extensions = required(movie, b"mvex", "movie extensions")?;
    while let Some(atom) = next_box(&mut extensions)? {
        if atom.kind != b"trex" {
            continue;
        }
        let mut entry = atom.body;
        full_box(&mut entry)?;
        if word(&mut entry)? == track_id {
            let description = word(&mut entry)? as usize;
            word(&mut entry)?; // duration
            let size = word(&mut entry)? as usize;
            word(&mut entry)?; // flags
            return Ok(SampleDefaults { description, size });
        }
    }
    Err(Mp4Error::Invalid("fragment track defaults"))
}

fn decode_fragments(
    file: &[u8],
    media: &[Range<usize>],
    track: u32,
    descriptions: &[bool],
    defaults: SampleDefaults,
    push: &mut impl FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let mut atoms = file;
    while !atoms.is_empty() {
        let offset = file.len() - atoms.len();
        let atom = next_box(&mut atoms)?.ok_or(Mp4Error::Truncated)?;
        if atom.kind != b"moof" {
            continue;
        }
        let mut fragments = atom.body;
        let mut first_track = true;
        while let Some(fragment) = next_box(&mut fragments)? {
            if fragment.kind != b"traf" {
                continue;
            }
            let mut header = required(fragment.body, b"tfhd", "track fragment header")?;
            let flags = word(&mut header)?;
            let id = word(&mut header)?;
            if id == track {
                if flags & !0x03003b != 0 {
                    return Err(Mp4Error::Unsupported("track fragment flags"));
                }
                let base = if flags & 1 != 0 {
                    usize::try_from(u64::from_be_bytes(*take(&mut header)?))
                        .map_err(|_| Mp4Error::Invalid("fragment base offset"))?
                } else if flags & 0x020000 != 0 || first_track {
                    offset
                } else {
                    return Err(Mp4Error::Unsupported("implicit fragment base offset"));
                };
                let description = if flags & 2 != 0 {
                    word(&mut header)? as usize
                } else {
                    defaults.description
                };
                if description.checked_sub(1).and_then(|i| descriptions.get(i)) != Some(&true) {
                    return Err(Mp4Error::Unsupported("sample description"));
                }
                if flags & 8 != 0 {
                    word(&mut header)?; // duration
                }
                let size = if flags & 0x10 != 0 {
                    word(&mut header)? as usize
                } else {
                    defaults.size
                };
                if flags & 0x20 != 0 {
                    word(&mut header)?; // sample flags
                }
                let mut offset = base;
                let mut runs = fragment.body;
                while let Some(run) = next_box(&mut runs)? {
                    if run.kind == b"trun" {
                        offset = decode_run(file, media, run.body, base, offset, size, push)?;
                    }
                }
            }
            first_track = false;
        }
    }
    Ok(())
}

fn decode_run(
    file: &[u8],
    media: &[Range<usize>],
    mut run: &[u8],
    base: usize,
    mut offset: usize,
    default_size: usize,
    push: &mut impl FnMut(&[u8]) -> Result<()>,
) -> Result<usize> {
    let flags = word(&mut run)?;
    if flags & !0x01000f05 != 0 {
        return Err(Mp4Error::Unsupported("track run flags"));
    }
    let count = word(&mut run)?;
    if flags & 1 != 0 {
        let relative = i32::from_be_bytes(*take(&mut run)?);
        offset = base
            .checked_add_signed(relative as isize)
            .ok_or(Mp4Error::Invalid("fragment data offset"))?;
    }
    if flags & 4 != 0 {
        word(&mut run)?; // first sample flags
    }
    if count == 0 {
        return Ok(offset);
    }
    let bound = media
        .iter()
        .find(|span| span.contains(&offset))
        .ok_or(Mp4Error::Invalid("fragment outside media data"))?
        .end;
    for _ in 0..count {
        if flags & 0x100 != 0 {
            word(&mut run)?; // duration
        }
        let size = if flags & 0x200 != 0 {
            word(&mut run)? as usize
        } else {
            default_size
        };
        if size == 0 {
            return Err(Mp4Error::Invalid("empty VP9 sample"));
        }
        if flags & 0x400 != 0 {
            word(&mut run)?; // flags
        }
        if flags & 0x800 != 0 {
            word(&mut run)?; // composition time offset
        }
        let end = offset
            .checked_add(size)
            .filter(|&end| end <= bound)
            .ok_or(Mp4Error::Truncated)?;
        push(&file[offset..end])?;
        offset = end;
    }
    Ok(offset)
}

fn track_id(track: &[u8]) -> Result<u32> {
    let header = required(track, b"tkhd", "track header")?;
    let offset = match *header.first().ok_or(Mp4Error::Truncated)? {
        0 => 12,
        1 => 20,
        _ => return Err(Mp4Error::Unsupported("track header version")),
    };
    word(&mut header.get(offset..).ok_or(Mp4Error::Truncated)?)
}

/// Decodes the first VP9 video track in an MP4.
///
/// Frames are emitted in VP9 display order. Audio and container timing are ignored.
pub fn decode_mp4(file: &[u8], mut output: impl FnMut(Frame)) -> Result<()> {
    decode_mp4_pictures(file, |picture| output(picture.to_owned_frame()))
}

/// Decodes the first VP9 video track in an MP4 with shared output planes.
pub fn decode_mp4_pictures(file: &[u8], mut output: impl FnMut(Picture)) -> Result<()> {
    let mut top = file;
    let mut movie = None;
    let mut media = Vec::new();
    let mut fragmented = false;
    while let Some(atom) = next_box(&mut top)? {
        match atom.kind {
            b"moov" if movie.is_none() => movie = Some(atom.body),
            b"mdat" => {
                let end = file.len() - top.len();
                media.push(end - atom.body.len()..end);
            }
            b"moof" => fragmented = true,
            _ => {}
        }
    }
    let movie = movie.ok_or(Mp4Error::Invalid("movie box"))?;
    let mut tracks = movie;
    while let Some(track) = next_box(&mut tracks)? {
        if track.kind != b"trak" {
            continue;
        }
        let media_box = required(track.body, b"mdia", "track media")?;
        let handler = required(media_box, b"hdlr", "media handler")?;
        if handler.get(8..12).ok_or(Mp4Error::Truncated)? != b"vide" {
            continue;
        }
        let info = required(media_box, b"minf", "media information")?;
        let table = required(info, b"stbl", "sample table")?;
        let descriptions = sample_descriptions(required(table, b"stsd", "sample descriptions")?)?;
        if descriptions.contains(&true) {
            let mut decoder = Decoder::new();
            let mut shown = false;
            let mut push = |sample: &[u8]| {
                if let Some(frame) = decoder.push_picture(sample).map_err(Mp4Error::Decode)? {
                    output(frame);
                    shown = true;
                }
                Ok(())
            };
            decode_samples(file, &media, table, &descriptions, &mut push)?;
            if fragmented {
                let id = track_id(track.body)?;
                let defaults = fragment_defaults(movie, id)?;
                decode_fragments(file, &media, id, &descriptions, defaults, &mut push)?;
            }
            return if shown {
                Ok(())
            } else {
                Err(Mp4Error::NoVideo)
            };
        }
    }
    Err(Mp4Error::NoVideo)
}
