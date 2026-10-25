use super::*;
use std::hint::black_box;
include!("density_kernel_original.rs");
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

fn original(data: &[u8], lanes: usize) -> Vec<usize> {
    fn scan<const L: usize>(data: &[u8]) -> Vec<usize> {
        let mut out = Vec::with_capacity(data.len() / ((L + 1) * 16) + 1);
        original_scan::<L>(data, None, |n| {
            out.push(n);
            true
        });
        out
    }
    match lanes {
        5 => scan::<5>(data),
        8 => scan::<8>(data),
        10 => scan::<10>(data),
        _ => scan::<4>(data),
    }
}

fn chart(lanes: usize, kind: usize) -> Vec<u8> {
    let mut data = Vec::new();
    for measure in 0..128 {
        for row in 0..64 {
            for col in 0..lanes {
                data.push(if kind == 0 || (kind == 1 && row % 4 != 0) {
                    b'0'
                } else if col == (row + measure) % lanes {
                    b"124"[row % 3]
                } else {
                    b'0'
                });
            }
            data.push(b'\n');
        }
        data.extend_from_slice(b",\n");
    }
    data
}

#[test]
fn density_matches_original_for_all_bytes_and_malformed_rows() {
    for lanes in [0, 4, 5, 8, 10, 12] {
        let width = if matches!(lanes, 5 | 8 | 10) {
            lanes
        } else {
            4
        };
        for byte in 0..=255 {
            for col in 0..width {
                let mut data = vec![b'0'; width];
                data[col] = byte;
                data.extend_from_slice(b"\r\n,\n1000\n;\n");
                assert_eq!(
                    measure_densities(&data, lanes),
                    original(&data, lanes),
                    "lanes={lanes}, byte={byte}, col={col}"
                );
            }
        }
        let mut state = 0x1234_5678u32;
        for _ in 0..1000 {
            let data: Vec<u8> = (0..127)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    b"01234MmFfLl\r\n\t /,;\0\xff"
                        [state as usize % b"01234MmFfLl\r\n\t /,;\0\xff".len()]
                })
                .collect();
            assert_eq!(measure_densities(&data, lanes), original(&data, lanes));
        }
    }
}

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_density_kernel() {
    for lanes in [4, 5, 8, 10] {
        for kind in 0..3 {
            let data = chart(lanes, kind);
            assert_eq!(measure_densities(&data, lanes), original(&data, lanes));
            let (_, before) = super::stream_perf::alloc::measure(|| original(&data, lanes));
            let (_, after) = super::stream_perf::alloc::measure(|| measure_densities(&data, lanes));
            assert_eq!(before, after);
            println!("density/{lanes}/kind={kind} allocations {after:?}");
            bench::compare(
                &format!("density/{lanes}/kind={kind}/bytes={}", data.len()),
                200,
                |current| {
                    let scan: fn(&[u8], usize) -> Vec<usize> =
                        black_box(if current { measure_densities } else { original });
                    black_box(scan(black_box(&data), black_box(lanes)));
                },
            );
        }
    }
}
