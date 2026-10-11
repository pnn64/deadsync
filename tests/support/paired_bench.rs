use std::hint::black_box;
use std::time::Instant;

/// Alternate original and current implementations, then report paired medians.
pub fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    compare_prepared(label, iterations, || (), |(), current| work(current));
}

/// Prepare consumed inputs outside the timed region, preserving their capacities.
pub fn compare_prepared<T>(
    label: &str,
    iterations: usize,
    mut prepare: impl FnMut() -> T,
    mut work: impl FnMut(T, bool),
) {
    let mut elapsed = [Vec::with_capacity(9), Vec::with_capacity(9)];
    let mut counts = [iterations; 2];
    for sample in 0..10 {
        for variant in [sample % 2, 1 - sample % 2] {
            let count = counts[variant];
            let inputs: Vec<_> = (0..count).map(|_| prepare()).collect();
            let start = Instant::now();
            for input in inputs {
                work(input, black_box(variant == 1));
            }
            let nanos = start.elapsed().as_nanos().max(1);
            if sample == 0 {
                // Give fast paths roughly two milliseconds per measured batch.
                // Bound prepared input memory even for an unusually fast warmup.
                let batches = 2_000_000_u128.div_ceil(nanos).clamp(1, 1000) as usize;
                counts[variant] = count.saturating_mul(batches).min(1_000_000);
            } else {
                elapsed[variant].push(nanos as f64 / count as f64);
            }
        }
    }
    for samples in &mut elapsed {
        samples.sort_by(f64::total_cmp);
    }
    let original = elapsed[0][4];
    let current = elapsed[1][4];
    println!(
        "{label}: original {original:.2} ns/op, current {current:.2} ns/op, {:.2}x throughput",
        original / current
    );
}
