//! Compare the output kernel with Rust's saturating float-to-integer contract.
use super::write_i16_samples;
use std::hint::black_box;

fn assert_matches_cast(src: &[f32]) {
    let expected: Vec<_> = src
        .iter()
        .map(|&sample| (sample * 32768.0) as i16)
        .collect();
    let mut actual = vec![12345; src.len()];
    write_i16_samples(black_box(src), black_box(&mut actual), true);
    assert_eq!(actual, expected);
}

#[test]
fn pcm_conversion_matches_boundaries_and_float_patterns() {
    let mut src = Vec::new();
    // Every PCM truncation boundary and the adjacent representable floats.
    for pcm in i16::MIN..=i16::MAX {
        let sample = f32::from(pcm) / 32768.0;
        src.extend([sample.next_down(), sample, sample.next_up()]);
    }
    for bits in [
        0,
        1,
        0x007f_ffff,
        0x0080_0000,
        0x3f80_0000,
        0x4000_0000,
        0x7f7f_ffff,
        0x7f80_0000,
        0x7f80_0001,
        0x7fc0_0000,
        0x7fff_ffff,
    ] {
        src.extend([f32::from_bits(bits), f32::from_bits(bits | 0x8000_0000)]);
    }
    let mut state = 0x1234_5678u32;
    for _ in 0..200_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        src.push(f32::from_bits(state));
    }
    assert_matches_cast(&src);
}

#[test]
fn pcm_conversion_preserves_tails_guards_and_silence() {
    let src: [f32; 80] = std::array::from_fn(|i| {
        if i % 7 == 0 {
            [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0][i / 7 % 4]
        } else {
            f32::from_bits((i as u32).wrapping_mul(0x9e37_79b9))
        }
    });
    for src_len in 0..67 {
        for dst_len in 0..67 {
            for offset in 0..8 {
                for has_audio in [false, true] {
                    let input = &src[offset..offset + src_len];
                    let mut expected = [12345; 80];
                    let mut actual = expected;
                    let expected_slice = &mut expected[offset..offset + dst_len];
                    if has_audio {
                        for (dst, &sample) in expected_slice.iter_mut().zip(input) {
                            *dst = (sample * 32768.0) as i16;
                        }
                    } else {
                        expected_slice.fill(0);
                    }
                    write_i16_samples(
                        black_box(input),
                        black_box(&mut actual[offset..offset + dst_len]),
                        black_box(has_audio),
                    );
                    assert_eq!(
                        actual, expected,
                        "src={src_len}, dst={dst_len}, offset={offset}, audio={has_audio}"
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "exhaustive 2^32 float-pattern check; run explicitly in release mode"]
fn pcm_conversion_matches_every_f32_bit_pattern() {
    let mut src = [0.0; 4096];
    let mut expected = [0; 4096];
    let mut actual = [0; 4096];
    for base in (0u64..1u64 << 32).step_by(src.len()) {
        for (i, (sample, dst)) in src.iter_mut().zip(&mut expected).enumerate() {
            *sample = f32::from_bits((base + i as u64) as u32);
            *dst = (*sample * 32768.0) as i16;
        }
        write_i16_samples(black_box(&src), black_box(&mut actual), true);
        assert_eq!(actual, expected, "starting at float bits {base:08x}");
    }
}

// CPU cycles are sampled on the pinned benchmark thread. All inputs and
// allocations are outside the timed interval, and output stays observable.
#[cfg(windows)]
#[test]
#[allow(
    unsafe_code,
    reason = "Windows thread-cycle counter and thread affinity for benchmark"
)]
#[ignore = "manual release benchmark; compare immutable baseline/candidate binaries"]
fn pcm_conversion_thread_cycles() {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
        fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
    }
    fn cycles(thread: *mut c_void) -> u64 {
        let mut value = 0;
        assert_ne!(unsafe { QueryThreadCycleTime(thread, &raw mut value) }, 0);
        value
    }

    let thread = unsafe { GetCurrentThread() };
    assert_ne!(unsafe { SetThreadAffinityMask(thread, 1) }, 0);
    let iterations: usize = std::env::var("DEAD_PCM_BENCH_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(12000);
    let size = 4096;
    let mut random = 0x1234_5678u32;
    let normal: Vec<f32> = (0..size)
        .map(|_| {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            (random as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect();
    let clipped: Vec<f32> = (0..size)
        .map(|i| (i as i32 % 17 - 8) as f32 / 3.0)
        .collect();
    let special: Vec<f32> = (0..size)
        .map(|i| match i % 16 {
            0 => f32::NAN,
            1 => f32::NEG_INFINITY,
            2 => f32::INFINITY,
            3 => -0.0,
            _ => f32::from_bits((i as u32).wrapping_mul(0x9e37_79b9)),
        })
        .collect();

    let mut output = vec![0i16; size];
    for (name, samples) in [
        ("normal", &normal),
        ("clipped", &clipped),
        ("special", &special),
    ] {
        let input = black_box(samples.as_slice());
        // Warm both instruction and data caches before starting the counter.
        for _ in 0..64 {
            write_i16_samples(input, black_box(output.as_mut_slice()), true);
        }
        let before = cycles(thread);
        for _ in 0..iterations {
            write_i16_samples(input, black_box(output.as_mut_slice()), true);
        }
        let total = cycles(thread) - before;
        println!(
            "PCM_CYCLES {name} total={total} samples={} iterations={iterations} cycles_per_sample={:.5} result={}",
            size,
            total as f64 / (size * iterations) as f64,
            black_box(output[0]),
        );
    }
}
