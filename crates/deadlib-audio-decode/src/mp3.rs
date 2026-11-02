use crate::DecodeOptions;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use symphonia::core::codecs::audio::{
    AudioCodecParameters, AudioDecoder, AudioDecoderOptions, well_known::CODEC_ID_MP3,
};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Duration, Timestamp};

// Decode at least this many frames before a seek target so the MP3 bit
// reservoir and polyphase filterbank state are primed and post-seek audio
// approximates a linear decode. MP3 granules are 576 frames and the bit
// reservoir spans only a handful of frames, so this window is generous; we
// still retry with a larger window (and finally from the stream start) for
// safety.
const SEEK_PREROLL_FRAMES: u64 = 1 << 14;
// Bound the first-frame metadata probe to 25,000 non-ID3 bytes.
const FIRST_FRAME_SCAN_BYTES: usize = 25_000;

pub(crate) struct OpenFile {
    pub reader: Reader,
    pub channels: usize,
    pub sample_rate_hz: u32,
    pub frames_total_hint: Option<u64>,
}

pub struct Reader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    channels: usize,
    // Timestamp of the first emitted audio sample, after optional gapless trimming.
    base_ts: Timestamp,
    gapless: bool,
    // Optional silence representing an Info metadata frame removed by the demuxer.
    info_lead_frames: u64,
    info_lead_pending: u64,
    pending: Option<Vec<i16>>,
    cursor_frames: u64,
}

#[derive(Clone, Copy)]
struct Mpeg3Header {
    version: u8,
    sample_rate_index: u8,
    frame_bytes: usize,
    frame_samples: u64,
    side_info_bytes: usize,
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
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    deadlib_platform::logging::with_file_context(path, || {
        symphonia::default::get_probe()
            .probe(
                &hint,
                mss,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|e| format!("Cannot probe MP3 '{}': {e}", path.display()).into())
    })
}

fn parse_mpeg3_header(bytes: &[u8]) -> Option<Mpeg3Header> {
    let header = u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?);
    if header >> 21 != 0x7ff {
        return None;
    }

    let version = ((header >> 19) & 0x3) as u8;
    let layer = ((header >> 17) & 0x3) as u8;
    let bitrate_index = ((header >> 12) & 0xf) as usize;
    let sample_rate_index = ((header >> 10) & 0x3) as usize;
    if version == 1 || layer != 1 || matches!(bitrate_index, 0 | 15) || sample_rate_index == 3 {
        return None;
    }

    const MPEG1_BITRATES: [u32; 16] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    const MPEG2_BITRATES: [u32; 16] = [
        0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
    ];
    const MPEG1_RATES: [u32; 3] = [44_100, 48_000, 32_000];

    let mpeg1 = version == 3;
    let bitrate_kbps =
        [MPEG2_BITRATES[bitrate_index], MPEG1_BITRATES[bitrate_index]][mpeg1 as usize];
    let rate_divisor = [4, 0, 2, 1][version as usize];
    let sample_rate = MPEG1_RATES[sample_rate_index] / rate_divisor;
    let factor = if mpeg1 { 144 } else { 72 };
    let padding = (header >> 9) & 1;
    let frame_bytes = (factor * bitrate_kbps * 1_000 / sample_rate + padding) as usize;
    let mono = (header >> 6) & 0x3 == 3;
    let side_info_bytes = match (mpeg1, mono) {
        (true, true) => 17,
        (true, false) => 32,
        (false, true) => 9,
        (false, false) => 17,
    };

    Some(Mpeg3Header {
        version,
        sample_rate_index: sample_rate_index as u8,
        frame_bytes,
        frame_samples: if mpeg1 { 1_152 } else { 576 },
        side_info_bytes,
    })
}

fn compatible_frame(first: Mpeg3Header, next: Mpeg3Header) -> bool {
    first.version == next.version && first.sample_rate_index == next.sample_rate_index
}

fn info_frames_in_prefix(bytes: &[u8]) -> u64 {
    for offset in 0..bytes.len().saturating_sub(4) {
        let Some(header) = parse_mpeg3_header(&bytes[offset..]) else {
            continue;
        };
        let next_offset = offset.saturating_add(header.frame_bytes);
        let Some(next) = bytes.get(next_offset..).and_then(parse_mpeg3_header) else {
            continue;
        };
        if !compatible_frame(header, next) {
            continue;
        }

        let info_offset = offset
            .saturating_add(4)
            .saturating_add(header.side_info_bytes);
        return if bytes.get(info_offset..info_offset.saturating_add(4)) == Some(b"Info") {
            header.frame_samples
        } else {
            0
        };
    }
    0
}

fn skip_id3v2(file: &mut File) -> std::io::Result<u64> {
    let mut offset = 0u64;
    loop {
        file.seek(SeekFrom::Start(offset))?;
        let mut header = [0u8; 10];
        if let Err(error) = file.read_exact(&mut header) {
            return if error.kind() == std::io::ErrorKind::UnexpectedEof {
                Ok(offset)
            } else {
                Err(error)
            };
        }
        if &header[..3] != b"ID3" {
            return Ok(offset);
        }
        if header[6..10].iter().any(|byte| byte & 0x80 != 0) {
            return Ok(offset);
        }
        let size = header[6..10]
            .iter()
            .fold(0u64, |size, byte| (size << 7) | u64::from(*byte));
        let footer = u64::from(header[3] == 4 && header[5] & 0x10 != 0) * 10;
        offset = offset
            .saturating_add(10)
            .saturating_add(size)
            .saturating_add(footer);
    }
}

fn info_lead_frames(path: &Path, options: DecodeOptions) -> std::io::Result<u64> {
    if !options.mp3_info_silence {
        return Ok(0);
    }
    let mut file = File::open(path)?;
    let audio_start = skip_id3v2(&mut file)?;
    file.seek(SeekFrom::Start(audio_start))?;
    let mut prefix = vec![0u8; FIRST_FRAME_SCAN_BYTES];
    let read = file.read(&mut prefix)?;
    prefix.truncate(read);
    Ok(info_frames_in_prefix(&prefix))
}

fn frames_hint(track: &Track, info_lead_frames: u64, options: DecodeOptions) -> Option<u64> {
    track.num_frames.map(|frames| {
        let frames = if options.mp3_gapless {
            frames
        } else {
            frames
                .saturating_add(u64::from(track.delay.unwrap_or(0)))
                .saturating_add(u64::from(track.padding.unwrap_or(0)))
        };
        frames.saturating_add(info_lead_frames)
    })
}

fn mp3_track(tracks: &[Track]) -> Option<(&Track, &AudioCodecParameters)> {
    tracks.iter().find_map(|track| {
        let params = track.codec_params.as_ref()?.audio()?;
        (params.codec == CODEC_ID_MP3).then_some((track, params))
    })
}

pub(crate) fn open_file(
    path: &Path,
    options: DecodeOptions,
) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    let info_lead_frames = info_lead_frames(path, options)?;
    let format = probe_format(path)?;

    let (track_id, channels, sample_rate_hz, frames_total_hint, decoder) = {
        let (track, cp) = mp3_track(format.tracks())
            .ok_or_else(|| format!("MP3 '{}' has no MP3 track", path.display()))?;
        let channels = cp
            .channels
            .as_ref()
            .map(symphonia::core::audio::Channels::count)
            .unwrap_or(0);
        if channels == 0 {
            return Err(format!("MP3 '{}' has unknown channel layout", path.display()).into());
        }
        let sample_rate_hz = cp
            .sample_rate
            .ok_or_else(|| format!("MP3 '{}' has unknown sample rate", path.display()))?;
        let frames_total_hint = frames_hint(track, info_lead_frames, options);
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(
                cp,
                &AudioDecoderOptions::default().gapless(options.mp3_gapless),
            )
            .map_err(|e| format!("Cannot create MP3 decoder for '{}': {e}", path.display()))?;
        (
            track.id,
            channels,
            sample_rate_hz,
            frames_total_hint,
            decoder,
        )
    };

    let mut reader = Reader {
        format,
        decoder,
        track_id,
        channels,
        base_ts: Timestamp::ZERO,
        gapless: options.mp3_gapless,
        info_lead_frames,
        info_lead_pending: info_lead_frames,
        pending: None,
        cursor_frames: 0,
    };

    // Prime the first decoded packet and record its emitted timestamp as the origin
    // for seeks. A restored Info frame, if any, is emitted before this packet.
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
        frames_total_hint,
    })
}

pub(crate) fn file_length_seconds(path: &Path, options: DecodeOptions) -> Result<f32, String> {
    let info_lead_frames =
        info_lead_frames(path, options).map_err(|e| format!("Cannot inspect MP3 file: {e}"))?;
    let mut format = probe_format(path).map_err(|e| format!("Cannot open MP3 file: {e}"))?;

    let (track_id, sample_rate, n_frames, decoder) = {
        let (track, cp) =
            mp3_track(format.tracks()).ok_or_else(|| "MP3 file has no MP3 track".to_string())?;
        let sample_rate = cp
            .sample_rate
            .ok_or_else(|| "MP3 sample rate is invalid".to_string())?;
        let n_frames = frames_hint(track, info_lead_frames, options);
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(
                cp,
                &AudioDecoderOptions::default().gapless(options.mp3_gapless),
            )
            .map_err(|e| format!("Cannot create MP3 decoder: {e}"))?;
        (track.id, sample_rate, n_frames, decoder)
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
    let mut total_frames = info_lead_frames;
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
        if self.info_lead_pending != 0 {
            let frames = self.info_lead_pending;
            self.info_lead_pending = 0;
            out.clear();
            out.resize(frames as usize * self.channels, 0);
            self.cursor_frames = self.cursor_frames.saturating_add(frames);
            return Ok(true);
        }
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
        if target_frame < self.info_lead_frames {
            self.info_lead_pending = 0;
            match self.seek_and_collect(self.base_ts, self.base_ts, target_frame)? {
                SeekOutcome::Landed => {
                    self.info_lead_pending = self.info_lead_frames - target_frame;
                    return Ok(());
                }
                SeekOutcome::Overshoot => {
                    return Err("MP3 rewind overshot the first decoded frame".into());
                }
            }
        }

        self.info_lead_pending = 0;
        let decoded_target = target_frame - self.info_lead_frames;
        let target_ts = self.base_ts.saturating_add(Duration::new(decoded_target));

        // Try progressively larger prerolls; a larger window guarantees we land
        // before the target so the post-seek audio reproduces a linear decode.
        for preroll in [SEEK_PREROLL_FRAMES, SEEK_PREROLL_FRAMES * 4] {
            let seek_ts = target_ts
                .saturating_sub(Duration::new(preroll))
                .max(self.base_ts);
            match self.seek_and_collect(seek_ts, target_ts, target_frame)? {
                SeekOutcome::Landed => return Ok(()),
                SeekOutcome::Overshoot => continue,
            }
        }

        // Final fallback: decode from the first retained audio frame.
        match self.seek_and_collect(self.base_ts, target_ts, target_frame)? {
            SeekOutcome::Landed => Ok(()),
            SeekOutcome::Overshoot => Err("MP3 seek overshot after decoding from the start".into()),
        }
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
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(None),
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                // This reader handles one logical stream; a reset marks its end.
                Err(SymphoniaError::ResetRequired) => return Ok(None),
                Err(e) => return Err(format!("MP3 read error: {e}").into()),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            // The decoder removes trim_start samples only in gapless mode.
            // Seeks must use the timestamp of the first sample actually emitted.
            let ts = if self.gapless {
                packet.pts.saturating_add(packet.trim_start)
            } else {
                packet.pts
            };
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
            audio.copy_to_vec_interleaved::<i16>(out);
            return Ok(Some(ts));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{file_length_seconds, info_frames_in_prefix, open_file};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    const UNTRIMMED_INFO: crate::DecodeOptions = crate::DecodeOptions {
        mp3_gapless: false,
        mp3_info_silence: true,
    };

    const MPEG_FRAME_BYTES: usize = 417;
    const MPEG_FRAME_SAMPLES: u64 = 1_152;
    const AUDIO_FRAMES: u32 = 8;

    struct TempMp3(PathBuf);

    impl TempMp3 {
        fn new(bytes: &[u8]) -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("deadlib-mp3-{}-{id}.mp3", std::process::id()));
            std::fs::write(&path, bytes).expect("write synthetic MP3 fixture");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempMp3 {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn mpeg_frame() -> Vec<u8> {
        let mut frame = vec![0u8; MPEG_FRAME_BYTES];
        frame[..4].copy_from_slice(&[0xff, 0xfb, 0x90, 0x00]);
        frame
    }

    fn tagged_mp3(tag: &[u8; 4], with_id3: bool) -> Vec<u8> {
        let mut bytes = if with_id3 {
            let mut id3 = vec![0u8; 15];
            id3[..10].copy_from_slice(&[b'I', b'D', b'3', 4, 0, 0, 0, 0, 0, 5]);
            id3
        } else {
            Vec::new()
        };
        let mut header = mpeg_frame();
        let tag_offset = 4 + 32;
        header[tag_offset..tag_offset + 4].copy_from_slice(tag);
        header[tag_offset + 4..tag_offset + 8].copy_from_slice(&1u32.to_be_bytes());
        header[tag_offset + 8..tag_offset + 12].copy_from_slice(&AUDIO_FRAMES.to_be_bytes());

        // A valid zero-CRC LAME extension with 576 frames each of encoder delay
        // and padding. Symphonia expands those values to 1,105 + 47 frames.
        let lame_offset = tag_offset + 12;
        header[lame_offset..lame_offset + 9].copy_from_slice(b"LAME3.100");
        let trim = (576u32 << 12) | 576;
        header[lame_offset + 21..lame_offset + 24].copy_from_slice(&trim.to_be_bytes()[1..]);
        bytes.extend_from_slice(&header);
        for _ in 0..AUDIO_FRAMES {
            bytes.extend_from_slice(&mpeg_frame());
        }
        bytes
    }

    fn remaining_frames(reader: &mut super::Reader, channels: usize) -> u64 {
        let mut packet = Vec::new();
        let mut frames = 0u64;
        while reader
            .read_dec_packet_into(&mut packet)
            .expect("decode synthetic MP3")
        {
            frames += (packet.len() / channels) as u64;
        }
        frames
    }

    #[test]
    fn info_header_restores_one_frame_but_xing_does_not() {
        let info = tagged_mp3(b"Info", false);
        let xing = tagged_mp3(b"Xing", false);
        assert_eq!(info_frames_in_prefix(&info), MPEG_FRAME_SAMPLES);
        assert_eq!(info_frames_in_prefix(&xing), 0);
    }

    #[test]
    fn info_header_after_id3_restores_one_frame() {
        let fixture = TempMp3::new(&tagged_mp3(b"Info", true));
        let mut opened = open_file(fixture.path(), UNTRIMMED_INFO).expect("open Info MP3 fixture");
        let expected = (u64::from(AUDIO_FRAMES) + 1) * MPEG_FRAME_SAMPLES;
        assert_eq!(opened.frames_total_hint, Some(expected));
        let expected_sec = expected as f32 / 44_100.0;
        assert!(
            (file_length_seconds(fixture.path(), UNTRIMMED_INFO).expect("read MP3 length")
                - expected_sec)
                .abs()
                < 1e-6
        );

        let mut first = Vec::new();
        assert!(
            opened
                .reader
                .read_dec_packet_into(&mut first)
                .expect("decode restored Info frame")
        );
        assert_eq!(first.len() / opened.channels, MPEG_FRAME_SAMPLES as usize);
        assert!(first.iter().all(|sample| *sample == 0));
        assert_eq!(
            MPEG_FRAME_SAMPLES + remaining_frames(&mut opened.reader, opened.channels),
            expected
        );
    }

    #[test]
    fn info_seeks_use_the_restored_timeline() {
        let fixture = TempMp3::new(&tagged_mp3(b"Info", false));
        let expected = (u64::from(AUDIO_FRAMES) + 1) * MPEG_FRAME_SAMPLES;
        for target in [0, 100, MPEG_FRAME_SAMPLES - 1, MPEG_FRAME_SAMPLES, 4_000] {
            let mut opened =
                open_file(fixture.path(), UNTRIMMED_INFO).expect("open Info MP3 fixture");
            opened.reader.seek_frame(target).expect("seek Info fixture");
            assert_eq!(opened.reader.current_frame(), target);
            assert_eq!(
                remaining_frames(&mut opened.reader, opened.channels),
                expected - target
            );
        }
    }

    #[test]
    fn xing_seeks_keep_untrimmed_decoder_delay() {
        let fixture = TempMp3::new(&tagged_mp3(b"Xing", false));
        let expected = u64::from(AUDIO_FRAMES) * MPEG_FRAME_SAMPLES;
        let mut opened = open_file(fixture.path(), UNTRIMMED_INFO).expect("open Xing MP3 fixture");
        assert_eq!(opened.frames_total_hint, Some(expected));
        assert_eq!(
            remaining_frames(&mut opened.reader, opened.channels),
            expected
        );

        for target in [0, 1, 2_000, 7_000] {
            opened.reader.seek_frame(target).expect("seek Xing fixture");
            assert_eq!(opened.reader.current_frame(), target);
            assert_eq!(
                remaining_frames(&mut opened.reader, opened.channels),
                expected - target
            );
        }
    }

    fn decoded_samples(reader: &mut crate::Reader) -> Vec<i16> {
        let mut samples = Vec::new();
        let mut packet = Vec::new();
        while reader
            .read_dec_packet_into(&mut packet)
            .expect("decode fixture")
        {
            samples.extend_from_slice(&packet);
        }
        samples
    }

    fn pcm_hash(samples: &[i16]) -> u64 {
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .fold(0xcbf29ce484222325, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            })
    }

    #[test]
    fn untrimmed_pcm_duration_and_seeks_match_original_decoder() {
        // Captured from the original decoder before introducing options.
        let info_hashes = [
            15560969893992054619,
            14983663084225290539,
            2877442967195712795,
            5854043011290087115,
            10585874522369775451,
            7922433964078692820,
            15622118114292247424,
            6677188889937414518,
            3890426504494482371,
            2210928693814431973,
        ];
        let audio_hashes = [
            10585874522369775451,
            724827192895732011,
            8524194524827948315,
            7085875132947688989,
            7940950622967182410,
            1314969034773985117,
            5865137172014919503,
            7660452509041732399,
            7415682538886177335,
            16757446904589955706,
        ];
        let info = include_bytes!("../tests/fixtures/timeline.mp3");
        let mut xing = info.to_vec();
        xing[36..40].copy_from_slice(b"Xing");
        let mut id3 = vec![b'I', b'D', b'3', 4, 0, 0, 0, 0, 0, 5];
        id3.extend_from_slice(&[0; 5]);
        id3.extend_from_slice(info);
        for (bytes, frames, duration_bits, hashes) in [
            (info.as_slice(), 108_288, 0x401d2714, info_hashes),
            (xing.as_slice(), 107_136, 0x401b7b17, audio_hashes),
            (&info[313..], 107_136, 0x401b7b17, audio_hashes),
            (id3.as_slice(), 108_288, 0x401d2714, info_hashes),
        ] {
            let fixture = TempMp3::new(bytes);
            let mut opened =
                crate::open_file(fixture.path(), UNTRIMMED_INFO).expect("open fixture");
            assert_eq!((opened.channels, opened.sample_rate_hz), (2, 44_100));
            assert_eq!(opened.frames_total_hint, Some(frames));
            assert_eq!(
                crate::file_length_seconds(fixture.path(), UNTRIMMED_INFO)
                    .expect("duration")
                    .to_bits(),
                duration_bits
            );
            let linear = decoded_samples(&mut opened.reader);
            assert_eq!(linear.len() as u64, frames * 2);
            assert_eq!(pcm_hash(&linear), hashes[0]);
            for (target, expected_hash) in
                [0, 1, 100, 1151, 1152, 2000, 16383, 40000, 80000, 100000]
                    .into_iter()
                    .zip(hashes)
            {
                opened.reader.seek_frame(target).expect("seek fixture");
                assert_eq!(opened.reader.current_frame(), target);
                let samples = decoded_samples(&mut opened.reader);
                assert_eq!(samples.len() as u64, (frames - target) * 2);
                assert_eq!(pcm_hash(&samples), expected_hash, "target {target}");
                assert_eq!(opened.reader.current_frame(), frames);
            }
        }
    }

    #[test]
    fn timeline_options_preserve_samples_and_seek_in_emitted_frames() {
        let bytes = include_bytes!("../tests/fixtures/timeline.mp3");
        let mut xing = bytes.to_vec();
        xing[36..40].copy_from_slice(b"Xing");
        for (bytes, has_info) in [(bytes.as_slice(), true), (xing.as_slice(), false)] {
            let fixture = TempMp3::new(bytes);
            let raw_options = crate::DecodeOptions {
                mp3_gapless: false,
                mp3_info_silence: false,
            };
            let mut raw = crate::open_file(fixture.path(), raw_options).expect("open raw");
            let raw = decoded_samples(&mut raw.reader);
            // The generated 2.4s stereo fixture has 1,105 delay + 191 padding frames.
            assert_eq!(raw.len(), 107_136 * 2);
            for gapless in [false, true] {
                for info_silence in [false, true] {
                    let options = crate::DecodeOptions {
                        mp3_gapless: gapless,
                        mp3_info_silence: info_silence,
                    };
                    let mut opened =
                        crate::open_file(fixture.path(), options).expect("open options");
                    let mut expected = vec![
                        0;
                        if has_info && info_silence {
                            1152 * 2
                        } else {
                            0
                        }
                    ];
                    expected.extend_from_slice(if gapless {
                        &raw[1105 * 2..raw.len() - 191 * 2]
                    } else {
                        &raw
                    });
                    let frames = (expected.len() / 2) as u64;
                    assert_eq!(opened.frames_total_hint, Some(frames));
                    assert_eq!(
                        crate::file_length_seconds(fixture.path(), options).expect("duration"),
                        (frames as f64 / 44_100.0) as f32
                    );
                    assert!(
                        decoded_samples(&mut opened.reader) == expected,
                        "linear PCM mismatch"
                    );
                    for target in [
                        0,
                        1,
                        100,
                        1151,
                        1152,
                        2000,
                        16383,
                        40000,
                        80000,
                        frames - 1,
                        frames,
                    ] {
                        opened
                            .reader
                            .seek_frame(target)
                            .expect("seek option timeline");
                        assert_eq!(opened.reader.current_frame(), target);
                        assert!(
                            decoded_samples(&mut opened.reader) == expected[target as usize * 2..],
                            "gapless={gapless} info={info_silence} target={target}"
                        );
                        assert_eq!(opened.reader.current_frame(), frames);
                    }
                }
            }
        }
    }

    #[test]
    fn gapless_seeks_skip_fully_trimmed_mpeg2_packets() {
        // MPEG-2 mono, 64 kbps at 22,050 Hz: 576 samples per frame.
        let mut frame = vec![0; 208];
        frame[..4].copy_from_slice(&[0xff, 0xf3, 0x80, 0xc0]);
        let mut bytes = frame.clone();
        bytes[13..17].copy_from_slice(b"Info");
        bytes[17..21].copy_from_slice(&1_u32.to_be_bytes());
        bytes[21..25].copy_from_slice(&AUDIO_FRAMES.to_be_bytes());
        bytes[25..34].copy_from_slice(b"LAME3.100");
        let trim = (576_u32 << 12) | 576;
        bytes[46..49].copy_from_slice(&trim.to_be_bytes()[1..]);
        for _ in 0..AUDIO_FRAMES {
            bytes.extend_from_slice(&frame);
        }
        let fixture = TempMp3::new(&bytes);
        for gapless in [false, true] {
            for info_silence in [false, true] {
                let options = crate::DecodeOptions {
                    mp3_gapless: gapless,
                    mp3_info_silence: info_silence,
                };
                let expected =
                    8 * 576 - if gapless { 1152 } else { 0 } + if info_silence { 576 } else { 0 };
                let mut opened = open_file(fixture.path(), options).expect("open MPEG-2");
                assert_eq!(opened.frames_total_hint, Some(expected));
                assert_eq!(
                    remaining_frames(&mut opened.reader, opened.channels),
                    expected
                );
                for target in [0, 1, 575, 576, 1152, expected - 1, expected] {
                    opened.reader.seek_frame(target).expect("seek MPEG-2");
                    assert_eq!(opened.reader.current_frame(), target);
                    assert_eq!(
                        remaining_frames(&mut opened.reader, opened.channels),
                        expected - target
                    );
                }
            }
        }
    }
}
