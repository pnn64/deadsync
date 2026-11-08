//! Complete encoded bytes, validation errors, page debug output and write calls.
use deadlib_audio_decode_ogg::*;
use std::io::{self, Write};
use std::mem::size_of;

struct Partial {
    bytes: Vec<u8>,
    calls: Vec<(usize, usize, u8)>,
    cap: usize,
    fault: u8,
    flushes: usize,
}
impl Write for Partial {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let call = self.calls.len() + 1;
        let tag = if self.fault == 1 && call == 2 {
            1
        } else if self.fault == 2 && call == 5 {
            2
        } else {
            0
        };
        let n = self.cap.min(bytes.len());
        self.calls.push((bytes.len(), n, tag));
        match tag {
            1 => Err(io::ErrorKind::Interrupted.into()),
            2 => Err(io::Error::other("injected output failure")),
            _ => {
                self.bytes.extend_from_slice(&bytes[..n]);
                Ok(n)
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

fn main() {
    if std::env::args().any(|arg| arg == "--layout") {
        println!("Page {}", size_of::<Page<'static>>());
        println!("OptionPage {}", size_of::<Option<Page<'static>>>());
        println!("LocatedPage {}", size_of::<LocatedPage<'static>>());
        println!("PageProgress {}", size_of::<PageProgress<'static>>());
        println!("IndexEvent {}", size_of::<IndexEvent<'static>>());
        return;
    }
    let layouts = [
        vec![],
        vec![0],
        vec![1],
        vec![255, 0],
        vec![255; 255],
        vec![0; 255],
        vec![17; 255],
        vec![0; 256],
    ];
    let mut out = io::BufWriter::new(io::stdout().lock());
    for flags in 0..=255u8 {
        for seed in [0, u32::MAX] {
            for (id, lacing) in layouts.iter().enumerate() {
                let sum: usize = lacing.iter().map(|&n| n as usize).sum();
                for length in [sum.saturating_sub(1), sum, sum + 1] {
                    let fields = PageHeader {
                        flags,
                        granule_position: if flags & 1 == 0 {
                            (seed as u64) << 32 | id as u64
                        } else {
                            u64::MAX
                        },
                        serial: seed ^ (flags as u32).wrapping_mul(0x0101_0101),
                        sequence: seed.wrapping_sub(id as u32 * 31),
                    };
                    let body: Vec<_> = (0..length)
                        .map(|i| (i * 37 + flags as usize + id * 17) as u8)
                        .collect();
                    write!(out, "CASE{fields:?}/{id}/{length};").unwrap();
                    match encode_page(fields, lacing, &body) {
                        Ok(bytes) => {
                            write!(out, "ENCODE{}:", bytes.len()).unwrap();
                            out.write_all(&bytes).unwrap();
                            let page = Page::parse(&bytes).unwrap().0;
                            write!(out, "DEBUG{page:?};PRETTY{page:#?};").unwrap();
                        }
                        Err(error) => write!(out, "ENCODE_ERR{error:?};").unwrap(),
                    }
                    let mut bytes = Vec::new();
                    let result = write_page(&mut bytes, fields, lacing, &body);
                    write!(out, "WRITE{result:?}N{}:", bytes.len()).unwrap();
                    out.write_all(&bytes).unwrap();
                }
            }
        }
    }
    for n in [0, 36, 228, 4096, 65025] {
        let mut lacing = vec![255; n / 255];
        if lacing.len() < 255 {
            lacing.push((n % 255) as u8);
        }
        let body: Vec<_> = (0..n).map(|i| (i * 37) as u8).collect();
        for cap in [1, 7, usize::MAX] {
            for fault in 0..3 {
                let mut writer = Partial {
                    bytes: Vec::new(),
                    calls: Vec::new(),
                    cap,
                    fault,
                    flushes: 0,
                };
                let result = write_page(
                    &mut writer,
                    PageHeader {
                        flags: BOS | EOS,
                        granule_position: u64::MAX - 1,
                        serial: 0x1234_5678,
                        sequence: u32::MAX,
                    },
                    &lacing,
                    &body,
                );
                write!(
                    out,
                    "PARTIAL{n}/{cap}/{fault}:{result:?}N{}:",
                    writer.bytes.len()
                )
                .unwrap();
                out.write_all(&writer.bytes).unwrap();
                write!(out, "FLUSH{}CALLS{:?};", writer.flushes, writer.calls).unwrap();
            }
        }
    }
}
