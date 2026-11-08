//! Paired authored-Lua/native callback execution, loading and retained payload.
use mlua::program::{
    self, Action, Binding, Bindings, Cell, Datum, Field, FieldValue, Machine, Parameter, Types,
};
use mlua::{Function, InspectionValue, Lua, Table, Value};
use std::{
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    },
};
#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;

#[derive(Default)]
struct Output {
    calls: AtomicU64,
    visible: AtomicBool,
}
struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Relaxed);
    }
}
struct Live {
    lua: Lua,
    callback: Function,
    dropped: Arc<AtomicBool>,
    actor: usize,
}
enum Prepared {
    Live(Live),
    Native(Machine),
}

const EVENTS: [(&str, &str, f64); 8] = [
    ("Left", "InputEventType_FirstPress", 0.5),
    ("Right", "InputEventType_FirstPress", 1.0),
    ("Right", "InputEventType_Repeat", 0.25),
    ("Right", "InputEventType_Release", 0.0),
    ("Left", "InputEventType_Repeat", 0.5),
    ("Right", "InputEventType_FirstPress", 1.0),
    ("Right", "InputEventType_Release", 0.0),
    ("Other", "InputEventType_Release", 0.0),
];

fn load(kind: usize, output: Arc<Output>) -> Live {
    let lua = Lua::new();
    let dropped = Arc::new(AtomicBool::new(false));
    lua.set_app_data(DropFlag(dropped.clone()));
    let actor = lua.create_table().unwrap();
    let visible = lua
        .create_function({
            let actor = actor.clone();
            move |_, (_, value): (Table, bool)| {
                output.visible.store(value, Relaxed);
                output.calls.fetch_add(1, Relaxed);
                Ok(actor.clone())
            }
        })
        .unwrap();
    actor.set("visible", visible.clone()).unwrap();
    let body = match kind {
        0 => {
            "if e.button=='Right' then actor:visible(e.type~='InputEventType_Release');return true end;return false"
        }
        1 => {
            "if e.button=='Right' then shared.held=not alias.held;shared.count=alias.count+e.level;actor:visible(shared.held);return shared.count>8 end;return false"
        }
        2 => {
            "if e.button=='Right' then held=not held;if e.type=='InputEventType_Release' then held=false end;actor:visible(held);return held end;return false"
        }
        _ => unreachable!(),
    };
    let factory:Function=lua.load(format!("return function(actor) local shared={{held=false,count=0}};local alias=shared;local held=false;return function(e) {body} end end")).set_name("@compiled-program-fixture.lua").eval().unwrap();
    let callback: Function = factory.call(actor.clone()).unwrap();
    assert!(actor.metatable().is_none());
    assert_eq!(actor.raw_get::<Function>("visible").unwrap(), visible);
    Live {
        lua,
        callback,
        dropped,
        actor: actor.to_pointer() as usize,
    }
}

fn lower(live: &Live) -> Machine {
    let mut bindings = Bindings {
        arguments: vec![Binding::Object(0)],
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
                object: 0,
                name: b"type".as_slice().into(),
                value: FieldValue::Value(Binding::Input {
                    slot: 1,
                    types: Types::BYTES,
                }),
            },
            Field {
                object: 0,
                name: b"level".as_slice().into(),
                value: FieldValue::Value(Binding::Input {
                    slot: 2,
                    types: Types::NUMBER,
                }),
            },
            Field {
                object: 1,
                name: b"visible".as_slice().into(),
                value: FieldValue::Action(Action {
                    tag: 0,
                    parameters: vec![Parameter::Object(1), Parameter::Type(Types::BOOLEAN)].into(),
                    result: Some(Binding::Object(1)),
                }),
            },
        ],
        ..Bindings::default()
    };
    let mut shared = None;
    live.callback
        .for_each_upvalue_as::<InspectionValue>(|_, name, value| {
            bindings.upvalues.push(match (name, value) {
                (b"actor", InspectionValue::Table(table)) => {
                    assert_eq!(table.to_pointer() as usize, live.actor);
                    Binding::Object(1)
                }
                (b"shared" | b"alias", InspectionValue::Table(table)) => {
                    if let Some(pointer) = shared {
                        assert_eq!(table.to_pointer() as usize, pointer);
                    } else {
                        assert!(table.metatable().is_none());
                        let held = table.raw_get::<bool>("held")?;
                        let count = table.raw_get::<f64>("count")?;
                        let slot = bindings.cells.len();
                        bindings.cells.extend([Cell::Boolean(held), Cell::Number(count)]);
                        bindings.fields.extend([
                            Field {
                                object: 2,
                                name: b"held".as_slice().into(),
                                value: FieldValue::Value(Binding::Cell(slot)),
                            },
                            Field {
                                object: 2,
                                name: b"count".as_slice().into(),
                                value: FieldValue::Value(Binding::Cell(slot + 1)),
                            },
                        ]);
                        shared = Some(table.to_pointer() as usize);
                    }
                    Binding::Object(2)
                }
                (b"held", InspectionValue::Boolean(value)) => {
                    let slot = bindings.cells.len();
                    bindings.cells.push(Cell::Boolean(value));
                    Binding::Cell(slot)
                }
                _ => panic!("fixture capture schema changed"),
            });
            Ok(())
        })
        .unwrap();
    program::compile(&live.callback, &bindings).unwrap().machine()
}

fn event(lua: &Lua, index: usize) -> Table {
    let (button, kind, level) = EVENTS[index];
    let t = lua.create_table().unwrap();
    t.set("button", button).unwrap();
    t.set("type", kind).unwrap();
    t.set("level", level).unwrap();
    t
}
fn native(machine: &mut Machine, output: &Output, index: usize) -> bool {
    let (button, kind, level) = EVENTS[index];
    let frame = [
        Datum::Bytes(button.as_bytes()),
        Datum::Bytes(kind.as_bytes()),
        Datum::Number(level),
    ];
    machine
        .run(&frame, |tag, args| {
            assert_eq!(tag, 0);
            let Some(Datum::Boolean(value)) = args.get(1) else {
                panic!("action contract")
            };
            output.visible.store(value, Relaxed);
            output.calls.fetch_add(1, Relaxed);
        })
        .unwrap()
        .is_truthy()
}
fn pin() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut std::ffi::c_void;
            fn SetThreadAffinityMask(t: *mut std::ffi::c_void, m: usize) -> usize;
        }
        // SAFETY: only this disposable benchmark worker's affinity changes.
        assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) }, 0);
    }
}
fn compare(name: &str, batch: usize, mut work: impl FnMut(bool) -> u64, rows: &mut Vec<serde_json::Value>) {
    for sample in 0..10 {
        let mut expected = None;
        for current in [sample % 2 != 0, sample % 2 == 0] {
            let measure = metrics::Measurement::start();
            let mut digest = 0_u64;
            for _ in 0..batch {
                digest = digest
                    .wrapping_mul(0x100000001b3)
                    .wrapping_add(black_box(work(black_box(current))));
            }
            let mut row = measure.finish();
            if let Some(old) = expected {
                assert_eq!(old, digest);
            } else {
                expected = Some(digest);
            }
            if sample == 0 {
                continue;
            }
            row["workload"] = name.into();
            row["current"] = current.into();
            row["operations"] = batch.into();
            row["sample"] = sample.into();
            rows.push(row);
        }
    }
}

fn main() {
    pin();
    let mut rows = Vec::new();
    let mut memory = Vec::new();
    for kind in 0..3 {
        let before = Arc::new(Output::default());
        let after = Arc::new(Output::default());
        let live = load(kind, before.clone());
        live.lua.gc_collect().unwrap();
        let lua_bytes = live.lua.used_memory();
        let native_live = load(kind, Arc::new(Output::default()));
        let mut machine = lower(&native_live);
        let dropped = native_live.dropped.clone();
        drop(native_live);
        assert!(dropped.load(Relaxed));
        memory.push(serde_json::json!({"kind":kind,"lua_used_memory_lower_bound":lua_bytes,"native_owned_payload":machine.retained_bytes(),"loading_vm_destroyed":dropped.load(Relaxed)}));
        let events: Vec<_> = (0..EVENTS.len()).map(|i| event(&live.lua, i)).collect();
        for _ in 0..4 {
            for (index, event) in events.iter().enumerate() {
                let returned = live.callback.call::<Value>(event.clone()).unwrap();
                assert_eq!(
                    !matches!(returned, Value::Nil | Value::Boolean(false)),
                    native(&mut machine, &after, index)
                );
                assert_eq!(before.calls.load(Relaxed), after.calls.load(Relaxed));
                assert_eq!(before.visible.load(Relaxed), after.visible.load(Relaxed));
            }
        }
        for constructed in [false, true] {
            compare(
                &format!(
                    "rule_{kind}_{}",
                    if constructed {
                        "new_event"
                    } else {
                        "prepared_event"
                    }
                ),
                256,
                |current| {
                    let mut digest = 0_u64;
                    for (index, prepared) in events.iter().enumerate() {
                        let handled = if current {
                            native(&mut machine, &after, index)
                        } else {
                            let e = if constructed {
                                event(&live.lua, index)
                            } else {
                                prepared.clone()
                            };
                            !matches!(
                                live.callback.call::<Value>(e).unwrap(),
                                Value::Nil | Value::Boolean(false)
                            )
                        };
                        digest = digest.rotate_left(1) ^ u64::from(handled);
                    }
                    digest
                },
                &mut rows,
            );
            assert_eq!(before.calls.load(Relaxed), after.calls.load(Relaxed));
            assert_eq!(before.visible.load(Relaxed), after.visible.load(Relaxed));
        }
        compare(
            &format!("rule_{kind}_native_lower_prepare_destroy"),
            1024,
            |_| {
                black_box(lower(&live));
                0
            },
            &mut rows,
        );
        for sample in 0..10 {
            for current in [sample % 2 != 0, sample % 2 == 0] {
                let mut prepared = Vec::with_capacity(32);
                let measure = metrics::Measurement::start();
                for _ in 0..32 {
                    let fixture = load(kind, Arc::new(Output::default()));
                    prepared.push(if current {
                        let machine = lower(&fixture);
                        drop(fixture);
                        Prepared::Native(machine)
                    } else {
                        Prepared::Live(fixture)
                    });
                }
                let mut row = measure.finish();
                // Touch retained artifacts after timing; final destruction is excluded.
                for artifact in &prepared {
                    match artifact {
                        Prepared::Live(v) => {
                            black_box(&v.callback);
                        }
                        Prepared::Native(v) => {
                            black_box(v.retained_bytes());
                        }
                    }
                }
                drop(prepared);
                if sample == 0 {
                    continue;
                }
                row["workload"] = format!("rule_{kind}_load_prepare").into();
                row["current"] = current.into();
                row["operations"] = 32.into();
                row["sample"] = sample.into();
                rows.push(row);
            }
        }
    }
    // A real large-register body with a cheap early-return input branch. Both
    // flags run the same native code; compare separately preserved binaries.
    let mut sparse = {
        let lua = Lua::new();
        let names = (0..190).map(|i| format!("n{i}")).collect::<Vec<_>>().join(",");
        let values = vec!["e.level"; 190].join(",");
        let callback: Function = lua.load(format!("return function(e) if e.button~='Right' then return false end;local {names}={values};return n189>0 end")).eval().unwrap();
        let fields = [("button", 0, Types::BYTES), ("level", 1, Types::NUMBER)]
            .into_iter()
            .map(|(name, slot, types)| Field {
                object: 0,
                name: name.as_bytes().into(),
                value: FieldValue::Value(Binding::Input { slot, types }),
            })
            .collect();
        program::compile(
            &callback,
            &Bindings {
                arguments: vec![Binding::Object(0)],
                fields,
                ..Bindings::default()
            },
        )
        .unwrap()
        .machine()
    };
    compare(
        "sparse_early_return_native",
        4096,
        |_| {
            let frame = [Datum::Bytes(b"Left"), Datum::Number(1.0)];
            let result = sparse.run(&frame, |_, _| unreachable!()).unwrap().is_truthy();
            assert!(!result);
            u64::from(!result)
        },
        &mut rows,
    );
    let lua = Lua::new();
    let f:Function=lua.load("local a,b,c,d,e,f,g,h='one','two','three','four','five','six','seven','eight';return function() return a,b,c,d,e,f,g,h end").eval().unwrap();
    compare(
        "inspect_8_owned_strings",
        4096,
        |current| {
            let mut sum = 0;
            if current {
                f.for_each_upvalue_as::<InspectionValue>(|_, _, value| {
                    let InspectionValue::Bytes(bytes) = value else {
                        panic!("string")
                    };
                    sum += bytes.len() as u64;
                    Ok(())
                })
                .unwrap();
            } else {
                f.for_each_upvalue(|_, _, value| {
                    let Value::String(value) = value else {
                        panic!("string")
                    };
                    let bytes: Box<[u8]> = value.as_bytes().as_ref().into();
                    sum += bytes.len() as u64;
                    black_box(bytes);
                    Ok(())
                })
                .unwrap();
            }
            sum
        },
        &mut rows,
    );
    std::fs::write(std::env::args().nth(1).expect("compiled_program REPORT"),serde_json::to_vec_pretty(&serde_json::json!({"rows":rows,"retained_memory":memory,"validation":"input branches/actions/stateful sequences match authored Lua; each native program's loading VM is destroyed"})).unwrap()).unwrap();
}
