use ogg::{Packet, PacketReader};
use opusic_c::{Channels, Decoder as OpusDecoder, SampleRate};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const OPUS_HEAD_MAGIC: &[u8; 8] = b"OpusHead";
const OPUS_TAGS_MAGIC: &[u8; 8] = b"OpusTags";
const OPUS_SAMPLE_RATE_HZ: u32 = 48_000;
const OPUS_MAX_PACKET_FRAMES: usize = 5760;

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
        decode_buf: vec![0; OPUS_MAX_PACKET_FRAMES * header.channels],
        pending: None,
        skip_frames: header.pre_skip_frames,
        cursor_frames: 0,
        decoded_frames: 0,
        ended: false,
    };
    let Some(first_packet) = reader.decode_next_packet()? else {
        return Err(format!(
            "Opus '{}' contained no decodable audio frames",
            path.display()
        )
        .into());
    };
    reader.pending = Some(first_packet);
    Ok(OpenFile {
        reader,
        channels: header.channels,
        sample_rate_hz: OPUS_SAMPLE_RATE_HZ,
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
    Ok((total_frames as f64 / f64::from(OPUS_SAMPLE_RATE_HZ)) as f32)
}

impl Reader {
    pub(crate) fn read_dec_packet_itl(
        &mut self,
    ) -> Result<Option<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(packet) = self.pending.take() {
            self.cursor_frames = self
                .cursor_frames
                .saturating_add((packet.len() / self.channels) as u64);
            return Ok(Some(packet));
        }
        let Some(packet) = self.decode_next_packet()? else {
            return Ok(None);
        };
        self.cursor_frames = self
            .cursor_frames
            .saturating_add((packet.len() / self.channels) as u64);
        Ok(Some(packet))
    }

    pub(crate) fn seek_frame(
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
                let Some(packet) = self.decode_next_packet()? else {
                    return Ok(());
                };
                packet
            };
            let packet_frames = packet.len() / self.channels;
            let remaining = (target_frame - self.cursor_frames) as usize;
            if remaining >= packet_frames {
                self.cursor_frames = self.cursor_frames.saturating_add(packet_frames as u64);
                continue;
            }
            let drop_samples = remaining * self.channels;
            self.pending = Some(packet[drop_samples..].to_vec());
            self.cursor_frames = target_frame;
            return Ok(());
        }
        Ok(())
    }

    fn decode_next_packet(
        &mut self,
    ) -> Result<Option<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
        if self.ended {
            return Ok(None);
        }
        loop {
            let Some(packet) = self.reader.read_packet()? else {
                self.ended = true;
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
                if self.ended {
                    return Ok(None);
                }
                continue;
            }
            let skip = self.skip_frames.min(valid_frames);
            self.skip_frames -= skip;
            if skip == valid_frames {
                if self.ended {
                    return Ok(None);
                }
                continue;
            }
            let start = skip * self.channels;
            let end = valid_frames * self.channels;
            let mut out = Vec::with_capacity(end - start);
            out.extend(
                self.decode_buf[start..end]
                    .iter()
                    .map(|&sample| sample as i16),
            );
            return Ok(Some(out));
        }
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

#[cfg(test)]
mod tests {
    use super::{
        file_length_seconds, open_file, parse_head_data, path_is_opus, validate_tags_data,
    };
    use std::fs;
    use std::path::Path;

    const SMOKE_OPUS: [u8; 407] = [
        79, 103, 103, 83, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 90, 96, 189, 126, 0, 0, 0, 0, 175, 165,
        120, 129, 1, 19, 79, 112, 117, 115, 72, 101, 97, 100, 1, 2, 56, 1, 128, 187, 0, 0, 0, 0, 0,
        79, 103, 103, 83, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 90, 96, 189, 126, 1, 0, 0, 0, 238, 98, 228,
        28, 1, 61, 79, 112, 117, 115, 84, 97, 103, 115, 12, 0, 0, 0, 76, 97, 118, 102, 54, 49, 46,
        55, 46, 49, 48, 48, 1, 0, 0, 0, 29, 0, 0, 0, 101, 110, 99, 111, 100, 101, 114, 61, 76, 97,
        118, 99, 54, 49, 46, 49, 57, 46, 49, 48, 49, 32, 108, 105, 98, 111, 112, 117, 115, 79, 103,
        103, 83, 0, 4, 152, 10, 0, 0, 0, 0, 0, 0, 90, 96, 189, 126, 2, 0, 0, 0, 7, 94, 27, 135, 3,
        80, 80, 80, 252, 255, 254, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 252, 255,
        254, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 252, 255, 254, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];

    #[test]
    fn opus_extension_detection_is_case_insensitive() {
        assert!(path_is_opus(Path::new("song.opus")));
        assert!(path_is_opus(Path::new("song.OPUS")));
        assert!(!path_is_opus(Path::new("song.ogg")));
    }

    #[test]
    fn parses_basic_opus_head() {
        let parsed = parse_head_data(&[
            b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd', 1, 2, 0x38, 0x01, 0, 0, 0, 0, 0, 0, 0,
        ])
        .unwrap();
        assert_eq!(parsed.channels, 2);
        assert_eq!(parsed.pre_skip_frames, 312);
        assert_eq!(parsed.gain_q8_db, 0);
    }

    #[test]
    fn rejects_non_opus_tags() {
        assert!(validate_tags_data(b"VorbisTags").is_err());
    }

    #[test]
    fn decodes_embedded_silence_fixture() {
        let path =
            std::env::temp_dir().join(format!("deadsync-opus-smoke-{}.opus", std::process::id()));
        fs::write(&path, SMOKE_OPUS).unwrap();
        let mut opened = open_file(&path).unwrap();
        assert_eq!(opened.channels, 2);
        assert_eq!(opened.sample_rate_hz, 48_000);
        let mut total_samples = 0usize;
        while let Some(packet) = opened.reader.read_dec_packet_itl().unwrap() {
            total_samples += packet.len();
            assert!(packet.iter().all(|&sample| sample == 0));
        }
        let seconds = file_length_seconds(&path).unwrap();
        let _ = fs::remove_file(&path);
        assert_eq!(total_samples, 4_800);
        assert!((seconds - 0.05).abs() < 1e-6);
    }
}
