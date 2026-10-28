//! Non-seekable tagged gapless validation driver.
use deadlib_audio_decode_mp3::*;
use std::io::{Cursor, Read, Write};
struct Chunks<R> {
    source: R,
    size: usize,
}
impl<R: Read> Read for Chunks<R> {
    fn read(&mut self, data: &mut [u8]) -> std::io::Result<usize> {
        let n = data.len().min(self.size);
        self.source.read(&mut data[..n])
    }
}
fn run(
    source: impl Read,
    output: &str,
    options: StreamOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder = GaplessDecoder::with_options(source, options);
    let mut output = std::io::BufWriter::new(std::fs::File::create(output)?);
    output.write_all(b"DLMP3FI1")?;
    let mut position = 0;
    while let Some(out) = decoder.next_frame()? {
        assert_eq!(out.position, position);
        position += out.pcm.samples() as u64;
        for value in [
            out.header.sample_rate(),
            out.pcm.channels() as u32,
            out.pcm.samples() as u32,
        ] {
            output.write_all(&value.to_le_bytes())?;
        }
        for n in 0..out.pcm.samples() {
            for plane in out.pcm.planes() {
                output.write_all(&plane[n].to_le_bytes())?;
            }
        }
    }
    assert!(decoder.next_frame()?.is_none());
    assert_eq!(decoder.position(), position);
    output.flush()?;
    let raw = decoder.stats();
    println!(
        "samples\t{}\t{}\t{}\t{}",
        position, raw.raw_samples, raw.decoded_frames, raw.concealed_frames
    );
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() >= 3,
        "gapless INPUT OUTPUT CHUNK [recover] [discard]"
    );
    let size: usize = args[2].parse()?;
    let options = StreamOptions {
        recovery: if args.iter().any(|s| s == "recover") {
            Recovery::Conceal
        } else {
            Recovery::Strict
        },
        truncated: if args.iter().any(|s| s == "discard") {
            TruncatedPolicy::Discard
        } else {
            TruncatedPolicy::Error
        },
        ..Default::default()
    };
    if size == 0 {
        run(std::fs::File::open(&args[0])?, &args[1], options)
    } else {
        run(
            Chunks {
                source: Cursor::new(std::fs::read(&args[0])?),
                size,
            },
            &args[1],
            options,
        )
    }
}
