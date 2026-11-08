use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
mod original {
    include!("song_projection_original.rs");
}

fn source(extension: &str, kind: &str, rows: &str, charts: usize) -> Vec<u8> {
    let mut input = "#VERSION:0.83;\n#TITLE:[12] Source;\n#BPMS:0=120,16=180;\n#STOPS:8=0.25;\n#DELAYS:12=0.125;\n#WARPS:24=1;\n".to_string();
    for _ in 0..charts {
        if extension == "sm" {
            input.push_str(&format!(
                "#NOTES2:{kind}:[24] Author:Challenge:1:0,0,0,0,0:\n{rows};\n"
            ));
        } else {
            input.push_str(&format!("#NOTEDATA:;\n#STEPSTYPE:{kind};\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES2:\n{rows};\n"));
        }
    }
    input.into_bytes()
}

#[test]
fn projection_preserves_annotations_comments_and_source_order() {
    for extension in ["sm", "ssc"] {
        for rows in [
            "0000\n1000\n",
            "1[24]02[13]0\n003[24]0\n,\n4[]000\n3[unterminated\n",
            " // [12] comment\n 1[0]0[1]0[2]0[3]\r\n,\r\n\t0001[24] // tail [24]\n",
            "0000[123]\n,\n1000 trailing[24]\n",
        ] {
            let data = source(extension, "dance-single", rows, 3);
            let parsed = rssp::parse::extract_sections(&data, extension).unwrap();
            let expected = original::note_analysis_bytes(&data, &parsed.notes_list);
            let actual = note_analysis_bytes(&data, &parsed.notes_list);
            assert_eq!(actual, expected);
            assert_eq!(
                matches!(actual, Cow::Borrowed(_)),
                matches!(expected, Cow::Borrowed(_))
            );
        }
    }
    // SSC permits a notedata block with no NOTES/NOTES2 tag. Its empty note
    // slice need not point into the source allocation.
    let data = b"#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Challenge;";
    let parsed = rssp::parse::extract_sections(data, "ssc").unwrap();
    assert!(matches!(
        note_analysis_bytes(data, &parsed.notes_list),
        Cow::Borrowed(_)
    ));
}

#[test]
fn projection_matches_generated_note_syntax() {
    let alphabet = b"000011234MFLK[]abc/ \t\r\n,";
    let mut seed = 0x8513_a2f7u32;
    for extension in ["sm", "ssc"] {
        for len in [0, 1, 4, 16, 128, 1024] {
            for _ in 0..128 {
                let rows: String = (0..len)
                    .map(|_| {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        char::from(alphabet[seed as usize % alphabet.len()])
                    })
                    .collect();
                let data = source(extension, "dance-single", &rows, 3);
                let parsed = rssp::parse::extract_sections(&data, extension).unwrap();
                assert_eq!(
                    note_analysis_bytes(&data, &parsed.notes_list),
                    original::note_analysis_bytes(&data, &parsed.notes_list),
                    "{rows:?}"
                );
            }
        }
    }
}

#[test]
fn unplayed_bounds_preserve_global_own_and_lossy_timing() {
    for extension in ["sm", "ssc"] {
        let mut data = source(extension, "techno-single4", "1000\n0200\n0300\n0001\n", 2);
        for own in [false, true] {
            if own {
                let point = data
                    .windows(8)
                    .position(|bytes| bytes == b"#NOTES2:")
                    .unwrap();
                data.splice(
                    point..point,
                    b"#BPMS:0=150;\n#OFFSET:0.25;\n".iter().copied(),
                );
            }
            for lossy in [false, true] {
                let mut input = data.clone();
                if lossy {
                    let point = input.windows(4).position(|bytes| bytes == b"=120").unwrap();
                    input[point + 1] = 0xff;
                }
                let parsed = rssp::parse::extract_sections(&input, extension).unwrap();
                let expected = original::unplayed_chart_note_bounds(&parsed, extension);
                let actual = unplayed_chart_note_bounds(&parsed, extension);
                assert!(actual.is_some());
                assert_eq!(
                    actual.map(|times| times.map(f32::to_bits)),
                    expected.map(|times| times.map(f32::to_bits))
                );
            }
        }
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_direct_parsing_projection() {
    for (name, rows) in [
        ("plain", "1000\n0100\n0010\n0001\n".repeat(256)),
        (
            "sparse",
            format!("{}1[24]000\n", "1000\n0100\n0010\n0001\n".repeat(256)),
        ),
        (
            "dense",
            "1[24]0[0]0[1]0[4]\n010[22]0\n0010\n0001\n".repeat(256),
        ),
    ] {
        let data = source("sm", "dance-single", &rows, 4);
        let parsed = rssp::parse::extract_sections(&data, "sm").unwrap();
        let label = format!("projection/{name}");
        let work = || {
            black_box(note_analysis_bytes(
                black_box(&data),
                black_box(&parsed.notes_list),
            ));
        };
        let (_, churn) = crate::metadata_perf::measure(work);
        println!("{label}: allocation {churn:?}");
        bench::measure(&label, 200, work);
    }
    for (kind, charts) in [
        ("dance-single", 8),
        ("techno-single4", 1),
        ("techno-single4", 8),
    ] {
        let data = source("ssc", kind, "1000\n0200\n0300\n0001\n", charts);
        let parsed = rssp::parse::extract_sections(&data, "ssc").unwrap();
        let label = format!("unplayed/{kind}/charts={charts}");
        let work = || {
            black_box(unplayed_chart_note_bounds(
                black_box(&parsed),
                black_box("ssc"),
            ));
        };
        let (_, churn) = crate::metadata_perf::measure(work);
        println!("{label}: allocation {churn:?}");
        bench::measure(&label, 1000, work);
    }
}
