use crate::perf::measure;

pub fn compare(label: &str, mut original: impl FnMut(), mut current: impl FnMut()) {
    let mut elapsed = [Vec::with_capacity(9), Vec::with_capacity(9)];
    let mut counts = [1usize; 2];
    for _ in 0..3 {
        original();
        current();
    }
    for sample in 0..10 {
        for variant in [sample % 2, 1 - sample % 2] {
            let count = counts[variant];
            let start = std::time::Instant::now();
            for _ in 0..count {
                if variant == 0 {
                    original();
                } else {
                    current();
                }
            }
            let nanos = start.elapsed().as_nanos().max(1);
            if sample == 0 {
                counts[variant] = 25_000_000_u128.div_ceil(nanos).clamp(1, 1_000_000) as usize;
            } else {
                elapsed[variant].push(nanos as f64 / count as f64);
            }
        }
    }
    for samples in &mut elapsed {
        samples.sort_by(f64::total_cmp);
    }
    println!(
        "{label}: original {:.2} ns/op, current {:.2} ns/op, {:.3}x throughput",
        elapsed[0][4],
        elapsed[1][4],
        elapsed[0][4] / elapsed[1][4]
    );
    let (_, before) = measure(&mut original);
    let (_, after) = measure(&mut current);
    println!(
        "{label}: allocations {} -> {}, reallocations {} -> {}, bytes {} -> {}",
        before.allocs,
        after.allocs,
        before.reallocs,
        after.reallocs,
        before.allocated_bytes,
        after.allocated_bytes
    );
    println!(
        "{label}: peak added live bytes {} -> {}",
        before.peak_bytes, after.peak_bytes
    );
}
