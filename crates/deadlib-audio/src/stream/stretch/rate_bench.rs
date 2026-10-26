//! Additional whole-stream fixtures at slow, fast, and boundary rates.
use super::*;

#[test]
#[ignore = "CPU-cycle benchmark; one test thread"]
fn benchmark_sola_rates() {
    for (channels, kind, rate) in [
        (2, "noise", 0.05),
        (2, "noise", 0.5),
        (2, "tone", 0.5),
        (6, "noise", 0.5),
        (2, "noise", 2.0),
        (2, "noise", 8.0),
    ] {
        let input = stream_input(channels, kind);
        let mut stretch = SolaStretcher::new(channels, 48_000);
        stretch.set_speed_ratio(rate);
        let mut output = vec![Vec::with_capacity(256); channels];
        let hash = drain(&mut stretch, &input, &mut output);
        let label = format!("sola/rate-{channels}ch-{kind}-{rate}");
        println!("{label}: fingerprint {hash:016x}");
        let mut work = || {
            stretch.reset();
            black_box(drain(&mut stretch, black_box(&input), &mut output));
        };
        let (_, counts) = allocations::measure(&mut work);
        println!("{label}: allocation {counts:?}");
        bench::measure(&label, 2, work);
    }
}
