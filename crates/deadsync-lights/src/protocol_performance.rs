use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod allocations;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "protocol_original.rs"]
#[allow(dead_code)]
mod original;

fn random(state: &mut u64) -> u8 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as u8
}

fn meaningful(bits: u16) -> State {
    State {
        cabinet: (bits & 63) as u8,
        buttons: [((bits >> 6) & 15) as u8, ((bits >> 10) & 15) as u8],
        menu_buttons: [((bits >> 14) as u8 & 1) << 4, ((bits >> 15) as u8) << 4],
    }
}

fn inputs(mut visit: impl FnMut(State)) {
    for bits in 0..=u16::MAX {
        visit(meaningful(bits));
    }
    for fill in [0, 255] {
        for field in 0..5 {
            for value in 0..=u8::MAX {
                let mut bytes = [fill; 5];
                bytes[field] = value;
                visit(State {
                    cabinet: bytes[0],
                    buttons: [bytes[1], bytes[2]],
                    menu_buttons: [bytes[3], bytes[4]],
                });
            }
        }
    }
    let mut seed = 0x8299_3ac1_5555_4612;
    for _ in 0..50_000 {
        visit(State {
            cabinet: random(&mut seed),
            buttons: [random(&mut seed), random(&mut seed)],
            menu_buttons: [random(&mut seed), random(&mut seed)],
        });
    }
}

#[test]
fn blue_dot_matches_every_meaningful_combination_and_ignored_bits() {
    inputs(|state| {
        assert_eq!(
            hid_blue_dot::performance_reports(&state),
            original::hid_blue_dot::performance_reports(&state),
            "{state:?}"
        )
    });
}

#[test]
fn minimaid_matches_every_meaningful_combination_and_ignored_bits() {
    inputs(|state| {
        assert_eq!(
            minimaid_hid::performance_report(&state),
            original::minimaid_hid::performance_report(&state),
            "{state:?}"
        )
    });
}

#[test]
fn pacdrive_matches_both_orderings_and_ignored_bits() {
    inputs(|state| {
        for ordering in [PacDriveLightOrdering::OpenItg, PacDriveLightOrdering::Sm5] {
            assert_eq!(
                pac_drive::performance_report(&state, ordering),
                original::pac_drive::performance_report(&state, ordering),
                "{state:?}, {ordering:?}"
            );
        }
    });
}

#[test]
fn gpb_matches_all_menu_pairs_and_ignored_state_fields() {
    for first in 0..=u8::MAX {
        for second in 0..=u8::MAX {
            let state = State {
                cabinet: first ^ second,
                buttons: [first.rotate_left(1), second.rotate_left(1)],
                menu_buttons: [first, second],
            };
            assert_eq!(
                gpb::performance_report(&state),
                original::gpb::performance_report(&state)
            );
        }
    }
    inputs(|state| {
        assert_eq!(
            gpb::performance_report(&state),
            original::gpb::performance_report(&state)
        )
    });
}

#[test]
fn linux_outputs_preserve_duplicates_unmapped_outputs_and_all_state_bits() {
    assert_eq!(
        linux_leds::performance_entry_size(),
        original::linux_leds::performance_entry_size()
    );
    inputs(|state| {
        for itgio in [false, true] {
            assert_eq!(
                linux_leds::performance_outputs(&state, itgio),
                original::linux_leds::performance_outputs(&state, itgio),
                "{state:?}, itgio={itgio}"
            );
        }
    });
}

#[test]
#[ignore = "archive deterministic inputs and all protocol wire bytes"]
fn dump_protocol_trace() {
    let mut trace = Vec::new();
    inputs(|state| {
        trace.extend_from_slice(&[
            state.cabinet,
            state.buttons[0],
            state.buttons[1],
            state.menu_buttons[0],
            state.menu_buttons[1],
        ]);
        for report in hid_blue_dot::performance_reports(&state) {
            trace.extend_from_slice(&report);
        }
        trace.extend_from_slice(&minimaid_hid::performance_report(&state));
        trace.extend_from_slice(&gpb::performance_report(&state));
        for itgio in [false, true] {
            for output in linux_leds::performance_outputs(&state, itgio) {
                trace.push(match output {
                    None => 0,
                    Some(false) => 1,
                    Some(true) => 2,
                });
            }
        }
        for ordering in [PacDriveLightOrdering::OpenItg, PacDriveLightOrdering::Sm5] {
            trace.extend_from_slice(&pac_drive::performance_report(&state, ordering));
        }
    });
    std::fs::write(
        std::env::var_os("LIGHT_PROTOCOL_TRACE_OUT").expect("trace path"),
        trace,
    )
    .unwrap();
}

fn compare(label: &str, mut work: impl FnMut(bool)) {
    bench::compare(label, 1000, &mut work);
    for (variant, current) in [("original", false), ("current", true)] {
        let (_, churn) = allocations::measure(|| work(current));
        assert_eq!(
            churn.allocs
                + churn.reallocs
                + churn.frees
                + churn.allocated_bytes
                + churn.freed_bytes
                + churn.peak_bytes,
            0
        );
        println!("{label}/{variant}: allocation {churn:?}");
    }
}

#[test]
#[ignore = "paired CPU cycles, throughput and allocation benchmark"]
fn benchmark_protocols() {
    for mode in ["sparse", "dense", "mixed"] {
        let mut seed = 0x65aa_8799_1138_9432;
        let states: Vec<_> = (0..256)
            .map(|index| match mode {
                "sparse" => meaningful(1 << (index % 16)),
                "dense" => meaningful(!(1 << (index % 16))),
                _ => State {
                    cabinet: random(&mut seed),
                    buttons: [random(&mut seed), random(&mut seed)],
                    menu_buttons: [random(&mut seed), random(&mut seed)],
                },
            })
            .collect();
        for cabinet in [true, false] {
            let label = if cabinet { "blue_cabinet" } else { "blue_pad" };
            compare(&format!("{label}/{mode}/256"), |current| {
                let build = black_box(match (current, cabinet) {
                    (true, true) => {
                        hid_blue_dot::performance_cabinet_report as fn(&State) -> [u8; 3]
                    }
                    (true, false) => hid_blue_dot::performance_pad_report as fn(&State) -> [u8; 3],
                    (false, true) => {
                        original::hid_blue_dot::performance_cabinet_report as fn(&State) -> [u8; 3]
                    }
                    (false, false) => {
                        original::hid_blue_dot::performance_pad_report as fn(&State) -> [u8; 3]
                    }
                });
                for state in &states {
                    black_box(build(black_box(state)));
                }
            });
        }
        compare(&format!("minimaid/{mode}/256"), |current| {
            let build = black_box(if current {
                minimaid_hid::performance_report as fn(&State) -> [u8; 9]
            } else {
                original::minimaid_hid::performance_report as fn(&State) -> [u8; 9]
            });
            for state in &states {
                black_box(build(black_box(state)));
            }
        });
        let menu_states: Vec<_> = (0..256)
            .map(|index| {
                let mut state = states[index];
                if mode != "mixed" {
                    state.menu_buttons = if mode == "dense" { [63; 2] } else { [0; 2] };
                    state.menu_buttons[(index % 12) / 6] ^= 1 << (index % 6);
                }
                state
            })
            .collect();
        compare(&format!("gpb/{mode}/256"), |current| {
            let build = black_box(if current {
                gpb::performance_report as fn(&State) -> [u8; 17]
            } else {
                original::gpb::performance_report as fn(&State) -> [u8; 17]
            });
            for state in &menu_states {
                black_box(build(black_box(state)));
            }
        });
        for (label, itgio) in [("piuio", false), ("itgio", true)] {
            compare(&format!("linux_{label}/{mode}/256"), |current| {
                let build = black_box(if current {
                    linux_leds::performance_outputs as fn(&State, bool) -> [Option<bool>; 32]
                } else {
                    original::linux_leds::performance_outputs
                        as fn(&State, bool) -> [Option<bool>; 32]
                });
                for state in &states {
                    black_box(build(black_box(state), black_box(itgio)));
                }
            });
        }
        for (label, ordering) in [
            ("pac_openitg", PacDriveLightOrdering::OpenItg),
            ("pac_sm5", PacDriveLightOrdering::Sm5),
        ] {
            compare(&format!("{label}/{mode}/256"), |current| {
                let build = black_box(if current {
                    pac_drive::performance_report as fn(&State, PacDriveLightOrdering) -> [u8; 5]
                } else {
                    original::pac_drive::performance_report
                        as fn(&State, PacDriveLightOrdering) -> [u8; 5]
                });
                for state in &states {
                    black_box(build(black_box(state), black_box(ordering)));
                }
            });
        }
    }
}
