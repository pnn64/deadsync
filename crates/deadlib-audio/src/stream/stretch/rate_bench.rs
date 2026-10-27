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

#[test]
#[ignore = "CPU-cycle benchmark; packet-sized planar unpacking"]
fn benchmark_sola_packets() {
    for channels in [6, 8, 16] {
        let samples = stream_input(channels, "noise");
        for frames in [137, 997] {
            let input = &samples[..frames * channels];
            let mut stretch = SolaStretcher::new(channels, 48_000);
            stretch.push_interleaved_i16(input);
            let mut hash = 0xcbf29ce484222325u64;
            for channel in &stretch.state {
                for &sample in &channel.data {
                    hash = (hash ^ u64::from(sample as u16)).wrapping_mul(0x100000001b3);
                }
            }
            let label = format!("sola/packet-{channels}ch-{frames}");
            println!("{label}: fingerprint {hash:016x}");
            let mut work = || {
                stretch.reset();
                stretch.push_interleaved_i16(black_box(input));
                black_box(&stretch.state);
            };
            let (_, counts) = allocations::measure(&mut work);
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, 256, work);
        }
    }
}
