//! Public synchronization state across rolling appends and cursor limits.
use deadlib_audio_decode_ogg::*;
use std::io::{self, Write};
use std::mem::size_of;

fn frame(n: usize) -> Vec<u8> {
    let mut lacing = vec![255; n / 255];
    if lacing.len() < 255 {
        lacing.push((n % 255) as u8);
    }
    let body: Vec<_> = (0..n).map(|i| (i * 37 + i / 255) as u8).collect();
    encode_page(
        PageHeader {
            flags: 0,
            granule_position: 37,
            serial: 19,
            sequence: 3,
        },
        &lacing,
        &body,
    )
    .unwrap()
}
fn state(sync: &SyncBuffer, out: &mut impl Write) {
    write!(
        out,
        "STATE{}/{}/{}/{};",
        sync.position(),
        sync.buffered_bytes(),
        sync.storage_bytes(),
        sync.is_unsynced()
    )
    .unwrap();
    if let Some(current) = sync.current_page() {
        write!(
            out,
            "CURRENT{}N{}:",
            current.offset,
            current.page.bytes().len()
        )
        .unwrap();
        out.write_all(current.page.bytes()).unwrap();
    } else {
        write!(out, "NONE;").unwrap();
    }
}
fn poll(sync: &mut SyncBuffer, output: bool, out: &mut impl Write) -> bool {
    let result = if output {
        match sync.page_out() {
            Ok(PageOut::Page(found)) => {
                write!(out, "OUT{}N{}:", found.offset, found.page.bytes().len()).unwrap();
                out.write_all(found.page.bytes()).unwrap();
                true
            }
            Ok(PageOut::Hole) => {
                write!(out, "HOLE;").unwrap();
                true
            }
            Ok(PageOut::NeedMore) => {
                write!(out, "MORE;").unwrap();
                false
            }
            Err(error) => {
                write!(out, "ERROR{error:?};").unwrap();
                false
            }
        }
    } else {
        match sync.page_seek() {
            Ok(PageSeek::Page(found)) => {
                write!(out, "SEEK{}N{}:", found.offset, found.page.bytes().len()).unwrap();
                out.write_all(found.page.bytes()).unwrap();
                true
            }
            Ok(PageSeek::Skipped(n)) => {
                write!(out, "SKIP{n};").unwrap();
                true
            }
            Ok(PageSeek::NeedMore) => {
                write!(out, "MORE;").unwrap();
                false
            }
            Err(error) => {
                write!(out, "ERROR{error:?};").unwrap();
                false
            }
        }
    };
    state(sync, out);
    result
}
fn main() {
    if std::env::args().any(|arg| arg == "--layout") {
        println!("SyncBuffer {}", size_of::<SyncBuffer>());
        println!("SyncReader {}", size_of::<SyncReader<&'static [u8]>>());
        return;
    }
    let mut out = io::BufWriter::new(io::stdout().lock());
    for n in [0, 1, 17, 4096, 65025] {
        let bytes = frame(n);
        for window in [1, 2, 8, 32] {
            let warm = bytes.repeat(window * 2);
            let rejected = vec![0; warm.len() + 1];
            for base in [0, 1 << 40, u64::MAX - bytes.len() as u64] {
                for output in [false, true] {
                    write!(out, "CASE{n}/{window}/{base}/{output};").unwrap();
                    let mut sync = SyncBuffer::with_limit(warm.len());
                    sync.reset_at(base);
                    write!(out, "PUSH{:?};", sync.push(&warm)).unwrap();
                    state(&sync, &mut out);
                    for _ in 0..window {
                        if !poll(&mut sync, output, &mut out) {
                            break;
                        }
                    }
                    for _ in 0..24 {
                        poll(&mut sync, output, &mut out);
                        write!(out, "REJECT{:?};", sync.push(&rejected)).unwrap();
                        state(&sync, &mut out);
                        write!(out, "APPEND{:?};", sync.push(&bytes)).unwrap();
                        state(&sync, &mut out);
                    }
                    for _ in 0..window + 2 {
                        if !poll(&mut sync, output, &mut out) {
                            break;
                        }
                    }
                    sync.reset_at(42);
                    write!(out, "NOISE{:?};", sync.push(b"NOg")).unwrap();
                    poll(&mut sync, output, &mut out);
                    write!(out, "HEADER{:?};", sync.push(&bytes[..20])).unwrap();
                    poll(&mut sync, output, &mut out);
                    write!(out, "BODY{:?};", sync.push(&bytes[20..])).unwrap();
                    for _ in 0..5 {
                        poll(&mut sync, output, &mut out);
                    }
                    write!(out, "EMPTY{:?};", sync.push(&[])).unwrap();
                    state(&sync, &mut out);
                    let mut growth = SyncBuffer::with_limit(warm.len() * 4);
                    growth.reset_at(base);
                    write!(out, "GROW{:?};", growth.push(&warm)).unwrap();
                    for _ in 0..window {
                        if !poll(&mut growth, output, &mut out) {
                            break;
                        }
                    }
                    for step in 0..window * 3 {
                        write!(out, "GROW_APPEND{:?};", growth.push(&bytes)).unwrap();
                        state(&growth, &mut out);
                        if step % 3 == 0 {
                            poll(&mut growth, output, &mut out);
                        }
                    }
                }
            }
        }
    }
}
