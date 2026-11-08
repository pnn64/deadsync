use super::*;
use std::hint::black_box;

#[path = "../../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "original.rs"]
mod original;

#[path = "projection.rs"]
mod projection;

fn random(state: &mut u64) -> usize {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as usize
}

fn chart(width: usize, measures: usize, rows: usize, pattern: usize) -> Vec<u8> {
    let mut data = Vec::new();
    for owner in 0..2 {
        if owner != 0 {
            data.extend_from_slice(b"\n&\n");
        }
        for measure in 0..measures {
            if measure != 0 {
                data.extend_from_slice(b",\n");
            }
            for row in 0..rows {
                for col in 0..width {
                    let at = row + measure * rows + col * 7 + owner * 11;
                    data.push(match pattern {
                        0 if at.is_multiple_of(31) => b'1',
                        1 if at.is_multiple_of(3) => b'1',
                        2 if at.is_multiple_of(16) => b'2',
                        2 if at % 16 == 7 => b'3',
                        2 if at % 16 == 10 => b'M',
                        _ => b'0',
                    });
                }
                data.push(b'\n');
            }
        }
    }
    data
}

fn cases() -> Vec<(Vec<u8>, usize)> {
    let mut cases = Vec::new();
    for width in [8, 10] {
        for data in [
            b"".as_slice(),
            b",,,",
            b" \r\n,\n\t,",
            b"1000000000 // &\n&\n0000000001",
            b"2[33]000000000\nK000000000\n3000000000\n&\n1000000000",
            b"2000000000\n1000000000\n3000000000\n&\n2000000000\n3000000000",
            b"2000000000\n3000000000\n&\n2000000000\n0000000000,3000000000",
            b"1000000000\n&\n0100000000\n&\n0010000000",
            b"1[unclosed\n2[4]300000000\n// ignored\nFMLK000000",
        ] {
            cases.push((data.to_vec(), width));
        }
        for rows in [1, 4, 7, 48, 128, 257] {
            for pattern in 0..3 {
                cases.push((chart(width, 2, rows, pattern), width));
            }
        }
    }
    let mut state = 0x9799_ba23_8345_1234;
    for _ in 0..240 {
        let width = [8, 10][random(&mut state) % 2];
        let mut data = Vec::new();
        for _ in 0..(random(&mut state) % 90) {
            for _ in 0..(random(&mut state) % 3) {
                data.extend_from_slice(
                    [b" \r\t".as_slice(), b",", b"\n", b"// & comment\n", b"&"]
                        [random(&mut state) % 5],
                );
            }
            for _ in 0..(random(&mut state) % (width + 3)) {
                data.push(b"00001234MKLF?"[random(&mut state) % 13]);
                if random(&mut state).is_multiple_of(17) {
                    data.extend_from_slice(b"[23]");
                }
            }
            data.push(b'\n');
        }
        cases.push((data, width));
    }
    cases
}

fn simfile(kind: &str, notes: &[u8], charts: usize, extension: &str) -> Vec<u8> {
    let mut data = b"#VERSION:0.83;#TITLE:Partners;#BPMS:0=120;".to_vec();
    for _ in 0..charts {
        if extension == "sm" {
            data.extend_from_slice(
                format!("#NOTES:{kind}:Partners:Hard:7:0,0,0,0,0:\n").as_bytes(),
            );
        } else {
            data.extend_from_slice(format!("#NOTEDATA:;#STEPSTYPE:{kind};#DESCRIPTION:Partners;#DIFFICULTY:Hard;#METER:7;#NOTES:\n").as_bytes());
        }
        data.extend_from_slice(notes);
        data.extend_from_slice(b";\n");
    }
    data
}

#[test]
fn partner_kinds_preserve_utf8_trimming_and_ascii_case() {
    for word in [
        "dance-couple",
        "dance-routine",
        "pump-couple",
        "pump-routine",
        "dance-single",
        "pump-halfdouble",
        "",
        "dance-routine-extra",
    ] {
        for (left, right) in [
            ("", ""),
            (" \t", "\r\n"),
            ("\u{2003}", "\u{a0}"),
            ("\u{feff}", ""),
        ] {
            for word in [word.to_owned(), word.to_ascii_uppercase()] {
                let input = format!("{left}{word}{right}");
                assert_eq!(
                    lanes(input.as_bytes()),
                    original::lanes(input.as_bytes()),
                    "{input:?}"
                );
            }
        }
    }
    let mut state = 0xb3ac_09dd_f812_6678;
    for _ in 0..20_000 {
        let bytes: Vec<_> = (0..random(&mut state) % 24)
            .map(|_| random(&mut state) as u8)
            .collect();
        assert_eq!(lanes(&bytes), original::lanes(&bytes), "{bytes:?}");
    }
}

#[test]
fn participants_preserve_measure_counts_and_all_track_fields() {
    for (data, width) in cases() {
        for (owner, part) in data.split(|&b| b == b'&').take(2).enumerate() {
            let (tracks, count) = load_participant(part, width, owner as u8);
            let tracks: Vec<Vec<_>> = tracks
                .into_iter()
                .map(|track| {
                    track
                        .into_iter()
                        .map(|(row, note)| (row, note.symbol, note.duration, note.owner))
                        .collect()
                })
                .collect();
            assert_eq!(
                (tracks, count),
                original::participant_snapshot(part, width, owner as u8),
                "{data:?}"
            );
        }
    }
}

#[test]
fn routine_grids_and_ownership_match_the_original() {
    for (data, width) in cases() {
        assert_eq!(
            merge_routine(&data, width),
            original::merge_routine(&data, width),
            "width={width}, {data:?}"
        );
    }
}

#[test]
fn sm_and_ssc_projections_preserve_bytes_owners_hashes_and_borrowing() {
    for extension in ["sm", "ssc"] {
        for kind in [
            "dance-couple",
            "DANCE-ROUTINE",
            "pump-couple",
            " pump-routine ",
            "\u{2003}dance-routine\u{a0}",
            "dance-single",
            "unsupported",
        ] {
            let width = if kind.contains("pump") { 10 } else { 8 };
            let data = simfile(kind, &chart(width, 2, 7, 2), 3, extension);
            let entries = rssp::parse::extract_sections(&data, extension).unwrap();
            let old = original::project(&data, &entries.notes_list);
            let new = project(&data, &entries.notes_list);
            assert_eq!(new.bytes, old.bytes, "{extension} {kind}");
            assert_eq!(new.owners, old.owners);
            assert_eq!(new.hash_notes, old.hash_notes);
            assert_eq!(
                matches!(new.bytes, Cow::Borrowed(_)),
                matches!(old.bytes, Cow::Borrowed(_))
            );
            if matches!(new.bytes, Cow::Borrowed(_)) {
                assert_eq!(new.bytes.as_ptr(), data.as_ptr());
            }
            let projected = rssp::parse::extract_sections(&new.bytes, extension).unwrap();
            assert_eq!(projected.notes_list.len(), entries.notes_list.len());
        }
    }
}

#[test]
#[ignore = "archive deterministic grids, owners, projection bytes and hashes"]
fn dump_partner_trace() {
    fn number(trace: &mut Vec<u8>, value: usize) {
        trace.extend_from_slice(&(value as u64).to_le_bytes());
    }
    fn bytes(trace: &mut Vec<u8>, value: &[u8]) {
        number(trace, value.len());
        trace.extend_from_slice(value);
    }
    let mut trace = Vec::new();
    for (data, width) in cases() {
        let (grid, owners) = merge_routine(&data, width);
        bytes(&mut trace, &grid);
        number(&mut trace, owners.len());
        for ((row, col), owner) in owners {
            number(&mut trace, row);
            number(&mut trace, col);
            trace.push(owner);
        }
    }
    for extension in ["sm", "ssc"] {
        for kind in [
            "dance-couple",
            "dance-routine",
            "pump-couple",
            "pump-routine",
            "dance-single",
        ] {
            let data = simfile(
                kind,
                &chart(if kind.starts_with("pump") { 10 } else { 8 }, 3, 7, 2),
                3,
                extension,
            );
            let parsed = rssp::parse::extract_sections(&data, extension).unwrap();
            let projected = project(&data, &parsed.notes_list);
            bytes(&mut trace, &projected.bytes);
            number(&mut trace, projected.owners.len());
            for ((chart, row, col), owner) in projected.owners {
                number(&mut trace, chart);
                number(&mut trace, row);
                number(&mut trace, col);
                trace.push(owner);
            }
            number(&mut trace, projected.hash_notes.len());
            for (chart, notes) in projected.hash_notes {
                number(&mut trace, chart);
                bytes(&mut trace, &notes);
            }
        }
    }
    std::fs::write(
        std::env::var_os("PARTNER_TRACE_OUT").expect("trace path"),
        trace,
    )
    .unwrap();
}

fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    bench::compare(label, iterations, &mut work);
    for (variant, current) in [("original", false), ("current", true)] {
        let (_, churn) = crate::perf::measure(|| work(current));
        println!("{label}/{variant}: allocation {churn:?}");
    }
}

#[test]
#[ignore = "optimized paired CPU and allocation benchmark"]
fn benchmark_partner_charts() {
    let valid = [
        b"dance-couple".as_slice(),
        b"dance-routine",
        b"PUMP-COUPLE",
        b"pump-routine",
    ];
    let mixed = [
        b"dance-single".as_slice(),
        b"dance-double",
        b"pump-single",
        b"dance-routine",
        b"unsupported",
        b"\xff",
    ];
    for (label, values) in [
        ("kind/valid/256", valid.as_slice()),
        ("kind/mixed/256", mixed.as_slice()),
    ] {
        compare(label, 2_000, |current| {
            for index in 0..256 {
                let bytes = black_box(values[index % values.len()]);
                black_box(if current {
                    lanes(bytes)
                } else {
                    original::lanes(bytes)
                });
            }
        });
    }
    for (label, pattern) in [("participant/sparse/16", 0), ("participant/dense/16", 1)] {
        let data = chart(8, 16, 48, pattern);
        let part = data.split(|&b| b == b'&').next().unwrap();
        compare(label, 60, |current| {
            if current {
                black_box(load_participant(black_box(part), 8, 0));
            } else {
                original::consume_participant(black_box(part), 8, 0);
            }
        });
    }
    for (label, pattern) in [
        ("merge/sparse/16", 0),
        ("merge/dense/16", 1),
        ("merge/holds/16", 2),
    ] {
        let data = chart(8, 16, 48, pattern);
        compare(label, 15, |current| {
            black_box(if current {
                merge_routine(black_box(&data), 8)
            } else {
                original::merge_routine(black_box(&data), 8)
            });
        });
    }
    for (label, kind, count) in [
        ("project/couple/16", "dance-couple", 16),
        ("project/routine/4", "dance-routine", 4),
    ] {
        let data = simfile(kind, &chart(8, 2, 16, 2), count, "sm");
        let parsed = rssp::parse::extract_sections(&data, "sm").unwrap();
        compare(label, 15, |current| {
            if current {
                black_box(project(black_box(&data), black_box(&parsed.notes_list)));
            } else {
                black_box(original::project(
                    black_box(&data),
                    black_box(&parsed.notes_list),
                ));
            }
        });
    }
}
