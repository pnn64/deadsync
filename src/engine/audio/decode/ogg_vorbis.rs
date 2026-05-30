use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_VORBIS, Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

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

pub(crate) struct Reader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    channels: usize,
    // Absolute timestamp of the stream's first sample (codec params start_ts);
    // used as the floor for seek positions.
    start_ts: u64,
    // Absolute timestamp of the first *emitted* audio frame. Frame 0 in our
    // cursor space maps to this timestamp, so seek arithmetic is independent of
    // any encoder pre-skip.
    base_ts: u64,
    sample_buf: Option<SampleBuffer<i16>>,
    sample_buf_frames: u64,
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
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("Cannot probe OGG '{}': {e}", path.display()))?;
    Ok(probed.format)
}

pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let format = probe_format(path)?;

    let (track_id, channels, sample_rate_hz, start_ts, decoder) = {
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec == CODEC_TYPE_VORBIS)
            .ok_or_else(|| format!("OGG '{}' has no Vorbis track", path.display()))?;
        let cp = &track.codec_params;
        let channels = cp.channels.map(|c| c.count() as usize).unwrap_or(0);
        if channels == 0 {
            return Err(format!("OGG '{}' has unknown channel layout", path.display()).into());
        }
        let sample_rate_hz = cp
            .sample_rate
            .ok_or_else(|| format!("OGG '{}' has unknown sample rate", path.display()))?;
        let decoder = symphonia::default::get_codecs()
            .make(cp, &DecoderOptions::default())
            .map_err(|e| format!("Cannot create Vorbis decoder for '{}': {e}", path.display()))?;
        (track.id, channels, sample_rate_hz, cp.start_ts, decoder)
    };

    let mut reader = Reader {
        format,
        decoder,
        track_id,
        channels,
        start_ts,
        base_ts: start_ts,
        sample_buf: None,
        sample_buf_frames: 0,
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
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec == CODEC_TYPE_VORBIS)
            .ok_or_else(|| "OGG file has no Vorbis track".to_string())?;
        let cp = &track.codec_params;
        let sample_rate = cp
            .sample_rate
            .ok_or_else(|| "OGG sample rate is invalid".to_string())?;
        (track.id, sample_rate, cp.start_ts, cp.n_frames)
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
            Ok(packet) => {
                if packet.track_id() == track_id {
                    last_end = last_end.max(packet.ts().saturating_add(packet.dur()));
                }
            }
            Err(SymphoniaError::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(format!("OGG decode failed: {e}")),
        }
    }
    let total = last_end.saturating_sub(start_ts);
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
        let target_ts = self.base_ts.saturating_add(target_frame);

        // Try progressively larger prerolls; a larger window guarantees we land
        // before the target so the post-seek audio reproduces a linear decode.
        for preroll in [SEEK_PREROLL_FRAMES, SEEK_PREROLL_FRAMES * 4] {
            let seek_ts = target_ts.saturating_sub(preroll).max(self.start_ts);
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
        seek_ts: u64,
        target_ts: u64,
        target_frame: u64,
    ) -> Result<SeekOutcome, Box<dyn std::error::Error + Send + Sync>> {
        self.format
            .seek(
                SeekMode::Accurate,
                SeekTo::TimeStamp {
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
            if ts.saturating_add(frames) <= target_ts {
                continue; // Entirely before the target.
            }
            if ts > target_ts {
                // Seek landed after the target; caller retries with more preroll.
                return Ok(SeekOutcome::Overshoot);
            }
            let skip = (target_ts - ts) as usize;
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
    ) -> Result<Option<u64>, Box<dyn std::error::Error + Send + Sync>> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
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
            if packet.track_id() != self.track_id {
                continue;
            }
            let ts = packet.ts();
            let audio = match self.decoder.decode(&packet) {
                Ok(audio) => audio,
                // Recoverable per symphonia's contract: skip and continue.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(format!("OGG decode error: {e}").into()),
            };
            let spec = *audio.spec();
            let frames = audio.capacity() as u64;
            if frames == 0 {
                // Vorbis warmup / priming packet — produces no output frames.
                continue;
            }
            if self.sample_buf.is_none() || self.sample_buf_frames < frames {
                self.sample_buf = Some(SampleBuffer::<i16>::new(frames, spec));
                self.sample_buf_frames = frames;
            }
            let buf = self.sample_buf.as_mut().expect("sample buffer present");
            buf.copy_interleaved_ref(audio);
            out.clear();
            out.extend_from_slice(buf.samples());
            return Ok(Some(ts));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Reader, file_length_seconds, open_file};
    use std::path::PathBuf;

    const SEEK_COMPARE_FRAMES: usize = 4096;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/music/credits.ogg")
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
        if !path.exists() {
            return;
        }
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

    // Characterization test: the symphonia decoder must reproduce the audio
    // fingerprint captured from the original lewton decoder (channels/sample
    // rate exactly; energy metrics within cross-decoder float-rounding
    // tolerance). A real regression (wrong decode, off-by-N seek, dropped
    // warmup, channel swap, truncation) moves a metric far outside tolerance.
    #[test]
    fn matches_golden_fingerprint() {
        let path = fixture_path();
        if !path.exists() {
            return;
        }
        let opened = open_file(&path).expect("open fixture");
        let channels = opened.channels;
        let sample_rate = opened.sample_rate_hz;
        assert_eq!(channels, 2, "channels");
        assert_eq!(sample_rate, 44100, "sample_rate");

        let mut reader = opened.reader;
        let mut packet = Vec::new();
        let mut frame_count: u64 = 0;
        let mut sum_sq = [0f64; 2];
        let mut peak = [0i32; 2];
        loop {
            let more = reader.read_dec_packet_into(&mut packet).expect("decode");
            if !more {
                break;
            }
            let frames = packet.len() / channels;
            frame_count += frames as u64;
            for f in 0..frames {
                for c in 0..channels {
                    let s = i32::from(packet[f * channels + c]);
                    let a = s.abs();
                    if a > peak[c] {
                        peak[c] = a;
                    }
                    sum_sq[c] += (s as f64) * (s as f64);
                }
            }
        }
        let rms = [
            (sum_sq[0] / frame_count as f64).sqrt(),
            (sum_sq[1] / frame_count as f64).sqrt(),
        ];
        let length = file_length_seconds(&path).expect("length");

        // Per-sample (combined-channel) RMS of a 0.25s window after each seek.
        let targets = [90794u64, 138600, 235200];
        const WINDOW: usize = 11025;
        let mut seek_rms = [0f64; 3];
        for (i, &target) in targets.iter().enumerate() {
            let mut sk = open_file(&path).expect("open seek").reader;
            sk.seek_frame(target).expect("seek");
            let window = read_frames(&mut sk, WINDOW);
            let mut ss = 0f64;
            for v in &window {
                ss += (f64::from(*v)) * (f64::from(*v));
            }
            seek_rms[i] = (ss / window.len() as f64).sqrt();
        }

        eprintln!(
            "ogg golden actuals: frame_count={frame_count} length={length} \
             peak={peak:?} rms={rms:?} seek_rms={seek_rms:?}"
        );

        // channels / sample_rate already asserted exactly above.
        // frame_count: tolerate at most one max Vorbis block (8192 frames) of
        // difference for end-of-stream trimming; catch any gross truncation.
        let frame_diff = frame_count as i64 - 2_892_139;
        assert!(
            (-8192..=8192).contains(&frame_diff),
            "frame_count={frame_count} (diff {frame_diff})"
        );
        assert!((length - 65.58138).abs() < 0.05, "length={length}");
        for c in 0..channels {
            assert!((peak[c] - 32768).abs() <= 64, "peak[{c}]={}", peak[c]);
        }
        let golden_rms = [6302.808355648021, 6223.901709056613];
        for c in 0..channels {
            let rel = (rms[c] - golden_rms[c]).abs() / golden_rms[c];
            assert!(rel < 0.01, "rms[{c}]={} rel={rel}", rms[c]);
        }
        let golden_seek_rms = [3122.937572171839, 5679.57538944586, 1719.3614305311685];
        for i in 0..3 {
            let rel = (seek_rms[i] - golden_seek_rms[i]).abs() / golden_seek_rms[i];
            assert!(rel < 0.02, "seek_rms[{i}]={} rel={rel}", seek_rms[i]);
        }
    }
}
