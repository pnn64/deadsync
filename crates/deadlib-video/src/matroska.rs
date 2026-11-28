//! Reads Matroska video tracks and indexes their block payloads in file order.
use crate::packet::{Codec, MAX_SAMPLES, PACKET_BYTES, Packet, Track};
use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
    ops::Range,
};

const SEGMENT: u64 = 0x1853_8067;
const INFO: u64 = 0x1549_a966;
const TRACKS: u64 = 0x1654_ae6b;
const CLUSTER: u64 = 0x1f43_b675;

struct Element {
    id: u64,
    offset: u64,
    body: Range<u64>,
}

impl Element {
    fn next(input: &mut BufReader<File>, remaining: &mut Range<u64>) -> io::Result<Option<Self>> {
        if remaining.is_empty() {
            return Ok(None);
        }
        let offset = remaining.start;
        input.seek(SeekFrom::Start(offset))?;
        let (id, id_width, _) = vint(input, true)?;
        let (size, size_width, unknown) = vint(input, false)?;
        let start = remaining.start + u64::from(id_width + size_width);
        let size = if unknown && id == SEGMENT {
            remaining.end.saturating_sub(start)
        } else if unknown {
            return Err(invalid("unknown-sized Matroska element"));
        } else {
            size
        };
        let end = start
            .checked_add(size)
            .filter(|&end| end <= remaining.end)
            .ok_or_else(|| invalid("Matroska element size"))?;
        remaining.start = end;
        Ok(Some(Self {
            id,
            offset,
            body: start..end,
        }))
    }

    fn uint(&self, input: &mut BufReader<File>) -> io::Result<u64> {
        Ok(u64::from_be_bytes(self.number(input)?))
    }

    fn number(&self, input: &mut BufReader<File>) -> io::Result<[u8; 8]> {
        let width = self.body.end - self.body.start;
        if width > 8 {
            return Err(invalid("Matroska number size"));
        }
        let mut bytes = [0; 8];
        input.read_exact(&mut bytes[8 - width as usize..])?;
        Ok(bytes)
    }

    fn bytes(self, input: &mut BufReader<File>) -> io::Result<Vec<u8>> {
        let size = self.body.end - self.body.start;
        if size > 1 << 20 {
            return Err(invalid("Matroska track field exceeds the allocation limit"));
        }
        let mut data = vec![0; size as usize];
        input.read_exact(&mut data)?;
        Ok(data)
    }
}

#[derive(Default)]
struct Stream {
    number: u64,
    kind: u64,
    name: Vec<u8>,
    config: Vec<u8>,
    width: u32,
    height: u32,
    frame_ns: u64,
    encoded: bool,
}

pub(crate) fn index(input: &mut BufReader<File>) -> io::Result<Option<Track>> {
    input.rewind()?;
    let mut signature = [0; 4];
    if input.read_exact(&mut signature).is_err() || signature != 0x1a45_dfa3u32.to_be_bytes() {
        return Ok(None);
    }
    let mut top = 0..input.get_ref().metadata()?.len();
    while let Some(element) = Element::next(input, &mut top)? {
        if element.id == SEGMENT {
            return segment(input, element.body);
        }
    }
    Ok(None)
}

fn segment(input: &mut BufReader<File>, mut body: Range<u64>) -> io::Result<Option<Track>> {
    let mut stream = None;
    let mut clock = (1_000_000, None);
    let mut first_cluster = None;
    let end = body.end;
    while let Some(element) = Element::next(input, &mut body)? {
        match element.id {
            INFO => clock = clock_info(input, element.body)?,
            TRACKS => stream = video_stream(input, element.body)?,
            CLUSTER => {
                first_cluster = Some(first_cluster.unwrap_or(element.offset));
                if stream.is_some() {
                    // Revisit the first cluster so streams with Tracks late in the segment work.
                    break;
                }
            }
            _ => {}
        }
        if first_cluster.is_some() && stream.is_some() {
            break;
        }
    }
    let (Some(stream), Some(first_cluster)) = (stream, first_cluster) else {
        return Ok(None);
    };
    let codec = match stream.name.as_slice() {
        b"V_MPEG4/ISO/AVC" if !stream.encoded => Codec::Avc(stream.config),
        b"V_MPEGH/ISO/HEVC" if !stream.encoded => Codec::Hevc(stream.config),
        b"V_VP9" if !stream.encoded => Codec::Vp9,
        _ => return Ok(None),
    };

    // The enclosing segment cursor records the first cluster's element header.
    let mut clusters = first_cluster..end;
    let mut packets = Vec::new();
    let mut timestamps = Vec::new();
    while let Some(element) = Element::next(input, &mut clusters)? {
        if element.id == CLUSTER {
            index_cluster(
                input,
                element.body,
                stream.number,
                clock.0,
                &mut packets,
                &mut timestamps,
            )?;
        }
    }
    if packets.is_empty() {
        return Ok(None);
    }
    timestamps.sort_unstable();
    let first = timestamps[0];
    let length = timestamps
        .last()
        .unwrap()
        .checked_sub(first)
        .ok_or_else(|| invalid("Matroska timestamp span"))? as u64;
    let frame_ns = if stream.frame_ns != 0 {
        stream.frame_ns
    } else if timestamps.len() > 1 {
        length / (timestamps.len() - 1) as u64
    } else {
        1_000_000_000 / 30
    };
    for timestamp in &mut timestamps {
        *timestamp -= first;
    }
    Ok(Some(Track {
        codec,
        width: stream.width,
        height: stream.height,
        presentation_times: timestamps,
        timescale: 1_000_000_000,
        duration: length.saturating_add(frame_ns),
        reported_duration: clock.1,
        decode_duration: frame_ns.saturating_mul(packets.len() as u64),
        packets,
        color: None,
    }))
}

fn clock_info(input: &mut BufReader<File>, mut body: Range<u64>) -> io::Result<(u64, Option<u64>)> {
    let mut scale = 1_000_000;
    let mut duration = None;
    while let Some(element) = Element::next(input, &mut body)? {
        match element.id {
            0x2a_d7b1 => scale = element.uint(input)?,
            0x4489 => {
                let width = element.body.end - element.body.start;
                let number = element.number(input)?;
                duration = match width {
                    4 => Some(f64::from(f32::from_be_bytes(
                        number[4..].try_into().unwrap(),
                    ))),
                    8 => Some(f64::from_be_bytes(number)),
                    _ => return Err(invalid("Matroska duration size")),
                };
            }
            _ => {}
        }
    }
    if scale == 0 {
        return Err(invalid("Matroska clock scale"));
    }
    Ok((
        scale,
        duration
            .filter(|d| d.is_finite() && *d > 0.0)
            .map(|d| (d * scale as f64).round() as u64),
    ))
}

fn video_stream(input: &mut BufReader<File>, mut body: Range<u64>) -> io::Result<Option<Stream>> {
    while let Some(entry) = Element::next(input, &mut body)? {
        if entry.id != 0xae {
            continue;
        }
        let mut stream = Stream::default();
        let mut fields = entry.body;
        while let Some(element) = Element::next(input, &mut fields)? {
            match element.id {
                0xd7 => stream.number = element.uint(input)?,
                0x83 => stream.kind = element.uint(input)?,
                0x86 => stream.name = element.bytes(input)?,
                0x63a2 => stream.config = element.bytes(input)?,
                0x23_e383 => stream.frame_ns = element.uint(input)?,
                0x6d80 => stream.encoded = true,
                0xe0 => {
                    let mut video = element.body;
                    while let Some(field) = Element::next(input, &mut video)? {
                        match field.id {
                            0xb0 => {
                                stream.width = u32::try_from(field.uint(input)?)
                                    .map_err(|_| invalid("Matroska width"))?
                            }
                            0xba => {
                                stream.height = u32::try_from(field.uint(input)?)
                                    .map_err(|_| invalid("Matroska height"))?
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        if stream.kind == 1 && stream.number != 0 {
            return Ok(Some(stream));
        }
    }
    Ok(None)
}

fn index_cluster(
    input: &mut BufReader<File>,
    mut body: Range<u64>,
    track: u64,
    scale: u64,
    packets: &mut Vec<Packet>,
    timestamps: &mut Vec<i64>,
) -> io::Result<()> {
    let original = body.clone();
    let mut time = None;
    while let Some(element) = Element::next(input, &mut body)? {
        if element.id == 0xe7 {
            time = Some(element.uint(input)?);
            break;
        }
    }
    let time = time.ok_or_else(|| invalid("missing Matroska cluster timestamp"))?;
    let mut body = original;
    while let Some(element) = Element::next(input, &mut body)? {
        let block = match element.id {
            0xa3 => Some(element),
            0xa0 => {
                let mut group = element.body;
                let mut block = None;
                while let Some(field) = Element::next(input, &mut group)? {
                    if field.id == 0xa1 {
                        block = Some(field);
                        break;
                    }
                }
                block
            }
            _ => None,
        };
        let Some(block) = block else { continue };
        let (number, width, _) = vint(input, false)?;
        if number != track {
            continue;
        }
        let offset = block.body.start + u64::from(width) + 3;
        if offset >= block.body.end {
            return Err(invalid("Matroska video block header"));
        }
        let mut header = [0; 3];
        input.read_exact(&mut header)?;
        if header[2] & 6 != 0 {
            return Err(invalid("laced Matroska video blocks"));
        }
        let size = block.body.end - offset;
        if size > u64::from(PACKET_BYTES) || packets.len() >= MAX_SAMPLES {
            return Err(invalid(
                "Matroska video packets exceed the allocation limit",
            ));
        }
        let relative = i128::from(i16::from_be_bytes([header[0], header[1]]));
        let pts = (i128::from(time) + relative)
            .checked_mul(i128::from(scale))
            .and_then(|value| i64::try_from(value).ok())
            .ok_or_else(|| invalid("Matroska timestamp overflow"))?;
        packets.push(Packet {
            offset,
            size: size as u32,
        });
        timestamps.push(pts);
    }
    Ok(())
}

fn vint(input: &mut BufReader<File>, id: bool) -> io::Result<(u64, u8, bool)> {
    let mut head = [0];
    input.read_exact(&mut head)?;
    let width = head[0].leading_zeros() + 1;
    if width > if id { 4 } else { 8 } {
        return Err(invalid("Matroska variable integer width"));
    }
    let mut bytes = [0; 8];
    let start = 8 - width as usize;
    bytes[start] = if id {
        head[0]
    } else {
        head[0] ^ (0x80 >> (width - 1))
    };
    input.read_exact(&mut bytes[start + 1..])?;
    let value = u64::from_be_bytes(bytes);
    Ok((value, width as u8, !id && value == (1 << (7 * width)) - 1))
}

fn invalid(description: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, description)
}
