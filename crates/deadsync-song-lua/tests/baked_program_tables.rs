#[path = "support/baked_program.rs"]
mod program;
use deadlib_lua::{HostCall, Value, Vm};
use program::{Binding, Bindings, BooleanTable, Datum, Field, FieldValue, Types};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct Closed(Arc<AtomicBool>);
impl Drop for Closed {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn bindings() -> Bindings {
    Bindings {
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
                name: b"release".as_slice().into(),
                value: FieldValue::Value(Binding::Input {
                    slot: 1,
                    types: Types::BOOLEAN,
                }),
            },
            Field {
                object: 1,
                name: b"visible".as_slice().into(),
                value: FieldValue::Action(program::Action {
                    tag: 3,
                    parameters: vec![
                        program::Parameter::Receiver,
                        program::Parameter::Type(Types::BOOLEAN),
                    ]
                    .into(),
                    result: Some(Binding::Object(1)),
                }),
            },
        ],
        boolean_tables: [7, 8]
            .map(|object| BooleanTable {
                object,
                entries: vec![(b"hidden".as_slice().into(), false)],
            })
            .to_vec(),
        ..Bindings::default()
    }
}

#[test]
fn byte_keyed_state_keeps_nil_false_aliases_actions_and_vm_independence() {
    let closed = Arc::new(AtomicBool::new(false));
    let (mut group, mut independent, expected) = {
        let mut vm = Vm::new();
        program::shutdown_marker(&mut vm, Closed(closed.clone()));
        let [callback, held, actor] = program::eval(&mut vm,
            "local held={hidden=false};local actor={};return function(e) if e.release then held[e.button]=nil else held[e.button]=true end;if held.Left or held.Right then actor:visible(true) else actor:visible(false) end;return held[e.button] end,held,actor"
        ).try_into().unwrap();
        let seen = Arc::new(std::sync::Mutex::new(Vec::<bool>::new()));
        let visible = vm
            .new_host_function_with_data(
                |call: &mut HostCall<'_>| {
                    let owner = call.arguments()[0];
                    let Value::Boolean(value) = call.arguments()[1] else {
                        panic!("fixture Boolean action")
                    };
                    call.function_data::<Arc<std::sync::Mutex<Vec<bool>>>>()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .push(value);
                    call.push(owner)
                },
                &[],
                seen.clone(),
            )
            .unwrap();
        program::set(&mut vm, actor, b"visible", visible);
        let mut schema = bindings();
        for (name, _) in program::upvalues(&mut vm, callback) {
            schema.upvalues.push(Binding::Object(match name.as_slice() {
                b"held" => 7,
                b"actor" => 1,
                _ => panic!("unexpected capture"),
            }));
        }
        let program = program::compile(&mut vm, callback, &schema).unwrap();
        let independent = program.machine();
        let group = program::Group::new(vec![program.clone(), program]).unwrap();
        let mut expected = Vec::new();
        for step in 0..96 {
            let key: &[u8] = [
                b"Left".as_slice(),
                b"Right",
                b"\0\xffunknown",
                b"Left",
                b"hidden",
            ][step % 5];
            let release = step % 3 == 0;
            let event = program::table(&mut vm);
            let button = program::string(&mut vm, key);
            program::set(&mut vm, event, b"button", button);
            program::set(&mut vm, event, b"release", Value::Boolean(release));
            let result = match program::call(&mut vm, callback, &[event])[0] {
                Value::Nil => None,
                Value::Boolean(v) => Some(v),
                _ => panic!("non-Boolean result"),
            };
            let mut state = Vec::new();
            for (key, value) in program::entries(&mut vm, held) {
                let Value::Boolean(value) = value else {
                    panic!("fixture table Boolean")
                };
                state.push((program::bytes(&vm, key), value));
            }
            state.sort();
            expected.push((
                key.to_vec(),
                release,
                result,
                state,
                std::mem::take(&mut *seen.lock().unwrap()),
            ));
        }
        (group, independent, expected)
    };
    assert!(closed.load(Ordering::SeqCst));
    for (step, (key, release, expected, state, actions)) in expected.into_iter().enumerate() {
        let input = [Datum::Bytes(&key), Datum::Boolean(release)];
        let mut observed = Vec::new();
        let result = group
            .run(step % 2, &input, |tag, args| {
                assert_eq!(tag, 3);
                assert_eq!(args.get(0), Some(Datum::Object(1)));
                let Some(Datum::Boolean(value)) = args.get(1) else {
                    panic!("invalid action")
                };
                observed.push(value);
            })
            .unwrap()
            .unwrap();
        assert_eq!(result, expected.map_or(Datum::Nil, Datum::Boolean));
        assert_eq!(observed, actions);
        assert_eq!(group.boolean(7, &key), Some(expected));
        let mut actual = group
            .boolean_entries(7)
            .unwrap()
            .map(|(key, value)| (key.to_vec(), value))
            .collect::<Vec<_>>();
        actual.sort();
        assert_eq!(actual, state);
        assert_eq!(group.boolean(8, b"hidden"), Some(Some(false)));
        assert_eq!(
            group.boolean(8, &key),
            Some(if key == b"hidden" { Some(false) } else { None })
        );
    }
    assert_eq!(independent.boolean(7, b"Left"), Some(None));
    let input = [Datum::Bytes(b"Left"), Datum::Boolean(false)];
    independent.run(&input, |_, _| {}).unwrap();
    assert_eq!(independent.boolean(7, b"Left"), Some(Some(true)));
    assert_eq!(independent.boolean(99, b"Left"), None);
}

#[test]
fn table_schemas_and_unsupported_keys_values_are_rejected_without_execution() {
    let mut vm = Vm::new();
    for source in [
        "local h={};return function(e) h[e.button]=1 end",
        "local h={};return function(e) h[1]=true end",
        "local h={};return function(e) return #h end",
        "local h={};return function(e) for k,v in pairs(h) do return v end end",
    ] {
        let callback = program::eval(&mut vm, source)[0];
        let mut schema = bindings();
        schema.upvalues.push(Binding::Object(7));
        assert!(program::compile(&mut vm, callback, &schema).is_err());
    }
    let mut schema = bindings();
    schema.boolean_tables.push(schema.boolean_tables[0].clone());
    assert!(program::Compiler::new(&schema).is_err());
    schema.boolean_tables.pop();
    schema.boolean_tables[0]
        .entries
        .push((b"hidden".as_slice().into(), true));
    assert!(program::Compiler::new(&schema).is_err());
}

#[test]
fn bounded_array_choices_preserve_table_aliases_and_independent_machine_state() {
    let mut vm = Vm::new();
    let callback = program::eval(
        &mut vm,
        "local a={hidden=false};local b={hidden=false};local tables={a,b,a};return function(e) local p=e.owner=='third' and 3 or (e.owner=='second' and 2 or 1);if e.release then tables[p][e.button]=nil else tables[p][e.button]=true end;return tables[p][e.button] end",
    )[0];
    let mut schema = bindings();
    schema.fields.push(Field {
        object: 0,
        name: b"owner".as_slice().into(),
        value: FieldValue::Value(Binding::Input {
            slot: 2,
            types: Types::BYTES,
        }),
    });
    schema
        .boolean_table_arrays
        .push(program::BooleanTableArray {
            object: 9,
            tables: vec![7, 8, 7],
        });
    schema.upvalues.push(Binding::Object(9));
    let program = program::compile(&mut vm, callback, &schema).unwrap();
    drop(vm);
    drop(schema);
    let mut first = program.machine();
    let second = program.machine();
    for (owner, release, expected) in [
        (b"first".as_slice(), false, Some(true)),
        (b"third", true, None),
        (b"second", false, Some(true)),
        (b"first", true, None),
    ] {
        let input = [
            Datum::Bytes(b"left"),
            Datum::Boolean(release),
            Datum::Bytes(owner),
        ];
        assert_eq!(
            first.run(&input, |_, _| {}).unwrap(),
            expected.map_or(Datum::Nil, Datum::Boolean)
        );
    }
    assert_eq!(first.boolean(7, b"left"), Some(None));
    assert_eq!(first.boolean(8, b"left"), Some(Some(true)));
    assert_eq!(second.boolean(8, b"left"), Some(None));
    let mut vm = Vm::new();
    let callback = program::eval(
        &mut vm,
        "local tables={};return function(e) return tables[e.index].key end",
    )[0];
    let mut schema = bindings();
    schema.upvalues.push(Binding::Object(9));
    schema
        .boolean_table_arrays
        .push(program::BooleanTableArray {
            object: 9,
            tables: vec![7, 8],
        });
    schema.fields.push(Field {
        object: 0,
        name: b"index".as_slice().into(),
        value: FieldValue::Value(Binding::Input {
            slot: 2,
            types: Types::NUMBER,
        }),
    });
    assert!(
        program::compile(&mut vm, callback, &schema).is_err(),
        "unbounded indices cannot prove a table reference"
    );
    schema.boolean_table_arrays[0].tables.push(99);
    assert!(program::Compiler::new(&schema).is_err());
}

#[test]
fn released_key_cache_preserves_nil_and_stays_bounded_under_distinct_inputs() {
    let mut vm = Vm::new();
    let callback = program::eval(
        &mut vm,
        "local h={hidden=false};return function(e) if e.release then h[e.button]=nil else h[e.button]=true end end",
    )[0];
    let mut schema = bindings();
    schema.upvalues.push(Binding::Object(7));
    let mut machine = program::compile(&mut vm, callback, &schema)
        .unwrap()
        .machine();
    drop(vm);
    let initial = machine.retained_bytes();
    for index in 0..4096 {
        let key = format!("key-{index:08}");
        for release in [false, true, true] {
            let inputs = [Datum::Bytes(key.as_bytes()), Datum::Boolean(release)];
            machine.run(&inputs, |_, _| {}).unwrap();
            assert_eq!(
                machine.boolean(7, key.as_bytes()),
                Some(if release { None } else { Some(true) })
            );
        }
        assert_eq!(machine.boolean_entries(7).unwrap().count(), 1);
    }
    assert!(
        machine.retained_bytes() < initial + 32768,
        "inactive input keys must not grow without bound"
    );
    assert_eq!(machine.boolean(7, b"hidden"), Some(Some(false)));
}
