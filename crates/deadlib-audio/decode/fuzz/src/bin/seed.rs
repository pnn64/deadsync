use deadlib_audio_decode_fuzz::{FIXTURES, MAX_INPUT, packets};
use deadlib_audio_decode_ogg::{PacketWriter, PageReader, encode_page};
use std::{fs, path::Path};

fn put(root: &Path, target: &str, name: &str, bytes: &[u8]) {
    let directory = root.join(target);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join(name), bytes).unwrap();
}
fn ogg_seed(root: &Path, name: &str, data: &[u8]) {
    for target in ["ogg_container", "vorbis_stream"] {
        for control in [0, 3, 17, 255] {
            let mut bytes = vec![control];
            bytes.extend_from_slice(&data[..data.len().min(MAX_INPUT)]);
            put(root, target, &format!("{name}-{control}"), &bytes);
        }
    }
}
fn codec_seeds(root: &Path, number: usize, data: &[u8], raw_fixture: bool) {
    let packets = packets(data);
    for kind in 0..3 {
        let mut bytes = vec![kind];
        if kind == 2 {
            bytes.extend_from_slice(&packets[0]);
        }
        bytes.extend_from_slice(&packets[kind as usize]);
        put(root, "vorbis_headers", &format!("{number}-{kind}"), &bytes);
    }
    if raw_fixture {
        for (group, packets) in packets[3..].chunks(8).take(8).enumerate() {
            let mut bytes = vec![number as u8];
            for packet in packets {
                if packet.len() > u16::MAX as usize {
                    continue;
                }
                bytes.extend_from_slice(&(packet.len() as u16).to_le_bytes());
                bytes.extend_from_slice(packet);
            }
            put(root, "vorbis_packets", &format!("{number}-{group}"), &bytes);
        }
        for (group, chunk) in packets[3..].chunks(8).take(8).enumerate() {
            for profile in [0, 64] {
                for schedule in 0..3 {
                    let mut bytes = vec![number as u8 | profile, 0];
                    for (i, p) in chunk.iter().enumerate() {
                        if p.len() > u16::MAX as usize {
                            continue;
                        }
                        bytes.push(if schedule == 1 && i < 2 { 2 } else { 1 });
                        bytes.extend_from_slice(&(p.len() as u16).to_le_bytes());
                        bytes.extend_from_slice(&(i as u64 + 3).to_le_bytes());
                        bytes.extend_from_slice(
                            &(if schedule == 0 {
                                -1i64
                            } else if i % 3 == 0 {
                                0
                            } else {
                                37
                            })
                            .to_le_bytes(),
                        );
                        bytes.push((i + 1 == chunk.len()) as u8);
                        bytes.extend_from_slice(p);
                        bytes.extend_from_slice(&[5, if schedule == 2 { 1 } else { 2 }]);
                        if schedule != 2 || i % 3 == 2 {
                            bytes.push(4);
                        }
                    }
                    put(
                        root,
                        "vorbis_synthesis",
                        &format!("synthesis-{number}-{group}-{profile}-{schedule}"),
                        &bytes,
                    );
                }
            }
        }
    }
}
fn pages(data: &[u8], serial: u32) -> Vec<Vec<u8>> {
    let mut reader = PageReader::new(data);
    let mut pages = Vec::new();
    while let Some(page) = reader.next_page().unwrap() {
        let mut header = page.page.header();
        header.serial = serial;
        pages.push(encode_page(header, page.page.lacing(), page.page.body()).unwrap());
    }
    pages
}
fn main() {
    let mut args = std::env::args_os().skip(1);
    let root = args
        .next()
        .expect("seed OUTPUT_DIRECTORY [EXTERNAL_VORBIS_FILES...]");
    let root = Path::new(&root);
    assert!(!root.exists(), "seed output directory must be new");
    fs::create_dir_all(root).unwrap();
    for block in 0..8 {
        for precision in 0..4 {
            let mut seed = vec![block, block, precision, 4, 0, 17];
            for i in 0..4096i32 {
                seed.extend_from_slice(&((i * 79) as i16).to_le_bytes());
            }
            put(
                root,
                "vorbis_encode",
                &format!("pcm-{block}-{precision}"),
                &seed,
            );
        }
    }
    for raw in [0u32, 0x7fc00000, 0x7f800000, 0x40800000, 0xc0800000] {
        let mut seed = vec![9, 0, 3, 4, 7, 1];
        for _ in 0..255 {
            seed.extend_from_slice(&raw.to_le_bytes());
        }
        put(root, "vorbis_encode", &format!("float-{raw:x}"), &seed);
    }
    for (number, bytes) in FIXTURES.iter().enumerate() {
        ogg_seed(root, &format!("fixture-{number}"), bytes);
        codec_seeds(root, number, bytes, true);
    }
    if let Some(path) = std::env::var_os("DEADLIB_VORBIS_FLOOR0") {
        let bytes = fs::read(path).unwrap();
        codec_seeds(root, FIXTURES.len(), &bytes, true);
        // Header plus early audio is enough to seed the expensive LSP path;
        // raw packet seeds separately cover complete floor-zero synthesis.
        ogg_seed(root, "floor0", &bytes[..bytes.len().min(32768)]);
    }
    for (number, path) in args.enumerate() {
        let bytes = fs::read(path).unwrap();
        codec_seeds(root, 10 + number, &bytes, false);
        ogg_seed(root, &format!("external-{number}"), &bytes);
    }
    let first = pages(FIXTURES[0], 101);
    let second = pages(FIXTURES[1], 202);
    let mut chain = Vec::new();
    for page in first.iter().chain(&second) {
        chain.extend_from_slice(page);
    }
    ogg_seed(root, "chain", &chain);
    let mut multiplex = Vec::new();
    for i in 0..first.len().max(second.len()) {
        for stream in [&first, &second] {
            if let Some(page) = stream.get(i) {
                multiplex.extend_from_slice(page);
            }
        }
    }
    ogg_seed(root, "multiplex", &multiplex);
    let mut writer = PacketWriter::new(Vec::new(), 303);
    writer.write_packet(b"header", 0, false).unwrap();
    writer.write_packet(&vec![0x55; 70000], 100, false).unwrap();
    writer.write_packet(&[], 200, false).unwrap();
    writer.flush_page().unwrap();
    writer.finish().unwrap();
    ogg_seed(root, "span-empty-nil-eos", &writer.into_inner());
    println!("Fuzz seeds retained at {}", root.display());
}
