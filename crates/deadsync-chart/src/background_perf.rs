use super::*;
use crate::perf_alloc as alloc;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

fn original_crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn movie_seeds_and_shuffles_preserve_itg_crc() {
    let bytes: Vec<_> = (0..4096)
        .map(|i| (i as u32).wrapping_mul(0x9e37_79b9).rotate_left(13) as u8)
        .collect();
    for len in 0..=bytes.len() {
        assert_eq!(crc32(&bytes[..len]), original_crc32(&bytes[..len]));
    }
    for seed in ["", "RandomMovies", "Songs/Pack/Song", "日本語/曲", "a\0b"] {
        let paths: Vec<_> = (0..32)
            .map(|i| PathBuf::from(format!("movie-{i}.mp4")))
            .collect();
        let mut expected = paths.clone();
        shuffle_paths(&mut expected, u64::from(original_crc32(seed.as_bytes())));
        expected.truncate(RANDOM_MOVIE_LIMIT);
        assert_eq!(MovieCycle::new(paths, seed).paths, expected);
    }
}

#[test]
#[ignore = "CPU-cycle benchmark; run with --release --ignored --nocapture --test-threads=1"]
fn benchmark_movie_seed() {
    for length in [0, 12, 64, 256] {
        let seed = "Songs/Pack/Song/".repeat(18)[..length].to_owned();
        let (_, churn) = alloc::measure(|| crc32(black_box(seed.as_bytes())));
        assert_eq!(churn, alloc::Churn::default());
        println!("movie-crc/bytes={length}: allocation {churn:?}");
        bench::measure(&format!("movie-crc/bytes={length}"), 32_768, || {
            black_box(crc32(black_box(seed.as_bytes())));
        });
        let paths: Vec<_> = (0..16)
            .map(|i| PathBuf::from(format!("movie-{i}.mp4")))
            .collect();
        let (_, churn) = alloc::measure(|| MovieCycle::new(paths.clone(), &seed));
        println!("movie-cycle/bytes={length}/paths=16: allocation {churn:?}");
        bench::measure(
            &format!("movie-cycle/bytes={length}/paths=16"),
            4096,
            || {
                black_box(MovieCycle::new(black_box(paths.clone()), black_box(&seed)));
            },
        );
    }
}
