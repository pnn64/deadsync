//! Direct C vector-add comparisons for partial residue partitions.
use super::*;
use crate::{CodebookBehavior, ParseOptions, bitpack::BitWriter};
use std::{
    collections::BTreeSet,
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
#[derive(Clone, Copy, Debug)]
struct Book {
    dim: u16,
    empty: bool,
    sequence: bool,
}
impl Book {
    fn packed(self) -> Vec<u8> {
        let mut b = BitWriter::new();
        for (v, n) in [
            (0x564342, 24),
            (self.dim as u32, 16),
            (4, 24),
            (0, 1),
            (self.empty as u32, 1),
        ] {
            b.write(v, n).unwrap();
        }
        for _ in 0..4 {
            if self.empty {
                b.write(0, 1).unwrap();
            } else {
                b.write(1, 5).unwrap();
            }
        }
        for (v, n) in [
            (2, 4),
            (0x80000000 | (787 << 21) | 1, 32),
            ((786 << 21) | 1, 32),
            (2, 4),
            (self.sequence as u32, 1),
        ] {
            b.write(v, n).unwrap();
        }
        for i in 0..4 * self.dim as u32 {
            b.write(i % 7 + 1, 3).unwrap();
        }
        let mut out = b.finish().unwrap();
        out.push(0);
        out
    }
}
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Case {
    kind: u8,
    prefix: u32,
    exhausted: bool,
    start: usize,
    size: usize,
    channels: usize,
    bins: usize,
    data: Vec<u8>,
}
fn cases(book: Book, compact: bool) -> Vec<Case> {
    let mut out = BTreeSet::new();
    let data: Vec<_> = (0..64).map(|i| (i * 73 + 0xd8) as u8).collect();
    for kind in 0..=2 {
        for channels in if compact {
            vec![1, 3]
        } else {
            vec![1, 2, 3, 5]
        } {
            let bins = 32;
            let actual = bins * if kind == 2 { channels } else { 1 };
            for start in [0, 1, 3] {
                let lengths: BTreeSet<_> = [
                    0,
                    1,
                    (book.dim as usize).saturating_sub(1),
                    book.dim as usize,
                    book.dim as usize + 1,
                    book.dim as usize * 2 + 1,
                    actual - start,
                ]
                .into_iter()
                .map(|n| n.min(actual - start))
                .collect();
                for size in lengths {
                    for prefix in if compact { vec![0, 7] } else { vec![0, 3, 7] } {
                        for length in if compact {
                            vec![1, 64]
                        } else {
                            vec![0, 1, 2, 3, 8, 64]
                        } {
                            if length == 0 && prefix != 0 {
                                continue;
                            }
                            let c = Case {
                                kind,
                                prefix,
                                exhausted: false,
                                start,
                                size,
                                channels,
                                bins,
                                data: data[..length].to_vec(),
                            };
                            out.insert(c.clone());
                            if size == 1 && prefix == 0 && length == 1 {
                                out.insert(Case {
                                    exhausted: true,
                                    ..c
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}
fn take<'a>(wire: &mut &'a [u8], n: usize) -> &'a [u8] {
    let (v, r) = wire.split_at(n);
    *wire = r;
    v
}
fn u64_in(wire: &mut &[u8]) -> u64 {
    u64::from_le_bytes(take(wire, 8).try_into().unwrap())
}
fn u32_in(wire: &mut &[u8]) -> u32 {
    u32::from_le_bytes(take(wire, 4).try_into().unwrap())
}
fn packet(out: &mut Vec<u8>, data: &[u8]) {
    out.extend((data.len() as u64).to_le_bytes());
    out.extend(data);
}
fn request(book: Book, cases: &[Case]) -> Vec<u8> {
    let mut out = Vec::new();
    packet(&mut out, &book.packed());
    out.extend((cases.len() as u64).to_le_bytes());
    for c in cases {
        for v in [
            c.kind as u64,
            c.prefix as u64,
            c.exhausted as u64,
            c.start as u64,
            c.size as u64,
            c.channels as u64,
            c.bins as u64,
        ] {
            out.extend(v.to_le_bytes());
        }
        packet(&mut out, &c.data);
    }
    out
}
fn oracle(label: &str, input: &[u8]) -> Vec<u8> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "deadlib-residue-add-{}-{}.bin",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, input).unwrap();
    let r = Command::new(std::env::var_os("DEADLIB_LIBVORBIS_ORACLE").expect("set oracle"))
        .arg("vqaddcases")
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_RESIDUE_TAIL_INPUTS") {
        fs::create_dir_all(&root).unwrap();
        fs::write(
            std::path::Path::new(&root).join(format!("{label}.bin")),
            input,
        )
        .unwrap();
        fs::write(
            std::path::Path::new(&root).join(format!("{label}.out")),
            &r.stdout,
        )
        .unwrap();
    }
    r.stdout
}
#[derive(Default)]
struct Counts {
    cases: usize,
    values: usize,
    eop: usize,
    exhausted: usize,
}
fn compare(mut input: &[u8], mut reference: &[u8]) -> Counts {
    let n = u64_in(&mut input) as usize;
    let data = take(&mut input, n);
    let book = Codebook::read_with_options(
        &mut BitReader::new(data),
        ParseOptions {
            allow_empty_codebooks: true,
            codebook_behavior: CodebookBehavior::Libvorbis,
            ..ParseOptions::default()
        },
    )
    .unwrap();
    let count = u64_in(&mut input);
    assert_eq!(u64_in(&mut reference), count);
    let mut total = Counts::default();
    for record in 0..count {
        let kind = u64_in(&mut input) as u8;
        let prefix = u64_in(&mut input) as u32;
        let exhausted = u64_in(&mut input) != 0;
        let start = u64_in(&mut input) as usize;
        let size = u64_in(&mut input) as usize;
        let channels = u64_in(&mut input) as usize;
        let bins = u64_in(&mut input) as usize;
        let n = u64_in(&mut input) as usize;
        let data = take(&mut input, n);
        let success = take(&mut reference, 1)[0] != 0;
        let position = u64_in(&mut reference) as usize;
        let eof = take(&mut reference, 1)[0] != 0;
        let stride = bins + 16;
        let physical = channels * 2 + 1;
        let sentinel = -1234.25f32;
        let mut storage = vec![sentinel; physical * stride];
        let bundle: Vec<_> = (0..channels)
            .map(|ch| (channels - 1 - ch) * 2 + 1)
            .collect();
        for (ch, &plane) in bundle.iter().enumerate() {
            for bin in 0..bins {
                storage[8 + plane * stride + bin] =
                    0.125 + ch as f32 * 0.0625 + bin as f32 * 0.00390625;
            }
        }
        let mut bits = BitReader::new(data);
        bits.read(prefix).unwrap();
        if exhausted {
            bits.mark_exhausted();
        }
        let result = decode_partition(
            kind,
            &mut bits,
            &book,
            &mut Output {
                behavior: ResidueBehavior::Libvorbis,
                layout: Residue2Layout::Libvorbis,
                spectrum: &mut storage[8..],
                stride,
                bins,
                bundle: &bundle,
                nonzero: &[],
                classes: &mut [],
            },
            0,
            start,
            size,
        );
        assert_eq!(
            result.is_ok(),
            success,
            "record {record}, kind {kind}, start {start}, size {size}"
        );
        assert!(result.is_ok() || matches!(result, Err(Error::EndOfPacket)));
        assert_eq!(bits.is_exhausted(), eof, "record {record}");
        if !eof {
            assert_eq!(bits.position(), position, "record {record}");
        } else {
            total.exhausted += 1;
        }
        if !success {
            total.eop += 1;
        }
        for &plane in &bundle {
            for bin in 0..bins {
                assert_eq!(
                    storage[8 + plane * stride + bin].to_bits(),
                    u32_in(&mut reference),
                    "record {record}, plane {plane}, bin {bin}"
                );
                total.values += 1;
            }
        }
        for plane in 0..physical {
            for i in 0..stride {
                if !bundle.contains(&plane) || !(8..8 + bins).contains(&i) {
                    assert_eq!(
                        storage[plane * stride + i],
                        sentinel,
                        "record {record}, guard {plane}/{i}"
                    );
                }
            }
        }
        total.cases += 1;
    }
    assert!(input.is_empty() && reference.is_empty());
    total
}
fn books() -> Vec<Book> {
    let mut out = Vec::new();
    for dim in [0, 1, 2, 3, 4, 5, 8, 31, 255, 65535] {
        for empty in [false, true] {
            if dim == 0 && !empty {
                continue;
            }
            for sequence in [false, true] {
                out.push(Book {
                    dim,
                    empty,
                    sequence,
                });
            }
        }
    }
    out
}
#[test]
#[ignore = "requires external libvorbis vector-add callbacks"]
fn partial_vector_additions_match_reference_callbacks() {
    let configs = books();
    let mut total = Counts::default();
    for (i, book) in configs.iter().copied().enumerate() {
        for (j, cases) in cases(book, false).chunks(128).enumerate() {
            let input = request(book, cases);
            let reference = oracle(&format!("vq-add-{i}-batch-{j}"), &input);
            let n = compare(&input, &reference);
            total.cases += n.cases;
            total.values += n.values;
            total.eop += n.eop;
            total.exhausted += n.exhausted;
        }
    }
    eprintln!(
        "partial VQ additions: {} books, {} cases, {} exact values, {} failed words, {} sticky EOF cases",
        configs.len(),
        total.cases,
        total.values,
        total.eop,
        total.exhausted
    );
}

fn golden_books() -> Vec<Book> {
    [
        Book {
            dim: 0,
            empty: true,
            sequence: false,
        },
        Book {
            dim: 1,
            empty: false,
            sequence: false,
        },
        Book {
            dim: 3,
            empty: false,
            sequence: true,
        },
        Book {
            dim: 5,
            empty: false,
            sequence: false,
        },
        Book {
            dim: 31,
            empty: true,
            sequence: true,
        },
        Book {
            dim: 255,
            empty: false,
            sequence: true,
        },
        Book {
            dim: 65535,
            empty: false,
            sequence: false,
        },
    ]
    .to_vec()
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis vector-add callbacks"]
fn regenerate_residue_tail_vector_golden() {
    let books = golden_books();
    let mut wire = b"DRTVQA01".to_vec();
    wire.extend((books.len() as u32).to_le_bytes());
    for (i, book) in books.into_iter().enumerate() {
        let cases: Vec<_> = cases(book, true).into_iter().step_by(7).collect();
        let input = request(book, &cases);
        let output = oracle(&format!("vq-add-golden-{i}"), &input);
        packet(&mut wire, &input);
        packet(&mut wire, &output);
    }
    fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/residue-tail-vectors-v1.libvorbis"),
        wire,
    )
    .unwrap();
}
#[test]
fn partial_vector_additions_match_retained_reference() {
    let mut wire = include_bytes!("../tests/fixtures/residue-tail-vectors-v1.libvorbis").as_slice();
    assert_eq!(take(&mut wire, 8), b"DRTVQA01");
    assert_eq!(u32_in(&mut wire) as usize, golden_books().len());
    let mut total = 0;
    for book in golden_books() {
        let cases: Vec<_> = cases(book, true).into_iter().step_by(7).collect();
        let n = u64_in(&mut wire) as usize;
        let input = take(&mut wire, n);
        assert_eq!(input, request(book, &cases));
        let n = u64_in(&mut wire) as usize;
        let response = take(&mut wire, n);
        total += compare(input, response).cases;
    }
    assert!(wire.is_empty());
    assert_eq!(total, 355);
}
#[test]
fn reference_classword_eof_preserves_reader_state() {
    let book = Codebook::read_with_options(
        &mut BitReader::new(
            &Book {
                dim: 3,
                empty: false,
                sequence: false,
            }
            .packed(),
        ),
        ParseOptions::default(),
    )
    .unwrap();
    let config = Residue {
        kind: 0,
        begin: 0,
        end: 1,
        partition_size: 1,
        classbook: 0,
        books: vec![[Some(0), None, None, None, None, None, None, None]; 4],
    };
    for behavior in [ResidueBehavior::Vorbis, ResidueBehavior::Libvorbis] {
        for exhausted in [false, true] {
            let mut bits = BitReader::new(&[]);
            if exhausted {
                bits.mark_exhausted();
            }
            let mut spectrum = [0.0; 8];
            let mut classes = [0; 1];
            assert_eq!(
                decode(
                    &config,
                    std::slice::from_ref(&book),
                    &mut bits,
                    &mut Output {
                        behavior,
                        layout: Residue2Layout::Libvorbis,
                        spectrum: &mut spectrum,
                        stride: 8,
                        bins: 8,
                        bundle: &[0],
                        nonzero: &[true],
                        classes: &mut classes
                    }
                ),
                Err(Error::EndOfPacket)
            );
            assert_eq!(bits.position(), 0);
            assert_eq!(
                bits.is_exhausted(),
                exhausted || behavior == ResidueBehavior::Vorbis
            );
            assert_eq!(spectrum, [0.0; 8]);
        }
    }
}
