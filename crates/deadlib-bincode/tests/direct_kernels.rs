use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_signed_codec() {
    let config = bincode::config::standard();
    for kind in ["small", "mixed", "wide", "positive"] {
        let mut state = 0x1234_5678u32;
        let values: Vec<i32> = (0..4096)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                match kind {
                    "small" => (state % 241) as i32 - 120,
                    "mixed" => (state % 60_001) as i32 - 30_000,
                    "positive" => (state % 30_001) as i32,
                    _ => state as i32,
                }
            })
            .collect();
        let encoded = bincode::encode_to_vec(&values, config).unwrap();
        let mut bytes = Vec::with_capacity(encoded.len());
        let mut decoded = Vec::<i32>::with_capacity(values.len());
        for encode in [true, false] {
            let label = format!("signed/{kind}/{}", if encode { "encode" } else { "decode" });
            let mut work = || {
                if encode {
                    bincode::encode_into_vec(black_box(&values), black_box(&mut bytes), config)
                        .unwrap();
                    black_box(&bytes);
                } else {
                    bincode::decode_from_slice_into_vec(
                        black_box(&encoded),
                        black_box(&mut decoded),
                        config,
                    )
                    .unwrap();
                    black_box(&decoded);
                }
            };
            let (_, counts) = perf::measure(&mut work);
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, 300, &mut work);
        }
        assert_eq!(bytes, encoded);
        assert_eq!(decoded, values);
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_bool_codec() {
    let config = bincode::config::standard();
    for len in [0, 1, 7, 8, 9, 16, 256, 4096, 65536] {
        let values: Vec<bool> = (0..len).map(|i| i % 3 == 0).collect();
        let encoded = bincode::encode_to_vec(&values, config).unwrap();
        for invalid in [false, true] {
            if invalid && len == 0 {
                continue;
            }
            let mut input = encoded.clone();
            if invalid {
                *input.last_mut().unwrap() = 2;
            }
            let mut decoded = Vec::<bool>::with_capacity(len);
            let label = format!("bool/len={len}/invalid={invalid}");
            let mut work = || {
                black_box(bincode::decode_from_slice_into_vec(
                    black_box(&input),
                    black_box(&mut decoded),
                    config,
                ))
                .ok();
                black_box(&decoded);
            };
            let (_, counts) = perf::measure(&mut work);
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, (2_000_000 / len.max(1)).max(100), &mut work);
            assert_eq!(decoded, values[..len - usize::from(invalid)]);
        }
    }
}
