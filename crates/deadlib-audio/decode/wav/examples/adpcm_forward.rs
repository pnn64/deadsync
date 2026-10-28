//! Decode through a Read-only wrapper to raw interleaved i16 LE.
use deadlib_audio_decode_wav::AdpcmForwardReader;
use std::io::{Read, Write};
struct Input {
    file: std::fs::File,
    width: usize,
    calls: usize,
}
impl Read for Input {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(13) {
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        let n = out.len().min(self.width);
        self.file.read(&mut out[..n])
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if !(2..=4).contains(&args.len()) {
        return Err("usage: adpcm_forward INPUT [BUFFER_FRAMES [READ_WIDTH]]".into());
    }
    let capacity = args.get(2).map_or(Ok(127), |v| v.parse::<usize>())?;
    let width = args.get(3).map_or(Ok(17), |v| v.parse::<usize>())?;
    if capacity == 0 || width == 0 || capacity > 1048576 {
        return Err("invalid buffer/read width".into());
    }
    let mut reader = AdpcmForwardReader::new(Input {
        file: std::fs::File::open(&args[1])?,
        width,
        calls: 0,
    })?;
    let channels = usize::from(reader.format().channels);
    let expected = reader.frames();
    let mut frames = 0;
    let mut samples = vec![0i16; capacity * channels];
    let mut bytes = vec![0; 2 * samples.len()];
    let mut sink = std::io::BufWriter::new(std::io::stdout().lock());
    loop {
        let n = reader.read_i16(&mut samples)?;
        if n == 0 {
            break;
        }
        for (s, b) in samples[..n].iter().zip(bytes.as_chunks_mut::<2>().0) {
            *b = s.to_le_bytes();
        }
        sink.write_all(&bytes[..2 * n])?;
        frames += (n / channels) as u64;
    }
    assert_eq!(frames, expected);
    reader.finish()?;
    sink.flush()?;
    Ok(())
}
