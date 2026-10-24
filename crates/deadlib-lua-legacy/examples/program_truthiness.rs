//! Paired predicate execution, including ordinary result conversion.
use mlua::program::{self, Binding, Bindings, Datum, Group, Types};
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
    // SAFETY: changes only this disposable benchmark thread's affinity.
    assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) }, 0);
}
#[cfg(not(windows))]
fn pin() {}

fn main() {
    pin();
    let mut rows = Vec::new();
    let mut payloads = Vec::new();
    for (name, types, values) in [
        (
            "nullable_scalars",
            Types::NIL | Types::BOOLEAN | Types::NUMBER | Types::BYTES,
            [
                Datum::Nil,
                Datum::Boolean(false),
                Datum::Number(-0.0),
                Datum::Bytes(b"\xff\0"),
            ],
        ),
        (
            "bytes",
            Types::BYTES,
            [
                Datum::Bytes(b""),
                Datum::Bytes(b"a"),
                Datum::Bytes(b"\xff\0"),
                Datum::Bytes(b"runtime input"),
            ],
        ),
    ] {
        let program = {
            let lua = Lua::new();
            let callback: Function = lua
                .load("return function(value) return value end")
                .eval()
                .unwrap();
            program::compile(
                &callback,
                &Bindings {
                    arguments: vec![Binding::Input { slot: 0, types }],
                    ..Bindings::default()
                },
            )
            .unwrap()
        };
        let mut machine = program.machine();
        let mut group = Group::new(vec![program]).unwrap();
        payloads.push(serde_json::json!({"fixture":name,"machine":machine.retained_bytes(),"group":group.retained_bytes()}));
        for grouped in [false, true] {
            for sample in 0..14 {
                let mut expected = None;
                for current in [sample % 2 == 0, sample % 2 != 0] {
                    let measure = metrics::Measurement::start();
                    let mut digest = 0usize;
                    for index in 0..1_048_576 {
                        let input = [black_box(values[index & 3])];
                        let value = if grouped {
                            if current {
                                black_box(&mut group)
                                    .run_truthy(0, &input, |_, _| unreachable!())
                                    .unwrap()
                                    .unwrap()
                            } else {
                                black_box(&mut group)
                                    .run(0, &input, |_, _| unreachable!())
                                    .unwrap()
                                    .unwrap()
                                    .is_truthy()
                            }
                        } else if current {
                            black_box(&mut machine)
                                .run_truthy(&input, |_, _| unreachable!())
                                .unwrap()
                        } else {
                            black_box(&mut machine)
                                .run(&input, |_, _| unreachable!())
                                .unwrap()
                                .is_truthy()
                        };
                        digest += usize::from(value);
                    }
                    let mut row = measure.finish();
                    black_box(digest);
                    if let Some(value) = expected {
                        assert_eq!(value, digest);
                    } else {
                        expected = Some(digest);
                    }
                    assert_eq!(digest, if name == "bytes" { 1_048_576 } else { 524_288 });
                    if sample == 0 {
                        continue;
                    }
                    row["fixture"] = name.into();
                    row["grouped"] = grouped.into();
                    row["current"] = current.into();
                    row["sample"] = sample.into();
                    row["operations"] = 1_048_576.into();
                    rows.push(row);
                }
            }
        }
    }
    std::fs::write(std::env::args().nth(1).expect("program_truthiness REPORT"), serde_json::to_vec_pretty(&serde_json::json!({"rows":rows,"payloads":payloads,"validation":"Paired digests and Lua truthiness of nil/false/zero/empty and binary bytes are checked; all timed calls outlive the loading VM."})).unwrap()).unwrap();
}
