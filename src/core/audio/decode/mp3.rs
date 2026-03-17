use minimp3::{Decoder, Error, Frame};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub(crate) struct OpenFile {
    pub reader: Reader<BufReader<File>>,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

pub(crate) struct Reader<R> {
    decoder: Decoder<R>,
    channels: usize,
    sample_rate_hz: u32,
    pending: Option<Vec<i16>>,
    cursor_frames: u64,
}

#[inline(always)]
pub(crate) fn path_is_mp3(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mp3"))
}

pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(path)?;
    let mut decoder = Decoder::new(BufReader::new(file));
    let Some(frame) = next_audio_frame(&mut decoder)? else {
        return Err(format!(
            "MP3 '{}' contained no decodable audio frames",
            path.display()
        )
        .into());
    };
    let channels = frame.channels;
    let sample_rate_hz = frame.sample_rate.max(1) as u32;
    Ok(OpenFile {
        reader: Reader {
            decoder,
            channels,
            sample_rate_hz,
            pending: Some(frame.data),
            cursor_frames: 0,
        },
        channels,
        sample_rate_hz,
    })
}

pub(crate) fn file_length_seconds(path: &Path) -> Result<f32, String> {
    let file = File::open(path).map_err(|e| format!("Cannot open file: {e}"))?;
    let mut decoder = Decoder::new(BufReader::new(file));
    let mut total_frames = 0u64;
    let mut sample_rate_hz = 0u32;
    while let Some(frame) = next_audio_frame(&mut decoder).map_err(|e| e.to_string())? {
        let frame_rate_hz = frame.sample_rate.max(1) as u32;
        if sample_rate_hz == 0 {
            sample_rate_hz = frame_rate_hz;
        } else if frame_rate_hz != sample_rate_hz {
            return Err(format!(
                "variable MP3 sample rates are unsupported ({sample_rate_hz} -> {frame_rate_hz})"
            ));
        }
        total_frames =
            total_frames.saturating_add((frame.data.len() / frame.channels.max(1)) as u64);
    }
    if sample_rate_hz == 0 {
        return Err("MP3 contained no decodable audio frames".to_string());
    }
    Ok((total_frames as f64 / f64::from(sample_rate_hz)) as f32)
}

impl<R: std::io::Read> Reader<R> {
    pub(crate) fn read_dec_packet_itl(
        &mut self,
    ) -> Result<Option<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(frame) = self.pending.take() {
            self.cursor_frames = self
                .cursor_frames
                .saturating_add((frame.len() / self.channels.max(1)) as u64);
            return Ok(Some(frame));
        }
        let Some(frame) = next_audio_frame(&mut self.decoder)? else {
            return Ok(None);
        };
        self.validate_frame(&frame)?;
        self.cursor_frames = self
            .cursor_frames
            .saturating_add((frame.data.len() / self.channels.max(1)) as u64);
        Ok(Some(frame.data))
    }

    pub(crate) fn seek_frame(
        &mut self,
        target_frame: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if target_frame <= self.cursor_frames {
            return Ok(());
        }
        while self.cursor_frames < target_frame {
            let frame_data = if let Some(frame) = self.pending.take() {
                frame
            } else {
                let Some(frame) = next_audio_frame(&mut self.decoder)? else {
                    return Ok(());
                };
                self.validate_frame(&frame)?;
                frame.data
            };
            let frame_samples = frame_data.len();
            let frame_frames = frame_samples / self.channels.max(1);
            let remaining = (target_frame - self.cursor_frames) as usize;
            if remaining >= frame_frames {
                self.cursor_frames = self.cursor_frames.saturating_add(frame_frames as u64);
                continue;
            }
            let drop_samples = remaining * self.channels;
            self.pending = Some(frame_data[drop_samples..].to_vec());
            self.cursor_frames = target_frame;
            return Ok(());
        }
        Ok(())
    }

    #[inline(always)]
    fn validate_frame(
        &self,
        frame: &Frame,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let channels = frame.channels.max(1);
        let sample_rate_hz = frame.sample_rate.max(1) as u32;
        if channels != self.channels || sample_rate_hz != self.sample_rate_hz {
            return Err(format!(
                "unsupported MP3 format change ({} ch @ {} Hz -> {} ch @ {} Hz)",
                self.channels, self.sample_rate_hz, channels, sample_rate_hz
            )
            .into());
        }
        Ok(())
    }
}

fn next_audio_frame<R: std::io::Read>(
    decoder: &mut Decoder<R>,
) -> Result<Option<Frame>, Box<dyn std::error::Error + Send + Sync>> {
    loop {
        match decoder.next_frame() {
            Ok(frame) => return Ok(Some(frame)),
            Err(Error::Eof) => return Ok(None),
            Err(Error::InsufficientData) | Err(Error::SkippedData) => continue,
            Err(err) => return Err(format!("MP3 decode failed: {err}").into()),
        }
    }
}
