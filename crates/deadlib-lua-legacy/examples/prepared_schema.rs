//! Paired native schema compilation, including complete schema preparation.
use mlua::program::{self, Binding, Bindings, Cell, Compiler, Field, FieldValue, Types};
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
    // SAFETY: changes only this disposable worker thread's CPU affinity.
    assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) }, 0);
}
#[cfg(not(windows))]
fn pin() {}
fn main() {
    pin();
    let lua = Lua::new();
    let callback: Function = lua
        .load(
            "local s={held=false};return function(e) if e.button=='Right' then s.held=not s.held end;return s.held end",
        )
        .eval()
        .unwrap();
    let mut rows = Vec::new();
    for field_count in [16, 64, 256] {
        let mut bindings = Bindings {
            arguments: vec![Binding::Object(0)],
            upvalues: vec![Binding::Object(1)],
            cells: vec![Cell::Boolean(false)],
            fields: vec![
                Field {
                    object: 0,
                    name: b"button".as_slice().into(),
                    value: FieldValue::Value(Binding::Input {
                        slot: 0,
                        types: Types::BYTES,
                    }),
                },
                Field {
                    object: 1,
                    name: b"held".as_slice().into(),
                    value: FieldValue::Value(Binding::Cell(0)),
                },
            ],
            ..Bindings::default()
        };
        for index in bindings.fields.len()..field_count {
            bindings.fields.push(Field {
                object: index as u32,
                name: format!("unused{index}").into_bytes().into(),
                value: FieldValue::Value(Binding::Literal(program::Literal::Nil)),
            });
        }
        for callbacks in [1, 16, 64] {
            for sample in 0..14 {
                let mut expected = None;
                for current in [sample % 2 == 0, sample % 2 != 0] {
                    let measure = metrics::Measurement::start();
                    let mut digest = 0usize;
                    for _ in 0..32 {
                        let compiler = if current {
                            Some(Compiler::new(black_box(&bindings)).unwrap())
                        } else {
                            None
                        };
                        for _ in 0..callbacks {
                            let program = if let Some(compiler) = &compiler {
                                compiler.compile(
                                    black_box(&callback),
                                    &bindings.arguments,
                                    &bindings.upvalues,
                                )
                            } else {
                                program::compile(black_box(&callback), black_box(&bindings))
                            }
                            .unwrap();
                            let mut machine = program.machine();
                            digest += usize::from(
                                machine
                                    .run(&[program::Datum::Bytes(b"Right")], |_, _| {})
                                    .unwrap()
                                    .is_truthy(),
                            );
                            black_box(machine);
                        }
                    }
                    let mut row = measure.finish();
                    if let Some(d) = expected {
                        assert_eq!(d, digest)
                    } else {
                        expected = Some(digest)
                    };
                    if sample == 0 {
                        continue;
                    }
                    row["current"] = current.into();
                    row["sample"] = sample.into();
                    row["operations"] = (32 * callbacks).into();
                    row["workload"] = format!("fields_{field_count}_callbacks_{callbacks}").into();
                    rows.push(row);
                }
            }
        }
    }
    std::fs::write(std::env::args().nth(1).expect("prepared_schema REPORT"),serde_json::to_vec_pretty(&serde_json::json!({"rows":rows,"validation":"Every prepared/ordinary digest is equal; measurements include schema setup and machine preparation/execution/destruction."})).unwrap()).unwrap();
}
