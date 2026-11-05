use super::*;
use crate::{perf, polling_bench as bench};
mod original {
    include!("smx_view_original.rs");
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

fn readings(raw: u16) -> [SensorReading; 4] {
    std::array::from_fn(|i| SensorReading {
        firmware_index: 3 - i,
        label: [Some("U"), None, Some(""), Some("é")][i],
        value: raw.wrapping_add(i as u16),
        threshold: raw.rotate_left(3).wrapping_add(i as u16),
        enabled: (raw >> i) & 1 != 0,
    })
}
#[test]
fn fsr_fields_float_bits_and_aggregates_match() {
    for raw in 0..=u16::MAX {
        same(
            &original::fsr_button_view("L", readings(raw), raw & 1 != 0),
            &fsr_button_view("L", readings(raw), raw & 1 != 0),
        );
    }
    perf::assert_no_churn(|| {
        std::hint::black_box(fsr_button_view("L", readings(250), false));
    });
}
#[test]
fn load_cell_fields_float_bits_and_aggregates_match() {
    for raw in 0..=u16::MAX {
        let values = std::array::from_fn(|i| raw.wrapping_add(i as u16));
        same(
            &original::load_cell_button_view(
                "R",
                values,
                raw.rotate_left(3),
                raw.wrapping_add(7),
                raw & 1 != 0,
            ),
            &load_cell_button_view(
                "R",
                values,
                raw.rotate_left(3),
                raw.wrapping_add(7),
                raw & 1 != 0,
            ),
        );
    }
    perf::assert_no_churn(|| {
        std::hint::black_box(load_cell_button_view(
            "R",
            [0, 249, 250, 65535],
            250,
            10,
            true,
        ));
    });
}
fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    for (c, name) in [(false, "original"), (true, "current")] {
        let (_, churn) = perf::measure(|| work(c));
        println!("{label}/{name}: allocation {churn:?}");
        assert_eq!(churn, perf::Churn::default());
    }
    bench::compare(label, iterations, work);
}
#[test]
#[ignore = "paired SMX builder CPU/allocation measurements"]
fn benchmark() {
    use std::hint::black_box as bb;
    for raw in [100u16, 250, 500, 65535] {
        compare(&format!("smx/fsr/{raw}"), 20000, |c| {
            let r = readings(bb(raw));
            drop(bb(if c {
                fsr_button_view("L", r, bb(false))
            } else {
                original::fsr_button_view("L", r, bb(false))
            }));
        });
        let values = std::array::from_fn(|i| raw.wrapping_add(i as u16));
        compare(&format!("smx/load/{raw}"), 20000, |c| {
            drop(bb(if c {
                load_cell_button_view("L", bb(values), bb(raw.rotate_left(3)), bb(7), bb(true))
            } else {
                original::load_cell_button_view(
                    "L",
                    bb(values),
                    bb(raw.rotate_left(3)),
                    bb(7),
                    bb(true),
                )
            }));
        });
    }
}
fn emit_u64(w: &mut impl std::io::Write, n: usize) {
    w.write_all(&(n as u64).to_le_bytes()).unwrap();
}
fn emit_button(w: &mut impl std::io::Write, b: &ButtonView) {
    match b.label {
        ButtonLabel::Named(s) => {
            emit_u64(w, s.len());
            w.write_all(s.as_bytes()).unwrap();
        }
        ButtonLabel::Hid(_) => panic!("unexpected HID label"),
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
        ValueCurve::Linear { max_raw } => w.write_all(&max_raw.to_le_bytes()).unwrap(),
        _ => panic!("unexpected curve"),
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
#[ignore = "deterministic complete SMX builder output trace"]
fn dump_trace() {
    use std::io::Write;
    let file =
        std::fs::File::create(std::env::var_os("DEADSYNC_SMX_TRACE_OUT").expect("trace path"))
            .unwrap();
    let mut out = std::io::BufWriter::with_capacity(1024 * 1024, file);
    for raw in 0..=u16::MAX {
        emit_button(&mut out, &fsr_button_view("L", readings(raw), raw & 1 != 0));
        emit_button(
            &mut out,
            &load_cell_button_view(
                "R",
                std::array::from_fn(|i| raw.wrapping_add(i as u16)),
                raw.rotate_left(3),
                raw.wrapping_add(7),
                raw & 1 != 0,
            ),
        );
    }
    out.flush().unwrap();
}
