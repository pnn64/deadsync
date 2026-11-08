use deadsync_profile::pad_config::{PadConfigProfile, resolve};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/pad_lookup_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn profile(index: usize, serials: usize) -> PadConfigProfile {
    PadConfigProfile {
        name: format!("Profile {index}"),
        backend: "SMX".to_owned(),
        pad_type: Some("FSR".to_owned()),
        serial: None,
        default_for_serials: (0..serials).map(|i| format!("other-{index}-{i}")).collect(),
        global_default: false,
        settings: Vec::new(),
    }
}

fn check(profiles: &[PadConfigProfile], backend: &str, pad_type: Option<&str>, serial: &str) {
    let actual = resolve(profiles, backend, pad_type, serial);
    let expected = original::resolve(profiles, backend, pad_type, serial);
    assert_eq!(
        actual.map(std::ptr::from_ref),
        expected.map(std::ptr::from_ref)
    );
}

#[test]
fn lookup_preserves_reference_identity_compatibility_and_default_priority() {
    let mut state = 0x243f6a88u32;
    for count in [0, 1, 2, 8, 32] {
        for _ in 0..128 {
            let profiles: Vec<_> = (0..count)
                .map(|i| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    let mut p = profile(i, i % 4);
                    p.backend =
                        ["", "SMX", "smx", "RawInput", "\u{96ea}"][state as usize % 5].to_owned();
                    p.pad_type = [None, Some(""), Some("FSR"), Some("LCS")]
                        [(state >> 5) as usize % 4]
                        .map(str::to_owned);
                    p.global_default = state & 0x1000 != 0;
                    if state & 0x2000 != 0 {
                        p.default_for_serials.push("wanted".to_owned());
                    }
                    if state & 0x4000 != 0 {
                        p.default_for_serials.push(String::new());
                    }
                    p
                })
                .collect();
            for backend in ["", "SMX", "smx", "RawInput", "\u{96ea}"] {
                for pad in [None, Some(""), Some("FSR"), Some("LCS")] {
                    for serial in ["", "wanted", "missing", "other-0-0", "\u{96ea}"] {
                        check(&profiles, backend, pad, serial);
                    }
                }
            }
        }
    }
}

#[test]
fn later_serial_beats_first_global_and_first_matching_serial_wins() {
    let mut profiles: Vec<_> = (0..8).map(|i| profile(i, 2)).collect();
    profiles[0].global_default = true;
    profiles[1].global_default = true;
    profiles[2].default_for_serials.push("wanted".to_owned());
    profiles[4].default_for_serials.push("wanted".to_owned());
    let found = resolve(&profiles, "SMX", Some("FSR"), "wanted").unwrap();
    assert!(std::ptr::eq(found, &profiles[2]));
    let found = resolve(&profiles, "SMX", Some("FSR"), "missing").unwrap();
    assert!(std::ptr::eq(found, &profiles[0]));
    profiles[0].pad_type = Some("LCS".to_owned());
    check(&profiles, "SMX", Some("FSR"), "missing");
    check(&profiles, "SMX", None, "missing");
}

fn measure(label: &str, repetitions: usize, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, repetitions, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_profile_lookup_growth() {
    for count in [1, 16, 128] {
        for kind in [
            "serialfirst",
            "seriallast",
            "globalfirst",
            "globallast",
            "missing",
        ] {
            let mut profiles: Vec<_> = (0..count).map(|i| profile(i, 2)).collect();
            match kind {
                "serialfirst" => profiles[0].default_for_serials.push("wanted".to_owned()),
                "seriallast" => profiles[count - 1]
                    .default_for_serials
                    .push("wanted".to_owned()),
                "globalfirst" => profiles[0].global_default = true,
                "globallast" => profiles[count - 1].global_default = true,
                _ => {}
            }
            measure(&format!("resolve/{kind}{count}"), 64, || {
                for _ in 0..64 {
                    black_box(resolve(
                        black_box(&profiles),
                        black_box("SMX"),
                        black_box(Some("FSR")),
                        black_box("wanted"),
                    ));
                }
            });
        }
    }
    for kind in ["empty", "incompatible", "dense"] {
        let mut profiles: Vec<_> = (0..if kind == "empty" { 0 } else { 128 })
            .map(|i| profile(i, if kind == "dense" { 16 } else { 2 }))
            .collect();
        if kind == "incompatible" {
            for profile in &mut profiles {
                profile.backend = "RawInput".to_owned();
            }
        }
        measure(&format!("resolve/{kind}"), 64, || {
            for _ in 0..64 {
                black_box(resolve(
                    black_box(&profiles),
                    black_box("SMX"),
                    black_box(Some("FSR")),
                    black_box("wanted"),
                ));
            }
        });
    }
}
