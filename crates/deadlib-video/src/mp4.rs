//! A bounded index for indexed MP4 and QuickTime video tracks. Packet bytes
//! remain in the file and are read into one reusable buffer by the decoder.
use crate::packet::{Codec, Color, MAX_SAMPLES, PACKET_BYTES, Packet, Track};
use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
    ops::Range,
};

const TABLE_BYTES: usize = 64 << 20;

/// Returns None for a container or track layout owned by another reader.
pub(crate) fn index(input: &mut BufReader<File>) -> io::Result<Option<Track>> {
    let file_size = input.get_ref().metadata()?.len();
    let mut offset = 0;
    let mut movie = None;
    let mut media = Vec::new();
    let mut fragments = Vec::new();
    while offset < file_size {
        input.seek(SeekFrom::Start(offset))?;
        let mut base = [0; 8];
        input.read_exact(&mut base)?;
        let kind = &base[4..];
        if offset == 0
            && !matches!(
                kind,
                b"ftyp" | b"moov" | b"mdat" | b"wide" | b"free" | b"skip"
            )
        {
            return Ok(None);
        }
        let (size, header) = match u32_at(&base, 0)? {
            0 => (file_size - offset, 8),
            1 => {
                let mut wide = [0; 8];
                input.read_exact(&mut wide)?;
                (u64::from_be_bytes(wide), 16)
            }
            size => (u64::from(size), 8),
        };
        let end = offset
            .checked_add(size)
            .filter(|&end| size >= header && end <= file_size)
            .ok_or_else(|| invalid("top-level box size"))?;
        match kind {
            b"mdat" => {
                if media.len() == 4096 {
                    return Err(invalid("too many media boxes"));
                }
                media.push(offset + header..end);
            }
            b"moov" if movie.is_none() => {
                let count = usize::try_from(size - header)
                    .ok()
                    .filter(|&count| count <= TABLE_BYTES)
                    .ok_or_else(|| invalid("movie table exceeds the allocation limit"))?;
                let mut bytes = vec![0; count];
                input.read_exact(&mut bytes)?;
                movie = Some(bytes);
            }
            b"moof" => {
                if fragments.len() == 65_536 || size - header > TABLE_BYTES as u64 {
                    return Err(invalid("movie fragment index exceeds the limit"));
                }
                fragments.push(Fragment {
                    offset,
                    body: offset + header..end,
                });
            }
            _ => {}
        }
        offset = end;
    }

    let Some(movie) = movie.as_deref() else {
        return Ok(None);
    };
    let mut tracks = movie;
    while let Some((kind, track)) = next_box(&mut tracks)? {
        if kind != *b"trak" {
            continue;
        }
        let mdia = required(track, b"mdia")?;
        if required(mdia, b"hdlr")?.get(8..12) != Some(b"vide") {
            continue;
        }
        return video_track(input, movie, track, mdia, &media, &fragments);
    }
    Ok(None)
}

fn video_track(
    input: &mut BufReader<File>,
    movie: &[u8],
    track: &[u8],
    mdia: &[u8],
    media: &[Range<u64>],
    fragments: &[Fragment],
) -> io::Result<Option<Track>> {
    let table = required(required(mdia, b"minf")?, b"stbl")?;
    let descriptions = required(table, b"stsd")?;
    if u32_at(descriptions, 4)? != 1 {
        return Ok(None);
    }
    let mut entries = &descriptions[8..];
    let Some((kind, visual)) = next_box(&mut entries)? else {
        return Err(invalid("missing video sample description"));
    };
    let children = visual
        .get(78..)
        .ok_or_else(|| invalid("visual sample entry"))?;
    let codec = match &kind {
        b"avc1" | b"avc3" => Codec::Avc(required(children, b"avcC")?.to_vec()),
        b"hvc1" | b"hev1" => Codec::Hevc(required(children, b"hvcC")?.to_vec()),
        b"vp09" => Codec::Vp9,
        b"mp4v" => Codec::Mpeg4 {
            config: mpeg4_config(required(children, b"esds")?)?.to_vec(),
            avi: false,
        },
        _ => return Ok(None),
    };
    let width = u32::from(u16::from_be_bytes(bytes_at(visual, 24)?));
    let height = u32::from(u16::from_be_bytes(bytes_at(visual, 26)?));
    let color = match find(children, b"colr")? {
        Some(color) if matches!(color.get(..4), Some(b"nclx" | b"nclc")) => Some(Color {
            matrix: u16::from_be_bytes(bytes_at(color, 8)?),
            full_range: color.get(10).map(|&flags| flags & 128 != 0),
        }),
        _ => None,
    };
    let header = required(mdia, b"mdhd")?;
    let (timescale, media_duration) = header_time(header)?;
    let (duration, origin) = match find(track, b"edts")? {
        Some(edts) => match simple_edit(movie, required(edts, b"elst")?, timescale)? {
            Some((edited_duration, origin)) if media_duration != 0 => {
                (edited_duration.min(media_duration), origin)
            }
            Some(edit) => edit,
            None => return Ok(None),
        },
        None => (media_duration, 0),
    };
    let (packets, mut pts, end_dts, decode_duration) = if fragments.is_empty() {
        let packets = video_packets(table, media)?;
        let (pts, dts) = indexed_times(table, packets.len())?;
        (packets, pts, dts, dts as u64)
    } else {
        if u32_at(required(table, b"stsz")?, 8)? != 0 {
            return Ok(None);
        }
        let samples = fragment_samples(input, movie, track, media, fragments)?;
        (
            samples.packets,
            samples.pts,
            samples.dts,
            samples.decode_duration,
        )
    };
    for time in &mut pts {
        *time = time
            .checked_sub(origin)
            .ok_or_else(|| invalid("edit timestamp overflow"))?;
    }
    pts.sort_unstable();
    Ok(Some(Track {
        codec,
        width,
        height,
        packets,
        presentation_times: pts,
        timescale,
        duration: if duration == 0 {
            end_dts as u64
        } else {
            duration
        },
        reported_duration: None,
        decode_duration,
        color,
    }))
}

fn mpeg4_config(bytes: &[u8]) -> io::Result<&[u8]> {
    fn descriptor<'a>(cursor: &mut &'a [u8], expected: u8) -> io::Result<&'a [u8]> {
        let (&tag, body) = cursor
            .split_first()
            .ok_or_else(|| invalid("ES descriptor"))?;
        if tag != expected {
            return Err(invalid("unexpected ES descriptor tag"));
        }
        let mut size = 0usize;
        for (index, &byte) in body.iter().take(4).enumerate() {
            size = (size << 7) | usize::from(byte & 127);
            if byte < 128 {
                let (data, next) = body[index + 1..]
                    .split_at_checked(size)
                    .ok_or_else(|| invalid("ES descriptor size"))?;
                *cursor = next;
                return Ok(data);
            }
        }
        Err(invalid("ES descriptor length"))
    }

    let mut fields = bytes.get(4..).ok_or_else(|| invalid("esds version"))?;
    let es = descriptor(&mut fields, 3)?;
    let flags = *es.get(2).ok_or_else(|| invalid("ES flags"))?;
    let mut at = 3 + 2 * usize::from(flags & 128 != 0);
    if flags & 64 != 0 {
        let size = *es.get(at).ok_or_else(|| invalid("ES URL size"))?;
        at += 1 + usize::from(size);
    }
    at += 2 * usize::from(flags & 32 != 0);
    let decoder = descriptor(&mut es.get(at..).ok_or_else(|| invalid("ES header"))?, 4)?;
    if decoder.first() != Some(&0x20) {
        return Err(invalid("unsupported MPEG-4 Visual object type"));
    }
    descriptor(
        &mut decoder
            .get(13..)
            .ok_or_else(|| invalid("ES decoder configuration"))?,
        5,
    )
}

fn indexed_times(table: &[u8], packet_count: usize) -> io::Result<(Vec<i64>, i64)> {
    let times = fixed_entries::<8>(required(table, b"stts")?, 4)?;
    let ctts = find(table, b"ctts")?;
    let mut pts = Vec::with_capacity(packet_count);
    let mut dts = 0i64;
    for entry in times {
        let count = u32_at(entry, 0)? as usize;
        let step = i64::from(u32_at(entry, 4)?);
        if count > packet_count - pts.len() {
            return Err(invalid("sample duration table"));
        }
        for _ in 0..count {
            pts.push(dts);
            dts = dts
                .checked_add(step)
                .ok_or_else(|| invalid("decode timestamp overflow"))?;
        }
    }
    if pts.len() != packet_count {
        return Err(invalid("sample duration count"));
    }
    if let Some(ctts) = ctts {
        apply_composition_offsets(&mut pts, ctts)?;
    }
    Ok((pts, dts))
}

fn header_time(header: &[u8]) -> io::Result<(u32, u64)> {
    let (scale, duration) = match header.first() {
        Some(0) => (u32_at(header, 12)?, u64::from(u32_at(header, 16)?)),
        Some(1) => (
            u32_at(header, 20)?,
            u64::from_be_bytes(bytes_at(header, 24)?),
        ),
        _ => return Err(invalid("media time version")),
    };
    if scale == 0 {
        return Err(invalid("zero media timescale"));
    }
    Ok((scale, duration))
}

fn simple_edit(movie: &[u8], edit: &[u8], timescale: u32) -> io::Result<Option<(u64, i64)>> {
    let count = u32_at(edit, 4)?;
    if !(1..=2).contains(&count) {
        return Ok(None);
    }
    let version = edit[0];
    let entry_bytes = match version {
        0 => 12,
        1 => 20,
        _ => return Err(invalid("edit list version")),
    };
    // An initial empty edit delays the first picture on the movie clock.
    // The existing player starts counting at the first emitted picture.
    if count == 2 && edit_entry(version, &edit[8..])?.1 != -1 {
        return Ok(None);
    }
    let offset = 8 + entry_bytes * (count as usize - 1);
    let (duration, origin, rate) = edit_entry(
        version,
        edit.get(offset..).ok_or_else(|| invalid("edit entries"))?,
    )?;
    if origin < 0 || rate != 1 << 16 {
        return Ok(None);
    }
    let (movie_scale, _) = header_time(required(movie, b"mvhd")?)?;
    let ticks =
        u64::try_from(u128::from(duration) * u128::from(timescale) / u128::from(movie_scale))
            .map_err(|_| invalid("edit duration overflow"))?;
    Ok(Some((ticks, origin)))
}

fn edit_entry(version: u8, entry: &[u8]) -> io::Result<(u64, i64, u32)> {
    Ok(match version {
        0 => (
            u64::from(u32_at(entry, 0)?),
            i64::from(u32_at(entry, 4)? as i32),
            u32_at(entry, 8)?,
        ),
        1 => (
            u64::from_be_bytes(bytes_at(entry, 0)?),
            i64::from_be_bytes(bytes_at(entry, 8)?),
            u32_at(entry, 16)?,
        ),
        _ => unreachable!("version already validated"),
    })
}

fn apply_composition_offsets(times: &mut [i64], ctts: &[u8]) -> io::Result<()> {
    let signed = match ctts.first() {
        Some(0) => false,
        Some(1) => true,
        _ => return Err(invalid("composition time version")),
    };
    let mut used = 0;
    for entry in fixed_entries::<8>(ctts, 4)? {
        let count = u32_at(entry, 0)? as usize;
        let end = used + count.min(times.len() - used);
        if end - used != count {
            return Err(invalid("composition time count"));
        }
        let value = u32_at(entry, 4)?;
        let offset = if signed {
            i64::from(value as i32)
        } else {
            i64::from(value)
        };
        for time in &mut times[used..end] {
            *time = time
                .checked_add(offset)
                .ok_or_else(|| invalid("composition timestamp overflow"))?;
        }
        used = end;
    }
    if used != times.len() {
        return Err(invalid("composition time count"));
    }
    Ok(())
}

struct Fragment {
    offset: u64,
    body: Range<u64>,
}

#[derive(Clone, Copy)]
struct SampleDefaults {
    description: u32,
    duration: u32,
    size: u32,
}

#[derive(Default)]
struct FragmentSamples {
    packets: Vec<Packet>,
    pts: Vec<i64>,
    dts: i64,
    decode_duration: u64,
}

fn fragment_samples(
    input: &mut BufReader<File>,
    movie: &[u8],
    track: &[u8],
    media: &[Range<u64>],
    fragments: &[Fragment],
) -> io::Result<FragmentSamples> {
    let header = required(track, b"tkhd")?;
    let id = u32_at(
        header,
        match header.first() {
            Some(0) => 12,
            Some(1) => 20,
            _ => return Err(invalid("track header version")),
        },
    )?;
    let defaults = fragment_defaults(movie, id)?;
    let mut samples = FragmentSamples::default();
    let mut data = Vec::new();
    for fragment in fragments {
        input.seek(SeekFrom::Start(fragment.body.start))?;
        data.resize((fragment.body.end - fragment.body.start) as usize, 0);
        input.read_exact(&mut data)?;
        let mut tracks = data.as_slice();
        let mut first_track = true;
        while let Some((kind, track)) = next_box(&mut tracks)? {
            if kind != *b"traf" {
                continue;
            }
            let mut header = required(track, b"tfhd")?;
            let flags = read_word(&mut header)?;
            let track_id = read_word(&mut header)?;
            if track_id == id {
                let (base, description) =
                    fragment_header(header, flags, fragment.offset, first_track, defaults)?;
                if let Some(time) = find(track, b"tfdt")? {
                    samples.dts = match time.first() {
                        Some(0) => i64::from(u32_at(time, 4)?),
                        Some(1) => i64::try_from(u64::from_be_bytes(bytes_at(time, 4)?))
                            .map_err(|_| invalid("fragment decode time overflow"))?,
                        _ => return Err(invalid("fragment decode time version")),
                    };
                }
                let mut runs = track;
                let mut offset = base;
                while let Some((kind, run)) = next_box(&mut runs)? {
                    if kind == *b"trun" {
                        offset = read_run(&mut samples, run, media, base, offset, description)?;
                    }
                }
            }
            first_track = false;
        }
    }
    if samples.packets.is_empty() {
        return Err(invalid("fragmented track contains no packets"));
    }
    Ok(samples)
}

fn fragment_defaults(movie: &[u8], id: u32) -> io::Result<SampleDefaults> {
    let mut extensions = required(movie, b"mvex")?;
    while let Some((kind, defaults)) = next_box(&mut extensions)? {
        if kind == *b"trex" && u32_at(defaults, 4)? == id {
            return Ok(SampleDefaults {
                description: u32_at(defaults, 8)?,
                duration: u32_at(defaults, 12)?,
                size: u32_at(defaults, 16)?,
            });
        }
    }
    Err(invalid("missing fragment defaults"))
}

fn fragment_header(
    mut header: &[u8],
    flags: u32,
    fragment_offset: u64,
    first_track: bool,
    mut defaults: SampleDefaults,
) -> io::Result<(u64, SampleDefaults)> {
    if flags & !0x03003b != 0 {
        return Err(invalid("unsupported track fragment flags"));
    }
    let base = if flags & 1 != 0 {
        let (offset, rest) = header
            .split_first_chunk::<8>()
            .ok_or_else(|| invalid("fragment offset"))?;
        header = rest;
        u64::from_be_bytes(*offset)
    } else if flags & 0x020000 != 0 || first_track {
        fragment_offset
    } else {
        return Err(invalid("unsupported implicit track fragment offset"));
    };
    if flags & 2 != 0 {
        defaults.description = read_word(&mut header)?;
    }
    if defaults.description != 1 {
        return Err(invalid("unsupported fragment sample description"));
    }
    if flags & 8 != 0 {
        defaults.duration = read_word(&mut header)?;
    }
    if flags & 0x10 != 0 {
        defaults.size = read_word(&mut header)?;
    }
    if flags & 0x20 != 0 {
        read_word(&mut header)?;
    }
    Ok((base, defaults))
}

fn read_run(
    samples: &mut FragmentSamples,
    mut run: &[u8],
    media: &[Range<u64>],
    base: u64,
    mut offset: u64,
    defaults: SampleDefaults,
) -> io::Result<u64> {
    let flags = read_word(&mut run)?;
    let count = read_word(&mut run)? as usize;
    if flags & !0x01000f05 != 0 || count > MAX_SAMPLES - samples.packets.len() {
        return Err(invalid("fragment sample count or flags"));
    }
    if flags & 1 != 0 {
        let relative = i64::from(read_word(&mut run)? as i32);
        offset = base
            .checked_add_signed(relative)
            .ok_or_else(|| invalid("fragment data offset"))?;
    }
    if flags & 4 != 0 {
        read_word(&mut run)?;
    }
    if count == 0 {
        return Ok(offset);
    }
    let limit = media
        .iter()
        .find(|range| range.contains(&offset))
        .ok_or_else(|| invalid("fragment outside media data"))?
        .end;
    for _ in 0..count {
        let duration = if flags & 0x100 != 0 {
            read_word(&mut run)?
        } else {
            defaults.duration
        };
        let size = if flags & 0x200 != 0 {
            read_word(&mut run)?
        } else {
            defaults.size
        };
        if flags & 0x400 != 0 {
            read_word(&mut run)?;
        }
        let composition = if flags & 0x800 != 0 {
            let value = read_word(&mut run)?;
            if flags & 0x01000000 != 0 {
                i64::from(value as i32)
            } else {
                i64::from(value)
            }
        } else {
            0
        };
        let end = offset
            .checked_add(u64::from(size))
            .filter(|&end| end <= limit && (1..=PACKET_BYTES).contains(&size))
            .ok_or_else(|| invalid("fragment packet range"))?;
        if duration == 0 {
            return Err(invalid("zero fragment sample duration"));
        }
        samples.packets.push(Packet { offset, size });
        samples.pts.push(
            samples
                .dts
                .checked_add(composition)
                .ok_or_else(|| invalid("fragment presentation time overflow"))?,
        );
        samples.dts = samples
            .dts
            .checked_add(i64::from(duration))
            .ok_or_else(|| invalid("fragment decode time overflow"))?;
        samples.decode_duration += u64::from(duration);
        offset = end;
    }
    Ok(offset)
}

fn read_word(input: &mut &[u8]) -> io::Result<u32> {
    let (word, rest) = input
        .split_first_chunk::<4>()
        .ok_or_else(|| invalid("truncated fragment table"))?;
    *input = rest;
    Ok(u32::from_be_bytes(*word))
}

fn video_packets(table: &[u8], media: &[Range<u64>]) -> io::Result<Vec<Packet>> {
    let sizes = required(table, b"stsz")?;
    let uniform = u32_at(sizes, 4)?;
    let count = u32_at(sizes, 8)? as usize;
    if !(1..=MAX_SAMPLES).contains(&count) {
        return Err(invalid("sample count exceeds the limit"));
    }
    let explicit = if uniform == 0 {
        fixed_entries::<4>(sizes, 8)?
    } else {
        &[]
    };
    let chunks = fixed_entries::<12>(required(table, b"stsc")?, 4)?;
    let small_offsets = find(table, b"stco")?;
    let (small, large) = match small_offsets {
        Some(offsets) => (fixed_entries::<4>(offsets, 4)?, &[][..]),
        None => (&[][..], fixed_entries::<8>(required(table, b"co64")?, 4)?),
    };
    let chunk_count = small.len().max(large.len());
    let mut packets = Vec::with_capacity(count);
    let mut first_chunk = 0;
    for (i, chunk) in chunks.iter().enumerate() {
        let first = (u32_at(chunk, 0)? as usize).checked_sub(1);
        let per_chunk = u32_at(chunk, 4)? as usize;
        let end = match chunks.get(i + 1) {
            Some(next) => (u32_at(next, 0)? as usize).saturating_sub(1),
            None => chunk_count,
        };
        if first != Some(first_chunk)
            || end <= first_chunk
            || end > chunk_count
            || per_chunk == 0
            || u32_at(chunk, 8)? != 1
        {
            return Err(invalid("sample-to-chunk table"));
        }
        for chunk_index in first_chunk..end {
            if per_chunk > count - packets.len() {
                return Err(invalid("sample count"));
            }
            let mut offset = if small_offsets.is_some() {
                u64::from(u32::from_be_bytes(small[chunk_index]))
            } else {
                u64::from_be_bytes(large[chunk_index])
            };
            let limit = media
                .iter()
                .find(|range| range.contains(&offset))
                .ok_or_else(|| invalid("chunk outside media data"))?
                .end;
            for _ in 0..per_chunk {
                let size = if uniform == 0 {
                    u32::from_be_bytes(explicit[packets.len()])
                } else {
                    uniform
                };
                let end = offset
                    .checked_add(u64::from(size))
                    .filter(|&end| end <= limit && (1..=PACKET_BYTES).contains(&size))
                    .ok_or_else(|| invalid("packet range"))?;
                packets.push(Packet { offset, size });
                offset = end;
            }
        }
        first_chunk = end;
    }
    if packets.len() != count || first_chunk != chunk_count {
        return Err(invalid("sample count"));
    }
    Ok(packets)
}

fn next_box<'a>(input: &mut &'a [u8]) -> io::Result<Option<([u8; 4], &'a [u8])>> {
    if input.is_empty() {
        return Ok(None);
    }
    let kind = bytes_at(input, 4)?;
    let (size, header) = match u32_at(input, 0)? {
        0 => (input.len(), 8),
        1 => (
            usize::try_from(u64::from_be_bytes(bytes_at(input, 8)?))
                .map_err(|_| invalid("box size"))?,
            16,
        ),
        size => (size as usize, 8),
    };
    let body = input.get(header..size).ok_or_else(|| invalid("box size"))?;
    *input = &input[size..];
    Ok(Some((kind, body)))
}

fn find<'a>(mut input: &'a [u8], kind: &[u8; 4]) -> io::Result<Option<&'a [u8]>> {
    while let Some((name, body)) = next_box(&mut input)? {
        if name == *kind {
            return Ok(Some(body));
        }
    }
    Ok(None)
}

fn required<'a>(input: &'a [u8], kind: &[u8; 4]) -> io::Result<&'a [u8]> {
    find(input, kind)?.ok_or_else(|| invalid("missing required box"))
}

fn fixed_entries<const N: usize>(input: &[u8], offset: usize) -> io::Result<&[[u8; N]]> {
    let count = u32_at(input, offset)? as usize;
    let (entries, tail) = input[offset + 4..].as_chunks::<N>();
    if count != entries.len() || !tail.is_empty() {
        return Err(invalid("table entry count"));
    }
    Ok(entries)
}

fn bytes_at<const N: usize>(bytes: &[u8], offset: usize) -> io::Result<[u8; N]> {
    bytes
        .get(offset..offset + N)
        .map(|bytes| bytes.try_into().expect("fixed box field"))
        .ok_or_else(|| invalid("truncated box field"))
}

fn u32_at(bytes: &[u8], offset: usize) -> io::Result<u32> {
    bytes_at(bytes, offset).map(u32::from_be_bytes)
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
