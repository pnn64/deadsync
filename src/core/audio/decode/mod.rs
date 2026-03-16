pub(crate) mod flac;
pub(crate) mod mp3;
pub(crate) mod ogg_vorbis;

use lewton::inside_ogg::OggStreamReader;
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
    Ogg(OggStreamReader<BufReader<File>>),
}

impl Reader {
    pub(crate) fn read_dec_packet_itl(
        &mut self,
    ) -> Result<Option<Vec<i16>>, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Flac(reader) => reader.read_dec_packet_itl(),
            Self::Mp3(reader) => reader.read_dec_packet_itl(),
            Self::Ogg(reader) => Ok(reader.read_dec_packet_itl()?),
        }
    }

    pub(crate) fn seek_frame(
        &mut self,
        frame: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Flac(reader) => reader.seek_frame(frame),
            Self::Mp3(reader) => reader.seek_frame(frame),
            Self::Ogg(reader) => {
                reader.seek_absgp_pg(frame)?;
                Ok(())
            }
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
    if ogg_vorbis::path_is_ogg_vorbis(path) {
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
    if ogg_vorbis::path_is_ogg_vorbis(path) {
        return ogg_vorbis::file_length_seconds(path);
    }
    Err(format!("unsupported audio format for '{}'", path.display()))
}
