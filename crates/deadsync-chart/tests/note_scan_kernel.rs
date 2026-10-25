use deadsync_chart::notes::parse_chart_notes;
use std::hint::black_box;

#[path = "../../../tests/perf/core_alloc.rs"]
mod alloc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_note_scan_kernel() {
    for lanes in [4, 5, 8, 10] {
        for kind in ["sparse", "dense", "holds"] {
            let mut data = Vec::new();
            for row in 0..8192 {
                for col in 0..lanes {
                    data.push(match kind {
                        "sparse" if row % 4 == 0 && col == row % lanes => b'1',
                        "dense" => b"1FfmMLl1"[(row + col) % 8],
                        "holds" if col == 0 => b"2030"[row % 4],
                        _ => b'0',
                    });
                }
                data.push(b'\n');
            }
            let work = || {
                black_box(parse_chart_notes(black_box(&data), black_box(lanes)));
            };
            let (_, churn) = alloc::measure(work);
            println!("notes/{lanes}/{kind}: allocation {churn:?}");
            bench::measure(&format!("notes/{lanes}/{kind}"), 100, work);
        }
    }
}
