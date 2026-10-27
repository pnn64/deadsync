//! Table 7 ordering, multiplicity and cross-chunk constraints across all formats.
use deadlib_image_png::{
    DecodeEvent, DecodeOptions, DecodeStatus, StreamDecoder, decode, decode_reader,
};
use std::io::{self, Cursor, Read};
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[path = "support/ancillary_order.rs"]
mod order;
use order::{
    Context, Entry, NAMES, PAIR_SLOTS, companion, contexts, entry, expected, fixture, valid,
};

struct Fragments<'a>(&'a [u8], usize);
impl Read for Fragments<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let n = output.len().min(self.0.len()).min(self.1);
        output[..n].copy_from_slice(&self.0[..n]);
        self.0 = &self.0[n..];
        Ok(n)
    }
}

fn reference(bytes: &[u8], c: Context) {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut output = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut output).unwrap();
    assert_eq!(
        (
            info.width,
            info.height,
            info.color_type as u8,
            info.bit_depth as u8
        ),
        (1, 1, c.color, c.depth)
    );
    assert_eq!(&output[..info.buffer_size()], c.packed());
    reader.finish().unwrap();
}

fn check(c: Context, entries: &[Entry], split: usize) -> bool {
    let accepted = valid(c, entries);
    let bytes = fixture(c, entries, split);
    let results = [
        decode(&bytes),
        decode_reader(Fragments(&bytes, 1), &Default::default()),
        decode_reader(Fragments(&bytes, 7), &Default::default()),
    ];
    for result in results {
        assert_eq!(
            result.is_ok(),
            accepted,
            "{c:?} entries={:?} split={split}: {result:?}",
            entries.iter().map(|e| (e.kind, e.slot)).collect::<Vec<_>>()
        );
        if let Ok(png) = result {
            assert_eq!(png, expected(c, entries));
        }
    }
    if accepted {
        reference(&bytes, c);
    }
    accepted
}

#[test]
fn every_standard_ancillary_slot_and_all_ordered_pairs_match_table_seven() {
    let (mut singles, mut pairs, mut positives) = (0, 0, 0);
    for c in contexts() {
        for kind in NAMES {
            for slot in 0..6 {
                let mut entries = vec![entry(c, kind, slot)];
                companion(c, &mut entries);
                positives += usize::from(check(c, &entries, 3));
                singles += 1;
            }
        }
        for first in NAMES {
            for second in NAMES {
                for a in PAIR_SLOTS {
                    for b in PAIR_SLOTS {
                        let mut entries = vec![entry(c, first, a), entry(c, second, b)];
                        companion(c, &mut entries);
                        positives += usize::from(check(c, &entries, 3));
                        pairs += 1;
                    }
                }
            }
        }
    }
    assert_eq!((singles, pairs), (2052, 55404));
    assert_eq!(positives, 21637);
    println!(
        "Table 7: {singles} single placements and {pairs} ordered pairs, {positives} valid files, each checked by three native parsers; valid samples also checked by Rust png"
    );
}

#[test]
fn every_idat_split_keeps_known_ancillary_chunks_outside_the_consecutive_run() {
    let mut cases = 0;
    for c in contexts() {
        let len = chunks::stored(&[vec![0], c.packed()].concat()).len();
        for split in 0..=len {
            for kind in NAMES {
                for slot in [2, 3, 4] {
                    let mut entries = vec![entry(c, kind, slot)];
                    companion(c, &mut entries);
                    check(c, &entries, split);
                    cases += 1;
                }
            }
        }
    }
    println!("Known ancillary placements across every IDAT split: {cases}");
    assert_eq!(cases, 16470);
}

#[test]
fn distinct_suggested_palette_names_and_all_ancillary_permutations_remain_legal() {
    let mut cases = 0;
    for c in contexts() {
        for a in PAIR_SLOTS {
            for b in PAIR_SLOTS {
                for different in [false, true] {
                    let first = entry(c, *b"sPLT", a);
                    let mut second = entry(c, *b"sPLT", b);
                    if different {
                        second.data[0] = b'p';
                    }
                    check(c, &[first, second], 3);
                    cases += 1;
                }
            }
        }
        // No ordering relationships exist between these six ancillary types.
        // In particular mDCV may precede the required cICP companion.
        let entries = [*b"mDCV", *b"cICP", *b"sRGB", *b"gAMA", *b"cHRM", *b"tEXt"]
            .map(|kind| entry(c, kind, 1));
        chunks::permutations(&mut [0, 1, 2, 3, 4, 5], 0, &mut |indices| {
            let entries: Vec<_> = indices.iter().map(|&n| entries[n].clone()).collect();
            assert!(check(c, &entries, 3));
            cases += 1;
        });
    }
    assert_eq!(cases, 14022);
}

#[test]
fn direct_events_retain_order_and_make_ordering_errors_terminal() {
    let mut cases = 0;
    for c in contexts() {
        for kind in NAMES {
            for slot in [1, 2, 3, 4] {
                let mut entries = vec![entry(c, kind, slot)];
                companion(c, &mut entries);
                let accepted = valid(c, &entries);
                let bytes = fixture(c, &entries, 3);
                for fragment in [1, 7, bytes.len()] {
                    let mut decoder = StreamDecoder::new(DecodeOptions::default());
                    let mut offset = 0;
                    let mut observed = Vec::new();
                    loop {
                        let end = bytes.len().min(offset + fragment);
                        let progress = match decoder.process(&bytes[offset..end]) {
                            Ok(p) => p,
                            Err(error) => {
                                assert!(!accepted, "{c:?} {kind:?} {slot}: {error}");
                                assert_eq!(decoder.finish(), Err(error.clone()));
                                assert_eq!(decoder.process(&bytes), Err(error));
                                assert!(decoder.event().is_none());
                                break;
                            }
                        };
                        offset += progress.consumed;
                        match progress.status {
                            DecodeStatus::Event => {
                                if let DecodeEvent::Metadata { chunk, .. } =
                                    decoder.event().unwrap()
                                {
                                    observed.push(chunk.clone());
                                }
                            }
                            DecodeStatus::Done => {
                                // Push decoding is a prefix API; it cannot reject
                                // bytes that the caller retains after IEND.
                                assert!(accepted);
                                assert_eq!(offset, bytes.len());
                                assert_eq!(observed, expected(c, &entries).ancillary);
                                decoder.finish().unwrap();
                                break;
                            }
                            DecodeStatus::NeedInput => assert!(offset < bytes.len()),
                        }
                    }
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 4104);
}

#[test]
#[ignore = "requires DEADLIB_LIBPNG_ORACLE"]
fn c_libpng_checks_all_valid_single_placements_and_both_hdr_orders() {
    let oracle = std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE");
    let path = std::env::temp_dir().join(format!(
        "deadlib-png-ancillary-order-{}.png",
        std::process::id()
    ));
    let mut count = 0;
    let mut compare = |bytes: &[u8], c: Context| {
        std::fs::write(&path, bytes).unwrap();
        let result = std::process::Command::new(&oracle)
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{c:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let mut expected = [1u32, 1, u32::from(c.depth), u32::from(c.color)]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect::<Vec<_>>();
        expected.extend(c.packed());
        assert_eq!(result.stdout, expected);
        count += 1;
    };
    for c in contexts() {
        for kind in NAMES {
            for slot in PAIR_SLOTS {
                let mut entries = vec![entry(c, kind, slot)];
                companion(c, &mut entries);
                if valid(c, &entries) {
                    let bytes = fixture(c, &entries, 3);
                    compare(&bytes, c);
                }
            }
        }
        // Check both ancillary orders, independently of the factory companion.
        for kinds in [[*b"cICP", *b"mDCV"], [*b"mDCV", *b"cICP"]] {
            let entries = kinds.map(|kind| entry(c, kind, 1));
            compare(&fixture(c, &entries, 3), c);
        }
    }
    std::fs::remove_file(path).unwrap();
    assert_eq!(count, 678);
    println!("C libpng ancillary order: {count} exact native-sample files");
}
