//! Codec-neutral resident-input timing; binary verification stays untimed.
use deadlib_audio_decode_ogg::*;
use std::{
    fs,
    hint::black_box,
    io::{self, Read, Write},
    path::Path,
    time::Instant,
};

#[derive(Default, Clone, Copy)]
struct Stats {
    pages: u64,
    packets: u64,
    payload: u64,
    reads: u64,
    bytes: u64,
    checksum: u64,
}
impl Stats {
    fn add(&mut self, other: Self) {
        self.pages += other.pages;
        self.packets += other.packets;
        self.payload += other.payload;
        self.reads += other.reads;
        self.bytes += other.bytes;
        self.checksum += other.checksum;
    }
}
struct Source<'a> {
    data: &'a [u8],
    position: usize,
    cap: usize,
    reads: u64,
}
impl Read for Source<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        let n = output
            .len()
            .min(self.cap)
            .min(self.data.len() - self.position);
        output[..n].copy_from_slice(&self.data[self.position..self.position + n]);
        self.position += n;
        Ok(n)
    }
}
fn run(phase: &str, data: &[u8]) -> Stats {
    let startup = phase == "startup";
    let page_only = phase == "pages";
    let source = Source {
        data,
        position: 0,
        cap: if phase == "stream4096" {
            4096
        } else {
            usize::MAX
        },
        reads: 0,
    };
    let mut reader = PageReader::new(source);
    let mut demux = Demuxer::default();
    let mut stats = Stats::default();
    while let Some(page) = reader.next_page().unwrap() {
        stats.pages += 1;
        if !page_only {
            demux
                .push_page(page.page, page.offset, |packet| {
                    stats.packets += 1;
                    stats.payload += packet.data.len() as u64;
                    black_box(packet.data);
                })
                .unwrap();
        } else {
            black_box(page.page.body());
        }
        if startup && stats.packets != 0 {
            break;
        }
    }
    if !startup && !page_only {
        demux.finish().unwrap();
    }
    let source = reader.into_inner();
    stats.reads = source.reads;
    stats.bytes = source.position as u64;
    stats
}
fn verify(data: &[u8]) {
    let mut reader = PageReader::new(data);
    let mut demux = Demuxer::default();
    let mut out = io::BufWriter::new(io::stdout().lock());
    while let Some(page) = reader.next_page().unwrap() {
        let h = page.page.header();
        out.write_all(b"G").unwrap();
        out.write_all(&h.serial.to_le_bytes()).unwrap();
        out.write_all(&h.sequence.to_le_bytes()).unwrap();
        out.write_all(&h.granule_position.to_le_bytes()).unwrap();
        out.write_all(&[h.flags]).unwrap();
        out.write_all(&(page.page.bytes().len() as u32).to_le_bytes())
            .unwrap();
        demux
            .push_page(page.page, page.offset, |packet| {
                out.write_all(b"P").unwrap();
                out.write_all(&packet.info.serial.to_le_bytes()).unwrap();
                out.write_all(
                    &packet
                        .info
                        .granule_position
                        .unwrap_or(UNKNOWN_GRANULE_POSITION)
                        .to_le_bytes(),
                )
                .unwrap();
                out.write_all(&[
                    packet.info.first_in_stream as u8,
                    packet.info.last_in_stream as u8,
                ])
                .unwrap();
                out.write_all(&(packet.data.len() as u64).to_le_bytes())
                    .unwrap();
                out.write_all(packet.data).unwrap();
            })
            .unwrap();
    }
    demux.finish().unwrap();
    out.flush().unwrap();
}
fn pattern(n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| (i * 37 + (i >> 8) * 17 + 11) as u8)
        .collect()
}
fn physical(serial: u32, sizes: &[usize], nil: bool) -> Vec<u8> {
    let mut writer = PacketWriter::new(Vec::new(), serial);
    for (i, &n) in sizes.iter().enumerate() {
        writer
            .write_packet(&pattern(n), i as u64, i + 1 == sizes.len() && !nil)
            .unwrap();
    }
    if nil {
        writer.finish().unwrap();
    }
    writer.into_inner()
}
fn pages(data: &[u8]) -> Vec<Vec<u8>> {
    let mut reader = PageReader::new(data);
    let mut out = Vec::new();
    while let Some(page) = reader.next_page().unwrap() {
        out.push(page.page.bytes().to_vec());
    }
    out
}
fn generate(directory: &Path) {
    fs::create_dir(directory).unwrap();
    let mut sizes = vec![32];
    sizes.extend(std::iter::repeat_n(32768, 256));
    fs::write(directory.join("bulk.ogg"), physical(41, &sizes, false)).unwrap();
    let mut sizes = vec![4];
    sizes.extend((0..100000).map(|i| [0, 1, 2, 3, 17, 255][i % 6]));
    fs::write(directory.join("tiny.ogg"), physical(42, &sizes, true)).unwrap();
    let a = pages(&physical(11, &[4, 70000, 0, 130050, 255, 9], false));
    let b = pages(&physical(22, &[4, 255, 130050, 70000, 0, 9], true));
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&a[0]);
    bytes.extend_from_slice(&b[0]);
    for i in 1..a.len().max(b.len()) {
        if let Some(p) = a.get(i) {
            bytes.extend_from_slice(p);
        }
        if let Some(p) = b.get(i) {
            bytes.extend_from_slice(p);
        }
    }
    bytes.extend(physical(33, &[3, 0, 510, 1], true));
    fs::write(directory.join("span-mux-chain.ogg"), bytes).unwrap();
}
fn main() {
    let args: Vec<_> = std::env::args()
        .skip(1)
        .filter(|a| a != "--bench")
        .collect();
    if args[0] == "generate" {
        assert_eq!(args.len(), 2);
        generate(Path::new(&args[1]));
        return;
    }
    if args[0] == "verify" {
        assert_eq!(args.len(), 2);
        verify(&fs::read(&args[1]).unwrap());
        return;
    }
    assert_eq!(args.len(), 3);
    let phase = &args[0];
    let iterations: usize = args[1].parse().unwrap();
    assert!(iterations > 0);
    let mut stats = Stats::default();
    let elapsed;
    if phase == "crc" {
        let n: usize = args[2].parse().unwrap();
        assert!((27..=1 << 24).contains(&n));
        let mut data = pattern(n);
        data[22..26].fill(0);
        let expected = crc32(&data);
        black_box(expected);
        let begin = Instant::now();
        for _ in 0..iterations {
            stats.checksum += crc32(black_box(&data)) as u64;
        }
        elapsed = begin.elapsed().as_nanos();
        assert_eq!(stats.checksum, expected as u64 * iterations as u64);
        stats.bytes = n as u64 * iterations as u64;
    } else {
        assert!(["pages", "demux", "stream4096", "startup"].contains(&phase.as_str()));
        let data = fs::read(&args[2]).unwrap();
        black_box(run(phase, &data));
        let begin = Instant::now();
        for _ in 0..iterations {
            stats.add(run(phase, black_box(&data)));
        }
        elapsed = begin.elapsed().as_nanos();
    }
    println!(
        "native\t{phase}\t{iterations}\t{elapsed}\t{}\t{}\t{}\t{}\t{}\t{}",
        stats.pages, stats.packets, stats.payload, stats.reads, stats.bytes, stats.checksum
    );
}
