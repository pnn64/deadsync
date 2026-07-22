use std::fs::File;
use std::path::Path;
use symphonia::core::codecs::audio::{
    AudioCodecParameters, AudioDecoder, AudioDecoderOptions, well_known::CODEC_ID_MP3,
};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Duration, Timestamp};

// Decode at least this many frames before a seek target so the MP3 bit
// reservoir and polyphase filterbank state are primed and post-seek audio
// approximates a linear decode. MP3 granules are 576 frames and the bit
// reservoir spans only a handful of frames, so this window is generous; we
// still retry with a larger window (and finally from the stream start) for
// safety.
const SEEK_PREROLL_FRAMES: u64 = 1 << 14;

pub(crate) struct OpenFile {
    pub reader: Reader,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

pub struct Reader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    channels: usize,
    // Absolute timestamp of the stream's first sample (track start_ts); used as
    // the floor for seek positions.
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
pub(crate) fn path_is_mp3(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mp3"))
}

fn probe_format(
    path: &Path,
) -> Result<Box<dyn FormatReader>, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| format!("Cannot probe MP3 '{}': {e}", path.display()).into())
}

fn mp3_track(tracks: &[Track]) -> Option<(&Track, &AudioCodecParameters)> {
    tracks.iter().find_map(|track| {
        let params = track.codec_params.as_ref()?.audio()?;
        (params.codec == CODEC_ID_MP3).then_some((track, params))
    })
}

pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let format = probe_format(path)?;

    let (track_id, channels, sample_rate_hz, start_ts, decoder) = {
        let (track, cp) = mp3_track(format.tracks())
            .ok_or_else(|| format!("MP3 '{}' has no MP3 track", path.display()))?;
        let channels = cp.channels.as_ref().map(|c| c.count()).unwrap_or(0);
        if channels == 0 {
            return Err(format!("MP3 '{}' has unknown channel layout", path.display()).into());
        }
        let sample_rate_hz = cp
            .sample_rate
            .ok_or_else(|| format!("MP3 '{}' has unknown sample rate", path.display()))?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(cp, &AudioDecoderOptions::default())
            .map_err(|e| format!("Cannot create MP3 decoder for '{}': {e}", path.display()))?;
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
                "MP3 '{}' contained no decodable audio frames",
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
    let mut format = probe_format(path).map_err(|e| format!("Cannot open MP3 file: {e}"))?;

    let (track_id, sample_rate, n_frames, decoder) = {
        let (track, cp) =
            mp3_track(format.tracks()).ok_or_else(|| "MP3 file has no MP3 track".to_string())?;
        let sample_rate = cp
            .sample_rate
            .ok_or_else(|| "MP3 sample rate is invalid".to_string())?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(cp, &AudioDecoderOptions::default())
            .map_err(|e| format!("Cannot create MP3 decoder: {e}"))?;
        (track.id, sample_rate, track.num_frames, decoder)
    };
    if sample_rate == 0 {
        return Err("MP3 sample rate is invalid (0)".to_string());
    }

    if let Some(n_frames) = n_frames {
        return Ok((n_frames as f64 / f64::from(sample_rate)) as f32);
    }

    // Fallback: decode the whole stream and count the actual emitted frames.
    // Headerless/VBR MP3 files may lack a frame-count hint, so we count decoded
    // samples to match the duration the player will observe on playback.
    let mut decoder = decoder;
    let mut total_frames = 0u64;
    loop {
        match format.next_packet() {
            Ok(Some(packet)) => {
                if packet.track_id != track_id {
                    continue;
                }
                match decoder.decode(&packet) {
                    Ok(audio) => {
                        total_frames = total_frames.saturating_add(audio.frames() as u64);
                    }
                    Err(SymphoniaError::DecodeError(_)) => continue,
                    Err(e) => return Err(format!("MP3 decode failed: {e}")),
                }
            }
            Ok(None) => break,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(format!("MP3 decode failed: {e}")),
        }
    }
    Ok((total_frames as f64 / f64::from(sample_rate)) as f32)
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
            .map_err(|e| format!("MP3 seek error: {e}"))?;
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
            crate::resample::drop_front_samples(&mut scratch, drop_samples);
            self.pending = Some(scratch);
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
                // A reset request indicates a new logical stream; game music is
                // single-stream so we treat it as end-of-audio.
                Err(SymphoniaError::ResetRequired) => return Ok(None),
                Err(e) => return Err(format!("MP3 read error: {e}").into()),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let ts = packet.pts;
            let audio = match self.decoder.decode(&packet) {
                Ok(audio) => audio,
                // Recoverable per symphonia's contract: skip and continue.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(format!("MP3 decode error: {e}").into()),
            };
            let frames = audio.frames() as u64;
            if frames == 0 {
                // Empty / priming packet - produces no output frames.
                continue;
            }
            out.clear();
            audio.copy_to_vec_interleaved::<i16>(out);
            return Ok(Some(ts));
        }
    }
}
