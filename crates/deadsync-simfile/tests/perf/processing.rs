use crate::{bgchanges, sync_offset, tags};
use std::hint::black_box;

mod legacy_bgchanges;
mod legacy_offset;
mod legacy_tags;

fn next_random(seed: &mut u64) -> usize {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*seed >> 32) as usize
}

#[test]
fn tag_scanning_matches_legacy_at_every_truncation() {
    let requested = [b"#TITLE:".as_slice(), b"#FGCHANGES:", b"#OFFSET:"];
    let mut cases = vec![
        b"#TITLE:one;#title:two;#OFFSET:0.25;".to_vec(),
        b"#TITLE:a\\;b\\\\;#TITLE:c\\\\\\;d;".to_vec(),
        b"#TITLE:missing terminator\r\n \t#OFFSET:1;#FGCHANGES:0=x;".to_vec(),
        b"#TITLE:inline#hash;#FGCHANGES:\n0=a,\r\n4=b;".to_vec(),
        b"#TITLE:bad\xffutf8;\n#TITLE:trailing\\".to_vec(),
    ];
    let mut seed = 17;
    let alphabet = b"#TITLE:;\\\r\n\t ab09\xff";
    for _ in 0..80 {
        let mut input = b"#TITLE:".to_vec();
        input.extend((0..64).map(|_| alphabet[next_random(&mut seed) % alphabet.len()]));
        input.extend_from_slice(b"\n#OFFSET:0.125;");
        cases.push(input);
    }
    for input in cases {
        for end in 0..=input.len() {
            let input = &input[..end];
            assert_eq!(
                tags::named_tag_values(input, &requested).collect::<Vec<_>>(),
                legacy_tags::extract_named_tag_values(input, &requested),
                "input: {input:?}"
            );
            assert_eq!(
                tags::latest_simfile_tag_values(input, requested),
                legacy_tags::latest_simfile_tag_values(input, requested)
            );
        }
    }
}

fn borrowed_sets(input: &str, entries: &[String]) -> Vec<Vec<String>> {
    bgchanges::bgchange_sets(input, entries)
        .map(|fields| fields.into_iter().map(|field| field.into_owned()).collect())
        .collect()
}

#[test]
fn borrowed_background_fields_match_legacy_delimiters_and_newlines() {
    let entries: Vec<String> = ["movie,part.mp4", "layer=alt.png", "movie.mp4", "日本.mp4"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let mut cases: Vec<String> = [
        "",
        "\n",
        "\r\n",
        "\r",
        ",",
        "=",
        "0=movie.mp4",
        "0=movie.mp4,",
        "0=MOVIE,PART.MP4=1=0=0=0=0=layer=alt.png=CrossFade=1^0^0=1^1^1^1",
        "0=\r\nmo\nvi\r\ne.mp4=1,8=日本.mp4=0.5",
        "0=movie.mp4=1\r,4=layer=alt.png=2\n\r",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    cases.push(format!("0={}", "x=".repeat(48))); // Inline capacity spill.
    if !cfg!(debug_assertions) {
        cases.push(format!("0={}", "x=".repeat(300))); // Legacy u8 field-counter wrap.
    }
    let mut seed = 91;
    let alphabet = ['a', '=', ',', '\r', '\n', '日', 'é', '0'];
    for _ in 0..200 {
        cases.push(
            (0..64)
                .map(|_| alphabet[next_random(&mut seed) % alphabet.len()])
                .collect(),
        );
    }
    for input in cases {
        for entries in [&entries[..], &[]] {
            let expected = legacy_bgchanges::split_bgchange_sets_like_itg(&input, entries);
            assert_eq!(borrowed_sets(&input, entries), expected, "input: {input:?}");
            assert_eq!(
                bgchanges::split_bgchange_sets_like_itg(&input, entries),
                expected
            );
        }
    }
}

#[test]
fn background_entry_order_and_terminal_fields_stay_unchanged() {
    for entries in [
        vec!["a".into(), "a,b".into()],
        vec!["a,b".into(), "a".into()],
    ] {
        for input in ["0=a,b=1", "0=a,b", "0=a", "0=a=1=0=0=0=0=a,b"] {
            assert_eq!(
                borrowed_sets(input, &entries),
                legacy_bgchanges::split_bgchange_sets_like_itg(input, &entries)
            );
        }
    }
}

#[test]
fn offset_rewrite_matches_legacy_bytes_errors_and_float_rounding() {
    let cases: &[&[u8]] = &[
        b"",
        b"#OFFSET",
        b"#OFFSET:",
        b"#OFFSET:;",
        b"#OFFSET: \t ;",
        b"#OFFSET:bad;",
        b"#OFFSET:\xff;",
        b"#OFFSET:0.1\n#TITLE:x;",
        b"#offset: \t-0.0004 \r\n;#OFFSET:+0.1235;#OFFSET:1e-3;",
        b"#OFFSET:NaN;#OFFSET:inf;#OFFSET:-inf;#OFFSET:3.4028235e38;",
        b"#TITLE:#OFFSET:0.5;\r\n#NOTES:0000,1000;#OFFSET:1;tail\xff",
    ];
    for &input in cases {
        for end in 0..=input.len() {
            for delta in [0.0, -0.0, 0.001, -0.0035, f32::NAN, f32::INFINITY] {
                assert_eq!(
                    sync_offset::rewrite_simfile_offset_tags(&input[..end], delta),
                    legacy_offset::rewrite_simfile_offset_tags(&input[..end], delta),
                    "input: {:?}, delta: {delta}",
                    &input[..end]
                );
            }
        }
    }
    let mut seed = 55;
    for _ in 0..500 {
        let value = f32::from_bits(next_random(&mut seed) as u32);
        let input = format!("#OFFSET:{value};");
        assert_eq!(
            sync_offset::rewrite_simfile_offset_tags(input.as_bytes(), -0.017),
            legacy_offset::rewrite_simfile_offset_tags(input.as_bytes(), -0.017)
        );
    }
}

fn simfile_bytes(size: usize, charts: usize) -> Vec<u8> {
    let mut input = b"#TITLE:Benchmark;#OFFSET:0.125;\n".to_vec();
    for _ in 0..charts {
        input.extend_from_slice(b"#NOTEDATA:;#OFFSET:0.125;#NOTES:\n");
        input.extend(std::iter::repeat_n(b'0', size / charts));
        input.extend_from_slice(b";\n#FGCHANGES:0=movie.mp4=1;\n");
    }
    input
}

fn background_text(sets: usize, multiline: bool) -> String {
    (0..sets)
        .map(|i| {
            format!(
                "{}=movie{}part.mp4=1=0=0=0=StretchNormal=layer=alt.png=CrossFade=1^1^1^1=0^0^0^1",
                i * 4,
                if multiline { ",\n" } else { "," }
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn tag_iteration_and_plain_background_fields_have_no_heap_churn() {
    let input = simfile_bytes(128 * 1024, 8);
    let requested = [b"#FGCHANGES:".as_slice()];
    let changes = background_text(128, false);
    let entries = vec!["movie,part.mp4".into(), "layer=alt.png".into()];
    crate::perf::assert_no_churn(|| {
        assert_eq!(tags::named_tag_values(&input, &requested).count(), 8);
        assert_eq!(bgchanges::bgchange_sets(&changes, &entries).count(), 128);
    });
}

#[test]
fn background_iteration_can_stop_before_allocating_later_fields() {
    let input = format!("0=movie.mp4=1,{}", "4=mo\nvie.mp4=1,".repeat(128));
    crate::perf::assert_no_churn(|| {
        let first = bgchanges::bgchange_sets(&input, &[]).next().unwrap();
        assert_eq!(first[1], "movie.mp4");
    });
}

#[test]
fn offset_rewrite_preserves_growth_and_repeated_chart_tags() {
    let input = b"#OFFSET:0;".repeat(256);
    for delta in [0.0, 0.001, 1000.0, -0.001] {
        assert_eq!(
            sync_offset::rewrite_simfile_offset_tags(&input, delta),
            legacy_offset::rewrite_simfile_offset_tags(&input, delta)
        );
    }
}

#[test]
fn offset_rewrite_allocates_only_the_output_buffer() {
    let input = simfile_bytes(128 * 1024, 32);
    crate::perf::assert_churn_budget(1, input.len() + 64, || {
        let (output, changed) = sync_offset::rewrite_simfile_offset_tags(&input, 0.001).unwrap();
        assert_eq!(changed, 33);
        assert_eq!(output.len(), input.len());
        black_box(output);
    });
}

fn checksum<'a>(
    sets: impl Iterator<Item = impl IntoIterator<Item = impl AsRef<str> + 'a>>,
) -> usize {
    sets.map(|fields| {
        fields
            .into_iter()
            .map(|field| black_box(field.as_ref()).len())
            .sum::<usize>()
    })
    .sum()
}

#[test]
#[ignore = "manual old/new simfile processing benchmark; run in release mode"]
fn simfile_processing_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for (label, size, charts, iterations) in [
        ("small", 4096, 1, 4000),
        ("metadata", 0, 128, 1000),
        ("large", 1024 * 1024, 32, 100),
    ] {
        let input = simfile_bytes(size, charts);
        let requested = [b"#FGCHANGES:".as_slice(), b"#TITLE:".as_slice()];
        for old in order {
            let suffix = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("tags_{label}_{suffix}"),
                iterations,
                input.len(),
                || {
                    if old {
                        legacy_tags::named_tag_values(black_box(&input), black_box(&requested))
                            .map(|value| black_box(value).len())
                            .sum::<usize>()
                    } else {
                        tags::named_tag_values(black_box(&input), black_box(&requested))
                            .map(|value| black_box(value).len())
                            .sum::<usize>()
                    }
                },
            );
            crate::perf::measure_sampled(
                &format!("offset_{label}_{suffix}"),
                iterations,
                input.len(),
                || {
                    if old {
                        legacy_offset::rewrite_simfile_offset_tags(
                            black_box(&input),
                            black_box(0.001),
                        )
                        .unwrap()
                    } else {
                        sync_offset::rewrite_simfile_offset_tags(
                            black_box(&input),
                            black_box(0.001),
                        )
                        .unwrap()
                    }
                },
            );
        }
    }
    let entries = vec!["movie,part.mp4".into(), "layer=alt.png".into()];
    for (label, count, multiline, iterations) in [
        ("single", 1, false, 10000),
        ("dense", 512, false, 100),
        ("multiline", 512, true, 100),
    ] {
        let input = background_text(count, multiline);
        for old in order {
            let suffix = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("background_{label}_{suffix}"),
                iterations,
                count,
                || {
                    if old {
                        checksum(
                            legacy_bgchanges::split_bgchange_sets_like_itg(
                                black_box(&input),
                                black_box(&entries),
                            )
                            .into_iter(),
                        )
                    } else {
                        checksum(bgchanges::bgchange_sets(
                            black_box(&input),
                            black_box(&entries),
                        ))
                    }
                },
            );
            crate::perf::measure_sampled(
                &format!("background_owned_{label}_{suffix}"),
                iterations,
                count,
                || {
                    if old {
                        legacy_bgchanges::split_bgchange_sets_like_itg(
                            black_box(&input),
                            black_box(&entries),
                        )
                    } else {
                        bgchanges::split_bgchange_sets_like_itg(
                            black_box(&input),
                            black_box(&entries),
                        )
                    }
                },
            );
        }
    }
}
