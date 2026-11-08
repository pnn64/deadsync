//! Validation CLI for known reordered spectra; not a production file decoder.
use deadlib_audio_decode_mp3::synthesis::{Hybrid, Synthesis};
use std::io::{Read, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "dsp INPUT OUTPUT");
    let mut reader = std::io::BufReader::new(std::fs::File::open(&args[0])?);
    let mut signature = [0; 8];
    reader.read_exact(&mut signature)?;
    assert_eq!(&signature, b"DLMDSP01");
    let mut count = [0; 4];
    reader.read_exact(&mut count)?;
    let count = u32::from_le_bytes(count);
    let mut writer = std::io::BufWriter::new(std::fs::File::create(&args[1])?);
    writer.write_all(b"DLMDOUT1")?;
    writer.write_all(&count.to_le_bytes())?;
    let mut hybrid = Hybrid::new();
    let mut synthesis = Synthesis::new();
    for _ in 0..count {
        let mut shape = [0; 4];
        reader.read_exact(&mut shape)?;
        let mut spectrum = [0.0; 576];
        for v in &mut spectrum {
            let mut bytes = [0; 8];
            reader.read_exact(&mut bytes)?;
            *v = f64::from_le_bytes(bytes);
        }
        let mut subbands = [0.0; 576];
        hybrid.process(&mut spectrum, shape[0], shape[1] as usize, &mut subbands)?;
        for v in subbands {
            writer.write_all(&v.to_le_bytes())?;
        }
        let mut pcm = [0.0; 576];
        for t in 0..18 {
            synthesis.process(
                subbands[t * 32..t * 32 + 32].try_into().unwrap(),
                (&mut pcm[t * 32..t * 32 + 32]).try_into().unwrap(),
            );
        }
        for v in pcm {
            writer.write_all(&v.to_le_bytes())?;
        }
    }
    writer.flush()?;
    Ok(())
}
