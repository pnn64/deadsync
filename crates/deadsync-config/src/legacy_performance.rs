use super::parse_legacy_present_mode;
use crate::dirs::resource_perf::{support as perf, text_codecs::bench};
use std::{hint::black_box as bb, io::Write};
#[path = "legacy_original.rs"]
mod original;
fn cases(step: usize, mut visit: impl FnMut(&str)) {
    for name in ["balanced", "unhinged", "maxfps", "max_fps", "max-fps"] {
        for mask in 0..1usize << name.len() {
            let s: String = name
                .bytes()
                .enumerate()
                .map(|(i, b)| {
                    char::from(if mask & (1 << i) == 0 {
                        b
                    } else {
                        b.to_ascii_uppercase()
                    })
                })
                .collect();
            visit(&s);
            visit(&format!("\u{2003}{s}\t"));
        }
        for position in 0..=name.len() {
            for byte in 0..128u8 {
                visit(&format!(
                    "{}{}{}",
                    &name[..position],
                    char::from(byte),
                    &name[position..]
                ));
            }
        }
    }
    for word in (0..0x110000).step_by(step) {
        if let Some(ch) = char::from_u32(word) {
            visit(&format!("{ch}max_fps"));
        }
    }
    for s in ["", "max fps", "max--fps", "balanced!", "\0", "\u{feff}"] {
        visit(s);
    }
}
const BALANCED: [u32; 2] = [0x7fc0_0001, 0x8000_0000];
const UNHINGED: [u32; 2] = [0xffff_ffff, 0];
#[test]
fn mode_preserves_aliases_unicode_and_opaque_policy_bits() {
    cases(1, |s| {
        assert_eq!(
            original::parse_legacy_present_mode(s, BALANCED, UNHINGED),
            parse_legacy_present_mode(s, BALANCED, UNHINGED),
            "{s:?}"
        )
    });
}
#[test]
#[ignore = "paired thread-cycle and allocation measurements"]
fn benchmark() {
    for (label, s) in [
        ("balanced", "BALANCED"),
        ("max", "MAX_FPS"),
        ("unhinged", "UnHinged"),
        ("invalid", "max fps"),
        ("empty", ""),
    ] {
        let label = format!("mode/{label}");
        let work = |c| {
            bb(if c {
                parse_legacy_present_mode(bb(s), bb(3u32), bb(7u32))
            } else {
                original::parse_legacy_present_mode(bb(s), bb(3u32), bb(7u32))
            });
        };
        for (c, name) in [(false, "original"), (true, "current")] {
            let (_, churn) = perf::measure(|| work(c));
            println!("{label}/{name}: allocation {churn:?}");
        }
        bench::compare(&label, 20000, work);
    }
}
#[test]
#[ignore = "deterministic complete policy option and payload bits"]
fn dump_trace() {
    let mut out = std::io::BufWriter::new(
        std::fs::File::create(std::env::var_os("DEADSYNC_LEGACY_TRACE_OUT").expect("trace path"))
            .unwrap(),
    );
    cases(17, |s| {
        out.write_all(&(s.len() as u64).to_le_bytes()).unwrap();
        out.write_all(s.as_bytes()).unwrap();
        match parse_legacy_present_mode(s, BALANCED, UNHINGED) {
            None => out.write_all(&[0]).unwrap(),
            Some(value) => {
                out.write_all(&[1]).unwrap();
                for n in value {
                    out.write_all(&n.to_le_bytes()).unwrap();
                }
            }
        }
    });
    out.flush().unwrap();
}
