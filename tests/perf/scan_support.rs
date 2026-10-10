// Paired measurements with owned-input setup outside timing and allocation counts.
use std::hint::black_box;

pub fn compare<T, R>(
    label: &str,
    mut setup: impl FnMut() -> T,
    mut original: impl FnMut(T) -> R,
    mut current: impl FnMut(T) -> R,
) {
    for _ in 0..3 {
        drop(black_box(original(setup())));
        drop(black_box(current(setup())));
    }
    let mut elapsed = [Vec::with_capacity(9), Vec::with_capacity(9)];
    let mut counts = [1usize; 2];
    for sample in 0..10 {
        for variant in [sample % 2, 1 - sample % 2] {
            let count = counts[variant];
            let inputs: Vec<_> = (0..count).map(|_| setup()).collect();
            let start = std::time::Instant::now();
            if variant == 0 {
                for input in inputs {
                    drop(black_box(original(black_box(input))));
                }
            } else {
                for input in inputs {
                    drop(black_box(current(black_box(input))));
                }
            }
            let nanos = start.elapsed().as_nanos().max(1);
            if sample == 0 {
                counts[variant] = 25_000_000_u128.div_ceil(nanos).clamp(1, 4096) as usize;
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
    let input = setup();
    let (_, before) = crate::scan_alloc::measure(|| drop(black_box(original(black_box(input)))));
    let input = setup();
    let (_, after) = crate::scan_alloc::measure(|| drop(black_box(current(black_box(input)))));
    println!(
        "{label}: allocations {} -> {}, reallocations {} -> {}, bytes {} -> {}",
        before.allocs,
        after.allocs,
        before.reallocs,
        after.reallocs,
        before.allocated_bytes,
        after.allocated_bytes
    );
}
