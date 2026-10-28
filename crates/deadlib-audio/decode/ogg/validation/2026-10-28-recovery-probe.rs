//! Compare complete public parser and IO observations across saved libraries.
use deadlib_audio_decode_ogg::*;
use std::io::{self, Read, Seek, SeekFrom, Write};

fn page(laces: &[u8], flags: u8) -> Vec<u8> {
    let length: usize = laces.iter().map(|&v| v as usize).sum();
    encode_page(
        PageHeader {
            flags,
            granule_position: 19,
            serial: 42,
            sequence: 7,
        },
        laces,
        &(0..length).map(|i| (i * 37) as u8).collect::<Vec<_>>(),
    )
    .unwrap()
}
fn fixtures() -> Vec<Vec<u8>> {
    let small = page(&[17], EOS);
    let mut all = vec![
        page(&[], 0),
        page(&[0; 255], 0),
        page(&[255; 255], CONTINUED),
    ];
    all.extend((0..=small.len()).map(|n| small[..n].to_vec()));
    for field in [4, 5] {
        for byte in 0..=255 {
            let mut changed = small.clone();
            changed[field] = byte;
            all.push(changed);
        }
    }
    for field in [0, 22, 26, 27, 28, small.len() - 1] {
        for bit in 0..8 {
            let mut changed = small.clone();
            changed[field] ^= 1 << bit;
            all.push(changed);
        }
    }
    for pattern in [b"%".as_slice(), b"O", b"Og", b"Ogg", b"NO\x80\xff"] {
        for n in (0..=16).chain([31, 255, 1024]) {
            let mut noise: Vec<_> = pattern.iter().copied().cycle().take(n).collect();
            noise.extend_from_slice(&small);
            noise.extend(page(&[0; 255], EOS));
            all.push(noise);
        }
    }
    let mut nested = page(&[255; 8], 0);
    nested[300..300 + small.len()].copy_from_slice(&small);
    all.push(nested);
    all
}
struct Input<'a> {
    bytes: &'a [u8],
    position: usize,
    cap: usize,
    calls: usize,
    fault: usize,
    trace: Vec<[u64; 4]>,
}
impl Read for Input<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        let tag = if self.fault == 1 && self.calls == 2 {
            1
        } else if self.fault == 2 && self.calls == 5 {
            2
        } else {
            0
        };
        let n = out
            .len()
            .min(self.cap)
            .min(self.bytes.len() - self.position);
        self.trace
            .push([out.len() as u64, self.position as u64, tag, n as u64]);
        if tag == 1 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if tag == 2 {
            return Err(io::Error::other("injected read failure"));
        }
        out[..n].copy_from_slice(&self.bytes[self.position..self.position + n]);
        self.position += n;
        Ok(n)
    }
}
impl Seek for Input<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::Current(n) => (self.position as u64).checked_add_signed(n),
            SeekFrom::End(n) => (self.bytes.len() as u64).checked_add_signed(n),
        }
        .filter(|&n| n <= self.bytes.len() as u64)
        .ok_or_else(|| io::Error::other("injected seek failure"))?;
        self.position = position as usize;
        Ok(position)
    }
}
fn observe(reader: &mut PageReader<Input<'_>>, out: &mut impl Write) -> bool {
    let ended = match reader.next_page() {
        Ok(Some(p)) => {
            write!(out, "P{}N{}:", p.offset, p.page.bytes().len()).unwrap();
            out.write_all(p.page.bytes()).unwrap();
            false
        }
        Ok(None) => {
            write!(out, "EOF;").unwrap();
            true
        }
        Err(e) => {
            write!(out, "ERR{e:?};").unwrap();
            true
        }
    };
    write!(
        out,
        "AT{}SKIP{};",
        reader.position(),
        reader.discarded_bytes()
    )
    .unwrap();
    if let Some(p) = reader.current_page() {
        write!(out, "CURRENT{}N{}:", p.offset, p.page.bytes().len()).unwrap();
        out.write_all(p.page.bytes()).unwrap();
    } else {
        write!(out, "NO_CURRENT;").unwrap();
    }
    ended
}
fn main() {
    let fixtures = fixtures();
    let mut out = io::BufWriter::new(io::stdout().lock());
    writeln!(out, "FIXTURES {}", fixtures.len()).unwrap();
    for (id, bytes) in fixtures.into_iter().enumerate() {
        write!(out, "INPUT{id}N{}:", bytes.len()).unwrap();
        out.write_all(&bytes).unwrap();
        for cap in [1, 7, 27, 4096] {
            for mode in [SyncMode::Strict, SyncMode::Recover] {
                for fault in 0..3 {
                    write!(out, "READER{cap}/{mode:?}/{fault};").unwrap();
                    let input = Input {
                        bytes: &bytes,
                        position: 0,
                        cap,
                        calls: 0,
                        fault,
                        trace: Vec::new(),
                    };
                    let mut reader = PageReader::with_mode(input, mode);
                    for _ in 0..128 {
                        if observe(&mut reader, &mut out) {
                            break;
                        }
                    }
                    observe(&mut reader, &mut out);
                    observe(&mut reader, &mut out);
                    write!(out, "BAD_SEEK{:?};", reader.seek(u64::MAX)).unwrap();
                    write!(out, "SEEK{:?};", reader.seek(0)).unwrap();
                    observe(&mut reader, &mut out);
                    observe(&mut reader, &mut out);
                    let input = reader.into_inner();
                    write!(
                        out,
                        "INNER{}CALLS{}TRACE{}:",
                        input.position,
                        input.calls,
                        input.trace.len()
                    )
                    .unwrap();
                    for entry in input.trace {
                        for value in entry {
                            out.write_all(&value.to_le_bytes()).unwrap();
                        }
                    }
                }
            }
        }
        for cap in [1, 7, 27, 4096, usize::MAX] {
            write!(out, "DECODER{cap};").unwrap();
            let mut decoder = PageDecoder::new();
            let mut offset = 0;
            while offset < bytes.len() {
                let end = bytes.len().min(offset.saturating_add(cap));
                match decoder.push(&bytes[offset..end]) {
                    Ok(p) => {
                        write!(out, "USED{};", p.consumed).unwrap();
                        assert_ne!(p.consumed, 0);
                        offset += p.consumed;
                        if let Some(page) = p.page {
                            write!(out, "P{}:", page.bytes().len()).unwrap();
                            out.write_all(page.bytes()).unwrap();
                        }
                    }
                    Err(e) => {
                        write!(out, "ERR{e:?};").unwrap();
                        break;
                    }
                }
            }
            for _ in 0..2 {
                match decoder.push(&[]) {
                    Ok(p) => {
                        write!(out, "EMPTY{}/{};", p.consumed, p.page.is_some()).unwrap();
                    }
                    Err(e) => {
                        write!(out, "EMPTY_ERR{e:?};").unwrap();
                    }
                }
                write!(out, "FINISH{:?};", decoder.finish()).unwrap();
            }
            decoder.reset();
            let clean = page(&[17], EOS);
            let progress = decoder.push(&clean).unwrap();
            write!(out, "RESET{}:", progress.consumed).unwrap();
            out.write_all(progress.page.unwrap().bytes()).unwrap();
            write!(out, "FINISH{:?};", decoder.finish()).unwrap();
        }
    }
}
