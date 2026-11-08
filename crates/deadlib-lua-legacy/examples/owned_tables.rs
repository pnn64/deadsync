//! Compare authored Lua with an owned byte-keyed state program.
//! Field names, source and events come from a caller-supplied JSON fixture.
use mlua::program::{self, Binding, Bindings, Datum, Field, FieldValue, Types};
use mlua::{Function, Lua, Table, Value};
use serde_json::{Value as Json, json};
use std::{
    hint::black_box,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;

#[cfg(windows)]
fn pin() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(t: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    // SAFETY: pins only this disposable benchmark worker to logical CPU 2.
    assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) }, 0);
}
#[cfg(not(windows))]
fn pin() {}
struct Closed(Arc<AtomicBool>);
impl Drop for Closed {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
fn mix(value: u64, next: u64) -> u64 {
    value.rotate_left(7).wrapping_add(next.wrapping_add(3))
}
fn observed(digest: &AtomicU64, value: bool) {
    digest
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            Some(mix(old, u64::from(value)))
        })
        .unwrap();
}

fn datum(value: &Json) -> Datum<'_> {
    match value {
        Json::Null => Datum::Nil,
        Json::Bool(v) => Datum::Boolean(*v),
        Json::Number(v) => Datum::Number(v.as_f64().unwrap()),
        Json::String(v) => Datum::Bytes(v.as_bytes()),
        _ => panic!("fixture inputs must be scalar JSON values"),
    }
}
fn types(value: &Json) -> Types {
    match datum(value) {
        Datum::Nil => Types::NIL,
        Datum::Boolean(_) => Types::BOOLEAN,
        Datum::Number(_) => Types::NUMBER,
        Datum::Bytes(_) => Types::BYTES,
        _ => unreachable!(),
    }
}
fn returned(value: Value) -> u64 {
    match value {
        Value::Nil => 0,
        Value::Boolean(false) => 1,
        Value::Boolean(true) => 2,
        _ => panic!("fixture result must be Boolean/nil"),
    }
}
fn native_return(value: Datum<'_>) -> u64 {
    match value {
        Datum::Nil => 0,
        Datum::Boolean(false) => 1,
        Datum::Boolean(true) => 2,
        _ => panic!("fixture result must be Boolean/nil"),
    }
}

struct Loaded {
    callback: Function,
    events: Vec<Table>,
    maps: Vec<(u32, Table)>,
    arrays: Vec<(u32, Table)>,
    actions: Vec<(usize, u32)>,
    program: Option<program::Program>,
    lua: Lua,
}
fn capture(
    table: Table,
    schema: &mut Bindings,
    maps: &mut Vec<(u32, Table)>,
    arrays: &mut Vec<(u32, Table)>,
) -> u32 {
    assert!(table.metatable().is_none());
    if let Some((object, _)) = maps
        .iter()
        .chain(arrays.iter())
        .find(|(_, previous)| previous.to_pointer() == table.to_pointer())
    {
        return *object;
    }
    let object = 100 + (maps.len() + arrays.len()) as u32;
    if table.raw_len() > 0 {
        let count = table.raw_len();
        assert!(count <= 4096);
        let mut children = vec![None; count];
        table
            .for_each::<mlua::InspectionValue, mlua::InspectionValue>(|key, value| {
                let index = match key {
                    mlua::InspectionValue::Integer(i) if i >= 1 && i as u64 <= count as u64 => i as usize,
                    mlua::InspectionValue::Number(i) if i >= 1.0 && i <= count as f64 && i.fract() == 0.0 => {
                        i as usize
                    }
                    _ => panic!("fixture arrays need dense numeric keys"),
                };
                let mlua::InspectionValue::Table(child) = value else {
                    panic!("fixture array elements must be tables")
                };
                children[index - 1] = Some(child);
                Ok(())
            })
            .unwrap();
        arrays.push((object, table));
        let references = children
            .into_iter()
            .map(|child| capture(child.expect("dense array"), schema, maps, arrays))
            .collect();
        schema.boolean_table_arrays.push(program::BooleanTableArray {
            object,
            tables: references,
        });
        return object;
    }
    let mut entries = Vec::new();
    table
        .for_each::<mlua::InspectionValue, mlua::InspectionValue>(|key, value| {
            let (mlua::InspectionValue::Bytes(key), mlua::InspectionValue::Boolean(value)) = (key, value)
            else {
                panic!("fixture state must have byte keys and Boolean values")
            };
            entries.push((key, value));
            Ok(())
        })
        .unwrap();
    entries.sort();
    schema
        .boolean_tables
        .push(program::BooleanTable { object, entries });
    maps.push((object, table));
    object
}
fn bind(loaded: &mut Loaded, fixture: &Json) -> Bindings {
    let mut schema = Bindings {
        arguments: vec![Binding::Object(0)],
        ..Bindings::default()
    };
    let fields = fixture["fields"].as_array().unwrap();
    let events = fixture["events"].as_array().unwrap();
    for (slot, name) in fields.iter().enumerate() {
        let mut accepted = types(&events[0][slot]);
        for event in &events[1..] {
            accepted |= types(&event[slot]);
        }
        schema.fields.push(Field {
            object: 0,
            name: name.as_str().unwrap().as_bytes().to_vec().into(),
            value: FieldValue::Value(Binding::Input {
                slot,
                types: accepted,
            }),
        });
    }
    for action in fixture["actions"].as_array().unwrap() {
        let object = action["object"].as_u64().unwrap() as u32;
        schema.fields.push(Field {
            object,
            name: action["field"].as_str().unwrap().as_bytes().to_vec().into(),
            value: FieldValue::Action(program::Action {
                tag: 3,
                parameters: vec![
                    program::Parameter::Receiver,
                    program::Parameter::Type(Types::BOOLEAN),
                ]
                .into(),
                result: Some(Binding::Object(object)),
            }),
        });
    }
    loaded
        .callback
        .for_each_upvalue(|_, _, value| {
            let Value::Table(table) = value else {
                panic!("fixture captures must be declared tables")
            };
            let object = loaded
                .actions
                .iter()
                .find(|(pointer, _)| *pointer == table.to_pointer() as usize)
                .map(|(_, object)| *object)
                .unwrap_or_else(|| capture(table, &mut schema, &mut loaded.maps, &mut loaded.arrays));
            schema.upvalues.push(Binding::Object(object));
            Ok(())
        })
        .unwrap();
    schema
}
fn load(fixture: &Json, native: bool, closed: Arc<AtomicBool>, digest: Arc<AtomicU64>) -> Loaded {
    let lua = Lua::new();
    lua.set_app_data(Closed(closed));
    let root: Value = lua.load(fixture["source"].as_str().unwrap()).eval().unwrap();
    let callback = match root {
        Value::Function(f) => f,
        Value::Table(t) => t.raw_get::<Function>(fixture["entry"].as_str().unwrap()).unwrap(),
        _ => panic!("fixture needs a function or module"),
    };
    let mut actions = Vec::new();
    callback
        .for_each_upvalue(|_, name, value| {
            if let Some(action) = fixture["actions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["capture"].as_str().unwrap().as_bytes() == name)
            {
                let Value::Table(table) = value else {
                    panic!("host object must be a table")
                };
                let object = action["object"].as_u64().unwrap() as u32;
                actions.push((table.to_pointer() as usize, object));
                let digest = digest.clone();
                table
                    .raw_set(
                        action["field"].as_str().unwrap(),
                        lua.create_function(move |_, (owner, value): (Table, bool)| {
                            observed(&digest, value);
                            Ok(owner)
                        })
                        .unwrap(),
                    )
                    .unwrap();
            }
            Ok(())
        })
        .unwrap();
    let events = fixture["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|values| {
            let event = lua.create_table().unwrap();
            for (name, value) in fixture["fields"]
                .as_array()
                .unwrap()
                .iter()
                .zip(values.as_array().unwrap())
            {
                let value = match datum(value) {
                    Datum::Nil => Value::Nil,
                    Datum::Boolean(v) => Value::Boolean(v),
                    Datum::Number(v) => Value::Number(v),
                    Datum::Bytes(v) => Value::String(lua.create_string(v).unwrap()),
                    _ => unreachable!(),
                };
                event.raw_set(name.as_str().unwrap(), value).unwrap();
            }
            event
        })
        .collect();
    let mut loaded = Loaded {
        callback,
        events,
        maps: Vec::new(),
        arrays: Vec::new(),
        actions,
        program: None,
        lua,
    };
    if native {
        let schema = bind(&mut loaded, fixture);
        loaded.program = Some(program::compile(&loaded.callback, &schema).unwrap());
    }
    loaded
}
fn lua_state(loaded: &Loaded) -> Json {
    let rows = loaded
        .maps
        .iter()
        .map(|(object, table)| {
            let mut entries = Vec::new();
            table
                .for_each::<mlua::LuaString, bool>(|key, value| {
                    entries.push((key.as_bytes().to_vec(), value));
                    Ok(())
                })
                .unwrap();
            entries.sort();
            (*object, entries)
        })
        .collect::<Vec<_>>();
    json!(rows)
}
fn native_state(machine: &program::Machine, objects: &[u32]) -> Json {
    let rows = objects
        .iter()
        .map(|object| {
            let mut entries = machine
                .boolean_entries(*object)
                .unwrap()
                .map(|(key, value)| (key.to_vec(), value))
                .collect::<Vec<_>>();
            entries.sort();
            (*object, entries)
        })
        .collect::<Vec<_>>();
    json!(rows)
}
fn frame(values: &Json) -> ([Datum<'_>; 16], usize) {
    let values = values.as_array().unwrap();
    assert!(values.len() <= 16);
    let mut inputs = [Datum::Nil; 16];
    for (dst, value) in inputs.iter_mut().zip(values) {
        *dst = datum(value);
    }
    (inputs, values.len())
}
fn native_cycle(machine: &mut program::Machine, events: &[Json], digest: &AtomicU64) -> u64 {
    let mut result = 0;
    for values in events {
        let (input, len) = frame(values);
        let value = machine
            .run(&input[..len], |tag, args| {
                assert_eq!(tag, 3);
                let Some(Datum::Boolean(value)) = args.get(1) else {
                    panic!("invalid action")
                };
                observed(digest, value);
            })
            .unwrap();
        result = mix(result, native_return(value));
    }
    result
}
fn main() {
    pin();
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    assert_eq!(args.len(), 3, "owned_tables FIXTURE REPORT lua|native");
    let mut fixture: Json = serde_json::from_slice(&std::fs::read(&args[0]).unwrap()).unwrap();
    if let Some(source) = fixture["source_path"].as_str() {
        let path = Path::new(&args[0]).parent().unwrap().join(source);
        fixture["source"] = std::fs::read_to_string(path).unwrap().into();
    }
    let events = fixture["events"].as_array().unwrap();
    let native = args[2] == "native";
    assert!(native || args[2] == "lua");
    let closed = Arc::new(AtomicBool::new(false));
    let digest = Arc::new(AtomicU64::new(0));
    let mut rows = Vec::new();
    let mut expected_digests = Vec::new();
    let (program, objects, final_state, lua_bytes) = {
        let loaded = load(&fixture, true, closed.clone(), digest.clone());
        let program = loaded.program.as_ref().unwrap().clone();
        let objects = loaded.maps.iter().map(|(object, _)| *object).collect::<Vec<_>>();
        let mut machine = program.machine();
        for (index, event) in loaded.events.iter().enumerate() {
            digest.store(0, Ordering::Relaxed);
            let expected = returned(loaded.callback.call::<Value>(event).unwrap());
            let actions = digest.load(Ordering::Relaxed);
            digest.store(0, Ordering::Relaxed);
            let (input, len) = frame(&events[index]);
            let actual = machine
                .run(&input[..len], |_, args| {
                    let Some(Datum::Boolean(v)) = args.get(1) else {
                        panic!()
                    };
                    observed(&digest, v);
                })
                .unwrap();
            assert_eq!(native_return(actual), expected);
            assert_eq!(digest.load(Ordering::Relaxed), actions);
            assert_eq!(native_state(&machine, &objects), lua_state(&loaded));
        }
        let final_state = lua_state(&loaded);
        if !native {
            for sample in 0..10 {
                digest.store(0, Ordering::Relaxed);
                let mut returned_digest = 0;
                let measure = metrics::Measurement::start();
                for _ in 0..1024 {
                    let mut cycle = 0;
                    for event in &loaded.events {
                        cycle = mix(cycle, returned(loaded.callback.call::<Value>(event).unwrap()));
                    }
                    returned_digest = mix(returned_digest, cycle);
                }
                let mut row = measure.finish();
                expected_digests.push((digest.load(Ordering::Relaxed), returned_digest));
                if sample > 0 {
                    row["sample"] = sample.into();
                    row["operations"] = (1024 * events.len()).into();
                    row["workload"] = "input_action".into();
                    rows.push(row);
                }
            }
            assert_eq!(lua_state(&loaded), final_state);
        }
        loaded.lua.gc_collect().unwrap();
        (program, objects, final_state, loaded.lua.used_memory())
    };
    assert!(closed.load(Ordering::SeqCst));
    let mut machine = program.machine();
    native_cycle(&mut machine, events, &digest);
    if native {
        for sample in 0..10 {
            digest.store(0, Ordering::Relaxed);
            let mut returned_digest = 0;
            let measure = metrics::Measurement::start();
            for _ in 0..1024 {
                returned_digest = mix(returned_digest, native_cycle(&mut machine, events, &digest));
            }
            let mut row = measure.finish();
            expected_digests.push((digest.load(Ordering::Relaxed), returned_digest));
            if sample > 0 {
                row["sample"] = sample.into();
                row["operations"] = (1024 * events.len()).into();
                row["workload"] = "input_action".into();
                rows.push(row);
            }
        }
        assert_eq!(native_state(&machine, &objects), final_state);
    }
    for compile in [false, true] {
        for sample in 0..6 {
            let measure = metrics::Measurement::start();
            for _ in 0..16 {
                let state = load(
                    &fixture,
                    compile,
                    Arc::new(AtomicBool::new(false)),
                    Arc::new(AtomicU64::new(0)),
                );
                if compile {
                    black_box(state.program.as_ref().unwrap().machine());
                }
                black_box(state);
            }
            let mut row = measure.finish();
            if sample > 0 {
                row["sample"] = sample.into();
                row["operations"] = 16.into();
                row["workload"] = if compile { "native_prepare" } else { "lua_prepare" }.into();
                rows.push(row);
            }
        }
    }
    let report = json!({"mode":args[2],"rows":rows,"digests":expected_digests,"end_state":final_state,"loading_vm_destroyed_before_native":closed.load(Ordering::SeqCst),"retained_memory":{"native_payload_bytes":machine.retained_bytes(),"lua_allocator_bytes":lua_bytes,"scope":"Native hash/allocator bookkeeping excluded; Lua memory after full collection; transient loading schema buffers excluded."},"logical_cpu":2});
    std::fs::write(Path::new(&args[1]), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
