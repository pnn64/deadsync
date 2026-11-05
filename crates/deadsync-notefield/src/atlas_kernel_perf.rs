use super::*;
use crate::transforms::numeric_performance::allocations as perf;
use std::hint::black_box as bb;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "atlas_original.rs"]
mod original;
fn cases(mut visit: impl FnMut(u32, usize, usize)) {
    let mut sizes = vec![
        0usize,
        1,
        2,
        3,
        4,
        5,
        7,
        8,
        16,
        31,
        64,
        255,
        65535,
        usize::MAX,
    ];
    for n in [1u64 << 31, 1u64 << 32, (1u64 << 32) + 1, (1u64 << 32) + 2] {
        if let Ok(n) = usize::try_from(n) {
            sizes.push(n);
        }
    }
    for &cols in &sizes {
        for &rows in &sizes {
            for frame in [
                0,
                1,
                2,
                3,
                5,
                6,
                7,
                15,
                31,
                255,
                65535,
                0x7fffffff,
                u32::MAX,
            ] {
                visit(frame, cols, rows);
            }
        }
    }
    for cols in 0..=16 {
        for rows in 0..=16 {
            for frame in 0..=256 {
                visit(frame, cols, rows);
            }
        }
    }
    let mut seed = 0x17fc3921u64;
    for i in 0..16384 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        visit(
            seed as u32,
            if i % 3 == 0 {
                1
            } else if i % 3 == 1 {
                2
            } else {
                seed as usize
            },
            (seed >> 19) as usize,
        );
    }
}
fn outcome(work: impl FnOnce() -> [f32; 4] + std::panic::UnwindSafe) -> Result<[u32; 4], String> {
    std::panic::catch_unwind(work)
        .map(|v| v.map(f32::to_bits))
        .map_err(|v| {
            v.downcast_ref::<String>()
                .cloned()
                .or_else(|| v.downcast_ref::<&str>().map(|v| v.to_string()))
                .unwrap_or_else(|| "opaque panic".into())
        })
}
#[test]
fn atlas_uv_preserves_bits_and_profile_specific_panic_outcomes() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    cases(|frame, cols, rows| {
        assert_eq!(
            outcome(|| original::uv(frame, cols, rows)),
            outcome(|| frame_uv(frame, cols, rows)),
            "{frame}/{cols}/{rows}"
        );
    });
    std::panic::set_hook(hook);
}
#[test]
#[ignore]
fn benchmark() {
    for (name, frame, cols, rows) in [
        ("one", 5, 1, 6),
        ("one-single", 0, 1, 1),
        ("two", 5, 2, 6),
        ("two-seven", 5, 2, 7),
        ("three", 5, 3, 7),
        ("four", 5, 4, 8),
        ("eight", 5, 8, 8),
        ("large", 12345, 32, 32),
        ("zero-cols", 5, 0, 6),
        ("zero-rows", 5, 2, 0),
        ("max-frame", u32::MAX, 2, 6),
        ("wide", u32::MAX, usize::MAX, usize::MAX),
    ] {
        for (current, label) in [(false, "original"), (true, "current")] {
            let (_, c) = perf::measure(|| {
                bb(if current {
                    frame_uv(bb(frame), bb(cols), bb(rows))
                } else {
                    original::uv(bb(frame), bb(cols), bb(rows))
                });
            });
            println!("atlas/{name}/{label}: allocation {c:?}");
        }
        bench::compare(&format!("atlas/{name}"), 100000, |current| {
            bb(if current {
                frame_uv(bb(frame), bb(cols), bb(rows))
            } else {
                original::uv(bb(frame), bb(cols), bb(rows))
            });
        });
    }
}
#[test]
#[ignore = "atlas bit and shared-panic trace"]
fn dump_trace() {
    use std::io::Write;
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(std::env::var_os("DEADSYNC_ATLAS_TRACE_OUT").expect("trace path"))
            .unwrap(),
    );
    cases(|frame, cols, rows| {
        // A wrapped zero row count underflows only in assertion-enabled builds;
        // ordinary tests compare old/current outcomes for that profile-specific case.
        if rows.max(1) as u32 == 0 {
            return;
        }
        out.write_all(&frame.to_le_bytes()).unwrap();
        out.write_all(&(cols as u64).to_le_bytes()).unwrap();
        out.write_all(&(rows as u64).to_le_bytes()).unwrap();
        match outcome(|| frame_uv(frame, cols, rows)) {
            Ok(bits) => {
                out.write_all(&[0]).unwrap();
                for n in bits {
                    out.write_all(&n.to_le_bytes()).unwrap();
                }
            }
            Err(message) => {
                out.write_all(&[1]).unwrap();
                out.write_all(&(message.len() as u64).to_le_bytes())
                    .unwrap();
                out.write_all(message.as_bytes()).unwrap();
            }
        }
    });
    out.flush().unwrap();
    std::panic::set_hook(hook);
}
