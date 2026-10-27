//! Raw-frame PCM comparison CLI. Includes metadata-frame samples and delay.
use deadlib_audio_decode_mp3::{
    tags::{Id3Tag, Id3v1},
    *,
};
use std::io::Write;
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::{AudioCodecParameters, AudioDecoderOptions, well_known::CODEC_ID_MP3},
    packet::PacketRef,
    units::{Duration, Timestamp},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 3, "decode native|symphonia INPUT OUTPUT");
    let data = std::fs::read(&args[1])?;
    let limits = Limits::default();
    let mut offset = 0;
    while data.get(offset..offset + 3) == Some(b"ID3".as_slice()) {
        offset += Id3Tag::parse(&data[offset..], limits)?.bytes;
    }
    let end = if Id3v1::parse(&data).is_some() {
        data.len() - 128
    } else {
        data.len()
    };
    let mut native = FrameDecoder::new();
    let mut reference = None;
    let mut free = None;
    let mut output = std::io::BufWriter::new(std::fs::File::create(&args[2])?);
    output.write_all(b"DLMP3PC1")?;
    while offset < end {
        let header = match FrameHeader::parse(&data[offset..end]) {
            Ok(header) => header,
            Err(_) => {
                offset += 1;
                continue;
            }
        };
        if header.bitrate().is_none() && free.is_none() {
            free = Some(FreeFormat::infer(&data[offset..end], limits)?);
        }
        let parsed = match Frame::parse(&data[offset..end], free, true, limits) {
            Ok(frame) => frame,
            Err(Error::Truncated) => break,
            Err(error) => return Err(format!("at byte {offset}: {error}").into()),
        };
        if args[0] == "native" {
            let frame = match native.decode(parsed.data, free) {
                Ok(frame) => frame,
                Err(Error::MissingReservoir { .. }) => {
                    eprintln!("concealed missing-reservoir frame at byte {offset}");
                    write_header(&mut output, header)?;
                    for _ in 0..header.samples() * header.channels() {
                        output.write_all(&0f32.to_le_bytes())?;
                    }
                    offset += parsed.data.len();
                    continue;
                }
                Err(error) => return Err(format!("at byte {offset}: {error}").into()),
            };
            write_header(&mut output, header)?;
            for n in 0..frame.pcm.samples() {
                for ch in 0..frame.pcm.channels() {
                    output.write_all(&frame.pcm.plane(ch).unwrap()[n].to_le_bytes())?;
                }
            }
        } else if args[0] == "symphonia" {
            if reference.is_none() {
                let mut params = AudioCodecParameters::new();
                params.for_codec(CODEC_ID_MP3);
                reference = Some(
                    symphonia::default::get_codecs()
                        .make_audio_decoder(&params, &AudioDecoderOptions::default())?,
                );
            }
            let packet = PacketRef::new(0, Timestamp::ZERO, Duration::ZERO, parsed.data);
            let GenericAudioBufferRef::F32(pcm) =
                reference.as_mut().unwrap().decode_ref(&packet)?
            else {
                return Err("expected f32 reference PCM".into());
            };
            if pcm.frames() != header.samples() {
                return Err("reference sample count".into());
            }
            write_header(&mut output, header)?;
            for n in 0..header.samples() {
                for ch in 0..header.channels() {
                    output.write_all(&pcm.plane(ch).unwrap()[n].to_le_bytes())?;
                }
            }
        } else {
            return Err("unknown decoder".into());
        }
        offset += parsed.data.len();
    }
    output.flush()?;
    Ok(())
}
fn write_header(output: &mut impl Write, header: FrameHeader) -> std::io::Result<()> {
    for value in [
        header.sample_rate(),
        header.channels() as u32,
        header.samples() as u32,
    ] {
        output.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}
