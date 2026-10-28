//! Resident-input probes. Fixture creation and exact output checks are untimed.
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
#[allow(dead_code)]
#[path = "../../../../../tests/support/perf.rs"]
mod heap;

use deadlib_audio_decode_mp3::{
    bits::BitReader,
    synthesis::{Hybrid, Synthesis},
    *,
};
use std::{
    hint::black_box,
    io::{self, Write},
    path::Path,
};

fn probe(label: &str, iterations: usize, mut work: impl FnMut()) {
    cpu::measure(label, iterations, &mut work);
    let (_, counts) = heap::measure(work);
    println!("{label}: allocation {counts:?}");
}

fn signal<const N: usize>(seed: u64) -> [f64; N] {
    let mut state = seed;
    std::array::from_fn(|_| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state as i64 as f64) * (1.0 / i64::MAX as f64)
    })
}

fn packets(path: &Path) -> Vec<(Vec<u8>, Option<FreeFormat>)> {
    let bytes = std::fs::read(path).unwrap();
    let mut reader = PacketReader::new(bytes.as_slice());
    let mut frames = Vec::new();
    while let Some(packet) = reader.next_packet().unwrap() {
        frames.push((packet.data.to_vec(), packet.free_format));
    }
    assert!(!frames.is_empty());
    frames
}

fn verify(paths: &[String]) {
    let mut out = io::BufWriter::new(io::stdout().lock());
    for seed in [1, 0x73429, 0x982374ab] {
        let mut synthesis = Synthesis::new();
        for slot in 0..40 {
            let mut input = signal::<32>(seed + slot);
            if slot > 30 {
                input.fill(0.0);
            }
            let mut pcm = [0.0; 32];
            synthesis.process(&input, &mut pcm);
            for value in pcm {
                out.write_all(&value.to_le_bytes()).unwrap();
            }
        }
        // Dense, sparse, zero and signed-zero spectra with overlap history.
        for (kind, mixed) in [(0, 0), (1, 0), (3, 0), (2, 0), (2, 2), (2, 4)] {
            let mut hybrid = Hybrid::new();
            for band in 0..580 {
                let mut input = if band == 576 {
                    signal(seed)
                } else {
                    [0.0; 576]
                };
                if band < 576 {
                    input[band] = -0.5;
                }
                if band == 578 {
                    input.fill(-0.0);
                }
                let mut output = [0.0; 576];
                hybrid
                    .process(&mut input, kind, mixed, &mut output)
                    .unwrap();
                for value in input.into_iter().chain(output) {
                    out.write_all(&value.to_le_bytes()).unwrap();
                }
            }
        }
    }
    for path in paths {
        let mut decoder = FrameDecoder::new();
        for (data, free) in packets(Path::new(path)) {
            let frame = decoder.decode(&data, free).unwrap();
            for value in frame.pcm.planes().flatten() {
                out.write_all(&value.to_le_bytes()).unwrap();
            }
        }
    }
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "verify") {
        verify(&args[1..]);
        return;
    }
    let input = signal::<32>(0x73429);
    let mut pcm = [0.0; 32];
    let mut synthesis = Synthesis::new();
    probe("synthesis/slot", 10_000, || {
        synthesis.process(black_box(&input), black_box(&mut pcm));
    });
    for (kind, mixed) in [(0, 0), (1, 0), (3, 0), (2, 0), (2, 2), (2, 4)] {
        let source = signal::<576>(0x73429);
        let mut hybrid = Hybrid::new();
        let mut output = [0.0; 576];
        probe(&format!("hybrid/{kind}/{mixed}"), 1000, || {
            let mut input = source;
            hybrid
                .process(black_box(&mut input), kind, mixed, black_box(&mut output))
                .unwrap();
        });
    }
    let bytes: Vec<_> = (0..4096).map(|n| (n * 37 + 13) as u8).collect();
    for width in [1, 4, 9, 16, 24, 32] {
        probe(&format!("bits/{width}"), 1000, || {
            let mut bits = BitReader::new(black_box(&bytes));
            for _ in 0..512 {
                black_box(bits.read(black_box(width)).unwrap());
            }
        });
    }
    for path in &args {
        let frames = packets(Path::new(path));
        let label = Path::new(path).file_name().unwrap().to_string_lossy();
        let mut decoder = FrameDecoder::new();
        probe(&format!("decode/{label}"), 2, || {
            decoder.reset();
            for (data, free) in &frames {
                let frame = decoder.decode(black_box(data), *free).unwrap();
                for plane in frame.pcm.planes() {
                    black_box(plane);
                }
            }
        });
    }
}
