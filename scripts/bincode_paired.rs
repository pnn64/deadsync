use std::hint::black_box;
#[path = "../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../tests/support/perf.rs"]
mod perf;
fn measure(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    for current in [false, true] {
        work(current);
        let (_, churn) = perf::measure(|| work(current));
        assert_eq!(churn, perf::Churn::default(), "{label}/current={current}");
        println!("{label}/current={current}: {churn:?}");
    }
    bench::compare(label, iterations, work);
}
fn main() {
    macro_rules! integers {
        ($ty:ty) => {
            for small in [true, false] {
                let mut state = 0x1234_5678_9abc_def0_1357_9bdf_2468_ace0u128;
                let values: Vec<$ty> = (0..4096)
                    .map(|_| {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        (if small { state % 241 } else { state }) as $ty
                    })
                    .collect();
                let encoded = bincode::encode_to_vec(&values, bincode::config::standard()).unwrap();
                assert_eq!(
                    encoded,
                    bincode_before::encode_to_vec(&values, bincode_before::config::standard())
                        .unwrap()
                );
                let mut bytes = Vec::with_capacity(encoded.len());
                let mut decoded = Vec::<$ty>::with_capacity(values.len());
                measure(
                    &format!("width/{}/{small}/encode", stringify!($ty)),
                    1000,
                    |current| {
                        if current {
                            bincode::encode_into_vec(
                                black_box(&values),
                                black_box(&mut bytes),
                                bincode::config::standard(),
                            )
                            .unwrap();
                        } else {
                            bincode_before::encode_into_vec(
                                black_box(&values),
                                black_box(&mut bytes),
                                bincode_before::config::standard(),
                            )
                            .unwrap();
                        }
                        black_box(&bytes);
                    },
                );
                assert_eq!(bytes, encoded);
                measure(
                    &format!("width/{}/{small}/decode", stringify!($ty)),
                    1000,
                    |current| {
                        if current {
                            bincode::decode_from_slice_into_vec(
                                black_box(&encoded),
                                black_box(&mut decoded),
                                bincode::config::standard(),
                            )
                            .unwrap();
                        } else {
                            bincode_before::decode_from_slice_into_vec(
                                black_box(&encoded),
                                black_box(&mut decoded),
                                bincode_before::config::standard(),
                            )
                            .unwrap();
                        }
                        black_box(&decoded);
                    },
                );
                assert_eq!(decoded, values);
            }
        };
    }
    integers!(u16);
    integers!(u32);
    integers!(u64);
    integers!(u128);
    integers!(usize);
    integers!(i64);
    integers!(i128);
    for len in [8usize, 64, 128, 4096, 65536] {
        for error in ["none", "first", "last"] {
            let values: Vec<bool> = (0..len).map(|i| i % 3 == 0).collect();
            let mut encoded = bincode::encode_to_vec(&values, bincode::config::standard()).unwrap();
            if error == "first" {
                let prefix = encoded.len() - len;
                encoded[prefix] = 255;
            }
            if error == "last" {
                *encoded.last_mut().unwrap() = 255;
            }
            let mut decoded = Vec::<bool>::with_capacity(len);
            measure(
                &format!("bool/{len}/{error}"),
                (10_000_000 / len).max(1000),
                |current| {
                    if current {
                        black_box(bincode::decode_from_slice_into_vec(
                            black_box(&encoded),
                            black_box(&mut decoded),
                            bincode::config::standard(),
                        ))
                        .ok();
                    } else {
                        black_box(bincode_before::decode_from_slice_into_vec(
                            black_box(&encoded),
                            black_box(&mut decoded),
                            bincode_before::config::standard(),
                        ))
                        .ok();
                    }
                    black_box(&decoded);
                },
            );
        }
    }
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
        let encoded = bincode::encode_to_vec(&values, bincode::config::standard()).unwrap();
        assert_eq!(
            encoded,
            bincode_before::encode_to_vec(&values, bincode_before::config::standard()).unwrap()
        );
        let mut bytes = Vec::with_capacity(encoded.len());
        measure(&format!("signed/{kind}/encode"), 3000, |current| {
            if current {
                bincode::encode_into_vec(
                    black_box(&values),
                    black_box(&mut bytes),
                    bincode::config::standard(),
                )
                .unwrap();
            } else {
                bincode_before::encode_into_vec(
                    black_box(&values),
                    black_box(&mut bytes),
                    bincode_before::config::standard(),
                )
                .unwrap();
            }
            black_box(&bytes);
        });
        assert_eq!(bytes, encoded);
        let mut decoded = Vec::<i32>::with_capacity(values.len());
        measure(&format!("signed/{kind}/decode"), 3000, |current| {
            if current {
                bincode::decode_from_slice_into_vec(
                    black_box(&encoded),
                    black_box(&mut decoded),
                    bincode::config::standard(),
                )
                .unwrap();
            } else {
                bincode_before::decode_from_slice_into_vec(
                    black_box(&encoded),
                    black_box(&mut decoded),
                    bincode_before::config::standard(),
                )
                .unwrap();
            }
            black_box(&decoded);
        });
        assert_eq!(decoded, values);
    }
    for len in [8, 4096] {
        macro_rules! endian {
            ($ty:ty, $make:expr) => {{
                let values: Vec<$ty> = (0..len).map($make).collect();
                let encoded =
                    bincode::encode_to_vec(&values, bincode::config::legacy().with_big_endian())
                        .unwrap();
                let mut decoded = Vec::<$ty>::with_capacity(len);
                measure(
                    &format!("endian/{}/{len}", stringify!($ty)),
                    10000,
                    |current| {
                        if current {
                            bincode::decode_from_slice_into_vec(
                                black_box(&encoded),
                                black_box(&mut decoded),
                                bincode::config::legacy().with_big_endian(),
                            )
                            .unwrap();
                        } else {
                            bincode_before::decode_from_slice_into_vec(
                                black_box(&encoded),
                                black_box(&mut decoded),
                                bincode_before::config::legacy().with_big_endian(),
                            )
                            .unwrap();
                        }
                        black_box(&decoded);
                    },
                );
                assert_eq!(
                    bincode::encode_to_vec(decoded, bincode::config::legacy().with_big_endian())
                        .unwrap(),
                    encoded
                );
            }};
        }
        endian!(u32, |i| (i as u32).wrapping_mul(0x9e3779b9));
        endian!(f64, |i| f64::from_bits(
            (i as u64).wrapping_mul(0x9e3779b97f4a7c15)
        ));
    }
}
