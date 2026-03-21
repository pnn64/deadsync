use claxon::{FlacReader, FlacReaderOptions};
use std::fs::File;
use std::path::Path;

pub(crate) struct OpenFile {
    pub reader: Reader,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

pub(crate) struct Reader {
    reader: FlacReader<File>,
    channels: usize,
    bits_per_sample: u32,
    block_buffer: Vec<i32>,
    pending: Option<Vec<i16>>,
    cursor_frames: u64,
}

#[inline(always)]
pub(crate) fn path_is_flac(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("flac"))
}

pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let reader = FlacReader::open(path)?;
    let info = reader.streaminfo();
    let channels = info.channels.max(1) as usize;
    let sample_rate_hz = info.sample_rate.max(1);
    let bits_per_sample = info.bits_per_sample.max(1);
    let mut reader = Reader {
        reader,
        channels,
        bits_per_sample,
        block_buffer: Vec::new(),
        pending: None,
        cursor_frames: 0,
    };
    let Some(first_packet) = reader.decode_next_packet()? else {
        return Err(format!(
            "FLAC '{}' contained no decodable audio frames",
            path.display()
        )
        .into());
    };
    reader.pending = Some(first_packet);
    Ok(OpenFile {
        reader,
        channels,
        sample_rate_hz,
    })
}

pub(crate) fn file_length_seconds(path: &Path) -> Result<f32, String> {
    let reader = FlacReader::open_ext(
        path,
        FlacReaderOptions {
            metadata_only: true,
            read_vorbis_comment: false,
        },
    )
    .map_err(|e| format!("Cannot open FLAC file: {e}"))?;
    let info = reader.streaminfo();
    if info.sample_rate == 0 {
        return Err("FLAC sample rate is invalid (0)".to_string());
    }
    if let Some(total_frames) = info.samples {
        return Ok((total_frames as f64 / f64::from(info.sample_rate)) as f32);
    }

    let mut reader = FlacReader::open(path).map_err(|e| format!("Cannot decode FLAC file: {e}"))?;
    let channels = reader.streaminfo().channels.max(1) as usize;
    let mut block_buffer = Vec::new();
    let mut total_frames = 0u64;
    loop {
        let block = reader
            .blocks()
            .read_next_or_eof(block_buffer)
            .map_err(|e| format!("FLAC decode failed: {e}"))?;
        let Some(block) = block else {
            break;
        };
        validate_channels(channels, block.channels() as usize)?;
        total_frames = total_frames.saturating_add(block.duration() as u64);
        block_buffer = block.into_buffer();
    }
    Ok((total_frames as f64 / f64::from(reader.streaminfo().sample_rate)) as f32)
}

impl Reader {
    pub(crate) fn read_dec_packet_itl(
        &mut self,
    ) -> Result<Option<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
        let Some(packet) = self.take_packet()? else {
            return Ok(None);
        };
        self.cursor_frames = self
            .cursor_frames
            .saturating_add(packet_frames(&packet, self.channels) as u64);
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
            let Some(packet) = self.take_packet()? else {
                return Ok(());
            };
            if self.finish_seek_packet(packet, target_frame) {
                return Ok(());
            }
        }
        Ok(())
    }

    #[inline(always)]
    fn take_packet(
        &mut self,
    ) -> Result<Option<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(packet) = self.pending.take() {
            return Ok(Some(packet));
        }
        self.decode_next_packet()
    }

    fn finish_seek_packet(&mut self, packet: Vec<i16>, target_frame: u64) -> bool {
        let packet_frames = packet_frames(&packet, self.channels);
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

    fn decode_next_packet(
        &mut self,
    ) -> Result<Option<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
        let block = self
            .reader
            .blocks()
            .read_next_or_eof(std::mem::take(&mut self.block_buffer))?;
        let Some(block) = block else {
            return Ok(None);
        };
        if let Err(err) = validate_channels(self.channels, block.channels() as usize) {
            return Err(err.into());
        }
        let frames = block.duration() as usize;
        let mut packet = Vec::with_capacity(frames.saturating_mul(self.channels));
        for frame in 0..frames as u32 {
            for ch in 0..self.channels as u32 {
                packet.push(sample_to_i16(block.sample(ch, frame), self.bits_per_sample));
            }
        }
        self.block_buffer = block.into_buffer();
        Ok(Some(packet))
    }
}

#[inline(always)]
fn packet_frames(packet: &[i16], channels: usize) -> usize {
    packet.len() / channels
}

#[inline(always)]
fn validate_channels(expected: usize, found: usize) -> Result<(), String> {
    if found != expected {
        return Err(format!(
            "unsupported FLAC channel change ({} -> {})",
            expected, found
        ));
    }
    Ok(())
}

#[inline(always)]
fn sample_to_i16(sample: i32, bits_per_sample: u32) -> i16 {
    if bits_per_sample == 16 {
        return sample.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
    if bits_per_sample > 16 {
        let shifted = sample >> (bits_per_sample - 16).min(31);
        return shifted.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    }
    let widened = (i64::from(sample)) << (16 - bits_per_sample);
    widened.clamp(i16::MIN as i64, i16::MAX as i64) as i16
}
