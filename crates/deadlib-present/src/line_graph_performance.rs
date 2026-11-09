use super::*;
use std::{hint::black_box, time::Instant};

#[path = "../../../tests/support/kernel_bench.rs"]
#[allow(
    unsafe_code,
    reason = "benchmark-only Windows affinity and cycle-count FFI"
)]
mod bench;
#[path = "line_graph_original.rs"]
mod original;

#[inline(never)]
fn production(points: &[[f32; 2]], thickness: f32, color: [f32; 4]) -> Arc<[MeshVertex]> {
    build_graph_line_mesh(points, thickness, color)
}

fn points(len: usize) -> Vec<[f32; 2]> {
    (0..len)
        .map(|i| [i as f32 * 0.75, (i as f32 * 0.01).sin() * 20.0])
        .collect()
}

#[test]
fn graph_vertices_match_original() {
    let mut cases = 0;
    let mut vertices = 0;
    let mut nan_differences = 0;
    let mut check = |points: &[[f32; 2]], thickness: f32, color: [f32; 4]| {
        let expected = original::build_graph_line_mesh(points, thickness, color);
        let actual = production(points, thickness, color);
        assert_eq!(expected.len(), actual.len());
        assert_eq!(Arc::strong_count(&actual), 1);
        assert_eq!(Arc::weak_count(&actual), 0);
        for (index, (a, b)) in expected.iter().zip(actual.iter()).enumerate() {
            for (a, b) in a
                .pos
                .into_iter()
                .chain(a.color)
                .zip(b.pos.into_iter().chain(b.color))
            {
                if a.is_nan() && b.is_nan() {
                    nan_differences += usize::from(a.to_bits() != b.to_bits());
                } else {
                    assert_eq!(
                        a.to_bits(),
                        b.to_bits(),
                        "case={cases}, vertex={index}, thickness={thickness:?}"
                    );
                }
            }
        }
        vertices += expected.len();
        cases += 1;
    };
    let special = [
        0.0,
        -0.0,
        f32::from_bits(1),
        -f32::from_bits(1),
        f32::MIN_POSITIVE,
        1.0,
        -1.0,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc12345),
        f32::from_bits(0xff812345),
    ];
    for len in [0, 1, 2, 3, 4, 7, 32, 100, 256, 2048] {
        for thickness in special {
            check(&points(len), thickness, [0.25, 0.5, 0.75, 0.8]);
        }
    }
    for x in special {
        for y in special {
            for thickness in special {
                check(&[[x, y], [y, x], [x, -y]], thickness, [x, y, -x, -y]);
            }
        }
    }
    let threshold = LINE_MIN_LEN_SQ.sqrt().to_bits();
    for bits in threshold - 3..=threshold + 3 {
        let d = f32::from_bits(bits);
        for points in [
            vec![[0.0, 0.0], [d, 0.0]],
            vec![[0.0, 0.0], [0.0, d]],
            vec![[0.0, 0.0], [1.0, 1.0], [1.0, 1.0]],
            vec![[0.0, 0.0], [0.0, 0.0], [1.0, 1.0]],
        ] {
            check(&points, 2.0, [1.0; 4]);
        }
    }
    let mut random = 7u32;
    for i in 0..10_000 {
        let mut next = || {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            random
        };
        let points = (0..i % 37)
            .map(|_| [f32::from_bits(next()), f32::from_bits(next())])
            .collect::<Vec<_>>();
        let thickness = if i % 3 == 0 {
            f32::from_bits(next())
        } else {
            1.5
        };
        check(
            &points,
            thickness,
            [
                f32::from_bits(next()),
                f32::from_bits(next()),
                f32::from_bits(next()),
                f32::from_bits(next()),
            ],
        );
    }
    for i in 0..1000 {
        let points = (0..i % 257)
            .map(|j| {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                [j as f32 * 0.75, (random as i16) as f32 / 256.0]
            })
            .collect::<Vec<_>>();
        check(&points, 0.125 + (i % 31) as f32, [0.125, 0.25, 0.75, 0.9]);
    }
    println!(
        "{cases} graphs / {vertices} vertices match non-NaN bits; {nan_differences} NaN-bit differences"
    );
}

#[test]
#[ignore = "release CPU benchmark for archived before/after binaries"]
fn benchmark_graph_line_mesh() {
    bench::pin_thread();
    let rotation = std::env::var("GRAPH_LINE_BENCH_ROTATION")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let mut cases = Vec::new();
    for n in [0, 1, 2, 4, 32, 100, 256, 2048, 16384] {
        cases.push((format!("graph-{n}"), points(n), 2.0));
    }
    cases.push(("coincident-256".into(), vec![[1.0, 2.0]; 256], 2.0));
    cases.push(("zero-thickness-256".into(), points(256), 0.0));
    cases.push(("nan-thickness-256".into(), points(256), f32::NAN));
    for j in 0..cases.len() {
        let (name, points, thickness) = &cases[(j + rotation) % cases.len()];
        let iterations = (32768 / points.len().max(1)).max(8);
        for round in 0..10 {
            let start_cycles = bench::cycles();
            let start = Instant::now();
            for _ in 0..iterations {
                let mesh = production(
                    black_box(points),
                    black_box(*thickness),
                    black_box([0.25, 0.5, 0.75, 0.8]),
                );
                black_box(&mesh);
                drop(mesh);
            }
            let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
            let cycles = (bench::cycles() - start_cycles) as f64 / iterations as f64;
            println!("graph-line,{name},{round},{iterations},{cycles:.4},{ns:.4}");
        }
    }
}
