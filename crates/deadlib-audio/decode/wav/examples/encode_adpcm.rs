//! Encode PCM16: encode_adpcm INPUT ms|ima|ima3 BLOCK_BYTES [extensible|extensible-known]
use deadlib_audio_decode_wav::{AdpcmKind, AdpcmSpec, AdpcmWriter, Wave, WriteOptions};
use std::io::{Cursor, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if !(4..=5).contains(&args.len()) {
        return Err(
            "usage: encode_adpcm INPUT ms|ima|ima3 BLOCK_BYTES [extensible|extensible-known]"
                .into(),
        );
    }
    let bytes = std::fs::read(&args[1])?;
    let wave = Wave::parse(&bytes)?;
    let kind = match args[2].as_str() {
        "ms" => AdpcmKind::Microsoft,
        "ima" => AdpcmKind::Ima,
        "ima3" => AdpcmKind::Ima3,
        _ => return Err("unknown codec".into()),
    };
    let spec = AdpcmSpec {
        kind,
        channels: wave.format.channels,
        sample_rate: wave.format.sample_rate,
        block_bytes: args[3].parse()?,
    };
    let pcm = wave.pcm()?;
    let known = match args.get(4).map(String::as_str) {
        None | Some("extensible") => false,
        Some("extensible-known") => true,
        _ => return Err("unknown output mode".into()),
    };
    let options = WriteOptions {
        force_extensible: args.len() == 5,
        ..WriteOptions::default()
    };
    let mut writer = if known {
        AdpcmWriter::with_frames(Cursor::new(Vec::new()), spec, options, &[], pcm.frames())?
    } else {
        AdpcmWriter::new(Cursor::new(Vec::new()), spec, options, &[])?
    };
    let mut samples = vec![0; usize::from(spec.channels) * 127];
    let mut frame = 0;
    while frame < pcm.frames() {
        let n = pcm.read_i16(frame, &mut samples)?;
        writer.write_i16(&samples[..n])?;
        frame += (n / usize::from(spec.channels)) as u64;
    }
    let output = if known {
        writer.finish_stream()?
    } else {
        writer.finish()?
    };
    std::io::stdout().lock().write_all(&output.into_inner())?;
    Ok(())
}
