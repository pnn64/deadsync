//! Decode borrowed wavl ADPCM to raw little-endian i16, optionally one exact range.
use deadlib_audio_decode_wav::{AdpcmWavl, Wave};
use std::io::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    let bytes = std::fs::read(
        args.get(1)
            .ok_or("usage: adpcm_wavl INPUT [START [FRAMES]]")?,
    )?;
    let wave = Wave::parse(&bytes)?;
    let audio = AdpcmWavl::new(&wave)?;
    let mut position = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(0);
    if position > audio.frames() {
        return Err("frame position".into());
    }
    let count = args
        .get(3)
        .map(|s| s.parse::<u64>())
        .transpose()?
        .unwrap_or(audio.frames() - position);
    let end = position
        .checked_add(count)
        .ok_or("frame range overflow")?
        .min(audio.frames());
    let channels = audio.codec().channels();
    let mut scratch = vec![0; audio.codec().scratch_samples()];
    let mut out = vec![0; channels * (8192 / channels).max(1)];
    let mut packed = vec![0; out.len() * 2];
    let stdout = std::io::stdout();
    let mut sink = std::io::BufWriter::new(stdout.lock());
    while position < end {
        let frames = (end - position).min((out.len() / channels) as u64) as usize;
        let n = audio.read_frames(position, &mut out[..frames * channels], &mut scratch)?;
        for (v, b) in out[..n].iter().zip(packed.as_chunks_mut::<2>().0) {
            *b = v.to_le_bytes();
        }
        sink.write_all(&packed[..n * 2])?;
        position += (n / channels) as u64;
    }
    sink.flush()?;
    Ok(())
}
