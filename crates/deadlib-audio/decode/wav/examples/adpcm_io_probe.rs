//! Count underlying File operations below a 64 KiB BufReader; no timing claim.
use deadlib_audio_decode_wav::AdpcmReader;
use std::{
    cell::Cell,
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
    path::Path,
    rc::Rc,
};
#[derive(Default)]
struct Counts {
    reads: Cell<u64>,
    bytes: Cell<u64>,
    seeks: Cell<u64>,
}
struct Source {
    file: File,
    counts: Rc<Counts>,
}
impl Read for Source {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.counts.reads.set(self.counts.reads.get() + 1);
        let n = self.file.read(out)?;
        self.counts.bytes.set(self.counts.bytes.get() + n as u64);
        Ok(n)
    }
}
impl Seek for Source {
    fn seek(&mut self, at: SeekFrom) -> io::Result<u64> {
        self.counts.seeks.set(self.counts.seeks.get() + 1);
        self.file.seek(at)
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("INPUT.wav required")?;
    let truth = std::fs::read(Path::new(&path).with_extension("period"))?;
    let (words, tail) = truth.as_chunks::<2>();
    assert!(tail.is_empty());
    let truth: Vec<_> = words
        .iter()
        .map(|b| i16::from_le_bytes(*b))
        .collect();
    let counts = Rc::new(Counts::default());
    let source = Source {
        file: File::open(&path)?,
        counts: Rc::clone(&counts),
    };
    let mut reader = AdpcmReader::new(BufReader::with_capacity(65536, source))?;
    let encoded = reader
        .chunks()
        .iter()
        .find(|c| c.id == *b"data")
        .ok_or("flat data required")?
        .size;
    let opening = (counts.reads.get(), counts.bytes.get(), counts.seeks.get());
    let mut out = vec![
        0;
        (4096 / usize::from(reader.format().channels)).max(1)
            * usize::from(reader.format().channels)
    ];
    let mut samples = 0;
    loop {
        let n = reader.read_i16(&mut out)?;
        for (i, &value) in out[..n].iter().enumerate() {
            assert_eq!(value, truth[(samples + i) % truth.len()]);
        }
        samples += n;
        if n == 0 {
            break;
        }
    }
    assert_eq!(
        samples as u64,
        reader.frames() * u64::from(reader.format().channels)
    );
    println!(
        "{{\"encoded_bytes\":{encoded},\"samples\":{samples},\"opening_reads\":{},\"opening_bytes\":{},\"opening_seeks\":{},\"stream_reads\":{},\"stream_bytes\":{},\"stream_seeks\":{}}}",
        opening.0,
        opening.1,
        opening.2,
        counts.reads.get() - opening.0,
        counts.bytes.get() - opening.1,
        counts.seeks.get() - opening.2,
    );
    Ok(())
}
