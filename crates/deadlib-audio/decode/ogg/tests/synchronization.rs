use deadlib_audio_decode_ogg::*;
use std::{
    cell::Cell,
    fs,
    io::{Cursor, Read, Seek, SeekFrom},
    process::Command,
};

#[derive(Clone)]
enum Op {
    Reset,
    Push(Vec<u8>),
    Seek,
    Out,
}
#[derive(Clone, Copy)]
enum ReadOp {
    Next(i64),
    Seek(u64),
    FailRead,
    FailSeek,
}
fn put(w: &mut Vec<u8>, n: u64) {
    w.extend(n.to_le_bytes());
}
fn put_blob(w: &mut Vec<u8>, b: &[u8]) {
    put(w, b.len() as u64);
    w.extend(b);
}
fn take<'a>(w: &mut &'a [u8], n: usize) -> &'a [u8] {
    let (a, b) = w.split_at(n);
    *w = b;
    a
}
fn num(w: &mut &[u8]) -> u64 {
    u64::from_le_bytes(take(w, 8).try_into().unwrap())
}
fn blob(w: &mut &[u8]) -> Vec<u8> {
    let n = num(w) as usize;
    take(w, n).to_vec()
}
fn request(ops: &[Op]) -> Vec<u8> {
    let mut w = Vec::new();
    put(&mut w, ops.len() as u64);
    for op in ops {
        match op {
            Op::Reset => put(&mut w, 0),
            Op::Push(b) => {
                put(&mut w, 1);
                put_blob(&mut w, b);
            }
            Op::Seek => put(&mut w, 2),
            Op::Out => put(&mut w, 3),
        }
    }
    w
}
fn read_request(source: &[u8], size: usize, ops: &[ReadOp]) -> Vec<u8> {
    let mut w = Vec::new();
    put_blob(&mut w, source);
    put(&mut w, size as u64);
    put(&mut w, ops.len() as u64);
    for op in ops {
        match op {
            ReadOp::Next(n) => {
                put(&mut w, 0);
                put(&mut w, *n as u64);
            }
            ReadOp::Seek(n) => {
                put(&mut w, 1);
                put(&mut w, *n);
            }
            ReadOp::FailRead => put(&mut w, 2),
            ReadOp::FailSeek => put(&mut w, 3),
        }
    }
    w
}
fn native(mut q: &[u8]) -> Vec<u8> {
    let count = num(&mut q);
    let mut w = Vec::new();
    put(&mut w, count);
    let mut sync = SyncBuffer::with_limit(32 << 20);
    for _ in 0..count {
        let mut page = Vec::new();
        let result = match num(&mut q) {
            0 => {
                sync.reset();
                0
            }
            1 => {
                sync.push(&blob(&mut q)).unwrap();
                0
            }
            2 => match sync.page_seek().unwrap() {
                PageSeek::NeedMore => 0,
                PageSeek::Skipped(n) => -(n as i64),
                PageSeek::Page(p) => {
                    page.extend(p.page.bytes());
                    page.len() as i64
                }
            },
            3 => match sync.page_out().unwrap() {
                PageOut::NeedMore => 0,
                PageOut::Hole => -1,
                PageOut::Page(p) => {
                    page.extend(p.page.bytes());
                    1
                }
            },
            _ => panic!(),
        };
        put(&mut w, result as u64);
        put(&mut w, sync.buffered_bytes() as u64);
        w.push(sync.is_unsynced() as u8);
        put_blob(&mut w, &page);
    }
    assert!(q.is_empty());
    w
}
struct Source {
    cursor: Cursor<Vec<u8>>,
    reads: u64,
    seeks: u64,
    fail_read: Cell<bool>,
    fail_seek: Cell<bool>,
}
impl Read for Source {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        self.reads += 1;
        if self.fail_read.replace(false) {
            return Err(std::io::Error::other("injected read"));
        }
        self.cursor.read(b)
    }
}
impl Seek for Source {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        self.seeks += 1;
        if self.fail_seek.replace(false) {
            return Err(std::io::Error::other("injected seek"));
        }
        self.cursor.seek(from)
    }
}
fn native_read(mut q: &[u8]) -> Vec<u8> {
    let source = Source {
        cursor: Cursor::new(blob(&mut q)),
        reads: 0,
        seeks: 0,
        fail_read: Cell::new(false),
        fail_seek: Cell::new(false),
    };
    let size = num(&mut q) as usize;
    let count = num(&mut q);
    let mut w = Vec::new();
    put(&mut w, count);
    let mut reader = SyncReader::with_options(
        source,
        SyncReadOptions {
            read_size: size,
            max_buffered_bytes: MAX_PAGE_SIZE + size,
        },
    )
    .unwrap();
    for _ in 0..count {
        let mut page = Vec::new();
        let result = match num(&mut q) {
            0 => {
                let n = num(&mut q) as i64;
                let boundary = if n < 0 {
                    SyncBoundary::Unbounded
                } else {
                    SyncBoundary::Bytes(n as u64)
                };
                match reader.next_page(boundary) {
                    Ok(SyncRead::Page(p)) => {
                        page.extend(p.page.bytes());
                        p.offset as i64
                    }
                    Ok(SyncRead::Boundary) => -1,
                    Ok(SyncRead::Eof) => -2,
                    Err(e) => {
                        assert!(matches!(e, Error::Io(_)), "{e:?}");
                        -128
                    }
                }
            }
            1 => match reader.seek(num(&mut q)) {
                Ok(()) => 0,
                Err(e) => {
                    assert!(matches!(e, Error::Io(_)));
                    -128
                }
            },
            2 => {
                reader.get_ref().fail_read.set(true);
                0
            }
            3 => {
                reader.get_ref().fail_seek.set(true);
                0
            }
            _ => panic!(),
        };
        put(&mut w, result as u64);
        put(&mut w, reader.position());
        put(&mut w, reader.get_ref().cursor.position());
        put(&mut w, reader.get_ref().reads);
        put(&mut w, reader.get_ref().seeks);
        put(&mut w, reader.buffered_bytes() as u64);
        w.push(0);
        put_blob(&mut w, &page);
    }
    assert!(q.is_empty());
    w
}
fn oracle(mode: &str, label: &str, q: &[u8]) -> Vec<u8> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "deadlib-sync-{}-{}.bin",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, q).unwrap();
    let r = Command::new(std::env::var_os("DEADLIB_LIBOGG_ORACLE").expect("set oracle"))
        .arg(mode)
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    assert!(
        r.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    if let Some(root) = std::env::var_os("DEADLIB_OGG_SYNC_INPUTS") {
        fs::create_dir_all(&root).unwrap();
        for (ext, b) in [("bin", q), ("out", r.stdout.as_slice())] {
            fs::write(
                std::path::Path::new(&root).join(format!("{label}.{ext}")),
                b,
            )
            .unwrap();
        }
    }
    r.stdout
}
fn page(len: usize) -> Vec<u8> {
    let body: Vec<_> = (0..len)
        .map(|i| (i.wrapping_mul(31) ^ (i >> 3)) as u8)
        .collect();
    let mut laces = vec![255; (len / 255).min(255)];
    if laces.len() < 255 {
        laces.push((len % 255) as u8);
    }
    encode_page(
        PageHeader {
            flags: 0,
            granule_position: 37,
            serial: 19,
            sequence: 3,
        },
        &laces,
        &body,
    )
    .unwrap()
}
fn repair(b: &mut [u8]) {
    b[22..26].fill(0);
    let crc = crc32(b);
    b[22..26].copy_from_slice(&crc.to_le_bytes());
}
fn streams() -> Vec<Vec<u8>> {
    let small = page(37);
    let mut invalid = small.clone();
    invalid[5] = 0xf8;
    repair(&mut invalid);
    let mut version = small.clone();
    version[4] = 255;
    repair(&mut version);
    let mut corrupt = page(512);
    corrupt[40] ^= 128;
    let mut nested = page(256);
    nested[60..60 + small.len()].copy_from_slice(&small);
    repair(&mut nested);
    let mut corrupt_nested = nested.clone();
    corrupt_nested[24] ^= 1;
    let mut mixed = b"junk O OggS nope \0".to_vec();
    for p in [
        &corrupt,
        &invalid,
        &version,
        &corrupt_nested,
        &small,
        &nested,
    ] {
        mixed.extend(p);
    }
    mixed.extend(b"OggS");
    vec![
        vec![],
        vec![b'O'; 26],
        vec![b'O'; 27],
        small.clone(),
        invalid,
        version,
        corrupt,
        corrupt_nested,
        nested,
        mixed,
        page(0),
        page(255),
        page(65025),
    ]
}
fn schedule(bytes: &[u8], chunk: usize, profile: usize) -> Vec<Op> {
    let mut ops = vec![Op::Reset, Op::Push(vec![]), Op::Seek, Op::Out];
    for (i, b) in bytes.chunks(chunk).enumerate() {
        ops.push(Op::Push(b.to_vec()));
        ops.push(if (i + profile).is_multiple_of(3) {
            Op::Out
        } else {
            Op::Seek
        });
    }
    for i in 0..32 {
        ops.push(if (i + profile).is_multiple_of(2) {
            Op::Out
        } else {
            Op::Seek
        });
    }
    ops
}
fn reader_schedule(bytes: &[u8]) -> Vec<ReadOp> {
    let mut ops = vec![
        ReadOp::Next(0),
        ReadOp::Next(1),
        ReadOp::Next(0),
        ReadOp::Seek(0),
        ReadOp::Next(-1),
        ReadOp::Next(0),
    ];
    for at in [
        0,
        1,
        4,
        26,
        27,
        28,
        bytes.len() / 2,
        bytes.len().saturating_sub(1),
        bytes.len(),
        bytes.len() + 1,
        0,
    ] {
        ops.push(ReadOp::Seek(at as u64));
        for bound in [0, 1, 27, 65536, -1, 0] {
            ops.push(ReadOp::Next(bound));
        }
    }
    ops.extend([
        ReadOp::Seek(0),
        ReadOp::FailRead,
        ReadOp::Next(-1),
        ReadOp::Next(-1),
        ReadOp::Seek(0),
        ReadOp::FailSeek,
        ReadOp::Seek(0),
        ReadOp::Seek(17),
        ReadOp::Next(-1),
        ReadOp::Seek(0),
        ReadOp::Next(-1),
    ]);
    ops
}
#[test]
#[ignore = "requires independent libogg synchronization oracle"]
fn buffered_sync_and_mixed_page_polling_match_libogg() {
    let mut cases = 0;
    let mut operations = 0;
    for (i, b) in streams().iter().enumerate() {
        for chunk in [1, 2, 3, 4, 7, 26, 27, 28, 254, 255, 256, 2048, 4096, 65536] {
            if b.len() > 8192 && chunk < 27 {
                continue;
            }
            for profile in 0..3 {
                let ops = schedule(b, chunk, profile);
                let q = request(&ops);
                let label = format!("sync-state-{i}-chunk-{chunk}-profile-{profile}");
                assert_eq!(native(&q), oracle("sync-ops", &label, &q), "{label}");
                cases += 1;
                operations += ops.len();
            }
        }
    }
    eprintln!("SYNC-STATE {cases} cases, {operations} operations");
}
#[test]
#[ignore = "requires independent libogg bounded read/seek oracle"]
fn bounded_cached_reads_repeated_seeks_and_io_errors_match_reference() {
    let mut cases = 0;
    let mut operations = 0;
    for (i, b) in streams().iter().enumerate() {
        for size in [1, 7, 27, 255, 2048, 4096, 65536] {
            let ops = reader_schedule(b);
            let q = read_request(b, size, &ops);
            let label = format!("sync-reader-{i}-read-{size}");
            assert_eq!(
                native_read(&q),
                oracle("sync-read-ops", &label, &q),
                "{label}"
            );
            cases += 1;
            operations += ops.len();
        }
    }
    eprintln!("SYNC-READER {cases} cases, {operations} operations");
}

#[test]
#[ignore = "requires independent libogg raw header/lacing oracle"]
fn all_transport_versions_flags_and_lacing_counts_match_reference() {
    let mut cases = 0;
    let mut operations = 0;
    for version in 0..=255u8 {
        let mut ops = vec![Op::Reset];
        for flags in 0..=255u8 {
            let mut p = encode_page(
                PageHeader {
                    flags: 0,
                    granule_position: [0, u64::MAX, i64::MAX as u64, 1u64 << 63]
                        [flags as usize % 4],
                    serial: (version as u32) << 24 | flags as u32,
                    sequence: (flags as u32).wrapping_mul(0x01010101),
                },
                &[],
                &[],
            )
            .unwrap();
            p[4] = version;
            p[5] = flags;
            repair(&mut p);
            ops.push(Op::Push(p));
            ops.push(if flags & 1 == 0 { Op::Seek } else { Op::Out });
        }
        let q = request(&ops);
        let label = format!("sync-raw-version-{version}");
        assert_eq!(native(&q), oracle("sync-ops", &label, &q), "{label}");
        cases += 1;
        operations += ops.len();
    }
    for lace in [0, 1, 254, 255] {
        let mut ops = vec![Op::Reset];
        for count in 0..=255 {
            let p = encode_page(
                PageHeader {
                    flags: 0,
                    granule_position: u64::MAX,
                    serial: 8,
                    sequence: count as u32,
                },
                &vec![lace; count],
                &vec![0; count * lace as usize],
            )
            .unwrap();
            ops.extend([
                Op::Push(p[..26].to_vec()),
                Op::Seek,
                Op::Push(p[26..27 + count].to_vec()),
                Op::Out,
                Op::Push(p[27 + count..].to_vec()),
                Op::Seek,
                Op::Out,
            ]);
        }
        let q = request(&ops);
        let label = format!("sync-lacing-{lace}");
        assert_eq!(native(&q), oracle("sync-ops", &label, &q), "{label}");
        cases += 1;
        operations += ops.len();
    }
    eprintln!(
        "SYNC-RAW {cases} batches, {operations} operations, 65536 version/flag pairs, 1024 lacing layouts"
    );
}
#[test]
fn transport_version_flags_and_nested_captures_are_explicit() {
    for (at, b) in streams().iter().enumerate() {
        let mut s = SyncBuffer::new();
        s.push(b).unwrap();
        let mut steps = 0;
        loop {
            steps += 1;
            assert!(steps <= b.len() + 2);
            match s.page_seek().unwrap() {
                PageSeek::NeedMore => break,
                PageSeek::Skipped(n) => assert!(n > 0),
                PageSeek::Page(p) => {
                    let strict = p.page.validated();
                    assert_eq!(
                        strict.is_ok(),
                        p.page.version() == 0 && p.page.header().flags & !7 == 0
                    );
                    if at == 4 || at == 5 {
                        assert!(strict.is_err());
                    }
                    if at == 8 {
                        assert_eq!(p.offset, 0);
                        assert_eq!(p.page.bytes().len(), b.len());
                    }
                    if at == 7 {
                        assert_eq!(p.offset, 60);
                    }
                }
            }
        }
    }
}
#[test]
fn append_limits_and_offset_errors_preserve_unread_data() {
    let p = page(37);
    let mut s = SyncBuffer::with_limit(p.len());
    s.push(&p).unwrap();
    assert!(s.push(&[1]).is_err());
    assert_eq!(s.position(), 0);
    assert_eq!(s.buffered_bytes(), p.len());
    assert!(matches!(s.page_seek().unwrap(), PageSeek::Page(_)));
    assert_eq!(s.buffered_bytes(), 0);
    assert!(s.push(&vec![0; p.len() + 1]).is_err());
    assert_eq!(s.current_page().unwrap().page.bytes(), p);
    s.reset_at(u64::MAX);
    s.push(&p).unwrap();
    assert!(s.page_seek().is_err());
    assert_eq!(s.position(), u64::MAX);
    assert_eq!(s.buffered_bytes(), p.len());
    assert!(matches!(
        SyncReader::with_options(
            Cursor::new(&p),
            SyncReadOptions {
                read_size: 0,
                max_buffered_bytes: 100
            }
        ),
        Err(Error::Invalid(_))
    ));
    let mut reader = SyncReader::with_options(
        Cursor::new(&p),
        SyncReadOptions {
            read_size: 27,
            max_buffered_bytes: 27,
        },
    )
    .unwrap();
    assert!(matches!(
        reader.next_page(SyncBoundary::Unbounded),
        Err(Error::LimitExceeded(_))
    ));
    assert_eq!(reader.position(), 0);
    assert_eq!(reader.buffered_bytes(), 27);
}

#[test]
fn partial_capture_length_survives_compaction_and_rejection_but_not_reset() {
    let large = page(65025);
    let small = page(37);
    let header = 27 + large[26] as usize;
    let mut input = small.clone();
    input.extend_from_slice(&large[..header]);
    let mut sync = SyncBuffer::with_limit(large.len() + small.len());
    sync.push(&input).unwrap();
    assert!(matches!(sync.page_seek().unwrap(), PageSeek::Page(_)));
    assert!(matches!(sync.page_seek().unwrap(), PageSeek::NeedMore));
    assert!(sync.push(&large).is_err());
    assert_eq!(sync.position(), small.len() as u64);
    assert_eq!(sync.buffered_bytes(), header);
    assert!(matches!(sync.page_seek().unwrap(), PageSeek::NeedMore));
    sync.push(&large[header..]).unwrap();
    let PageSeek::Page(found) = sync.page_seek().unwrap() else {
        panic!("compacted page missing");
    };
    assert_eq!(found.offset, small.len() as u64);
    assert_eq!(found.page.bytes(), large);
    sync.push(&large[..header]).unwrap();
    assert!(matches!(sync.page_seek().unwrap(), PageSeek::NeedMore));
    sync.reset_at(123);
    sync.push(&small).unwrap();
    let PageSeek::Page(found) = sync.page_seek().unwrap() else {
        panic!("page after reset missing");
    };
    assert_eq!(found.offset, 123);
    assert_eq!(found.page.bytes(), small);
}

#[test]
fn rolling_pages_preserve_capacity_current_offsets_and_rejected_append_state() {
    let frame = page(37);
    let warm = frame.repeat(8);
    let rejected = vec![0; warm.len() + 1];
    let base = 1u64 << 40;
    let mut sync = SyncBuffer::with_limit(warm.len());
    sync.reset_at(base);
    sync.push(&warm).unwrap();
    let capacity = sync.storage_bytes();
    for number in 0..4 {
        let PageSeek::Page(found) = sync.page_seek().unwrap() else {
            panic!("warm page missing");
        };
        assert_eq!(found.offset, base + number * frame.len() as u64);
    }
    for number in 4..36 {
        {
            let PageSeek::Page(found) = sync.page_seek().unwrap() else {
                panic!("rolling page missing");
            };
            assert_eq!(found.offset, base + number * frame.len() as u64);
            assert_eq!(found.page.bytes(), frame);
        }
        assert_eq!(
            sync.push(&rejected),
            Err(Error::LimitExceeded("sync buffer bytes"))
        );
        let current = sync.current_page().unwrap();
        assert_eq!(current.offset, base + number * frame.len() as u64);
        assert_eq!(current.page.bytes(), frame);
        assert_eq!(sync.buffered_bytes(), frame.len() * 3);
        sync.push(&frame).unwrap();
        assert!(sync.current_page().is_none());
        assert_eq!(sync.storage_bytes(), capacity);
    }
    for number in 36..40 {
        let PageSeek::Page(found) = sync.page_seek().unwrap() else {
            panic!("tail page missing");
        };
        assert_eq!(found.offset, base + number * frame.len() as u64);
        assert_eq!(found.page.bytes(), frame);
    }
    assert!(matches!(sync.page_seek().unwrap(), PageSeek::NeedMore));
    assert!(sync.current_page().is_none());
    sync.reset_at(u64::MAX - frame.len() as u64);
    sync.push(&frame).unwrap();
    assert!(matches!(sync.page_seek().unwrap(), PageSeek::Page(_)));
    assert_eq!(
        sync.current_page().unwrap().offset,
        u64::MAX - frame.len() as u64
    );
    assert_eq!(sync.position(), u64::MAX);
    sync.push(&frame).unwrap();
    assert!(sync.page_seek().is_err());
    assert_eq!(sync.buffered_bytes(), frame.len());
    assert!(sync.current_page().is_none());
}

fn golden_inputs() -> Vec<(u8, Vec<u8>)> {
    let s = streams();
    vec![
        (0, request(&schedule(&s[9], 27, 1))),
        (0, request(&schedule(&s[7], 7, 2))),
        (1, read_request(&s[9], 64, &reader_schedule(&s[9]))),
        (1, read_request(&s[12], 2048, &reader_schedule(&s[12]))),
    ]
}
#[test]
#[ignore = "manual external-oracle golden regeneration"]
fn regenerate_sync_state_golden() {
    let mut out = b"DSYOGG01".to_vec();
    out.extend(4u32.to_le_bytes());
    for (i, (kind, q)) in golden_inputs().into_iter().enumerate() {
        let mode = if kind == 0 {
            "sync-ops"
        } else {
            "sync-read-ops"
        };
        let r = oracle(mode, &format!("sync-golden-{i}"), &q);
        let a = if kind == 0 {
            native(&q)
        } else {
            native_read(&q)
        };
        assert_eq!(a, r);
        out.push(kind);
        put_blob(&mut out, &q);
        put_blob(&mut out, &r);
    }
    fs::create_dir_all(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures")).unwrap();
    fs::write(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/sync-state-v1.libogg"
        ),
        out,
    )
    .unwrap();
}
#[test]
fn retained_sync_state_golden() {
    let mut b = include_bytes!("fixtures/sync-state-v1.libogg").as_slice();
    assert_eq!(take(&mut b, 8), b"DSYOGG01");
    assert_eq!(u32::from_le_bytes(take(&mut b, 4).try_into().unwrap()), 4);
    for (kind, expected_q) in golden_inputs() {
        assert_eq!(take(&mut b, 1)[0], kind);
        let q = blob(&mut b);
        let expected = blob(&mut b);
        assert_eq!(q, expected_q);
        assert_eq!(
            if kind == 0 {
                native(&q)
            } else {
                native_read(&q)
            },
            expected
        );
    }
    assert!(b.is_empty());
}
