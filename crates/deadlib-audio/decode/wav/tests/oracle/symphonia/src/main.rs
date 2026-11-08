//! Reference-only WAV ADPCM decoding to raw signed 16-bit little-endian PCM.
use std::io::Write;
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::AudioDecoderOptions,
    formats::{FormatOptions, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: symphonia_adpcm INPUT")?;
    let mut hint = Hint::new();
    hint.with_extension("wav");
    let mut format = symphonia::default::get_probe().probe(
        &hint,
        MediaSourceStream::new(Box::new(std::fs::File::open(path)?), Default::default()),
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or("no audio track")?;
    let id = track.id;
    let mut decoder = symphonia::default::get_codecs().make_audio_decoder(
        track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or("no audio parameters")?,
        &AudioDecoderOptions::default(),
    )?;
    let mut sink = std::io::BufWriter::new(std::io::stdout().lock());
    while let Some(packet) = format.next_packet()? {
        if packet.track_id != id {
            continue;
        }
        let audio = decoder.decode(&packet)?;
        let GenericAudioBufferRef::S32(audio) = audio else {
            return Err("reference ADPCM output is not i32".into());
        };
        for i in 0..audio.frames() {
            for channel in 0..audio.num_planes() {
                let value = (audio.plane(channel).ok_or("missing channel")?[i] >> 16) as i16;
                sink.write_all(&value.to_le_bytes())?;
            }
        }
    }
    sink.flush()?;
    Ok(())
}
