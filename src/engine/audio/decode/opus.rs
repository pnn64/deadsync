use ogg::{Packet, PacketReader};
use opusic_c::{Channels, Decoder as OpusDecoder, SampleRate};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const OPUS_HEAD_MAGIC: &[u8; 8] = b"OpusHead";
const OPUS_TAGS_MAGIC: &[u8; 8] = b"OpusTags";
// Opus decode/sample time is always expressed on a 48 kHz timeline.
const OPUS_DECODE_RATE_HZ: u32 = 48_000;
const OPUS_MAX_PACKET_FRAMES: usize = 5760;
const OPUS_SEEK_PREROLL_FRAMES: u64 = (OPUS_DECODE_RATE_HZ as u64 * 80) / 1000;
const OPUS_LINEAR_SEEK_FRAMES: u64 = OPUS_MAX_PACKET_FRAMES as u64;

pub(crate) struct OpenFile {
    pub reader: Reader,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

pub(crate) struct Reader {
    reader: PacketReader<BufReader<File>>,
    decoder: OpusDecoder,
    stream_serial: u32,
    channels: usize,
    pre_skip_frames: usize,
    decode_buf: Vec<u16>,
    pending: Option<Vec<i16>>,
    skip_frames: usize,
    cursor_frames: u64,
    decoded_frames: u64,
    ended: bool,
}

#[derive(Clone, Copy)]
struct Header {
    stream_serial: u32,
    channels: usize,
    pre_skip_frames: usize,
    gain_q8_db: i32,
}

#[inline(always)]
pub(crate) fn path_is_opus(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("opus"))
}

pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(path)?;
    let mut packet_reader = PacketReader::new(BufReader::new(file));
    let header = read_headers(&mut packet_reader)?;
    let decoder = build_decoder(header)?;
    let mut reader = Reader {
        reader: packet_reader,
        decoder,
        stream_serial: header.stream_serial,
        channels: header.channels,
        pre_skip_frames: header.pre_skip_frames,
        decode_buf: vec![0; OPUS_MAX_PACKET_FRAMES * header.channels],
        pending: None,
        skip_frames: header.pre_skip_frames,
        cursor_frames: 0,
        decoded_frames: 0,
        ended: false,
    };
    let mut first_packet = Vec::new();
    if !reader.decode_next_packet_into(&mut first_packet)? {
        return Err(format!(
            "Opus '{}' contained no decodable audio frames",
            path.display()
        )
        .into());
    }
    reader.pending = Some(first_packet);
    Ok(OpenFile {
        reader,
        channels: header.channels,
        sample_rate_hz: OPUS_DECODE_RATE_HZ,
    })
}

pub(crate) fn file_length_seconds(path: &Path) -> Result<f32, String> {
    let file = File::open(path).map_err(|e| format!("Cannot open Opus file: {e}"))?;
    let mut reader = PacketReader::new(BufReader::new(file));
    let header = read_headers(&mut reader)?;
    let mut last_granule = None;
    while let Some(packet) = reader
        .read_packet()
        .map_err(|e| format!("Opus packet read failed: {e}"))?
    {
        if packet.stream_serial() != header.stream_serial {
            break;
        }
        last_granule = Some(packet.absgp_page());
        if packet.last_in_stream() {
            break;
        }
    }
    let total_frames = last_granule
        .ok_or_else(|| "Opus contained no audio packets".to_string())?
        .saturating_sub(header.pre_skip_frames as u64);
    Ok((total_frames as f64 / f64::from(OPUS_DECODE_RATE_HZ)) as f32)
}

impl Reader {
    pub(crate) fn read_dec_packet_into(
        &mut self,
        out: &mut Vec<i16>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(mut packet) = self.pending.take() {
            self.cursor_frames = self
                .cursor_frames
                .saturating_add((packet.len() / self.channels) as u64);
            std::mem::swap(out, &mut packet);
            return Ok(true);
        }
        if !self.decode_next_packet_into(out)? {
            return Ok(false);
        };
        self.cursor_frames = self
            .cursor_frames
            .saturating_add((out.len() / self.channels) as u64);
        Ok(true)
    }

    pub(crate) fn seek_frame(
        &mut self,
        target_frame: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if target_frame == self.cursor_frames {
            return Ok(());
        }
        if target_frame > self.cursor_frames
            && target_frame - self.cursor_frames <= OPUS_LINEAR_SEEK_FRAMES
        {
            return self.seek_frame_linear(target_frame);
        }
        let seek_frame = target_frame.saturating_sub(OPUS_SEEK_PREROLL_FRAMES);
        let seek_granule = seek_frame.saturating_add(self.pre_skip_frames as u64);
        if !self
            .reader
            .seek_absgp(Some(self.stream_serial), seek_granule)?
        {
            self.pending = None;
            self.ended = true;
            return Ok(());
        }
        self.decoder.reset().map_err(opus_error)?;
        self.pending = None;
        self.skip_frames = 0;
        self.ended = false;

        let Some((packets, first_packet_granule)) = self.collect_seek_page()? else {
            self.ended = true;
            return Ok(());
        };

        self.decoded_frames = first_packet_granule;
        self.skip_frames = self
            .pre_skip_frames
            .saturating_sub(first_packet_granule as usize);
        self.cursor_frames = first_packet_granule.saturating_sub(self.pre_skip_frames as u64);

        for packet in packets {
            let mut decoded = Vec::new();
            if !self.decode_packet_into(packet, &mut decoded)? {
                continue;
            }
            if self.finish_seek_packet(decoded, target_frame) {
                return Ok(());
            }
        }

        self.seek_frame_linear(target_frame)
    }

    fn decode_next_packet_into(
        &mut self,
        out: &mut Vec<i16>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if self.ended {
            out.clear();
            return Ok(false);
        }
        loop {
            let Some(packet) = self.reader.read_packet()? else {
                self.ended = true;
                out.clear();
                return Ok(false);
            };
            if self.decode_packet_into(packet, out)? {
                return Ok(true);
            }
            if self.ended {
                out.clear();
                return Ok(false);
            }
        }
    }

    fn seek_frame_linear(
        &mut self,
        target_frame: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if target_frame <= self.cursor_frames {
            return Ok(());
        }
        while self.cursor_frames < target_frame {
            let packet = if let Some(packet) = self.pending.take() {
                packet
            } else {
                let mut packet = Vec::new();
                if !self.decode_next_packet_into(&mut packet)? {
                    return Ok(());
                }
                packet
            };
            if self.finish_seek_packet(packet, target_frame) {
                return Ok(());
            }
        }
        Ok(())
    }

    fn finish_seek_packet(&mut self, packet: Vec<i16>, target_frame: u64) -> bool {
        let packet_frames = packet.len() / self.channels;
        let remaining = target_frame.saturating_sub(self.cursor_frames) as usize;
        if remaining >= packet_frames {
            self.cursor_frames = self.cursor_frames.saturating_add(packet_frames as u64);
            return false;
        }
        let drop_samples = remaining * self.channels;
        self.pending = Some(packet[drop_samples..].to_vec());
        self.cursor_frames = target_frame;
        true
    }

    fn collect_seek_page(
        &mut self,
    ) -> Result<Option<(Vec<Packet>, u64)>, Box<dyn std::error::Error + Send + Sync>> {
        let mut packets = Vec::with_capacity(4);
        let mut total_frames = 0u64;
        loop {
            let Some(packet) = self.reader.read_packet()? else {
                return Ok(None);
            };
            if packet.stream_serial() != self.stream_serial {
                return Err("chained Ogg Opus streams are unsupported".into());
            }
            if packet.data.is_empty() {
                if packet.last_in_stream() {
                    self.ended = true;
                    return Ok(None);
                }
                continue;
            }
            total_frames = total_frames.saturating_add(
                self.decoder
                    .get_nb_samples(&packet.data)
                    .map_err(opus_error)? as u64,
            );
            let last_in_page = packet.last_in_page();
            let page_granule = packet.absgp_page();
            packets.push(packet);
            if last_in_page {
                return Ok(Some((packets, page_granule.saturating_sub(total_frames))));
            }
        }
    }

    fn decode_packet_into(
        &mut self,
        packet: Packet,
        out: &mut Vec<i16>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if packet.stream_serial() != self.stream_serial {
            return Err("chained Ogg Opus streams are unsupported".into());
        }
        if packet.data.is_empty() {
            if packet.last_in_stream() {
                self.ended = true;
            }
            out.clear();
            return Ok(false);
        }
        let wanted_frames = self
            .decoder
            .get_nb_samples(&packet.data)
            .map_err(opus_error)?;
        let needed_samples = wanted_frames.saturating_mul(self.channels);
        if self.decode_buf.len() < needed_samples {
            self.decode_buf.resize(needed_samples, 0);
        }
        let decoded_before = self.decoded_frames;
        let decoded_frames = self
            .decoder
            .decode_to_slice(&packet.data, &mut self.decode_buf[..needed_samples], false)
            .map_err(opus_error)?;
        self.decoded_frames = self.decoded_frames.saturating_add(decoded_frames as u64);
        let mut valid_frames = decoded_frames;
        if packet.last_in_stream() {
            let allowed_frames = packet.absgp_page().saturating_sub(decoded_before) as usize;
            valid_frames = valid_frames.min(allowed_frames);
            self.ended = true;
        }
        if valid_frames == 0 {
            out.clear();
            return Ok(false);
        }
        let skip = self.skip_frames.min(valid_frames);
        self.skip_frames -= skip;
        if skip == valid_frames {
            out.clear();
            return Ok(false);
        }
        let start = skip * self.channels;
        let end = valid_frames * self.channels;
        out.clear();
        out.reserve(end - start);
        out.extend(
            self.decode_buf[start..end]
                .iter()
                .map(|&sample| sample as i16),
        );
        Ok(true)
    }
}

fn read_headers(reader: &mut PacketReader<BufReader<File>>) -> Result<Header, String> {
    let head = reader
        .read_packet()
        .map_err(|e| format!("Opus header read failed: {e}"))?
        .ok_or_else(|| "Opus file is missing the ID header".to_string())?;
    let header = parse_head_packet(&head)?;
    let tags = reader
        .read_packet()
        .map_err(|e| format!("Opus tags read failed: {e}"))?
        .ok_or_else(|| "Opus file is missing the comment header".to_string())?;
    validate_tags_packet(&tags, header.stream_serial)?;
    Ok(header)
}

fn parse_head_packet(packet: &Packet) -> Result<Header, String> {
    Ok(Header {
        stream_serial: packet.stream_serial(),
        ..parse_head_data(&packet.data)?
    })
}

fn validate_tags_packet(packet: &Packet, stream_serial: u32) -> Result<(), String> {
    if packet.stream_serial() != stream_serial {
        return Err("Opus tags packet switched logical streams".to_string());
    }
    validate_tags_data(&packet.data)
}

fn build_decoder(header: Header) -> Result<OpusDecoder, String> {
    let channels = match header.channels {
        1 => Channels::Mono,
        2 => Channels::Stereo,
        _ => unreachable!(),
    };
    let mut decoder = OpusDecoder::new(channels, SampleRate::Hz48000).map_err(opus_error)?;
    decoder.set_gain(header.gain_q8_db).map_err(opus_error)?;
    Ok(decoder)
}

#[inline(always)]
fn opus_error(err: opusic_c::ErrorCode) -> String {
    format!("Opus decode failed: {}", err.message())
}

fn parse_head_data(data: &[u8]) -> Result<Header, String> {
    if data.len() < 19 || &data[..8] != OPUS_HEAD_MAGIC {
        return Err("missing OpusHead packet".to_string());
    }
    if data[8] == 0 || data[8] & 0xF0 != 0 {
        return Err(format!("unsupported Opus ID header version {}", data[8]));
    }
    let channels = match data[9] {
        1 => 1,
        2 => 2,
        n => return Err(format!("unsupported Opus channel count {n}")),
    };
    let mapping_family = data[18];
    if mapping_family != 0 {
        return Err(format!(
            "unsupported Opus channel mapping family {mapping_family}"
        ));
    }
    Ok(Header {
        stream_serial: 0,
        channels,
        pre_skip_frames: u16::from_le_bytes([data[10], data[11]]) as usize,
        gain_q8_db: i16::from_le_bytes([data[16], data[17]]) as i32,
    })
}

fn validate_tags_data(data: &[u8]) -> Result<(), String> {
    if data.len() < 8 || &data[..8] != OPUS_TAGS_MAGIC {
        return Err("missing OpusTags packet".to_string());
    }
    Ok(())
}
