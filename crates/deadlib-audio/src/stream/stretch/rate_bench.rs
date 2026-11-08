//! Additional whole-stream fixtures at slow, fast, and boundary rates.
use super::*;

#[test]
#[ignore = "CPU-cycle benchmark; exact first candidates and a nonmatching control"]
fn benchmark_sola_first_match() {
    for len in [360, 720] {
        for kind in ["same-source", "silence", "noise"] {
            let input = noise(len * 2, 7);
            let pattern = if kind == "silence" {
                vec![0; len]
            } else {
                noise(len, 11)
            };
            let zeros = vec![0; len * 2];
            let (buffer, pattern) = match kind {
                "same-source" => (&input[..], &input[..len]),
                "silence" => (&zeros[..], &pattern[..]),
                _ => (&input[..], &pattern[..]),
            };
            let label = format!("sola/first-{kind}-{len}");
            println!(
                "{label}: fingerprint {:016x}",
                find_closest_match(buffer, pattern)
            );
            let work = || {
                black_box(find_closest_match(black_box(buffer), black_box(pattern)));
            };
            let (_, counts) = allocations::measure(work);
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, if kind == "noise" { 256 } else { 65_536 }, work);
        }
    }
}

#[test]
#[ignore = "CPU-cycle benchmark; retiring windows from large decoder packets"]
fn benchmark_sola_retire() {
    fn retire(stretch: &mut SolaStretcher, input: &[i16]) {
        stretch.reset();
        stretch.push_interleaved_i16(black_box(input));
        while stretch.data_avail_frames > stretch.window_frames {
            let frames = stretch.window_frames;
            for channel in &mut stretch.state {
                channel.correlated_pos += frames;
            }
            stretch.min_correlated_pos += frames;
            stretch.max_correlated_pos += frames;
            stretch.uncorrelated_pos += frames;
            stretch.erase_front(frames);
        }
        for channel in &stretch.state {
            black_box(&channel.data[stretch.data_start..]);
        }
        black_box(stretch.data_avail_frames);
    }
    for channels in [1, 2, 6] {
        for frames in [12_001, 65_536] {
            let input = noise(frames * channels, 7);
            let mut stretch = SolaStretcher::new(channels, 48_000);
            stretch.push_interleaved_i16(&input);
            let (_, counts) = allocations::measure(|| retire(&mut stretch, &input));
            let label = format!("sola/retire-{channels}ch-{frames}");
            let mut hash = 0xcbf29ce484222325u64;
            for channel in &stretch.state {
                for &sample in &channel.data[stretch.data_start..] {
                    hash = (hash ^ u64::from(sample as u16)).wrapping_mul(0x100000001b3);
                }
            }
            println!("{label}: fingerprint {hash:016x}");
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, 1024, || retire(&mut stretch, &input));
        }
    }
}

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
