//! Raw streaming validation driver. Chunk zero uses File directly; positive
//! chunks cap reads from an in-memory test source to avoid per-byte file IO.
use deadlib_audio_decode_mp3::*;
use std::io::{Cursor, Read, Write};
struct Chunks<R> {
    source: R,
    size: usize,
}
impl<R: Read> Read for Chunks<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let size = bytes.len().min(self.size);
        self.source.read(&mut bytes[..size])
    }
}
fn run(
    source: impl Read,
    path: &str,
    options: StreamOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = StreamDecoder::with_options(source, options);
    let mut output = std::io::BufWriter::new(std::fs::File::create(path)?);
    output.write_all(b"DLMP3PC1")?;
    while let Some(frame) = stream.next_frame()? {
        for value in [
            frame.header.sample_rate(),
            frame.pcm.channels() as u32,
            frame.pcm.samples() as u32,
        ] {
            output.write_all(&value.to_le_bytes())?;
        }
        for n in 0..frame.pcm.samples() {
            for plane in frame.pcm.planes() {
                output.write_all(&plane[n].to_le_bytes())?;
            }
        }
        println!(
            "frame\t{}\t{}\t{}\t{:?}",
            frame.byte_offset,
            frame.position,
            frame.pcm.samples(),
            frame.status
        );
    }
    output.flush()?;
    let stats = stream.stats();
    println!(
        "summary\t{}\t{}\t{}\t{}\t{}\t{}",
        stats.decoded_frames,
        stats.concealed_frames,
        stats.raw_samples,
        stream.skipped_bytes(),
        stream.discarded_tail_bytes(),
        stream.metadata().id3v2().len()
    );
    if let Some(tags::VbrHeader::Xing(tag)) = stream.metadata().vbr()? {
        println!(
            "xing\t{}\t{}\t{}",
            tag.frames.unwrap_or(0),
            tag.gapless.map_or(0, |g| g.encoder_delay),
            tag.gapless.map_or(0, |g| g.encoder_padding)
        );
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() >= 3,
        "stream INPUT OUTPUT CHUNK [recover] [discard]"
    );
    let chunk: usize = args[2].parse()?;
    let options = StreamOptions {
        recovery: if args.iter().any(|a| a == "recover") {
            Recovery::Conceal
        } else {
            Recovery::Strict
        },
        truncated: if args.iter().any(|a| a == "discard") {
            TruncatedPolicy::Discard
        } else {
            TruncatedPolicy::Error
        },
        ..Default::default()
    };
    if chunk == 0 {
        run(std::fs::File::open(&args[0])?, &args[1], options)
    } else {
        run(
            Chunks {
                source: Cursor::new(std::fs::read(&args[0])?),
                size: chunk,
            },
            &args[1],
            options,
        )
    }
}
