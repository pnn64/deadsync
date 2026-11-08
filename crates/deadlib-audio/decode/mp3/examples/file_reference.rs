//! Validation-only native Symphonia demuxer/decoder, with default gapless support.
use std::io::Write;
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::AudioDecoderOptions,
    formats::{FormatOptions, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "file_reference INPUT OUTPUT");
    let source =
        MediaSourceStream::new(Box::new(std::fs::File::open(&args[0])?), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let mut format = symphonia::default::get_probe().probe(
        &hint,
        source,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or("no audio track")?;
    let mut decoder = symphonia::default::get_codecs().make_audio_decoder(
        track
            .codec_params
            .as_ref()
            .ok_or("no codec parameters")?
            .audio()
            .ok_or("not audio")?,
        &AudioDecoderOptions::default(),
    )?;
    let track_id = track.id;
    let mut out = std::io::BufWriter::new(std::fs::File::create(&args[1])?);
    out.write_all(b"DLMP3FI1")?;
    while let Some(packet) = format.next_packet()? {
        if packet.track_id != track_id {
            continue;
        }
        let GenericAudioBufferRef::F32(pcm) = decoder.decode(&packet)? else {
            return Err("expected F32 reference output".into());
        };
        if pcm.frames() == 0 {
            continue;
        }
        for value in [
            pcm.spec().rate(),
            pcm.num_planes() as u32,
            pcm.frames() as u32,
        ] {
            out.write_all(&value.to_le_bytes())?;
        }
        for n in 0..pcm.frames() {
            for c in 0..pcm.num_planes() {
                out.write_all(&pcm.plane(c).ok_or("missing plane")?[n].to_le_bytes())?;
            }
        }
    }
    out.flush()?;
    Ok(())
}
