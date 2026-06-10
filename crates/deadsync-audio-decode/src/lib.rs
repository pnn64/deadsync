pub mod flac;
pub mod folder;
pub mod mp3;
pub mod ogg_vorbis;
pub mod opus;
pub mod resample;
pub mod wav;

use std::path::Path;

pub struct OpenFile {
    pub reader: Reader,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

pub enum Reader {
    Flac(flac::Reader),
    Mp3(mp3::Reader),
    Ogg(ogg_vorbis::Reader),
    Opus(opus::Reader),
    Wav(wav::Reader),
}

impl Reader {
    pub fn read_dec_packet_into(
        &mut self,
        out: &mut Vec<i16>,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Flac(reader) => reader.read_dec_packet_into(out),
            Self::Mp3(reader) => reader.read_dec_packet_into(out),
            Self::Ogg(reader) => reader.read_dec_packet_into(out),
            Self::Opus(reader) => reader.read_dec_packet_into(out),
            Self::Wav(reader) => reader.read_dec_packet_into(out),
        }
    }

    pub fn seek_frame(
        &mut self,
        frame: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Flac(reader) => reader.seek_frame(frame),
            Self::Mp3(reader) => reader.seek_frame(frame),
            Self::Ogg(reader) => reader.seek_frame(frame),
            Self::Opus(reader) => reader.seek_frame(frame),
            Self::Wav(reader) => reader.seek_frame(frame),
        }
    }

    pub fn current_frame(&self) -> u64 {
        match self {
            Self::Flac(reader) => reader.current_frame(),
            Self::Mp3(reader) => reader.current_frame(),
            Self::Ogg(reader) => reader.current_frame(),
            Self::Opus(reader) => reader.current_frame(),
            Self::Wav(reader) => reader.current_frame(),
        }
    }
}

#[inline(always)]
pub fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
    if flac::path_is_flac(path) {
        let opened = flac::open_file(path)?;
        return Ok(OpenFile {
            reader: Reader::Flac(opened.reader),
            channels: opened.channels,
            sample_rate_hz: opened.sample_rate_hz,
        });
    }
    if mp3::path_is_mp3(path) {
        let opened = mp3::open_file(path)?;
        return Ok(OpenFile {
            reader: Reader::Mp3(opened.reader),
            channels: opened.channels,
            sample_rate_hz: opened.sample_rate_hz,
        });
    }
    if wav::path_is_wav(path) {
        let opened = wav::open_file(path)?;
        return Ok(OpenFile {
            reader: Reader::Wav(opened.reader),
            channels: opened.channels,
            sample_rate_hz: opened.sample_rate_hz,
        });
    }
    if opus::path_is_opus(path) {
        let opened = opus::open_file(path)?;
        return Ok(OpenFile {
            reader: Reader::Opus(opened.reader),
            channels: opened.channels,
            sample_rate_hz: opened.sample_rate_hz,
        });
    }
    if ogg_vorbis::path_is_ogg_vorbis(path) {
        if let Ok(opened) = opus::open_file(path) {
            return Ok(OpenFile {
                reader: Reader::Opus(opened.reader),
                channels: opened.channels,
                sample_rate_hz: opened.sample_rate_hz,
            });
        }
        let opened = ogg_vorbis::open_file(path)?;
        return Ok(OpenFile {
            reader: Reader::Ogg(opened.reader),
            channels: opened.channels,
            sample_rate_hz: opened.sample_rate_hz,
        });
    }
    Err(format!("unsupported audio format for '{}'", path.display()).into())
}

#[inline(always)]
pub fn file_length_seconds(path: &Path) -> Result<f32, String> {
    if flac::path_is_flac(path) {
        return flac::file_length_seconds(path);
    }
    if mp3::path_is_mp3(path) {
        return mp3::file_length_seconds(path);
    }
    if wav::path_is_wav(path) {
        return wav::file_length_seconds(path);
    }
    if opus::path_is_opus(path) {
        return opus::file_length_seconds(path);
    }
    if ogg_vorbis::path_is_ogg_vorbis(path) {
        if let Ok(sec) = opus::file_length_seconds(path) {
            return Ok(sec);
        }
        return ogg_vorbis::file_length_seconds(path);
    }
    Err(format!("unsupported audio format for '{}'", path.display()))
}

#[inline(always)]
pub fn snap_start_forward_to_packet(path: &Path, start_sec: f64) -> Result<Option<f64>, String> {
    if ogg_vorbis::path_is_ogg_vorbis(path) {
        return ogg_vorbis::snap_start_forward_to_packet(path, start_sec);
    }
    Ok(None)
}
