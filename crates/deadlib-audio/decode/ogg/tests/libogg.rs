use deadlib_audio_decode_ogg::*;
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

fn oracle(mode: &str, file: &Path) -> Vec<u8> {
    let executable = std::env::var_os("DEADLIB_LIBOGG_ORACLE")
        .expect("set DEADLIB_LIBOGG_ORACLE to the compiled test helper");
    let output = Command::new(executable)
        .arg(mode)
        .arg(file)
        .output()
        .expect("run libogg oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn take<'a>(bytes: &mut &'a [u8], count: usize) -> &'a [u8] {
    let (out, rest) = bytes.split_at(count);
    *bytes = rest;
    out
}
fn u8_in(bytes: &mut &[u8]) -> u8 {
    take(bytes, 1)[0]
}
fn u32_in(bytes: &mut &[u8]) -> u32 {
    u32::from_le_bytes(take(bytes, 4).try_into().unwrap())
}
fn u64_in(bytes: &mut &[u8]) -> u64 {
    u64::from_le_bytes(take(bytes, 8).try_into().unwrap())
}

fn compare(path: &Path) -> (usize, usize) {
    let data = fs::read(path).unwrap();
    let reference = oracle("decode", path);
    let mut reference = reference.as_slice();
    let mut reader = PageReader::new(Cursor::new(data));
    let mut demux = Demuxer::default();
    let (mut pages, mut packets) = (0, 0);
    while let Some(located) = reader.next_page().unwrap() {
        let page = located.page;
        let header = page.header();
        assert_eq!(u8_in(&mut reference), b'G');
        assert_eq!(u32_in(&mut reference), header.serial);
        assert_eq!(u32_in(&mut reference), header.sequence);
        assert_eq!(u64_in(&mut reference), header.granule_position);
        assert_eq!(u8_in(&mut reference), header.flags);
        assert_eq!(u32_in(&mut reference) as usize, page.bytes().len());
        demux
            .push_page(page, located.offset, |packet| {
                assert_eq!(u8_in(&mut reference), b'P');
                assert_eq!(u32_in(&mut reference), packet.info.serial);
                assert_eq!(
                    u64_in(&mut reference),
                    packet
                        .info
                        .granule_position
                        .unwrap_or(UNKNOWN_GRANULE_POSITION)
                );
                assert_eq!(u8_in(&mut reference) != 0, packet.info.first_in_stream);
                assert_eq!(u8_in(&mut reference) != 0, packet.info.last_in_stream);
                let len = u64_in(&mut reference) as usize;
                assert_eq!(take(&mut reference, len), packet.data);
                packets += 1;
            })
            .unwrap_or_else(|error| panic!("{} at page {}: {error}", path.display(), pages));
        pages += 1;
    }
    demux.finish().unwrap();
    assert!(reference.is_empty());
    (pages, packets)
}

#[test]
#[ignore = "requires independently built libogg oracle; see tests/oracle/README.md"]
fn libogg_bidirectional_packet_and_page_parity() {
    let root = std::env::temp_dir().join(format!("deadlib-ogg-oracle-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let sizes = [12, 0, 1, 254, 255, 256, 65024, 65025, 130050, 1, 0];
    let mut native = PacketWriter::new(Vec::new(), 4_278_190_081);
    let mut protocol = 4_278_190_081u32.to_le_bytes().to_vec();
    let mut expected = Vec::new();
    for (i, &len) in sizes.iter().enumerate() {
        let data: Vec<u8> = (0..len).map(|n| (n * 17 + i * 21) as u8).collect();
        let granule = i as u64 * 97;
        let eos = i == sizes.len() - 1;
        native.write_packet(&data, granule, eos).unwrap();
        protocol.push(1);
        protocol.extend_from_slice(&(len as u64).to_le_bytes());
        protocol.extend_from_slice(&granule.to_le_bytes());
        protocol.push(eos as u8);
        protocol.extend_from_slice(&data);
        expected.push(data);
    }
    let file = root.join("native.ogg");
    fs::write(&file, native.into_inner()).unwrap();
    assert_eq!(compare(&file).1, sizes.len());
    let file = root.join("packets.bin");
    fs::write(&file, protocol).unwrap();
    let reference = oracle("encode", &file);
    let file = root.join("libogg.ogg");
    fs::write(&file, &reference).unwrap();
    assert_eq!(compare(&file).1, sizes.len());
    let mut reader = PageReader::new(Cursor::new(reference));
    let mut demux = Demuxer::default();
    let mut index = 0;
    while let Some(located) = reader.next_page().unwrap() {
        demux
            .push_page(located.page, located.offset, |p| {
                assert_eq!(p.data, expected[index]);
                index += 1;
            })
            .unwrap();
    }
    demux.finish().unwrap();
    assert_eq!(index, sizes.len());
    // Delete only our individually named artifacts; no recursive cleanup.
    for name in ["native.ogg", "packets.bin", "libogg.ogg"] {
        fs::remove_file(root.join(name)).unwrap();
    }
    fs::remove_dir(root).unwrap();
}

fn collect(root: &Path, paths: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            collect(&entry.path(), paths);
        } else if entry
            .path()
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ogg"))
        {
            paths.push(entry.path());
        }
    }
}
#[test]
#[ignore = "requires libogg oracle and DEADLIB_OGG_CORPUS directory"]
fn libogg_local_corpus() {
    let root = std::env::var_os("DEADLIB_OGG_CORPUS").expect("set DEADLIB_OGG_CORPUS");
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "corpus must contain Ogg files");
    let (mut pages, mut packets) = (0, 0);
    for path in &paths {
        let counts = compare(path);
        pages += counts.0;
        packets += counts.1;
    }
    eprintln!(
        "libogg parity: {} files, {pages} pages, {packets} packets",
        paths.len()
    );
}
