//! Background parser behavior and benchmarks against 0.5.1214.
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;
use std::hint::black_box;

#[allow(dead_code)]
#[path = "loading_diagnostics/bgchanges_baseline.rs"]
mod baseline;
#[allow(dead_code)]
#[path = "../src/bgchanges.rs"]
mod current;

fn rng(seed: &mut u64) -> usize {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*seed >> 32) as usize
}

#[test]
fn entry_matching_preserves_order_newlines_utf8_and_every_truncation() {
    let entries: Vec<String> = [
        "a",
        "a,b",
        "a=b",
        "movie.mp4",
        "movie,part.mp4",
        "layer=alt.png",
        "\u{65e5}\u{672c}.mp4",
        "x\ny",
        "x\ry",
        "x\r\ny",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let cases = [
        "0=a,b=1",
        "0=MOVIE,PART.MP4=1=0=0=0=0=layer=alt.png=CrossFade",
        "0=\r\nmo\nvi\r\ne.mp4\r\n=1",
        "0=\u{65e5}\u{672c}.mp4",
        "0=x\ry=1",
        "0=x\r\ny,",
        "0=movie.mp4\n",
        "0=movie.mp4\r\n=1",
        "0=a=b=1",
        ",=,=",
        "\n\r\n",
    ];
    for input in cases {
        for end in (0..=input.len()).filter(|&i| input.is_char_boundary(i)) {
            for entries in [
                entries.clone(),
                entries.iter().rev().cloned().collect(),
                vec![],
            ] {
                assert_eq!(
                    current::split_bgchange_sets_like_itg(&input[..end], &entries),
                    baseline::split_bgchange_sets_like_itg(&input[..end], &entries),
                    "input {:?}, entries {entries:?}",
                    &input[..end]
                );
            }
        }
    }
}

#[test]
fn randomized_background_records_match_frozen_parser() {
    let mut seed = 914;
    let alphabet = [
        'a', 'b', 'A', '=', ',', '\r', '\n', '\u{e9}', '\u{65e5}', '0',
    ];
    for _ in 0..1000 {
        let entries: Vec<String> = (0..12)
            .map(|_| {
                (0..1 + rng(&mut seed) % 20)
                    .map(|_| alphabet[rng(&mut seed) % alphabet.len()])
                    .collect()
            })
            .collect();
        let input: String = (0..96)
            .map(|_| alphabet[rng(&mut seed) % alphabet.len()])
            .collect();
        assert_eq!(
            current::split_bgchange_sets_like_itg(&input, &entries),
            baseline::split_bgchange_sets_like_itg(&input, &entries),
            "input {input:?}"
        );
        for entry in &entries {
            let input = format!("0={entry}=1=0=0=0=0={entry},");
            assert_eq!(
                current::split_bgchange_sets_like_itg(&input, &entries),
                baseline::split_bgchange_sets_like_itg(&input, &entries)
            );
        }
    }
}

#[test]
fn long_filenames_and_newlines_at_chunk_boundaries_match() {
    for len in [1, 60, 63, 64, 65, 128, 1024] {
        let name = format!("{},part.mp4", "a".repeat(len));
        let entries = vec![format!("{}wrong.mp4", "a".repeat(len)), name.clone()];
        for split in [0, 1, len, name.len()] {
            for newline in ["", "\n", "\r\n", "\r"] {
                for ending in ["", "=1", ",", "\r\n=1"] {
                    let input =
                        format!("0={}{}{}{ending}", &name[..split], newline, &name[split..]);
                    assert_eq!(
                        current::split_bgchange_sets_like_itg(&input, &entries),
                        baseline::split_bgchange_sets_like_itg(&input, &entries),
                        "len {len}, split {split}, input {input:?}"
                    );
                }
            }
        }
    }
}

fn fixture(count: usize, multiline: bool, diverse: bool) -> (String, Vec<String>) {
    let entries: Vec<String> = (0..count)
        .map(|i| {
            format!(
                "{}background_movie_{i:04},part.mp4",
                if diverse {
                    char::from(b'A' + (i % 26) as u8)
                } else {
                    'A'
                }
            )
        })
        .collect();
    let name = entries.last().map(String::as_str).unwrap_or("movie.mp4");
    let name = if multiline {
        name.replace('_', "_\r\n")
    } else {
        name.to_owned()
    };
    let input = (0..128)
        .map(|i| {
            format!(
                "{}={name}=1=0=0=0=StretchNormal={name}=CrossFade=1^1^1^1=0^0^0^1",
                i * 4
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    (input, entries)
}

fn checksum<'a>(
    sets: impl Iterator<Item = impl IntoIterator<Item = impl AsRef<str> + 'a>>,
) -> usize {
    sets.map(|fields| {
        fields
            .into_iter()
            .map(|f| black_box(f.as_ref()).len())
            .sum::<usize>()
    })
    .sum()
}

#[test]
fn ordinary_records_and_early_exit_still_allocate_nothing() {
    let (input, entries) = fixture(128, false, false);
    perf::assert_no_churn(|| {
        black_box(checksum(current::bgchange_sets(&input, &entries)));
    });
    perf::assert_no_churn(|| {
        black_box(current::bgchange_sets(&input, &entries).next());
    });
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_loading_diagnostics() {
    for (name, count, multiline, diverse, first) in [
        ("empty-directory", 0, false, false, false),
        ("two-entries", 2, false, false, false),
        ("128-entries", 128, false, false, false),
        ("512-entries", 512, false, false, false),
        ("diverse-entries", 128, false, true, false),
        ("multiline", 128, true, false, false),
        ("early-exit", 128, false, false, true),
    ] {
        let (input, entries) = fixture(count, multiline, diverse);
        assert_eq!(
            checksum(current::bgchange_sets(&input, &entries)),
            checksum(baseline::bgchange_sets(&input, &entries))
        );
        let versions = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in versions {
            perf::measure_sampled(
                &format!("background/{name}/{}", if old { "old" } else { "new" }),
                if first { 20000 } else { 150 },
                if first { 1 } else { 128 },
                || {
                    let input = black_box(input.as_str());
                    let entries = black_box(entries.as_slice());
                    if old {
                        checksum(baseline::bgchange_sets(input, entries).take(if first {
                            1
                        } else {
                            usize::MAX
                        }))
                    } else {
                        checksum(current::bgchange_sets(input, entries).take(if first {
                            1
                        } else {
                            usize::MAX
                        }))
                    }
                },
            );
        }
    }
}
