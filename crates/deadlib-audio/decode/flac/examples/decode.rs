//! Decode native FLAC to signed little-endian PCM (ceil(bit_depth/8) bytes).
use deadlib_audio_decode_flac::Reader;
use std::{fs::File, io::Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = args.next().ok_or("usage: decode INPUT.flac [OUTPUT.raw]")?;
    let mut output = args
        .next()
        .map(File::create)
        .transpose()?
        .map(std::io::BufWriter::new);
    let mut decoder = Reader::new(std::io::BufReader::new(File::open(input)?))?;
    let mut samples = 0u64;
    while let Some(frame) = decoder.read_next()? {
        samples += frame.pcm.samples() as u64;
        if let Some(out) = &mut output {
            for i in 0..frame.pcm.samples() {
                for plane in frame.pcm.planes() {
                    out.write_all(
                        &plane[i].to_le_bytes()
                            [..usize::from(frame.header.bits_per_sample.div_ceil(8))],
                    )?;
                }
            }
        }
    }
    if let Some(out) = &mut output {
        out.flush()?;
    }
    eprintln!("samples={samples} integrity={:?}", decoder.integrity());
    Ok(())
}
