use alloc::sync::Arc;

use crate::{units, Decoder, Error, Frame, Result};

struct Chunk<'a> {
    id: &'a [u8; 4],
    data: &'a [u8],
}

struct Chunks<'a>(&'a [u8]);

impl<'a> Chunks<'a> {
    fn next(&mut self) -> Result<Option<Chunk<'a>>> {
        if self.0.is_empty() {
            return Ok(None);
        }
        let (id, rest) = self.0.split_first_chunk::<4>().ok_or(Error::Truncated)?;
        let (size, body) = rest.split_first_chunk::<4>().ok_or(Error::Truncated)?;
        let size = u32::from_le_bytes(*size) as usize;
        let (data, trailing) = body.split_at_checked(size).ok_or(Error::Truncated)?;
        self.0 = trailing.get(size & 1..).ok_or(Error::Truncated)?;
        Ok(Some(Chunk { id, data }))
    }
}

impl<'a> Chunk<'a> {
    fn group(&self, id: &[u8; 4]) -> Option<&'a [u8]> {
        self.data.strip_prefix(id)
    }
}

struct Stream<'a> {
    number: u8,
    configuration: &'a [u8],
}

/// Decodes the first MPEG-4 Visual video track in a RIFF AVI file.
pub fn decode_avi(bytes: &[u8], mut output: impl FnMut(Arc<Frame>)) -> Result<()> {
    let mut file = Chunks(bytes);
    let first = file.next()?.ok_or(Error::Truncated)?;
    let root = first
        .group(b"AVI ")
        .filter(|_| first.id == b"RIFF")
        .ok_or(Error::Invalid("AVI RIFF header"))?;
    let mut stream = None;
    let mut decoder = Decoder::new();
    let mut found = false;
    let mut emit = |frame| {
        output(frame);
        found = true;
    };
    let mut children = Chunks(root);
    while let Some(chunk) = children.next()? {
        if chunk.id != b"LIST" {
            continue;
        }
        if let Some(header) = chunk.group(b"hdrl") {
            if stream.is_none() {
                let info = find_stream(header)?;
                decode_packet(&mut decoder, info.configuration, &mut emit)?;
                stream = Some(info);
            }
        } else if let Some(movie) = chunk.group(b"movi") {
            let info = stream.as_ref().ok_or(Error::Invalid("AVI stream list"))?;
            decode_movie(movie, info.number, &mut decoder, &mut emit)?;
        }
    }
    while let Some(chunk) = file.next()? {
        if chunk.id == b"RIFF" {
            if let (Some(body), Some(info)) = (chunk.group(b"AVIX"), &stream) {
                let mut children = Chunks(body);
                while let Some(child) = children.next()? {
                    if child.id == b"LIST" {
                        if let Some(movie) = child.group(b"movi") {
                            decode_movie(movie, info.number, &mut decoder, &mut emit)?;
                        }
                    }
                }
            }
        }
    }
    if !found {
        return Err(Error::NoVideo);
    }
    Ok(())
}

fn find_stream(header: &[u8]) -> Result<Stream<'_>> {
    let mut lists = Chunks(header);
    let mut number = 0;
    while let Some(chunk) = lists.next()? {
        let Some(body) = chunk.group(b"strl").filter(|_| chunk.id == b"LIST") else {
            continue;
        };
        let mut fields = Chunks(body);
        let (mut stream, mut format) = (&[][..], &[][..]);
        while let Some(field) = fields.next()? {
            match field.id {
                b"strh" => stream = field.data,
                b"strf" => format = field.data,
                _ => {}
            }
        }
        if stream.starts_with(b"vids")
            && (mpeg4_tag(stream.get(4..8)) || mpeg4_tag(format.get(16..20)))
        {
            let size =
                u32::from_le_bytes(*format.first_chunk::<4>().ok_or(Error::Truncated)?) as usize;
            if size < 40 {
                return Err(Error::Invalid("AVI bitmap header size"));
            }
            let configuration = format.get(size..).ok_or(Error::Truncated)?;
            return Ok(Stream {
                number,
                configuration,
            });
        }
        if number == 99 {
            break;
        }
        number += 1;
    }
    Err(Error::Unsupported("AVI video codec"))
}

fn mpeg4_tag(tag: Option<&[u8]>) -> bool {
    let Some(tag) = tag.and_then(|tag| tag.first_chunk::<4>()) else {
        return false;
    };
    matches!(
        &tag.map(|byte| byte.to_ascii_uppercase()),
        b"XVID" | b"DIVX" | b"DX50" | b"FMP4" | b"MP4V" | b"M4S2"
    )
}

fn decode_movie(
    movie: &[u8],
    number: u8,
    decoder: &mut Decoder,
    output: &mut impl FnMut(Arc<Frame>),
) -> Result<()> {
    let id = [b'0' + number / 10, b'0' + number % 10];
    let mut chunks = Chunks(movie);
    while let Some(chunk) = chunks.next()? {
        if chunk.id == b"LIST" {
            if let Some(record) = chunk.group(b"rec ") {
                let mut record = Chunks(record);
                while let Some(chunk) = record.next()? {
                    decode_video_chunk(&chunk, id, decoder, output)?;
                }
            }
        } else {
            decode_video_chunk(&chunk, id, decoder, output)?;
        }
    }
    Ok(())
}

fn decode_video_chunk(
    chunk: &Chunk<'_>,
    track: [u8; 2],
    decoder: &mut Decoder,
    output: &mut impl FnMut(Arc<Frame>),
) -> Result<()> {
    if chunk.id[..2] == track && matches!(&chunk.id[2..], b"dc" | b"db") {
        decode_packet(decoder, chunk.data, output)?;
    }
    Ok(())
}

fn decode_packet(
    decoder: &mut Decoder,
    bytes: &[u8],
    output: &mut impl FnMut(Arc<Frame>),
) -> Result<()> {
    for unit in units(bytes) {
        if let Some(frame) = decoder.push(unit)? {
            output(frame);
        }
    }
    Ok(())
}
