use std::fs::File;
use std::path::Path;
use symphonia::core::codecs::audio::{
    AudioCodecParameters, AudioDecoder, AudioDecoderOptions, well_known::CODEC_ID_VORBIS,
};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Duration, Timestamp};

// Decode at least this many frames before a seek target so the Vorbis MDCT
// overlap is primed and post-seek audio matches a linear decode. Vorbis blocks
// are at most 8192 frames, so one block of preroll is sufficient; we still retry
// with a larger window (and finally from the stream start) for safety.
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
pub(crate) fn path_is_ogg_vorbis(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ogg") || ext.eq_ignore_ascii_case("oga"))
}

fn probe_format(
    path: &Path,
) -> Result<Box<dyn FormatReader>, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| format!("Cannot probe OGG '{}': {e}", path.display()).into())
}

fn vorbis_track(tracks: &[Track]) -> Option<(&Track, &AudioCodecParameters)> {
    tracks.iter().find_map(|track| {
        let params = track.codec_params.as_ref()?.audio()?;
        (params.codec == CODEC_ID_VORBIS).then_some((track, params))
    })
}

pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let format = probe_format(path)?;

    let (track_id, channels, sample_rate_hz, start_ts, decoder) = {
        let (track, cp) = vorbis_track(format.tracks())
            .ok_or_else(|| format!("OGG '{}' has no Vorbis track", path.display()))?;
        let channels = cp.channels.as_ref().map(|c| c.count()).unwrap_or(0);
        if channels == 0 {
            return Err(format!("OGG '{}' has unknown channel layout", path.display()).into());
        }
        let sample_rate_hz = cp
            .sample_rate
            .ok_or_else(|| format!("OGG '{}' has unknown sample rate", path.display()))?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(cp, &AudioDecoderOptions::default())
            .map_err(|e| format!("Cannot create Vorbis decoder for '{}': {e}", path.display()))?;
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
                "OGG '{}' contained no decodable audio frames",
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
    let mut format = probe_format(path).map_err(|e| format!("Cannot open OGG file: {e}"))?;

    let (track_id, sample_rate, start_ts, n_frames) = {
        let (track, cp) = vorbis_track(format.tracks())
            .ok_or_else(|| "OGG file has no Vorbis track".to_string())?;
        let sample_rate = cp
            .sample_rate
            .ok_or_else(|| "OGG sample rate is invalid".to_string())?;
        (track.id, sample_rate, track.start_ts, track.num_frames)
    };
    if sample_rate == 0 {
        return Err("OGG sample rate is invalid (0)".to_string());
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
            Err(e) => return Err(format!("OGG decode failed: {e}")),
        }
    }
    let total = last_end.duration_from(start_ts).map_or(0, Duration::get);
    Ok((total as f64 / f64::from(sample_rate)) as f32)
}

pub(crate) fn snap_start_forward_to_packet(
    path: &Path,
    start_sec: f64,
) -> Result<Option<f64>, String> {
    if !start_sec.is_finite() || start_sec <= 0.0 {
        return Ok(None);
    }

    let mut format = probe_format(path).map_err(|e| format!("Cannot open OGG file: {e}"))?;
    let (track_id, sample_rate) = {
        let (track, cp) = vorbis_track(format.tracks())
            .ok_or_else(|| "OGG file has no Vorbis track".to_string())?;
        let sample_rate = cp
            .sample_rate
            .ok_or_else(|| "OGG sample rate is invalid".to_string())?;
        (track.id, sample_rate)
    };
    if sample_rate == 0 {
        return Err("OGG sample rate is invalid (0)".to_string());
    }

    let target_frame = (start_sec * f64::from(sample_rate)).ceil().max(0.0) as u64;
    let Some(base_ts) = next_packet_start_ts(&mut format, track_id)? else {
        return Ok(None);
    };
    let target_ts = base_ts.saturating_add(Duration::new(target_frame));
    let seeked = format.seek(
        SeekMode::Accurate,
        SeekTo::Timestamp {
            ts: target_ts,
            track_id,
        },
    );
    if seeked.is_err() {
        format = probe_format(path).map_err(|e| format!("Cannot reopen OGG file: {e}"))?;
        let _ = next_packet_start_ts(&mut format, track_id)?;
    }

    loop {
        let Some(ts) = next_packet_start_ts(&mut format, track_id)? else {
            return Ok(None);
        };
        let Some(frame) = ts.duration_from(base_ts).map(Duration::get) else {
            continue;
        };
        if frame >= target_frame {
            return Ok(Some(frame as f64 / f64::from(sample_rate)));
        }
    }
}

fn next_packet_start_ts(
    format: &mut Box<dyn FormatReader>,
    track_id: u32,
) -> Result<Option<Timestamp>, String> {
    loop {
        match format.next_packet() {
            Ok(Some(packet)) if packet.track_id == track_id => return Ok(Some(packet.pts)),
            Ok(Some(_)) => continue,
            Ok(None) => return Ok(None),
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(SymphoniaError::ResetRequired) => return Ok(None),
            Err(e) => return Err(format!("OGG read failed: {e}")),
        }
    }
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
            .map_err(|e| format!("OGG seek error: {e}"))?;
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
                // A chained/linked OGG stream needs a decoder reset; we treat the
                // end of the first logical stream as end-of-audio (game music is
                // single-stream).
                Err(SymphoniaError::ResetRequired) => return Ok(None),
                Err(e) => return Err(format!("OGG read error: {e}").into()),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            let ts = packet.pts;
            let audio = match self.decoder.decode(&packet) {
                Ok(audio) => audio,
                // Recoverable per symphonia's contract: skip and continue.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(format!("OGG decode error: {e}").into()),
            };
            let frames = audio.frames() as u64;
            if frames == 0 {
                // Vorbis warmup / priming packet - produces no output frames.
                continue;
            }
            out.clear();
            audio.copy_to_vec_interleaved::<i16>(out);
            return Ok(Some(ts));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Reader, open_file, snap_start_forward_to_packet};
    use std::path::PathBuf;

    const SEEK_COMPARE_FRAMES: usize = 4096;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/music/credits.ogg")
    }

    fn read_frames(reader: &mut Reader, frames: usize) -> Vec<i16> {
        let mut packet = Vec::new();
        let channels = reader.channels;
        let mut out = Vec::with_capacity(frames * channels);
        while out.len() < frames * channels {
            let more = reader
                .read_dec_packet_into(&mut packet)
                .expect("decode packet");
            if !more {
                break;
            }
            out.extend_from_slice(&packet);
        }
        out.truncate(frames * channels);
        out
    }

    #[test]
    fn seek_matches_linear_decode_after_warmup() {
        let path = fixture_path();
        assert!(
            path.is_file(),
            "missing bundled fixture: {}",
            path.display()
        );
        let opened = open_file(&path).expect("open fixture");
        let channels = opened.channels;
        let sample_rate = opened.sample_rate_hz as usize;
        let targets = [
            sample_rate * 2 + sample_rate / 17,
            sample_rate * 3 + sample_rate / 7,
            sample_rate * 5 + sample_rate / 3,
        ];

        for target in targets {
            let mut full = open_file(&path).expect("open full fixture").reader;
            let expected = read_frames(&mut full, target + SEEK_COMPARE_FRAMES);
            if expected.len() < (target + SEEK_COMPARE_FRAMES) * channels {
                continue;
            }
            let expected =
                expected[target * channels..(target + SEEK_COMPARE_FRAMES) * channels].to_vec();

            let mut seeked = open_file(&path).expect("open seek fixture").reader;
            seeked.seek_frame(target as u64).expect("seek fixture");
            assert_eq!(seeked.current_frame(), target as u64);
            let actual = read_frames(&mut seeked, SEEK_COMPARE_FRAMES);

            assert_eq!(actual, expected, "seek target frame {target}");
        }
    }

    #[test]
    fn packet_snap_never_moves_start_earlier() {
        let path = fixture_path();
        assert!(
            path.is_file(),
            "missing bundled fixture: {}",
            path.display()
        );

        for target in [0.25, 1.0, 2.125, 3.5] {
            let snapped = snap_start_forward_to_packet(&path, target)
                .expect("snap packet start")
                .expect("packet boundary");

            assert!(snapped >= target, "target={target} snapped={snapped}");
            assert!(
                snapped - target <= 0.25,
                "target={target} snapped={snapped}"
            );
        }
    }
}
