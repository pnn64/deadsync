extern crate cl_before;
extern crate cl_four;
extern crate cl_public;
extern crate cl_seeded;
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;
#[allow(dead_code)]
#[path = "../../../../../tests/support/perf.rs"]
mod heap;
use std::hint::black_box;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let size: usize = args[2].parse().unwrap();
    let input: Vec<_> = (0..size)
        .map(|i| (i.wrapping_mul(37) ^ (i >> 3) ^ (i >> 9)) as u8)
        .collect();
    let work: fn(&[u8]) -> u32 = black_box(match args[1].as_str() {
        "before" => cl_before::crc32,
        "public" => cl_public::crc32,
        "seeded" => cl_seeded::crc32,
        "four" => cl_four::crc32,
        _ => panic!("unknown stage"),
    });
    cpu::pin_thread();
    let start = cpu::cycles();
    let (result, counts) = heap::measure(|| work(black_box(&input)));
    let first = cpu::cycles() - start;
    let start = cpu::cycles();
    for _ in 0..64 {
        black_box(work(black_box(&input)));
    }
    let warm = (cpu::cycles() - start) as f64 / 64.0;
    assert_eq!(result, cl_before::crc32(&input));
    assert_eq!(counts, heap::Churn::default());
    println!("first cycles {first}; warm cycles {warm:.2}; crc {result}; allocation {counts:?}");
}
