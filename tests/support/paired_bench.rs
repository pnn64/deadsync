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
    for sample in 0..10 {
        for variant in [sample % 2, 1 - sample % 2] {
            let inputs: Vec<_> = (0..iterations).map(|_| prepare()).collect();
            let start = Instant::now();
            for input in inputs {
                work(input, black_box(variant == 1));
            }
            if sample > 0 {
                elapsed[variant].push(start.elapsed().as_nanos() as f64 / iterations as f64);
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
