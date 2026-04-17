pub(crate) mod flac;
pub(crate) mod mp3;
pub(crate) mod ogg_vorbis;
pub(crate) mod opus;
pub(crate) mod wav;

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub(crate) struct OpenFile {
    pub reader: Reader,
    pub channels: usize,
    pub sample_rate_hz: u32,
}

pub(crate) enum Reader {
    Flac(flac::Reader),
    Mp3(mp3::Reader<BufReader<File>>),
    Ogg(ogg_vorbis::Reader),
    Opus(opus::Reader),
    Wav(wav::Reader),
}

impl Reader {
    pub(crate) fn read_dec_packet_into(
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

    pub(crate) fn seek_frame(
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
}

#[inline(always)]
pub(crate) fn open_file(path: &Path) -> Result<OpenFile, Box<dyn std::error::Error + Send + Sync>> {
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
pub(crate) fn file_length_seconds(path: &Path) -> Result<f32, String> {
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
