//! Decode a WAV ADPCM file to raw signed 16-bit little-endian PCM.
use deadlib_audio_decode_wav::{AdpcmData, Wave};
use std::io::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("usage: adpcm INPUT.wav")?;
    let bytes = std::fs::read(path)?;
    let wave = Wave::parse(&bytes)?;
    let audio = AdpcmData::new(&wave)?;
    let mut scratch = vec![0; audio.codec().scratch_samples()];
    let mut out = vec![0; audio.codec().channels() * 4096];
    let mut packed = vec![0; out.len() * 2];
    let mut position = 0;
    let stdout = std::io::stdout();
    let mut sink = std::io::BufWriter::new(stdout.lock());
    while position < audio.frames() {
        let n = audio.read_frames(position, &mut out, &mut scratch)?;
        for (v, b) in out[..n].iter().zip(packed.as_chunks_mut::<2>().0) {
            *b = v.to_le_bytes();
        }
        sink.write_all(&packed[..n * 2])?;
        position += (n / audio.codec().channels()) as u64;
    }
    sink.flush()?;
    Ok(())
}
