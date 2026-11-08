// Starting-main reader; parsing and sample conversion are unchanged.
use super::super::{Spec, WAV_PACKET_FRAMES, decode_packet_into, parse_spec};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

pub(crate) struct OpenFile {
    pub reader: Reader,
    pub channels: usize,
    pub sample_rate_hz: u32,
    pub frames_total_hint: Option<u64>,
}

pub struct Reader {
    reader: BufReader<File>,
    spec: Spec,
    packet_buf: Vec<u8>,
    pending: Option<Vec<i16>>,
    cursor_frames: u64,
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
    let mut first_packet = Vec::new();
    if !reader.decode_next_packet_into(&mut first_packet)? {
        return Err(format!(
            "WAV '{}' contained no decodable audio frames",
            path.display()
        )
        .into());
    }
    reader.pending = Some(first_packet);
    Ok(OpenFile {
        reader,
        channels: spec.channels,
        sample_rate_hz: spec.sample_rate_hz,
        frames_total_hint: Some(spec.frames_total),
    })
}

impl Reader {
    pub(crate) fn read_dec_packet_into(
        &mut self,
        out: &mut Vec<i16>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(mut packet) = self.pending.take() {
            self.cursor_frames = self
                .cursor_frames
                .saturating_add((packet.len() / self.spec.channels) as u64);
            std::mem::swap(out, &mut packet);
            return Ok(true);
        }
        if !self.decode_next_packet_into(out)? {
            return Ok(false);
        }
        self.cursor_frames = self
            .cursor_frames
            .saturating_add((out.len() / self.spec.channels) as u64);
        Ok(true)
    }

    pub(crate) fn seek_frame(
        &mut self,
        target_frame: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let clamped = target_frame.min(self.spec.frames_total);
        let byte_offset = clamped.saturating_mul(self.spec.block_align as u64);
        self.reader.seek(SeekFrom::Start(
            self.spec.data_offset.saturating_add(byte_offset),
        ))?;
        self.pending = None;
        self.cursor_frames = clamped;
        Ok(())
    }

    #[inline(always)]
    pub(crate) const fn current_frame(&self) -> u64 {
        self.cursor_frames
    }

    fn decode_next_packet_into(
        &mut self,
        out: &mut Vec<i16>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let frames_left = self.spec.frames_total.saturating_sub(self.cursor_frames);
        if frames_left == 0 {
            out.clear();
            return Ok(false);
        }
        let frames = frames_left.min(WAV_PACKET_FRAMES as u64) as usize;
        let bytes = frames.saturating_mul(self.spec.block_align);
        self.packet_buf.resize(bytes, 0);
        self.reader.read_exact(&mut self.packet_buf)?;
        decode_packet_into(&self.packet_buf, self.spec.encoding, out)?;
        Ok(true)
    }
}

impl Reader {
    pub(super) fn storage_bytes(&self) -> usize {
        self.reader.capacity() + self.packet_buf.capacity()
    }
}
