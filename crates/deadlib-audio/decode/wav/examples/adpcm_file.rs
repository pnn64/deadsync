//! Decode file-backed ADPCM with an optional exact frame range to raw i16 LE.
use deadlib_audio_decode_wav::AdpcmReader;
use std::io::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if !(2..=4).contains(&args.len()) {
        return Err("usage: adpcm_file INPUT [START_FRAME [FRAMES]]".into());
    }
    let mut reader = AdpcmReader::new(std::fs::File::open(&args[1])?)?;
    let start = if args.len() > 2 {
        args[2].parse()?
    } else {
        0u64
    };
    reader.seek_frame(start)?;
    let mut remaining = if args.len() > 3 {
        args[3].parse::<u64>()?
    } else {
        reader.frames() - start
    };
    remaining = remaining.min(reader.frames() - start);
    let channels = usize::from(reader.format().channels);
    let mut samples = vec![0i16; 127 * channels];
    let mut bytes = vec![0; 2 * samples.len()];
    let mut sink = std::io::BufWriter::new(std::io::stdout().lock());
    while remaining > 0 {
        let count = (remaining.min(127) as usize) * channels;
        let n = reader.read_i16(&mut samples[..count])?;
        if n == 0 {
            return Err("unexpected ADPCM EOF".into());
        }
        for (v, b) in samples[..n].iter().zip(bytes.as_chunks_mut::<2>().0) {
            *b = v.to_le_bytes();
        }
        sink.write_all(&bytes[..2 * n])?;
        remaining -= (n / channels) as u64;
    }
    sink.flush()?;
    Ok(())
}
