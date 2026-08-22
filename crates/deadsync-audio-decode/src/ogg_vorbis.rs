use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
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
const OGG_PAGE_HEADER_LEN: usize = 27;
const OGG_CAPTURE_PATTERN: &[u8; 4] = b"OggS";
// The affected legacy encoder exposes its missing-granule layout immediately
// (Pandemonium's first bad page is sequence 3). Keep this compatibility probe
// bounded so opening an ordinary preview never reads the whole song first.
const LEGACY_GRANULE_PROBE_LEN: usize = 64 * 1024;

pub(crate) struct OpenFile {
    pub reader: Reader,
    pub channels: usize,
    pub sample_rate_hz: u32,
    pub frames_total_hint: Option<u64>,
}

pub struct Reader {
    path: PathBuf,
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
    legacy_missing_granules: bool,
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

// Ogg requires a granule position of u64::MAX only when a page completes no
// packets. Some old encoders also used it on pages that complete ordinary
// audio packets. Symphonia maps that value to timestamp -1 and trims every
// packet completed on the page to zero frames when gapless decoding is on.
fn has_legacy_missing_granules_in_prefix(path: &Path) -> std::io::Result<bool> {
    let mut prefix = Vec::with_capacity(LEGACY_GRANULE_PROBE_LEN);
    File::open(path)?
        .take(LEGACY_GRANULE_PROBE_LEN as u64)
        .read_to_end(&mut prefix)?;

    let mut page_start = 0_usize;
    loop {
        let Some(header_end) = page_start.checked_add(OGG_PAGE_HEADER_LEN) else {
            return Ok(false);
        };
        let Some(header) = prefix.get(page_start..header_end) else {
            return Ok(false);
        };
        if &header[..4] != OGG_CAPTURE_PATTERN || header[4] != 0 {
            return Ok(false);
        }

        let segment_count = usize::from(header[26]);
        let Some(segment_end) = header_end.checked_add(segment_count) else {
            return Ok(false);
        };
        let Some(segments) = prefix.get(header_end..segment_end) else {
            return Ok(false);
        };
        let missing_granule = header[6..14].iter().all(|&byte| byte == u8::MAX);
        if missing_granule && segments.iter().any(|&length| length < u8::MAX) {
            return Ok(true);
        }

        let body_len = segments.iter().map(|&length| usize::from(length)).sum();
        let Some(next_page) = segment_end.checked_add(body_len) else {
            return Ok(false);
        };
        if next_page > prefix.len() {
            return Ok(false);
        }
        page_start = next_page;
    }
}

fn vorbis_track(tracks: &[Track]) -> Option<(&Track, &AudioCodecParameters)> {
    tracks.iter().find_map(|track| {
        let params = track.codec_params.as_ref()?.audio()?;
        (params.codec == CODEC_ID_VORBIS).then_some((track, params))
    })
}

pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let legacy_missing_granules = has_legacy_missing_granules_in_prefix(path)?;
    let format = probe_format(path)?;

    let (track_id, channels, sample_rate_hz, frames_total_hint, start_ts, decoder) = {
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
        (
            track.id,
            channels,
            sample_rate_hz,
            track.num_frames,
            track.start_ts,
            decoder,
        )
    };

    let mut reader = Reader {
        path: path.to_owned(),
        format,
        decoder,
        track_id,
        channels,
        start_ts,
        base_ts: start_ts,
        pending: None,
        cursor_frames: 0,
        legacy_missing_granules,
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
        frames_total_hint,
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
    if has_legacy_missing_granules_in_prefix(path)
        .map_err(|e| format!("Cannot inspect OGG file: {e}"))?
    {
        // Packet timestamps are unreliable in these streams. The reader uses
        // a frame-counted seek path instead, which does not need snapping.
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

        // Legacy streams can still seek quickly when Symphonia lands on a page
        // with a valid granule. If both attempts overshoot, their timestamps are
        // unreliable in this region, so retain the exact frame-counted fallback.
        if self.legacy_missing_granules {
            return self.seek_from_start_by_frame(target_frame);
        }

        // Final fallback for ordinary streams: decode from the stream start.
        // The target is always >= base_ts, so this cannot overshoot.
        self.seek_and_collect(self.start_ts, target_ts, target_frame)?;
        Ok(())
    }

    #[inline(always)]
    pub(crate) const fn current_frame(&self) -> u64 {
        self.cursor_frames
    }

    fn seek_from_start_by_frame(
        &mut self,
        target_frame: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let reopened = open_file(&self.path)?;
        *self = reopened.reader;
        let mut scratch = Vec::new();
        let mut decoded_frames = 0_u64;

        loop {
            let has_packet = if let Some(mut pending) = self.pending.take() {
                std::mem::swap(&mut scratch, &mut pending);
                true
            } else {
                self.next_audio_packet(&mut scratch)?.is_some()
            };
            if !has_packet {
                self.pending = None;
                self.cursor_frames = target_frame;
                return Ok(());
            }
            let frames = (scratch.len() / self.channels) as u64;
            let packet_end = decoded_frames.saturating_add(frames);
            if packet_end <= target_frame {
                decoded_frames = packet_end;
                continue;
            }

            let skip_frames = target_frame.saturating_sub(decoded_frames) as usize;
            crate::resample::drop_front_samples(
                &mut scratch,
                skip_frames.saturating_mul(self.channels),
            );
            self.pending = Some(scratch);
            self.cursor_frames = target_frame;
            return Ok(());
        }
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
        let mut scratch = crate::resample::take_cleared_i16(&mut self.pending);
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
            let mut packet = match self.format.next_packet() {
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
            if self.legacy_missing_granules
                && packet.dur == Duration::ZERO
                && packet.trim_end > Duration::ZERO
            {
                // A completed packet on a legacy -1-granule page is valid
                // audio, not end padding. Restore the duration Symphonia moved
                // wholesale into trim_end so gapless decode keeps the frames.
                packet.dur = packet.trim_end;
                packet.trim_end = Duration::ZERO;
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
    use super::{
        OGG_CAPTURE_PATTERN, OGG_PAGE_HEADER_LEN, Reader, has_legacy_missing_granules_in_prefix,
        open_file, snap_start_forward_to_packet,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use symphonia::core::checksum::Crc32;
    use symphonia::core::io::Monitor;

    const SEEK_COMPARE_FRAMES: usize = 4096;
    static TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

    struct TempFixture(PathBuf);

    impl TempFixture {
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

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

    fn read_all_samples(reader: &mut Reader) -> Vec<i16> {
        let mut packet = Vec::new();
        let mut out = Vec::new();
        while reader
            .read_dec_packet_into(&mut packet)
            .expect("decode packet")
        {
            out.extend_from_slice(&packet);
        }
        out
    }

    fn missing_granule_fixture() -> TempFixture {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/sounds/boom.ogg");
        let mut bytes = fs::read(&source).expect("read OGG fixture");
        let mut offset = 0_usize;
        let mut modified = false;

        while offset + OGG_PAGE_HEADER_LEN <= bytes.len() {
            assert_eq!(&bytes[offset..offset + 4], OGG_CAPTURE_PATTERN);
            let segment_count = usize::from(bytes[offset + 26]);
            let segment_start = offset + OGG_PAGE_HEADER_LEN;
            let segment_end = segment_start + segment_count;
            let body_len = bytes[segment_start..segment_end]
                .iter()
                .map(|&length| usize::from(length))
                .sum::<usize>();
            let page_end = segment_end + body_len;
            assert!(page_end <= bytes.len());

            let sequence = u32::from_le_bytes(
                bytes[offset + 18..offset + 22]
                    .try_into()
                    .expect("page sequence"),
            );
            if sequence == 2 {
                bytes[offset + 6..offset + 14].fill(u8::MAX);
                bytes[offset + 22..offset + 26].fill(0);
                let mut crc = Crc32::new(0);
                crc.process_buf_bytes(&bytes[offset..page_end]);
                bytes[offset + 22..offset + 26].copy_from_slice(&crc.crc().to_le_bytes());
                modified = true;
            }
            offset = page_end;
        }

        assert!(modified, "fixture has no target audio page");
        let id = TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "deadsync-missing-ogg-granule-{}-{id}.ogg",
            std::process::id()
        ));
        fs::write(&path, bytes).expect("write malformed OGG fixture");
        TempFixture(path)
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

    #[test]
    fn completed_packets_with_missing_granules_keep_audio_and_seek_exactly() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/sounds/boom.ogg");
        assert!(!has_legacy_missing_granules_in_prefix(&source).expect("inspect source fixture"));

        let mut opened = open_file(&source).expect("open source fixture");
        let channels = opened.channels;
        let expected = read_all_samples(&mut opened.reader);

        let malformed = missing_granule_fixture();
        assert!(
            has_legacy_missing_granules_in_prefix(malformed.path())
                .expect("inspect malformed fixture")
        );
        assert_eq!(
            snap_start_forward_to_packet(malformed.path(), 0.1).expect("snap malformed fixture"),
            None
        );

        let mut opened = open_file(malformed.path()).expect("open malformed fixture");
        let actual = read_all_samples(&mut opened.reader);
        assert_eq!(actual, expected);

        let target = 8_000_usize;
        let frames = 512_usize;
        let mut seeked = open_file(malformed.path())
            .expect("open seek fixture")
            .reader;
        // This target lands through a valid-granule page. Make reopening
        // impossible so the test also guards the fast legacy seek path.
        seeked.path = malformed.path().with_extension("unavailable");
        assert!(!seeked.path.is_file());
        seeked.seek_frame(target as u64).expect("seek fixture");
        let actual = read_frames(&mut seeked, frames);
        assert_eq!(
            actual,
            expected[target * channels..(target + frames) * channels]
        );
    }
}
