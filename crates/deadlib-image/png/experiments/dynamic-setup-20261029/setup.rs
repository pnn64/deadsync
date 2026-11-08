use std::hint::black_box;
mod cpu;

fn main() {
    // A complete comb tree: one symbol per depth, two at the deepest depth.
    for max in 1..=15 {
        let mut lengths: Vec<u8> = (1..=max).collect();
        lengths.push(max);
        cpu::compare(&format!("complete-max-{max}"), 20000, |current| {
            if current {
                selected::zlib::__bench_setup(black_box(&lengths));
            } else {
                baseline::zlib::__bench_setup(black_box(&lengths));
            }
        });
    }
}
