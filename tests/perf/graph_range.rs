mod original {
    use super::*;
    include!("original_graph_range.rs");
}

fn bits(range: Option<(f64, f64)>) -> Option<(u64, u64)> {
    range.map(|(lo, hi)| (lo.to_bits(), hi.to_bits()))
}

fn assert_range(values: &[f64], clim: Option<(f64, f64)>) {
    assert_eq!(
        bits(sync_heat_value_range(values, clim)),
        bits(original::sync_heat_value_range(values, clim)),
        "{values:?} {clim:?}"
    );
}

#[test]
fn graph_ranges_preserve_nan_infinity_signed_zero_and_constant_behavior() {
    let nan = f64::from_bits(0x7ff8_0000_0000_0042);
    for values in [
        vec![],
        vec![nan],
        vec![nan, 3.0, nan],
        vec![f64::INFINITY],
        vec![f64::NEG_INFINITY, 1.0],
        vec![0.0, -0.0],
        vec![-0.0, 0.0],
        vec![2.0; 8],
        vec![-5.0, nan, 8.0, -2.0],
    ] {
        for clim in [
            None,
            Some((5.0, 95.0)),
            Some((0.0, 0.0)),
            Some((100.0, 0.0)),
        ] {
            assert_range(&values, clim);
        }
    }
}

#[test]
fn graph_ranges_match_original_for_seeded_matrices_and_percentiles() {
    let mut seed = 0x74ed_9813u64;
    for count in [0, 1, 2, 31, 32, 33, 1024] {
        let values: Vec<_> = (0..count)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                (seed as i64 % 1_000_000) as f64 / 1000.0
            })
            .collect();
        for clim in [
            None,
            Some((5.0, 95.0)),
            Some((0.0, 0.0)),
            Some((0.0, 100.0)),
        ] {
            assert_range(&values, clim);
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --nocapture --test-threads=1"]
fn benchmark_direct_scans() {
    for (label, count, constant, clim) in [
        ("graph/range_32", 32, false, None),
        ("graph/range_4096", 4096, false, None),
        ("graph/range_65536", 65536, false, None),
        ("graph/constant_4096", 4096, true, None),
        ("graph/percentile_control", 1024, false, Some((5.0, 95.0))),
    ] {
        let values: Vec<_> = (0..count)
            .map(|i| {
                if constant {
                    2.0
                } else {
                    ((i * 7919) % 997) as f64
                }
            })
            .collect();
        assert_range(&values, clim);
        crate::scan_perf::compare(
            label,
            || values.as_slice(),
            |values| original::sync_heat_value_range(values, clim),
            |values| sync_heat_value_range(values, clim),
        );
    }
}
