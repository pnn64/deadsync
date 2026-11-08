//! Compare complete IO traces and state observations against a saved library.
use deadlib_audio_decode_ogg::*;
use std::io::{self, Write};

struct Trace {
    bytes: Vec<u8>,
    events: Vec<[usize; 3]>,
    cap: usize,
    fail: usize,
}
impl Write for Trace {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let call = self.events.len() + 1;
        if call == self.fail {
            self.events.push([bytes.len(), 2, 0]);
            return Err(io::Error::other("injected failure"));
        }
        if call % 19 == 0 {
            self.events.push([bytes.len(), 1, 0]);
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = bytes.len().min(self.cap);
        self.events.push([bytes.len(), 0, n]);
        self.bytes.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        panic!("PacketWriter must not call Write::flush");
    }
}
fn number(out: &mut impl Write, n: usize) {
    out.write_all(&(n as u64).to_le_bytes()).unwrap();
}
fn observation(out: &mut impl Write, writer: &PacketWriter<Trace>, result: Result<(), Error>) {
    let result = format!("{result:?}");
    number(out, result.len());
    out.write_all(result.as_bytes()).unwrap();
    number(out, writer.get_ref().bytes.len());
    number(out, writer.get_ref().events.len());
}
fn main() {
    let mut out = io::BufWriter::new(io::stdout().lock());
    let data: Vec<_> = (0..130050).map(|i| (i * 37 + i / 255) as u8).collect();
    for target in [0, 1, 255, 8192, 65025, usize::MAX] {
        for cap in [7, usize::MAX] {
            for fail in [0, 1, 2, 3, 7, 31, 4097] {
                for eos in [false, true] {
                    for n in [target, cap, fail, usize::from(eos)] {
                        number(&mut out, n);
                    }
                    let mut writer = PacketWriter::with_options(
                        Trace {
                            bytes: Vec::new(),
                            events: Vec::new(),
                            cap,
                            fail,
                        },
                        42,
                        WriteOptions {
                            target_page_body: target,
                        },
                    );
                    let result = writer.write_packet(b"header", 0, false);
                    observation(&mut out, &writer, result);
                    // Exercise full lacing tables with very little buffered payload.
                    for i in 0..512 {
                        let result = writer.write_packet(&data[..i % 2], i as u64 + 1, false);
                        observation(&mut out, &writer, result);
                    }
                    let sizes = [
                        0, 1, 254, 255, 256, 510, 8191, 8192, 65024, 65025, 130050, 99,
                    ];
                    for (i, n) in sizes.into_iter().enumerate() {
                        let result = writer.write_packet(
                            &data[..n],
                            i as u64 + 513,
                            eos && i == sizes.len() - 1,
                        );
                        observation(&mut out, &writer, result);
                        if i % 3 == 0 {
                            let result = writer.flush_page();
                            observation(&mut out, &writer, result);
                        }
                    }
                    let result = writer.write_packet(b"invalid", UNKNOWN_GRANULE_POSITION, false);
                    observation(&mut out, &writer, result);
                    for _ in 0..2 {
                        let result = writer.finish();
                        observation(&mut out, &writer, result);
                    }
                    let result = writer.write_packet(b"late", 600, false);
                    observation(&mut out, &writer, result);
                    let trace = writer.into_inner();
                    number(&mut out, trace.bytes.len());
                    out.write_all(&trace.bytes).unwrap();
                    number(&mut out, trace.events.len());
                    for event in trace.events {
                        for n in event {
                            number(&mut out, n);
                        }
                    }
                }
            }
        }
    }
    // Pin the complete generated inputs of the reader/demux timing controls.
    for n in [1, 17, 255, 8192] {
        let mut writer = PacketWriter::new(Vec::new(), 42);
        writer.write_packet(b"header", 0, false).unwrap();
        let payload = vec![37; n];
        for i in 0..2048 {
            writer.write_packet(&payload, i, i == 2047).unwrap();
        }
        let bytes = writer.into_inner();
        number(&mut out, n);
        number(&mut out, bytes.len());
        out.write_all(&bytes).unwrap();
    }
}
