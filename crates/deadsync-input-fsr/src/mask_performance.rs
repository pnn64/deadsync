use super::*;
use crate::{perf, polling_bench as bench};
mod original {
    include!("mask_original.rs");
}
fn next(seed: &mut u64) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed as u32
}
fn same(a: &ButtonView, b: &ButtonView) {
    assert_eq!(a.label, b.label);
    assert_eq!(a.min_raw_threshold, b.min_raw_threshold);
    assert_eq!(a.max_raw_threshold, b.max_raw_threshold);
    assert_eq!(a.aggregate_value, b.aggregate_value);
    assert_eq!(a.aggregate_threshold, b.aggregate_threshold);
    assert_eq!(a.active, b.active);
    assert_eq!(a.value_curve, b.value_curve);
    assert_eq!(a.release_threshold, b.release_threshold);
    assert_eq!(a.sensors.len(), b.sensors.len());
    for (a, b) in a.sensors.iter().zip(&b.sensors) {
        assert_eq!(a.firmware_index, b.firmware_index);
        assert_eq!(a.label, b.label);
        assert_eq!(a.raw_value, b.raw_value);
        assert_eq!(a.raw_threshold, b.raw_threshold);
        assert_eq!(a.value_norm.to_bits(), b.value_norm.to_bits());
        assert_eq!(a.threshold_norm.to_bits(), b.threshold_norm.to_bits());
        assert_eq!(a.active, b.active);
        assert_eq!(a.enabled, b.enabled);
    }
}
fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    for (current, name) in [(false, "original"), (true, "current")] {
        let (_, churn) = perf::measure(|| work(current));
        println!("{label}/{name}: allocation {churn:?}");
        assert_eq!(churn, perf::Churn::default());
    }
    bench::compare(label, iterations, work);
}

#[test]
fn masks_and_callers_match() {
    let mut config = ConfigReport::default();
    let input = InputReport {
        sensor_values: std::array::from_fn(|i| (i * 5000) as u16),
    };
    for position in 0..12 {
        for byte in 0..256u16 {
            config.sensor_to_button_mapping = [-1; 12];
            config.sensor_to_button_mapping[position] = byte as u8 as i8;
            assert_eq!(
                original::mapped_button_mask(&config.sensor_to_button_mapping),
                mapped_button_mask(&config.sensor_to_button_mapping)
            );
        }
    }
    let mut seed = 0x12ab_34cd_56ef_7890;
    for _ in 0..16384 {
        for i in 0..12 {
            config.sensor_to_button_mapping[i] = next(&mut seed) as u8 as i8;
            config.sensor_thresholds[i] = next(&mut seed) as u16;
        }
        assert_eq!(
            original::mapped_button_mask(&config.sensor_to_button_mapping),
            mapped_button_mask(&config.sensor_to_button_mapping)
        );
        let a = original::button_views(&config, &input);
        let b = button_views(&config, &input);
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(&b) {
            same(a, b);
        }
        for group in (0..17).chain([usize::MAX]) {
            assert_eq!(
                original::group_sensor_indices(&config.sensor_to_button_mapping, group),
                group_sensor_indices(&config.sensor_to_button_mapping, group)
            );
        }
    }
}
#[test]
#[ignore]
fn benchmark() {
    use std::hint::black_box as bb;
    for (label, mapping) in [
        ("none", [-1; 12]),
        ("one", [0; 12]),
        ("eight", [0, 1, 2, 3, 4, 5, 6, 7, 0, 1, 2, 3]),
        ("high", [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]),
        ("mixed", [-1, 0, -128, 15, 16, 3, 127, 2, -1, 7, 1, 14]),
    ] {
        compare(&format!("mask/{label}"), 200000, |c| {
            let m = bb(&mapping);
            bb(if c {
                mapped_button_mask(m)
            } else {
                original::mapped_button_mask(m)
            });
        });
        let config = ConfigReport {
            sensor_to_button_mapping: mapping,
            sensor_thresholds: std::array::from_fn(|i| (i * 70) as u16),
            ..ConfigReport::default()
        };
        let input = InputReport {
            sensor_values: std::array::from_fn(|i| (i * 91) as u16),
        };
        compare(&format!("views/{label}"), 10000, |c| {
            drop(bb(if c {
                button_views(bb(&config), bb(&input))
            } else {
                original::button_views(bb(&config), bb(&input))
            }));
        });
        compare(&format!("group/{label}"), 20000, |c| {
            bb(if c {
                group_sensor_indices(bb(&mapping), bb(0))
            } else {
                original::group_sensor_indices(bb(&mapping), bb(0))
            });
        });
    }
}

fn emit_u64(w: &mut impl std::io::Write, n: usize) {
    w.write_all(&(n as u64).to_le_bytes()).unwrap();
}
#[test]
#[ignore = "deterministic mapping and caller trace"]
fn dump_trace() {
    use std::io::Write;
    let file =
        std::fs::File::create(std::env::var_os("DEADSYNC_MASK_TRACE_OUT").expect("trace path"))
            .unwrap();
    let mut out = std::io::BufWriter::new(file);
    let mut seed = 0x12ab_34cd_56ef_7890;
    for _ in 0..32768 {
        let mapping = std::array::from_fn(|_| next(&mut seed) as u8 as i8);
        out.write_all(&mapped_button_mask(&mapping).to_le_bytes())
            .unwrap();
        for group in (0..17).chain([usize::MAX]) {
            let indices = group_sensor_indices(&mapping, group);
            emit_u64(&mut out, indices.len());
            emit_u64(&mut out, indices.capacity());
            for n in indices {
                emit_u64(&mut out, n);
            }
        }
    }
    out.flush().unwrap();
}
