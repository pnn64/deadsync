use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const WAV_PACKET_FRAMES: usize = 4096;
const WAVE_FORMAT_PCM: u16 = 0x0001;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xfffe;
const SUBFORMAT_TAIL: [u8; 14] = [
    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];

pub(crate) struct OpenFile {
    pub reader: Reader,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

pub(crate) struct Reader {
    reader: BufReader<File>,
    spec: Spec,
    packet_buf: Vec<u8>,
    pending: Option<Vec<i16>>,
    cursor_frames: u64,
}

#[derive(Clone, Copy)]
struct Spec {
    channels: usize,
    sample_rate_hz: u32,
    block_align: usize,
    bytes_per_sample: usize,
    encoding: Encoding,
    data_offset: u64,
    frames_total: u64,
}

#[derive(Clone, Copy)]
enum Encoding {
    Pcm,
    Float,
}

#[inline(always)]
pub(crate) fn path_is_wav(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|ext| {
        ext.eq_ignore_ascii_case("wav") || ext.eq_ignore_ascii_case("wave")
    })
}

pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let spec = parse_spec(&mut reader)?;
    let mut reader = Reader {
        reader,
        spec,
        packet_buf: Vec::new(),
        pending: None,
        cursor_frames: 0,
    };
    let Some(first_packet) = reader.decode_next_packet()? else {
        return Err(format!(
            "WAV '{}' contained no decodable audio frames",
            path.display()
        )
        .into());
    };
    reader.pending = Some(first_packet);
    Ok(OpenFile {
        reader,
        channels: spec.channels,
        sample_rate_hz: spec.sample_rate_hz,
    })
}

pub(crate) fn file_length_seconds(path: &Path) -> Result<f32, String> {
    let file = File::open(path).map_err(|e| format!("Cannot open WAV file: {e}"))?;
    let mut reader = BufReader::new(file);
    let spec = parse_spec(&mut reader).map_err(|e| e.to_string())?;
    Ok((spec.frames_total as f64 / f64::from(spec.sample_rate_hz)) as f32)
}

impl Reader {
    pub(crate) fn read_dec_packet_itl(
        &mut self,
    ) -> Result<Option<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(packet) = self.pending.take() {
            self.cursor_frames = self
                .cursor_frames
                .saturating_add((packet.len() / self.spec.channels) as u64);
            return Ok(Some(packet));
        }
        let Some(packet) = self.decode_next_packet()? else {
            return Ok(None);
        };
        self.cursor_frames = self
            .cursor_frames
            .saturating_add((packet.len() / self.spec.channels) as u64);
        Ok(Some(packet))
    }

    pub(crate) fn seek_frame(
        &mut self,
        target_frame: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let clamped = target_frame.min(self.spec.frames_total);
        let byte_offset = clamped.saturating_mul(self.spec.block_align as u64);
        self.reader
            .seek(SeekFrom::Start(self.spec.data_offset.saturating_add(byte_offset)))?;
        self.pending = None;
        self.cursor_frames = clamped;
        Ok(())
    }

    fn decode_next_packet(
        &mut self,
    ) -> Result<Option<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
        let frames_left = self.spec.frames_total.saturating_sub(self.cursor_frames);
        if frames_left == 0 {
            return Ok(None);
        }
        let frames = frames_left.min(WAV_PACKET_FRAMES as u64) as usize;
        let bytes = frames.saturating_mul(self.spec.block_align);
        self.packet_buf.resize(bytes, 0);
        self.reader.read_exact(&mut self.packet_buf)?;
        decode_packet(&self.packet_buf, self.spec).map(Some)
    }
}

fn parse_spec(reader: &mut BufReader<File>) -> Result<Spec, Box<dyn std::error::Error + Send + Sync>> {
    let mut riff = [0u8; 12];
    reader.read_exact(&mut riff)?;
    if &riff[..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return Err("unsupported WAV container (expected RIFF/WAVE)".into());
    }

    let mut format = None;
    let mut data = None;
    loop {
        let Some((chunk_id, chunk_size)) = read_chunk_header(reader)? else {
            break;
        };
        let payload_offset = reader.stream_position()?;
        match &chunk_id {
            b"fmt " => format = Some(parse_format_chunk(reader, chunk_size)?),
            b"data" => data = Some((payload_offset, u64::from(chunk_size))),
            _ => {}
        }
        reader.seek(SeekFrom::Start(padded_end(payload_offset, chunk_size)))?;
        if format.is_some() && data.is_some() {
            break;
        }
    }

    let format = format.ok_or("WAV file is missing a fmt chunk")?;
    let (data_offset, data_len) = data.ok_or("WAV file is missing a data chunk")?;
    if format.block_align == 0 {
        return Err("WAV block alignment is invalid (0)".into());
    }
    let frames_total = data_len / format.block_align as u64;
    reader.seek(SeekFrom::Start(data_offset))?;
    Ok(Spec {
        data_offset,
        frames_total,
        ..format
    })
}

fn read_chunk_header(
    reader: &mut BufReader<File>,
) -> Result<Option<([u8; 4], u32)>, Box<dyn std::error::Error + Send + Sync>> {
    let mut chunk = [0u8; 8];
    match reader.read_exact(&mut chunk) {
        Ok(()) => Ok(Some((
            [chunk[0], chunk[1], chunk[2], chunk[3]],
            u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
        ))),
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn padded_end(start: u64, chunk_size: u32) -> u64 {
    start
        .saturating_add(u64::from(chunk_size))
        .saturating_add(u64::from(chunk_size & 1))
}

fn parse_format_chunk(
    reader: &mut BufReader<File>,
    chunk_size: u32,
) -> Result<Spec, Box<dyn std::error::Error + Send + Sync>> {
    if chunk_size < 16 {
        return Err(format!("WAV fmt chunk is too small ({chunk_size} bytes)").into());
    }
    let mut fmt = vec![0u8; chunk_size as usize];
    reader.read_exact(&mut fmt)?;

    let format_tag = le_u16(&fmt, 0)?;
    let channels = le_u16(&fmt, 2)? as usize;
    let sample_rate_hz = le_u32(&fmt, 4)?;
    let block_align = le_u16(&fmt, 12)? as usize;
    let bits_per_sample = le_u16(&fmt, 14)?;
    let bytes_per_sample = block_align
        .checked_div(channels.max(1))
        .ok_or("WAV block alignment is invalid")?;
    if channels == 0 || sample_rate_hz == 0 || bytes_per_sample == 0 || block_align == 0 {
        return Err("WAV format fields are invalid".into());
    }

    let encoding = match format_tag {
        WAVE_FORMAT_PCM => parse_pcm_format(bytes_per_sample)?,
        WAVE_FORMAT_IEEE_FLOAT => parse_float_format(bytes_per_sample)?,
        WAVE_FORMAT_EXTENSIBLE => parse_extensible_encoding(&fmt, bytes_per_sample, bits_per_sample)?,
        _ => return Err(format!("unsupported WAV format tag 0x{format_tag:04x}").into()),
    };

    Ok(Spec {
        channels,
        sample_rate_hz,
        block_align,
        bytes_per_sample,
        encoding,
        data_offset: 0,
        frames_total: 0,
    })
}

fn parse_pcm_format(
    bytes_per_sample: usize,
) -> Result<Encoding, Box<dyn std::error::Error + Send + Sync>> {
    match bytes_per_sample {
        1..=4 => Ok(Encoding::Pcm),
        _ => Err(format!("unsupported WAV PCM sample width ({bytes_per_sample} bytes)").into()),
    }
}

fn parse_float_format(
    bytes_per_sample: usize,
) -> Result<Encoding, Box<dyn std::error::Error + Send + Sync>> {
    match bytes_per_sample {
        4 | 8 => Ok(Encoding::Float),
        _ => Err(format!("unsupported WAV float sample width ({bytes_per_sample} bytes)").into()),
    }
}

fn parse_extensible_encoding(
    fmt: &[u8],
    bytes_per_sample: usize,
    bits_per_sample: u16,
) -> Result<Encoding, Box<dyn std::error::Error + Send + Sync>> {
    if fmt.len() < 40 || le_u16(fmt, 16)? < 22 {
        return Err("unsupported WAV extensible fmt chunk".into());
    }
    let valid_bits = le_u16(fmt, 18)?;
    if valid_bits == 0 || valid_bits > bits_per_sample {
        return Err("WAV extensible valid-bits field is invalid".into());
    }
    let subformat = &fmt[24..40];
    if subformat[2..] != SUBFORMAT_TAIL {
        return Err("unsupported WAV extensible subformat GUID".into());
    }
    match u16::from_le_bytes([subformat[0], subformat[1]]) {
        WAVE_FORMAT_PCM => parse_pcm_format(bytes_per_sample),
        WAVE_FORMAT_IEEE_FLOAT => parse_float_format(bytes_per_sample),
        other => Err(format!("unsupported WAV extensible format tag 0x{other:04x}").into()),
    }
}

fn decode_packet(
    bytes: &[u8],
    spec: Spec,
) -> Result<Vec<i16>, Box<dyn std::error::Error + Send + Sync>> {
    if bytes.len() % spec.bytes_per_sample != 0 {
        return Err("WAV packet ended mid-sample".into());
    }
    let mut out = Vec::with_capacity(bytes.len() / spec.bytes_per_sample);
    for sample in bytes.chunks_exact(spec.bytes_per_sample) {
        out.push(match spec.encoding {
            Encoding::Pcm => pcm_to_i16(sample)?,
            Encoding::Float => float_to_i16(sample)?,
        });
    }
    Ok(out)
}

fn pcm_to_i16(sample: &[u8]) -> Result<i16, Box<dyn std::error::Error + Send + Sync>> {
    Ok(match sample.len() {
        1 => (i16::from(sample[0]) - 128) << 8,
        2 => i16::from_le_bytes([sample[0], sample[1]]),
        3 => {
            let signed = i32::from_le_bytes([
                sample[0],
                sample[1],
                sample[2],
                if sample[2] & 0x80 != 0 { 0xff } else { 0x00 },
            ]);
            (signed >> 8).clamp(i16::MIN as i32, i16::MAX as i32) as i16
        }
        4 => {
            let signed = i32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
            (signed >> 16).clamp(i16::MIN as i32, i16::MAX as i32) as i16
        }
        n => return Err(format!("unsupported WAV PCM sample width ({n} bytes)").into()),
    })
}

fn float_to_i16(sample: &[u8]) -> Result<i16, Box<dyn std::error::Error + Send + Sync>> {
    let value = match sample.len() {
        4 => f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]) as f64,
        8 => f64::from_le_bytes([
            sample[0], sample[1], sample[2], sample[3], sample[4], sample[5], sample[6],
            sample[7],
        ]),
        n => return Err(format!("unsupported WAV float sample width ({n} bytes)").into()),
    };
    if !value.is_finite() {
        return Ok(0);
    }
    Ok((value * 32767.0).round().clamp(-32768.0, 32767.0) as i16)
}

fn le_u16(
    data: &[u8],
    offset: usize,
) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or("WAV chunk ended unexpectedly")?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn le_u32(
    data: &[u8],
    offset: usize,
) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or("WAV chunk ended unexpectedly")?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
