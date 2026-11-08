//! Exact progress/error/page traces, including empty polling and decoder reuse.
use deadlib_audio_decode_ogg::*;
use std::io::{self, Write};

fn step(out: &mut impl Write, decoder: &mut PageDecoder, input: &[u8]) -> Result<usize, ()> {
    match decoder.push(input) {
        Ok(progress) => {
            write!(
                out,
                "C{}P{}:",
                progress.consumed,
                progress.page.map_or(0, |p| p.bytes().len())
            )
            .unwrap();
            if let Some(page) = progress.page {
                out.write_all(page.bytes()).unwrap();
            }
            Ok(progress.consumed)
        }
        Err(error) => {
            write!(out, "E{error:?};").unwrap();
            Err(())
        }
    }
}
fn run(out: &mut impl Write, input: &[u8], chunk: usize, poll: bool, good: &[u8]) {
    write!(out, "I{}C{chunk}Q{poll};", input.len()).unwrap();
    let mut decoder = PageDecoder::new();
    'input: for bytes in input.chunks(chunk) {
        let mut bytes = bytes;
        while !bytes.is_empty() {
            let Ok(n) = step(out, &mut decoder, bytes) else {
                break 'input;
            };
            assert!(n > 0);
            bytes = &bytes[n..];
        }
        if poll {
            let _ = step(out, &mut decoder, &[]);
        }
    }
    write!(out, "F{:?}F{:?};", decoder.finish(), decoder.finish()).unwrap();
    let _ = step(out, &mut decoder, &[]);
    decoder.reset();
    let _ = step(out, &mut decoder, good);
    let _ = step(out, &mut decoder, good);
    write!(out, "F{:?};", decoder.finish()).unwrap();
}
fn main() {
    eprintln!("PageDecoder bytes: {}", std::mem::size_of::<PageDecoder>());
    let mut out = io::BufWriter::new(io::stdout().lock());
    let header = PageHeader {
        flags: 0,
        granule_position: 0,
        serial: 42,
        sequence: 0,
    };
    let good = encode_page(header, &[1], &[37]).unwrap();
    for segments in 0..=255 {
        let lacing: Vec<_> = (0..segments).map(|i| [0, 1, 17, 255, 254][i % 5]).collect();
        let body: Vec<_> = (0..lacing.iter().map(|&v| v as usize).sum())
            .map(|i| (i * 37) as u8)
            .collect();
        let valid = encode_page(header, &lacing, &body).unwrap();
        run(&mut out, &valid, usize::MAX, false, &good);
        if ![0, 1, 2, 17, 127, 254, 255].contains(&segments) {
            continue;
        }
        for chunk in [1, 7, 4096] {
            run(&mut out, &valid, chunk, true, &good);
        }
        for end in [0, 1, 2, 26, 27, 26 + segments, valid.len() - 1] {
            for chunk in [7, usize::MAX] {
                run(&mut out, &valid[..end], chunk, true, &good);
            }
        }
        for offset in [4, 5, 22, valid.len() - 1] {
            let mut corrupt = valid.clone();
            corrupt[offset] ^= 128;
            for chunk in [7, usize::MAX] {
                run(&mut out, &corrupt, chunk, true, &good);
            }
        }
    }
    let maximum = encode_page(header, &[255; 255], &[37; 65025]).unwrap();
    for chunk in [1, 7, 4096, usize::MAX] {
        run(&mut out, &maximum, chunk, true, &good);
    }
}
