use super::*;
use crate::metadata_perf::measure;
use std::hint::black_box;

#[path = "../../../../tests/support/kernel_bench.rs"]
mod bench;

#[test]
fn delimiter_matches_scalar_at_every_offset() {
    for len in 0..256 {
        for offset in 0..=len {
            for delimiter in *b"=,x" {
                let mut input = "é".repeat(len);
                input.insert(offset * 2, char::from(delimiter));
                for start in [0, offset * 2, input.len()] {
                    let expected = input.as_bytes()[start..]
                        .iter()
                        .position(|byte| matches!(byte, b'=' | b','))
                        .map(|index| (start + index, input.as_bytes()[start + index]));
                    assert_eq!(next_bgchange_delimiter(&input, start), expected);
                }
            }
        }
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_direct_parsing_delimiters() {
    for len in [8, 32, 128, 4096] {
        let input = format!("{}=next,", "x".repeat(len));
        let label = format!("delimiter/len={len}");
        let work = || {
            black_box(next_bgchange_delimiter(black_box(&input), black_box(0)));
        };
        let (_, churn) = measure(work);
        println!("{label}: allocation {churn:?}");
        bench::measure(&label, 100_000, work);
    }
    for len in [16, 128, 4096] {
        let input = format!("0={}movie.mp4=1,8=other.mp4=2", "x".repeat(len));
        let label = format!("bgchanges/len={len}");
        let work = || {
            black_box(split_bgchange_sets_like_itg(
                black_box(&input),
                black_box(&[]),
            ));
        };
        let (_, churn) = measure(work);
        println!("{label}: allocation {churn:?}");
        bench::measure(&label, 2000, work);
    }
}
