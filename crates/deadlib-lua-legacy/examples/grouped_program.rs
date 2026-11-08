//! Shared scratch preparation/execution against equivalent independent machines.
use mlua::program::{self, Binding, Bindings, Datum, Field, FieldValue, Group, Types};
use mlua::{Function, Lua};
use std::hint::black_box;
#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;

#[cfg(windows)]
fn pin() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    // SAFETY: changes only this disposable benchmark worker's affinity.
    assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) }, 0);
}
#[cfg(not(windows))]
fn pin() {}

fn main() {
    pin();
    let program = {
        let lua = Lua::new();
        let callback: Function = lua
            .load("return function(e)return e.button=='Right' and e.down end")
            .eval()
            .unwrap();
        let fields =
            [("button", 0, Types::BYTES), ("down", 1, Types::BOOLEAN)].map(|(name, slot, types)| Field {
                object: 0,
                name: name.as_bytes().into(),
                value: FieldValue::Value(Binding::Input { slot, types }),
            });
        program::compile(
            &callback,
            &Bindings {
                arguments: vec![Binding::Object(0)],
                fields: fields.into(),
                ..Bindings::default()
            },
        )
        .unwrap()
    };
    let mut rows = Vec::new();
    let mut memory = Vec::new();
    for count in [1, 16, 64, 256] {
        let mut machines = (0..count).map(|_| program.machine()).collect::<Vec<_>>();
        let mut group = Group::new(vec![program.clone(); count]).unwrap();
        // Both configurations share the same immutable code allocation.
        let before_bytes = std::mem::size_of_val(&machines)
            + machines
                .iter()
                .map(|m| m.retained_bytes() - program.retained_bytes())
                .sum::<usize>()
            + program.retained_bytes();
        memory.push(serde_json::json!({"callbacks":count,"independent_bytes":before_bytes,"group_bytes":group.retained_bytes()}));
        for sample in 0..14 {
            let mut expected = None;
            for current in [sample % 2 == 0, sample % 2 != 0] {
                let measure = metrics::Measurement::start();
                let mut digest = 0usize;
                for repeat in 0..1024 {
                    let inputs = [
                        Datum::Bytes(if repeat % 2 == 0 { b"Left" } else { b"Right" }),
                        Datum::Boolean(repeat % 3 == 0),
                    ];
                    if current {
                        for index in 0..group.len() {
                            digest += usize::from(
                                group
                                    .run(index, black_box(&inputs), |_, _| {})
                                    .unwrap()
                                    .unwrap()
                                    .is_truthy(),
                            );
                        }
                    } else {
                        for machine in &mut machines {
                            digest +=
                                usize::from(machine.run(black_box(&inputs), |_, _| {}).unwrap().is_truthy());
                        }
                    }
                }
                let mut row = measure.finish();
                black_box(digest);
                if let Some(value) = expected {
                    assert_eq!(value, digest);
                } else {
                    expected = Some(digest);
                }
                if sample == 0 {
                    continue;
                }
                row["workload"] = format!("execute_{count}").into();
                row["current"] = current.into();
                row["operations"] = (1024 * count).into();
                row["sample"] = sample.into();
                rows.push(row);
            }
        }
        for sample in 0..14 {
            for current in [sample % 2 == 0, sample % 2 != 0] {
                let measure = metrics::Measurement::start();
                for _ in 0..1024 {
                    if current {
                        black_box(Group::new(vec![program.clone(); count]).unwrap());
                    } else {
                        black_box((0..count).map(|_| program.machine()).collect::<Vec<_>>());
                    }
                }
                let mut row = measure.finish();
                if sample == 0 {
                    continue;
                }
                row["workload"] = format!("prepare_{count}").into();
                row["current"] = current.into();
                row["operations"] = 1024.into();
                row["sample"] = sample.into();
                rows.push(row);
            }
        }
    }
    std::fs::write(
        std::env::args().nth(1).expect("grouped_program REPORT"),
        serde_json::to_vec_pretty(&serde_json::json!({"rows":rows,"memory":memory})).unwrap(),
    )
    .unwrap();
}
