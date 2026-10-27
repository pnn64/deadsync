use deadlib_audio_decode_ogg::{Demuxer, PageReader};
use deadlib_audio_decode_vorbis::{bitpack::BitReader, *};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};

fn oracle(mode: &str, packets: &[&[u8]]) -> Vec<u8> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let executable = std::env::var_os("DEADLIB_LIBVORBIS_ORACLE")
        .expect("set DEADLIB_LIBVORBIS_ORACLE to compiled helper");
    let file = std::env::temp_dir().join(format!(
        "deadlib-vorbis-oracle-{}-{}-{mode}.bin",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut input = Vec::new();
    for packet in packets {
        input.extend((packet.len() as u64).to_le_bytes());
        input.extend(*packet);
    }
    fs::write(&file, input).unwrap();
    let output = Command::new(executable)
        .arg(mode)
        .arg(&file)
        .output()
        .unwrap();
    fs::remove_file(file).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn take<'a>(input: &mut &'a [u8], len: usize) -> &'a [u8] {
    let (out, rest) = input.split_at(len);
    *input = rest;
    out
}
fn u32_in(input: &mut &[u8]) -> u32 {
    u32::from_le_bytes(take(input, 4).try_into().unwrap())
}
fn u64_in(input: &mut &[u8]) -> u64 {
    u64::from_le_bytes(take(input, 8).try_into().unwrap())
}
fn vector<'a>(input: &mut &'a [u8]) -> &'a [u8] {
    let len = u32_in(input) as usize;
    take(input, len)
}

fn compare(bytes: &[u8]) -> (usize, usize, f32) {
    let mut reader = PageReader::new(Cursor::new(bytes));
    let mut demux = Demuxer::default();
    let mut packets = [Vec::new(), Vec::new(), Vec::new()];
    while let Some(located) = reader.next_page().unwrap() {
        demux
            .push_page(located.page, located.offset, |p| {
                if p.info.number < 3 {
                    packets[p.info.number as usize] = p.data.to_vec();
                }
            })
            .unwrap();
    }
    demux.finish().unwrap();
    let id = Identification::parse(&packets[0]).unwrap();
    let comments = Comments::parse(&packets[1], &Limits::default()).unwrap();
    let setup = Setup::parse(&packets[2], &id, &Limits::default()).unwrap();
    let reference = oracle("headers", &[&packets[0], &packets[1], &packets[2]]);
    let mut input = reference.as_slice();
    assert_eq!(u32_in(&mut input), id.channels as u32);
    assert_eq!(u32_in(&mut input), id.sample_rate);
    assert_eq!(u32_in(&mut input) as i32, id.bitrate_maximum);
    assert_eq!(u32_in(&mut input) as i32, id.bitrate_nominal);
    assert_eq!(u32_in(&mut input) as i32, id.bitrate_minimum);
    for size in id.block_sizes {
        assert_eq!(u32_in(&mut input) as usize, size);
    }
    assert_eq!(vector(&mut input), comments.vendor);
    assert_eq!(u32_in(&mut input) as usize, comments.comments.len());
    for comment in comments.comments {
        assert_eq!(vector(&mut input), comment);
    }
    assert!(input.is_empty());
    let reference = oracle("books", &[&packets[2]]);
    let mut input = reference.as_slice();
    assert_eq!(u32_in(&mut input) as usize, setup.codebooks().len());
    let (mut decoded, mut maximum_error) = (0, 0.0f32);
    for book in setup.codebooks() {
        assert_eq!(u32_in(&mut input) as usize, book.dimensions());
        assert_eq!(u32_in(&mut input) as usize, book.entries());
        assert_eq!(take(&mut input, 1)[0], book.lookup_type());
        assert_eq!(take(&mut input, book.entries()), book.lengths());
        for (entry, &length) in book.lengths().iter().enumerate() {
            if length == 0 || book.lookup_type() == 0 {
                continue;
            }
            for &native in book.vector(entry as u32).unwrap() {
                let reference = f32::from_bits(u32_in(&mut input));
                let difference = (native - reference).abs();
                maximum_error = maximum_error.max(difference);
                assert!(
                    difference <= 2.0e-6 * reference.abs().max(1.0),
                    "native={native}, libvorbis={reference}"
                );
            }
        }
        let bit_count = u64_in(&mut input) as usize;
        let wire = vector(&mut input);
        let mut bits = BitReader::new(wire);
        for (entry, &length) in book.lengths().iter().enumerate() {
            if length != 0 {
                assert_eq!(book.decode_scalar(&mut bits).unwrap(), entry as u32);
                decoded += 1;
            }
        }
        assert_eq!(bits.position(), bit_count);
    }
    assert!(input.is_empty());
    (setup.codebooks().len(), decoded, maximum_error)
}
#[test]
#[ignore = "requires external libvorbis oracle; see tests/oracle/README.md"]
fn libvorbis_generated_header_codebook_vq_and_huffman_parity() {
    for bytes in [
        include_bytes!("fixtures/stereo.ogg").as_slice(),
        include_bytes!("fixtures/eight-channel.ogg").as_slice(),
    ] {
        let (books, entries, error) = compare(bytes);
        eprintln!("{books} books, {entries} entries, max VQ error {error}");
    }
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
#[ignore = "requires external libvorbis oracle and DEADLIB_VORBIS_CORPUS"]
fn libvorbis_local_corpus_headers_codebooks_and_huffman() {
    let root = std::env::var_os("DEADLIB_VORBIS_CORPUS").expect("set DEADLIB_VORBIS_CORPUS");
    let mut paths = Vec::new();
    collect(Path::new(&root), &mut paths);
    paths.sort();
    assert!(!paths.is_empty());
    let (mut books, mut entries, mut error) = (0, 0, 0.0f32);
    for path in &paths {
        eprintln!("{}", path.display());
        let result = compare(&fs::read(path).unwrap());
        books += result.0;
        entries += result.1;
        error = error.max(result.2);
    }
    eprintln!(
        "libvorbis parity: {} files, {books} codebooks, {entries} entries, max VQ error {error}",
        paths.len()
    );
}
