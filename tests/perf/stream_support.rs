use std::time::Instant;

/// Alternate original and current implementations, then report paired medians.
pub fn compare(
    label: &str,
    iterations: usize,
    mut original: impl FnMut(),
    mut current: impl FnMut(),
) {
    let mut elapsed = [Vec::with_capacity(9), Vec::with_capacity(9)];
    let mut counts = [iterations; 2];
    for sample in 0..10 {
        for variant in [sample % 2, 1 - sample % 2] {
            let count = counts[variant];
            let nanos = if variant == 0 {
                batch(count, &mut original)
            } else {
                batch(count, &mut current)
            };
            if sample == 0 {
                // Give fast paths roughly five milliseconds per measured batch.
                // Bound iteration counts even for an unusually fast warmup.
                let batches = 5_000_000_u128.div_ceil(nanos).clamp(1, 1_000_000) as usize;
                counts[variant] = count.saturating_mul(batches).min(5_000_000);
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

fn batch(count: usize, work: &mut impl FnMut()) -> u128 {
    let start = Instant::now();
    for _ in 0..count {
        work();
    }
    start.elapsed().as_nanos().max(1)
}
