use std::fs::File;
use std::path::Path;
use symphonia::core::codecs::audio::{
    AudioCodecParameters, AudioDecoder, AudioDecoderOptions, well_known::CODEC_ID_FLAC,
};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Duration, Timestamp};

// FLAC is lossless and decodes sample-accurately, so a seek that lands on an
// earlier block and discards frames up to the target reproduces a linear decode
// exactly. We still seek a little ahead of the target (one block of preroll) and
// retry with a larger window so we always land before the target, mirroring the
// Vorbis decoder for consistency.
const SEEK_PREROLL_FRAMES: u64 = 1 << 14;

pub(crate) struct OpenFile {
    pub reader: Reader,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

pub(crate) struct Reader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    channels: usize,
    // Absolute timestamp of the stream's first sample (codec params start_ts);
    // used as the floor for seek positions.
    start_ts: Timestamp,
    // Absolute timestamp of the first *emitted* audio frame. Frame 0 in our
    // cursor space maps to this timestamp, so seek arithmetic is independent of
    // any encoder pre-skip.
    base_ts: Timestamp,
    pending: Option<Vec<i16>>,
    cursor_frames: u64,
}

enum SeekOutcome {
    Landed,
    Overshoot,
}

#[inline(always)]
pub(crate) fn path_is_flac(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("flac"))
}

fn probe_format(
    path: &Path,
) -> Result<Box<dyn FormatReader>, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("flac");
    symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| format!("Cannot probe FLAC '{}': {e}", path.display()).into())
}

fn flac_track(tracks: &[Track]) -> Option<(&Track, &AudioCodecParameters)> {
    tracks.iter().find_map(|track| {
        let params = track.codec_params.as_ref()?.audio()?;
        (params.codec == CODEC_ID_FLAC).then_some((track, params))
    })
}

pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let format = probe_format(path)?;

    let (track_id, channels, sample_rate_hz, start_ts, decoder) = {
        let (track, cp) = flac_track(format.tracks())
            .ok_or_else(|| format!("FLAC '{}' has no FLAC track", path.display()))?;
        let channels = cp.channels.as_ref().map(|c| c.count()).unwrap_or(0);
        if channels == 0 {
            return Err(format!("FLAC '{}' has unknown channel layout", path.display()).into());
        }
        let sample_rate_hz = cp
            .sample_rate
            .ok_or_else(|| format!("FLAC '{}' has unknown sample rate", path.display()))?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(cp, &AudioDecoderOptions::default())
            .map_err(|e| format!("Cannot create FLAC decoder for '{}': {e}", path.display()))?;
        (track.id, channels, sample_rate_hz, track.start_ts, decoder)
    };

    let mut reader = Reader {
        format,
        decoder,
        track_id,
        channels,
        start_ts,
        base_ts: start_ts,
        pending: None,
        cursor_frames: 0,
    };

    // Prime the first audio packet so linear reads start at the true first
    // sample, and record its timestamp as the frame origin for seeks.
    let mut first = Vec::new();
    match reader.next_audio_packet(&mut first)? {
        Some(ts) => {
            reader.base_ts = ts;
            reader.pending = Some(first);
        }
        None => {
            return Err(format!(
                "FLAC '{}' contained no decodable audio frames",
                path.display()
            )
            .into());
        }
    }

    Ok(OpenFile {
        reader,
        channels,
        sample_rate_hz,
    })
}

pub(crate) fn file_length_seconds(path: &Path) -> Result<f32, String> {
    let mut format = probe_format(path).map_err(|e| format!("Cannot open FLAC file: {e}"))?;

    let (track_id, sample_rate, start_ts, n_frames) = {
        let (track, cp) =
            flac_track(format.tracks()).ok_or_else(|| "FLAC file has no FLAC track".to_string())?;
        let sample_rate = cp
            .sample_rate
            .ok_or_else(|| "FLAC sample rate is invalid".to_string())?;
        (track.id, sample_rate, track.start_ts, track.num_frames)
    };
    if sample_rate == 0 {
        return Err("FLAC sample rate is invalid (0)".to_string());
    }

    if let Some(n_frames) = n_frames {
        return Ok((n_frames as f64 / f64::from(sample_rate)) as f32);
    }

    // Fallback: demux (without decoding) and track the maximum end timestamp.
    let mut last_end = start_ts;
    loop {
        match format.next_packet() {
            Ok(Some(packet)) => {
                if packet.track_id == track_id {
                    last_end = last_end.max(packet.pts.saturating_add(packet.dur));
                }
            }
            Ok(None) => break,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(format!("FLAC decode failed: {e}")),
        }
    }
    let total = last_end.duration_from(start_ts).map_or(0, Duration::get);
    Ok((total as f64 / f64::from(sample_rate)) as f32)
}

impl Reader {
    pub(crate) fn read_dec_packet_into(
        &mut self,
        out: &mut Vec<i16>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(mut packet) = self.pending.take() {
            std::mem::swap(out, &mut packet);
            self.cursor_frames = self
                .cursor_frames
                .saturating_add((out.len() / self.channels) as u64);
            return Ok(true);
        }
        match self.next_audio_packet(out)? {
            Some(_ts) => {
                self.cursor_frames = self
                    .cursor_frames
                    .saturating_add((out.len() / self.channels) as u64);
                Ok(true)
            }
            None => {
                out.clear();
                Ok(false)
            }
        }
    }

    pub(crate) fn seek_frame(
        &mut self,
        target_frame: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let target_ts = self.base_ts.saturating_add(Duration::new(target_frame));

        // Try progressively larger prerolls; a larger window guarantees we land
        // before the target so the post-seek audio reproduces a linear decode.
        for preroll in [SEEK_PREROLL_FRAMES, SEEK_PREROLL_FRAMES * 4] {
            let seek_ts = target_ts
                .saturating_sub(Duration::new(preroll))
                .max(self.start_ts);
            match self.seek_and_collect(seek_ts, target_ts, target_frame)? {
                SeekOutcome::Landed => return Ok(()),
                SeekOutcome::Overshoot => continue,
            }
        }

        // Final fallback: decode from the very start of the stream. The target
        // is always >= base_ts, so decoding from start_ts can never overshoot.
        self.seek_and_collect(self.start_ts, target_ts, target_frame)?;
        Ok(())
    }

    #[inline(always)]
    pub(crate) const fn current_frame(&self) -> u64 {
        self.cursor_frames
    }

    fn seek_and_collect(
        &mut self,
        seek_ts: Timestamp,
        target_ts: Timestamp,
        target_frame: u64,
    ) -> Result<SeekOutcome, Box<dyn std::error::Error + Send + Sync>> {
        self.format
            .seek(
                SeekMode::Accurate,
                SeekTo::Timestamp {
                    ts: seek_ts,
                    track_id: self.track_id,
                },
            )
            .map_err(|e| format!("FLAC seek error: {e}"))?;
        self.decoder.reset();
        self.pending = None;

        let mut scratch = Vec::new();
        loop {
            let ts = match self.next_audio_packet(&mut scratch)? {
                Some(ts) => ts,
                None => {
                    // Target is at or past the end of the stream; clamp.
                    self.cursor_frames = target_frame;
                    self.pending = None;
                    return Ok(SeekOutcome::Landed);
                }
            };
            let frames = (scratch.len() / self.channels) as u64;
            if ts.saturating_add(Duration::new(frames)) <= target_ts {
                continue; // Entirely before the target.
            }
            if ts > target_ts {
                // Seek landed after the target; caller retries with more preroll.
                return Ok(SeekOutcome::Overshoot);
            }
            let skip = target_ts.duration_from(ts).map_or(0, Duration::get) as usize;
            let drop_samples = skip * self.channels;
            self.pending = Some(scratch[drop_samples..].to_vec());
            self.cursor_frames = target_frame;
            return Ok(SeekOutcome::Landed);
        }
    }

    // Reads, decodes and interleaves the next non-empty audio packet for our
    // track into `out`, returning its absolute timestamp. Returns `None` at end
    // of stream. All field accesses are direct (no `&mut self` helper call while
    // the decoded buffer borrows `self.decoder`) to satisfy the borrow checker.
    fn next_audio_packet(
        &mut self,
        out: &mut Vec<i16>,
    ) -> Result<Option<Timestamp>, Box<dyn std::error::Error + Send + Sync>> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(None),
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(SymphoniaError::ResetRequired) => return Ok(None),
                Err(e) => return Err(format!("FLAC read error: {e}").into()),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let ts = packet.pts;
            let audio = match self.decoder.decode(&packet) {
                Ok(audio) => audio,
                // Recoverable per symphonia's contract: skip and continue.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(format!("FLAC decode error: {e}").into()),
            };
            let frames = audio.frames() as u64;
            if frames == 0 {
                // Defensive: an empty decoded buffer produces no output frames.
                continue;
            }
            out.clear();
            audio.copy_to_vec_interleaved::<i16>(out);
            return Ok(Some(ts));
        }
    }
}
