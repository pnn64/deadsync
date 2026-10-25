use deadsync_chart::notes::parse_chart_notes;
use std::hint::black_box;

#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod alloc;
#[allow(dead_code)]
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "support/notes_original.rs"]
mod original;

fn fixture(rows: usize, lanes: usize, kind: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for row in 0..rows {
        if row % 64 == 0 {
            bytes.extend_from_slice(b",\n");
        }
        for col in 0..lanes {
            bytes.push(if row % 4 != 0 || kind == 0 {
                b'0'
            } else {
                match (row / 4 + col) % 8 {
                    0 => b'1',
                    1 => b'2',
                    2 => b'3',
                    3 => b'4',
                    4 => b'3',
                    5 => b'M',
                    6 => b'F',
                    _ => b'L',
                }
            });
        }
        bytes.push(b'\n');
    }
    bytes
}

#[test]
fn parser_matches_original_for_arbitrary_bytes_and_lane_counts() {
    let mut state = 0x12345678u32;
    let alphabet = b"0011223344FfMmLl\r\n,;/ \t\xff";
    for lanes in [0, 1, 4, 5, 8, 10, 12] {
        for len in 0..512 {
            let bytes: Vec<_> = (0..len)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    alphabet[state as usize % alphabet.len()]
                })
                .collect();
            let old = original::parse_chart_notes(&bytes, lanes);
            let new = parse_chart_notes(&bytes, lanes);
            assert_eq!(old.capacity(), new.capacity());
            assert_eq!(old.len(), new.len());
            for (old, new) in old.iter().zip(&new) {
                assert_eq!(
                    (old.row_index, old.column, old.note_type, old.tail_row_index),
                    (new.row_index, new.column, new.note_type, new.tail_row_index)
                );
            }
        }
    }
}

#[test]
#[ignore = "CPU-cycle benchmark; run with --release --ignored --nocapture --test-threads=1"]
fn benchmark_note_parser() {
    for rows in [64, 8192] {
        for lanes in [4, 5, 8, 10] {
            for kind in 0..2 {
                let bytes = fixture(rows, lanes, kind);
                let (_, churn) = alloc::measure(|| parse_chart_notes(&bytes, lanes));
                println!("note-parser/{rows}/{lanes}/kind={kind}: allocation {churn:?}");
                bench::measure(
                    &format!("note-parser/{rows}/{lanes}/kind={kind}"),
                    128,
                    || {
                        black_box(parse_chart_notes(black_box(&bytes), black_box(lanes)));
                    },
                );
            }
        }
    }
}
