//! Portable deterministic replay/mutation runner; no libFuzzer dependency.
#[path = "../tests/fuzz_support/mod.rs"]
mod fuzz_support;
use std::{env, fs, path::PathBuf, time::Instant};

fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    assert!(
        args.len() == 4,
        "usage: mutate CORPUS ITERATIONS SEED FAILURE_PATH"
    );
    let mut paths = fs::read_dir(&args[0])
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.is_file())
        .collect::<Vec<_>>();
    paths.sort();
    let corpus = paths
        .iter()
        .map(|p| fs::read(p).unwrap())
        .collect::<Vec<_>>();
    assert!(
        !corpus.is_empty()
            && corpus
                .iter()
                .all(|v| !v.is_empty() && v.len() <= fuzz_support::MAX_INPUT)
    );
    let iterations: u64 = args[1].parse().unwrap();
    let mut state: u64 = args[2].parse().unwrap();
    assert_ne!(state, 0);
    let failure = PathBuf::from(&args[3]);
    let mut counts = [0u64; fuzz_support::PATHS.len()];
    let start = Instant::now();
    // Replay all original seeds before mutation so baseline fixture properties are mandatory.
    for i in 0..iterations + corpus.len() as u64 {
        let index = if i < corpus.len() as u64 {
            i as usize
        } else {
            (random(&mut state) as usize) % corpus.len()
        };
        let mut input = corpus[index].clone();
        if i >= corpus.len() as u64 {
            match random(&mut state) % 7 {
                0 => {
                    let n = random(&mut state) as usize % (input.len() + 1);
                    input.truncate(n);
                }
                1 => {
                    if input.len() < fuzz_support::MAX_INPUT {
                        let n = random(&mut state) as usize % (input.len() + 1);
                        input.insert(n, random(&mut state) as u8);
                    }
                }
                2 => {
                    let n = random(&mut state) as usize % input.len();
                    input[n] = [0, 1, 127, 128, 254, 255][random(&mut state) as usize % 6];
                }
                3 => {
                    let n = random(&mut state) as usize % input.len();
                    input[n] ^= 1 << (random(&mut state) % 8);
                }
                4 => {
                    let n = random(&mut state) as usize % input.len();
                    let len = (input.len() - n).min(8);
                    input[n..n + len].fill(if random(&mut state) & 1 == 0 { 0 } else { 255 });
                }
                5 => {
                    let n = random(&mut state) as usize % input.len();
                    input.remove(n);
                }
                _ => {
                    for _ in 0..1 + random(&mut state) % 8 {
                        let n = random(&mut state) as usize % input.len();
                        input[n] = random(&mut state) as u8;
                    }
                }
            }
        }
        let outcome = std::panic::catch_unwind(|| fuzz_support::exercise(&input));
        match outcome {
            Ok(mask) => {
                for (bit, count) in counts.iter_mut().enumerate() {
                    *count += u64::from(mask & (1 << bit) != 0);
                }
            }
            Err(panic) => {
                fs::write(&failure, &input).unwrap();
                eprintln!(
                    "failure iteration={i} corpus={} state={state} saved={}",
                    paths[index].display(),
                    failure.display()
                );
                std::panic::resume_unwind(panic);
            }
        }
    }
    println!(
        "{{\"iterations\":{iterations},\"seeds\":{},\"elapsed_seconds\":{},\"paths\":{{{}}}}}",
        corpus.len(),
        start.elapsed().as_secs_f64(),
        fuzz_support::PATHS
            .iter()
            .zip(counts)
            .map(|(name, n)| format!("\"{name}\":{n}"))
            .collect::<Vec<_>>()
            .join(",")
    );
}
