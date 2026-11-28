//! Indexes the first video stream in a RIFF AVI without retaining audio or packet bodies.
use crate::packet::{Codec, MAX_SAMPLES, PACKET_BYTES, Packet, Track};
use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
    ops::Range,
};

struct Chunk {
    id: [u8; 4],
    body: Range<u64>,
}

impl Chunk {
    fn next(input: &mut BufReader<File>, remaining: &mut Range<u64>) -> io::Result<Option<Self>> {
        if remaining.is_empty() {
            return Ok(None);
        }
        if remaining.end - remaining.start < 8 {
            return Err(invalid("AVI chunk header"));
        }
        input.seek(SeekFrom::Start(remaining.start))?;
        let mut header = [0; 8];
        input.read_exact(&mut header)?;
        let size = u64::from(u32::from_le_bytes(header[4..].try_into().unwrap()));
        let start = remaining.start + 8;
        let end = start + size;
        let padded_end = end + (size & 1);
        if padded_end > remaining.end {
            return Err(invalid("AVI chunk size"));
        }
        remaining.start = padded_end;
        Ok(Some(Self {
            id: header[..4].try_into().unwrap(),
            body: start..end,
        }))
    }

    fn group(self, input: &mut BufReader<File>) -> io::Result<([u8; 4], Range<u64>)> {
        if self.body.end - self.body.start < 4 {
            return Err(invalid("AVI group type"));
        }
        let mut kind = [0; 4];
        input.read_exact(&mut kind)?;
        Ok((kind, self.body.start + 4..self.body.end))
    }

    fn read(self, input: &mut BufReader<File>) -> io::Result<Vec<u8>> {
        let len = self.body.end - self.body.start;
        if len > 1 << 20 {
            return Err(invalid("AVI stream header exceeds the allocation limit"));
        }
        let mut bytes = vec![0; len as usize];
        input.read_exact(&mut bytes)?;
        Ok(bytes)
    }
}

struct Stream {
    number: u8,
    codec: Codec,
    width: u32,
    height: u32,
    scale: u32,
    rate: u32,
    length: u32,
}

pub(crate) fn index(input: &mut BufReader<File>) -> io::Result<Option<Track>> {
    input.rewind()?;
    let mut signature = [0; 12];
    if input.read_exact(&mut signature).is_err()
        || &signature[..4] != b"RIFF"
        || &signature[8..] != b"AVI "
    {
        return Ok(None);
    }
    let mut file = 0..input.get_ref().metadata()?.len();
    let mut stream = None;
    let mut samples = Samples::default();
    while let Some(chunk) = Chunk::next(input, &mut file)? {
        if chunk.id != *b"RIFF" {
            continue;
        }
        let (kind, mut riff) = chunk.group(input)?;
        if !matches!(&kind, b"AVI " | b"AVIX") {
            continue;
        }
        while let Some(chunk) = Chunk::next(input, &mut riff)? {
            if chunk.id != *b"LIST" {
                continue;
            }
            let (kind, body) = chunk.group(input)?;
            match &kind {
                b"hdrl" if stream.is_none() => {
                    stream = find_video_stream(input, body)?;
                    if stream.is_none() {
                        return Ok(None);
                    }
                }
                b"movi" => {
                    if let Some(stream) = &stream {
                        let id = [b'0' + stream.number / 10, b'0' + stream.number % 10];
                        index_movie(input, body, id, &mut samples, true)?;
                    }
                }
                _ => {}
            }
        }
    }
    let Some(stream) = stream.filter(|_| !samples.packets.is_empty()) else {
        return Ok(None);
    };
    let packet_count = samples.packets.len() as u64;
    let decode_duration = packet_count * u64::from(stream.scale);
    let duration = if stream.length == 0 {
        samples.slots as u64 * u64::from(stream.scale)
    } else {
        u64::from(stream.length) * u64::from(stream.scale)
    };
    let first = samples.timestamps[0];
    for timestamp in &mut samples.timestamps {
        *timestamp = (*timestamp - first) * i64::from(stream.scale);
    }
    Ok(Some(Track {
        codec: stream.codec,
        width: stream.width,
        height: stream.height,
        packets: samples.packets,
        presentation_times: samples.timestamps,
        timescale: stream.rate,
        duration: duration.saturating_sub(first as u64 * u64::from(stream.scale)),
        reported_duration: Some(duration),
        decode_duration,
        color: None,
    }))
}

fn find_video_stream(
    input: &mut BufReader<File>,
    mut header: Range<u64>,
) -> io::Result<Option<Stream>> {
    let mut number = 0;
    while let Some(chunk) = Chunk::next(input, &mut header)? {
        if chunk.id != *b"LIST" {
            continue;
        }
        let (kind, mut fields) = chunk.group(input)?;
        if kind != *b"strl" {
            continue;
        }
        let (mut stream, mut format) = (Vec::new(), Vec::new());
        while let Some(chunk) = Chunk::next(input, &mut fields)? {
            match &chunk.id {
                b"strh" => stream = chunk.read(input)?,
                b"strf" => format = chunk.read(input)?,
                _ => {}
            }
        }
        if stream.starts_with(b"vids") {
            let bitmap_header = field(&format, 0)? as usize;
            let config = format
                .get(bitmap_header..)
                .filter(|_| bitmap_header >= 40)
                .ok_or_else(|| invalid("AVI bitmap header"))?;
            let tag = |bytes: &[u8], at: usize| field(bytes, at).map(u32::to_le_bytes);
            let Some(codec) = codec(tag(&format, 16)?, config)
                .or_else(|| tag(&stream, 4).ok().and_then(|tag| codec(tag, config)))
            else {
                return Ok(None);
            };
            let scale = field(&stream, 20)?;
            let rate = field(&stream, 24)?;
            if scale == 0 || rate == 0 {
                return Err(invalid("AVI stream clock"));
            }
            return Ok(Some(Stream {
                number,
                codec,
                width: field(&format, 4)?,
                height: (field(&format, 8)? as i32).unsigned_abs(),
                scale,
                rate,
                length: field(&stream, 32)?,
            }));
        }
        if number == 99 {
            break;
        }
        number += 1;
    }
    Ok(None)
}

fn codec(tag: [u8; 4], config: &[u8]) -> Option<Codec> {
    match &tag.map(|byte| byte.to_ascii_uppercase()) {
        b"H264" | b"X264" | b"AVC1" | b"DAVC" => Some(if config.first() == Some(&1) {
            Codec::Avc(config.to_vec())
        } else {
            Codec::AnnexBAvc(config.to_vec())
        }),
        b"MPG1" | b"MPEG" | b"PIM1" => Some(Codec::Mpeg1),
        b"XVID" | b"DIVX" | b"DX50" | b"FMP4" | b"MP4V" | b"M4S2" => Some(Codec::Mpeg4 {
            config: config.to_vec(),
            avi: true,
        }),
        b"MP42" | b"DIV2" => Some(Codec::MsMpeg4v2),
        _ => None,
    }
}

#[derive(Default)]
struct Samples {
    packets: Vec<Packet>,
    timestamps: Vec<i64>,
    slots: usize,
}

fn index_movie(
    input: &mut BufReader<File>,
    mut body: Range<u64>,
    id: [u8; 2],
    samples: &mut Samples,
    allow_records: bool,
) -> io::Result<()> {
    while let Some(chunk) = Chunk::next(input, &mut body)? {
        if chunk.id[..2] == id && matches!(&chunk.id[2..], b"dc" | b"db") {
            let size = chunk.body.end - chunk.body.start;
            if size > u64::from(PACKET_BYTES) || samples.slots >= MAX_SAMPLES {
                return Err(invalid("AVI video packets exceed the allocation limit"));
            }
            if size != 0 {
                samples.packets.push(Packet {
                    offset: chunk.body.start,
                    size: size as u32,
                });
                samples.timestamps.push(samples.slots as i64);
            }
            samples.slots += 1;
        } else if allow_records && chunk.id == *b"LIST" {
            let (kind, record) = chunk.group(input)?;
            if kind == *b"rec " {
                index_movie(input, record, id, samples, false)?;
            }
        }
    }
    Ok(())
}

fn field(bytes: &[u8], offset: usize) -> io::Result<u32> {
    let bytes = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid("AVI stream field"))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn invalid(description: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, description)
}
