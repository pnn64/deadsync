use super::*;
use crate::{perf, polling_bench as bench};
mod original {
    include!("fsrio_original.rs");
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

fn check(config: &ConfigReport, input: &InputReport) {
    let a = original::button_views(config, input);
    let b = button_views(config, input);
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(&b) {
        same(a, b);
    }
    for group in (0..18).chain([usize::MAX]) {
        assert_eq!(
            original::group_sensor_indices(&config.sensor_to_button_mapping, group),
            group_sensor_indices(&config.sensor_to_button_mapping, group)
        );
    }
}
#[test]
fn mappings() {
    let mut config = ConfigReport::default();
    let mut input = InputReport::default();
    for (i, raw) in input.sensor_values.iter_mut().enumerate() {
        *raw = (i * 8191) as u16;
        config.sensor_thresholds[i] = (i * 4095) as u16;
    }
    for i in 0..12 {
        for byte in 0..256 {
            config.sensor_to_button_mapping = [-1; 12];
            config.sensor_to_button_mapping[i] = byte as u8 as i8;
            check(&config, &input);
        }
    }
    let mut seed = 0x89ab_cdef_1234_5678;
    for _ in 0..4096 {
        for i in 0..12 {
            config.sensor_to_button_mapping[i] = (next(&mut seed) % 24) as i8 - 4;
            config.sensor_thresholds[i] = next(&mut seed) as u16;
            input.sensor_values[i] = next(&mut seed) as u16;
        }
        check(&config, &input);
    }
}
#[test]
fn packets() {
    for n in 0..80 {
        for first in 0..256u16 {
            let mut bytes = vec![0; n];
            for (i, b) in bytes.iter_mut().enumerate() {
                *b = (i * 17) as u8;
            }
            if n > 0 {
                bytes[0] = first as u8;
            }
            assert_eq!(
                original::parse_input_report(&bytes).map(|r| r.sensor_values),
                parse_input_report(&bytes).map(|r| r.sensor_values)
            );
        }
    }
    for raw in 0..=u16::MAX {
        let mut bytes = [0; 27];
        bytes[0] = 1;
        for i in 0..12 {
            bytes[3 + i * 2..5 + i * 2].copy_from_slice(&raw.wrapping_add(i as u16).to_le_bytes());
        }
        assert_eq!(
            original::parse_input_report(&bytes).map(|r| r.sensor_values),
            parse_input_report(&bytes).map(|r| r.sensor_values)
        );
    }
}
#[test]
#[ignore = "paired FSRIO CPU and allocation measurements"]
fn benchmark() {
    use std::hint::black_box as bb;
    for n in [0, 1, 4, 8, 12] {
        let mut config = ConfigReport {
            sensor_to_button_mapping: [-1; 12],
            ..ConfigReport::default()
        };
        for i in 0..12 {
            if n > 0 {
                config.sensor_to_button_mapping[i] = (i % n) as i8;
            }
            config.sensor_thresholds[i] = (i * 70) as u16;
        }
        let input = InputReport {
            sensor_values: std::array::from_fn(|i| (i * 91) as u16),
        };
        compare(&format!("views/{n}"), 10000, |c| {
            let a = bb(&config);
            let b = bb(&input);
            drop(bb(if c {
                button_views(a, b)
            } else {
                original::button_views(a, b)
            }));
        });
        for group in [0, 7, 11, 12, usize::MAX] {
            compare(&format!("group/{n}/{group}"), 20000, |c| {
                let m = bb(&config.sensor_to_button_mapping);
                bb(if c {
                    group_sensor_indices(m, bb(group))
                } else {
                    original::group_sensor_indices(m, bb(group))
                });
            });
        }
    }
    for n in [0, 25, 26, 27, 64] {
        let mut bytes = vec![42; n];
        if n > 0 {
            bytes[0] = 1;
        }
        compare(&format!("packet/{n}"), 200000, |c| {
            let b = bb(bytes.as_slice());
            bb(if c {
                parse_input_report(b)
            } else {
                original::parse_input_report(b)
            });
        });
    }
}

#[test]
fn high_button_bits_full_raw_range_and_group_limits() {
    let config = ConfigReport {
        sensor_to_button_mapping: [15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4],
        ..ConfigReport::default()
    };
    for group in 0..12 {
        assert_eq!(
            group_sensor_indices(&config.sensor_to_button_mapping, group).as_slice(),
            [11 - group]
        );
    }
    assert!(group_sensor_indices(&config.sensor_to_button_mapping, 12).is_empty());
    assert!(group_sensor_indices(&config.sensor_to_button_mapping, usize::MAX).is_empty());
    perf::assert_no_churn(|| {
        std::hint::black_box(button_views(&config, &InputReport::default()));
        for group in 0..18 {
            std::hint::black_box(group_sensor_indices(
                &config.sensor_to_button_mapping,
                group,
            ));
        }
        std::hint::black_box(parse_input_report(&[0; 27]));
    });
    for raw in 0..=u16::MAX {
        let config = ConfigReport {
            sensor_to_button_mapping: [0; 12],
            sensor_thresholds: [raw.reverse_bits(); 12],
            ..ConfigReport::default()
        };
        let input = InputReport {
            sensor_values: [raw; 12],
        };
        let a = original::button_views(&config, &input);
        let b = button_views(&config, &input);
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(&b) {
            same(a, b);
        }
    }
}
fn emit_u64(w: &mut impl std::io::Write, n: usize) {
    w.write_all(&(n as u64).to_le_bytes()).unwrap();
}
fn emit_button(w: &mut impl std::io::Write, b: &ButtonView) {
    match b.label {
        ButtonLabel::Hid(n) => w.write_all(&[0, n]).unwrap(),
        ButtonLabel::Sensor(_) | ButtonLabel::Group(_) => panic!("unexpected serial label"),
        ButtonLabel::Named(s) => {
            w.write_all(&[1]).unwrap();
            emit_u64(w, s.len());
            w.write_all(s.as_bytes()).unwrap();
        }
    }
    for n in [
        b.min_raw_threshold,
        b.max_raw_threshold,
        b.aggregate_value,
        b.aggregate_threshold,
    ] {
        w.write_all(&n.to_le_bytes()).unwrap();
    }
    w.write_all(&[u8::from(b.active)]).unwrap();
    match b.value_curve {
        ValueCurve::Linear { max_raw } => {
            w.write_all(&[0]).unwrap();
            w.write_all(&max_raw.to_le_bytes()).unwrap();
        }
        ValueCurve::QuarticBlend {
            max_raw,
            quartic_weight,
            linear_weight,
        } => {
            w.write_all(&[1]).unwrap();
            w.write_all(&max_raw.to_le_bytes()).unwrap();
            w.write_all(&quartic_weight.to_bits().to_le_bytes())
                .unwrap();
            w.write_all(&linear_weight.to_bits().to_le_bytes()).unwrap();
        }
    }
    match b.release_threshold {
        Some(n) => {
            w.write_all(&[1]).unwrap();
            w.write_all(&n.to_le_bytes()).unwrap();
        }
        None => w.write_all(&[0]).unwrap(),
    }
    emit_u64(w, b.sensors.len());
    emit_u64(w, b.sensors.capacity());
    for s in &b.sensors {
        emit_u64(w, s.firmware_index);
        match s.label {
            Some(label) => {
                w.write_all(&[1]).unwrap();
                emit_u64(w, label.len());
                w.write_all(label.as_bytes()).unwrap();
            }
            None => w.write_all(&[0]).unwrap(),
        }
        w.write_all(&s.raw_value.to_le_bytes()).unwrap();
        w.write_all(&s.value_norm.to_bits().to_le_bytes()).unwrap();
        w.write_all(&s.raw_threshold.to_le_bytes()).unwrap();
        w.write_all(&s.threshold_norm.to_bits().to_le_bytes())
            .unwrap();
        w.write_all(&[u8::from(s.active), u8::from(s.enabled)])
            .unwrap();
    }
}
#[test]
#[ignore = "deterministic FSRIO view/group/packet trace"]
fn dump_trace() {
    use std::io::Write;
    let file =
        std::fs::File::create(std::env::var_os("DEADSYNC_FSRIO_TRACE_OUT").expect("trace path"))
            .unwrap();
    let mut out = std::io::BufWriter::with_capacity(1024 * 1024, file);
    let mut seed = 0x89ab_cdef_1234_5678;
    for _ in 0..8192 {
        let mut config = ConfigReport::default();
        let mut input = InputReport::default();
        for i in 0..12 {
            config.sensor_to_button_mapping[i] = (next(&mut seed) % 24) as i8 - 4;
            config.sensor_thresholds[i] = next(&mut seed) as u16;
            input.sensor_values[i] = next(&mut seed) as u16;
        }
        let buttons = button_views(&config, &input);
        emit_u64(&mut out, buttons.len());
        emit_u64(&mut out, buttons.capacity());
        for b in &buttons {
            emit_button(&mut out, b);
        }
        for group in (0..18).chain([usize::MAX]) {
            let indices = group_sensor_indices(&config.sensor_to_button_mapping, group);
            emit_u64(&mut out, indices.len());
            emit_u64(&mut out, indices.capacity());
            for n in indices {
                emit_u64(&mut out, n);
            }
        }
    }
    for raw in 0..=u16::MAX {
        let mut bytes = [0; 27];
        bytes[0] = 1;
        for i in 0..12 {
            bytes[3 + i * 2..5 + i * 2].copy_from_slice(&raw.wrapping_add(i as u16).to_le_bytes());
        }
        for n in [0, 25, 26, 27] {
            match parse_input_report(&bytes[..n]) {
                None => out.write_all(&[0]).unwrap(),
                Some(r) => {
                    out.write_all(&[1]).unwrap();
                    for value in r.sensor_values {
                        out.write_all(&value.to_le_bytes()).unwrap();
                    }
                }
            }
        }
    }
    out.flush().unwrap();
}
