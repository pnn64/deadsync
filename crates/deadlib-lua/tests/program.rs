#![cfg(feature = "lua51")]
use mlua::program::{self, Action, Binding, Bindings, Cell, Datum, Field, FieldValue, Parameter, Types};
use mlua::{Function, Lua, Table, Value};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Dropped(Arc<AtomicBool>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn input(name: &str, slot: usize, types: Types) -> Field {
    Field {
        object: 0,
        name: name.as_bytes().into(),
        value: FieldValue::Value(Binding::Input { slot, types }),
    }
}
fn property(object: u32, name: &str, cell: usize) -> Field {
    Field {
        object,
        name: name.as_bytes().into(),
        value: FieldValue::Value(Binding::Cell(cell)),
    }
}
fn visible() -> Field {
    Field {
        object: 1,
        name: b"visible".as_slice().into(),
        value: FieldValue::Action(Action {
            tag: 7,
            parameters: vec![Parameter::Object(1), Parameter::Type(Types::BOOLEAN)],
            result: Some(Binding::Object(1)),
        }),
    }
}

#[derive(Debug, PartialEq)]
struct Row {
    handled: bool,
    held: bool,
    count: u64,
    actions: Vec<bool>,
}

#[test]
fn grouped_callbacks_share_actual_cells_and_survive_loading_vm_destruction() {
    let dropped = Arc::new(AtomicBool::new(false));
    let (mut group, expected) = {
        let lua = Lua::new();
        lua.set_app_data(Dropped(dropped.clone()));
        let actions = Arc::new(Mutex::new(Vec::new()));
        let actor = lua.create_table().unwrap();
        actor
            .set(
                "visible",
                lua.create_function({
                    let actor = actor.clone();
                    let actions = actions.clone();
                    move |_, (_, value): (Table, bool)| {
                        actions.lock().unwrap().push(value);
                        Ok(actor.clone())
                    }
                })
                .unwrap(),
            )
            .unwrap();
        let factory: Function = lua.load(r#"return function(actor)
            local state={held=false,count=-0.0}
            local alias=state
            return function(e)
                if e.button=='Right' then state.held=not state.held;actor:visible(state.held) end
                return false
            end,function(e)
                if e.type=='Release' then alias.count=alias.count+1;alias.held=false;actor:visible(false) end
                return alias.held
            end,state
        end"#).eval().unwrap();
        let (first, second, shared): (Function, Function, Table) =
            factory.call(actor.clone()).unwrap();
        let mut bindings = Bindings {
            arguments: vec![Binding::Object(0)],
            cells: vec![Cell::Boolean(false), Cell::Number(-0.0)],
            fields: vec![
                input("button", 0, Types::BYTES),
                input("type", 1, Types::BYTES),
                property(2, "held", 0),
                property(2, "count", 1),
                visible(),
            ],
            ..Bindings::default()
        };
        let mut programs = Vec::new();
        for callback in [&first, &second] {
            bindings.upvalues.clear();
            callback
                .for_each_upvalue_as::<mlua::InspectionValue>(|_, _, value| {
                    let mlua::InspectionValue::Table(table) = value else {
                        panic!("table capture");
                    };
                    let object = if table.to_pointer() == actor.to_pointer() {
                        1
                    } else {
                        assert_eq!(table.to_pointer(), shared.to_pointer());
                        2
                    };
                    bindings.upvalues.push(Binding::Object(object));
                    Ok(())
                })
                .unwrap();
            programs.push(program::compile(callback, &bindings).unwrap());
        }
        let group = program::Group::new(programs).unwrap();
        let mut expected = Vec::new();
        for index in 0..24 {
            let event = lua.create_table().unwrap();
            event
                .set("button", if index % 3 == 0 { "Left" } else { "Right" })
                .unwrap();
            event
                .set("type", if index % 4 == 0 { "Release" } else { "Repeat" })
                .unwrap();
            let a = first.call::<Value>(event.clone()).unwrap();
            let b = second.call::<Value>(event).unwrap();
            expected.push((
                !matches!(a, Value::Nil | Value::Boolean(false)),
                !matches!(b, Value::Nil | Value::Boolean(false)),
                shared.get::<bool>("held").unwrap(),
                shared.get::<f64>("count").unwrap().to_bits(),
                std::mem::take(&mut *actions.lock().unwrap()),
            ));
        }
        (group, expected)
    };
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(group.cell(1), Some(Datum::Number(-0.0)));
    for (index, (a, b, held, count, expected_actions)) in expected.into_iter().enumerate() {
        let inputs = [
            Datum::Bytes(if index % 3 == 0 { b"Left" } else { b"Right" }),
            Datum::Bytes(if index % 4 == 0 {
                b"Release"
            } else {
                b"Repeat"
            }),
        ];
        let mut actions = Vec::new();
        let mut run = |which| {
            group
                .run(which, &inputs, |tag, args| {
                    assert_eq!(tag, 7);
                    let Some(Datum::Boolean(value)) = args.get(1) else {
                        panic!("boolean action")
                    };
                    actions.push(value);
                })
                .unwrap()
                .unwrap()
                .is_truthy()
        };
        assert_eq!(run(0), a);
        assert_eq!(run(1), b);
        assert_eq!(actions, expected_actions);
        assert_eq!(group.cell(0), Some(Datum::Boolean(held)));
        let Some(Datum::Number(value)) = group.cell(1) else {
            panic!("number cell")
        };
        assert_eq!(value.to_bits(), count);
    }
    assert!(
        group
            .run(2, &[], |_, _| panic!("invalid index has no effects"))
            .is_none()
    );
}

#[test]
fn groups_check_cell_layout_bits_and_clear_scratch_between_programs() {
    let lua = Lua::new();
    let callback: Function = lua
        .load("local n=0;return function()return n end")
        .eval()
        .unwrap();
    let compile = |cell| {
        program::compile(
            &callback,
            &Bindings {
                upvalues: vec![Binding::Cell(0)],
                cells: vec![cell],
                ..Bindings::default()
            },
        )
        .unwrap()
    };
    assert!(
        program::Group::new(vec![
            compile(Cell::Number(-0.0)),
            compile(Cell::Number(0.0))
        ])
        .is_err()
    );
    assert!(
        program::Group::new(vec![
            compile(Cell::Boolean(false)),
            compile(Cell::Number(0.0))
        ])
        .is_err()
    );
    let bits = 0x7ff8_0000_0000_1234;
    let group = program::Group::new(vec![
        compile(Cell::Number(f64::from_bits(bits))),
        compile(Cell::Number(f64::from_bits(bits))),
    ])
    .unwrap();
    let Some(Datum::Number(value)) = group.cell(0) else {
        panic!("number cell")
    };
    assert_eq!(value.to_bits(), bits);
    let a: Function = lua
        .load("return function()local a,b,c,d,e,f,g=1,2,3,4,5,6,7;return g end")
        .eval()
        .unwrap();
    let b: Function = lua
        .load("return function()local a;return a end")
        .eval()
        .unwrap();
    let mut group = program::Group::new(vec![
        program::compile(&a, &Bindings::default()).unwrap(),
        program::compile(&b, &Bindings::default()).unwrap(),
    ])
    .unwrap();
    for _ in 0..10 {
        assert_eq!(
            group.run(0, &[], |_, _| {}).unwrap().unwrap(),
            Datum::Number(7.0)
        );
        assert_eq!(group.run(1, &[], |_, _| {}).unwrap().unwrap(), Datum::Nil);
    }
}

#[test]
fn real_branching_inputs_actions_and_aliased_state_survive_vm_destruction() {
    let dropped = Arc::new(AtomicBool::new(false));
    let sequence = [
        ("Left", "FirstPress"),
        ("Right", "FirstPress"),
        ("Right", "Repeat"),
        ("Right", "Release"),
        ("Right", "FirstPress"),
        ("Left", "Repeat"),
        ("Right", "Repeat"),
        ("Right", "FirstPress"),
        ("Right", "Release"),
        ("Right", "Release"),
    ];
    let (mut machine, expected) = {
        let lua = Lua::new();
        lua.set_app_data(Dropped(dropped.clone()));
        let trace = Arc::new(Mutex::new(Vec::new()));
        let actor = lua.create_table().unwrap();
        actor
            .set(
                "visible",
                lua.create_function({
                    let trace = trace.clone();
                    let actor = actor.clone();
                    move |_, (_, value): (Table, bool)| {
                        trace.lock().unwrap().push(value);
                        Ok(actor.clone())
                    }
                })
                .unwrap(),
            )
            .unwrap();
        let factory = lua
            .load(
                r#"return function(actor)
            local shared={held=false,count=0}
            local alias=shared
            return function(e)
                if e.button=='Right' then
                    if e.type=='Release' then shared.held=false
                    else shared.held=not alias.held end
                    shared.count=alias.count+1
                    actor:visible(shared.held)
                    return shared.count==2 or e.type=='Release'
                end
                return false
            end,shared
        end"#,
            )
            .eval::<Function>()
            .unwrap();
        let (callback, shared): (Function, Table) = factory.call(actor).unwrap();
        let mut bindings = Bindings {
            arguments: vec![Binding::Object(0)],
            cells: vec![Cell::Boolean(false), Cell::Number(0.0)],
            fields: vec![
                input("button", 0, Types::BYTES),
                input("type", 1, Types::BYTES),
                property(2, "held", 0),
                property(2, "count", 1),
                visible(),
            ],
            ..Bindings::default()
        };
        callback
            .for_each_upvalue(|_, name, _| {
                bindings.upvalues.push(Binding::Object(match name {
                    b"actor" => 1,
                    b"shared" | b"alias" => 2,
                    _ => panic!("unexpected capture"),
                }));
                Ok(())
            })
            .unwrap();
        let program = program::compile(&callback, &bindings).unwrap();
        let mut expected = Vec::new();
        for (button, kind) in sequence {
            let event = lua.create_table().unwrap();
            event.set("button", button).unwrap();
            event.set("type", kind).unwrap();
            let value = callback.call::<Value>(event).unwrap();
            expected.push(Row {
                handled: !matches!(value, Value::Nil | Value::Boolean(false)),
                held: shared.get("held").unwrap(),
                count: shared.get::<f64>("count").unwrap().to_bits(),
                actions: std::mem::take(&mut *trace.lock().unwrap()),
            });
        }
        (program.machine(), expected)
    };
    assert!(
        dropped.load(Ordering::SeqCst),
        "compiled interaction must release its loading VM"
    );
    for ((button, kind), expected) in sequence.into_iter().zip(expected) {
        let mut actions = Vec::new();
        let inputs = [Datum::Bytes(button.as_bytes()), Datum::Bytes(kind.as_bytes())];
        let handled = machine
            .run(&inputs, |tag, args| {
                assert_eq!(tag, 7);
                assert_eq!(args.len(), 2);
                assert_eq!(args.get(0), Some(Datum::Object(1)));
                let Some(Datum::Boolean(value)) = args.get(1) else {
                    panic!("typed action")
                };
                actions.push(value);
            })
            .unwrap()
            .is_truthy();
        let Some(Datum::Boolean(held)) = machine.cell(0) else {
            panic!("boolean cell")
        };
        let Some(Datum::Number(count)) = machine.cell(1) else {
            panic!("number cell")
        };
        assert_eq!(
            Row {
                handled,
                held,
                count: count.to_bits(),
                actions
            },
            expected
        );
    }
}

#[test]
fn primitive_truthiness_short_circuit_and_byte_strings_match_lua() {
    let lua = Lua::new();
    for body in [
        "return e.value or 'fallback'",
        "return e.value and true or false",
        "return not e.value",
        "if e.value == 'abc' then return 7 else return 0 end",
        "return e.value == '\\255\\0'",
        "return e.value ~= false",
    ] {
        let callback: Function = lua.load(format!("return function(e) {body} end")).eval().unwrap();
        let bindings = Bindings {
            arguments: vec![Binding::Object(0)],
            fields: vec![input(
                "value",
                0,
                Types::NIL | Types::BOOLEAN | Types::NUMBER | Types::BYTES,
            )],
            ..Bindings::default()
        };
        let mut machine = program::compile(&callback, &bindings).unwrap().machine();
        for input in [
            Datum::Nil,
            Datum::Boolean(false),
            Datum::Boolean(true),
            Datum::Number(0.0),
            Datum::Number(-0.0),
            Datum::Number(f64::NAN),
            Datum::Bytes(b""),
            Datum::Bytes(b"abc"),
            Datum::Bytes(&[255, 0]),
        ] {
            let event = lua.create_table().unwrap();
            match input {
                Datum::Nil => {}
                Datum::Boolean(v) => event.set("value", v).unwrap(),
                Datum::Number(v) => event.set("value", v).unwrap(),
                Datum::Bytes(v) => event.set("value", lua.create_string(v).unwrap()).unwrap(),
                _ => unreachable!(),
            }
            let expected = callback.call::<Value>(event).unwrap();
            let inputs = [input];
            let actual = machine.run(&inputs, |_, _| panic!("no host action")).unwrap();
            match (expected, actual) {
                (Value::Nil, Datum::Nil) => {}
                (Value::Boolean(a), Datum::Boolean(b)) => assert_eq!(a, b),
                (Value::Integer(a), Datum::Number(b)) => assert_eq!(a as f64, b),
                (Value::Number(a), Datum::Number(b)) => assert_eq!(a.to_bits(), b.to_bits()),
                (Value::String(a), Datum::Bytes(b)) => assert_eq!(a.as_bytes().as_ref(), b),
                other => panic!("different result: {body}: {other:?}"),
            }
        }
    }
}

#[test]
fn scalar_cells_preserve_number_bits_and_are_independent_per_machine() {
    let lua = Lua::new();
    let factory: Function = lua
        .load("return function(n) return function(e) n=n+e.delta; return -n end end")
        .eval()
        .unwrap();
    for initial in [-0.0, 0.0, 1.5, f64::from_bits(0x7ff8_0000_0000_1234)] {
        let callback: Function = factory.call(initial).unwrap();
        let bindings = Bindings {
            arguments: vec![Binding::Object(0)],
            upvalues: vec![Binding::Cell(0)],
            cells: vec![Cell::Number(initial)],
            fields: vec![input("delta", 0, Types::NUMBER)],
            ..Bindings::default()
        };
        let program = program::compile(&callback, &bindings).unwrap();
        let mut machine = program.machine();
        let untouched = program.machine();
        for delta in [-0.0, 0.0, 2.25, -0.5] {
            let event = lua.create_table().unwrap();
            event.set("delta", delta).unwrap();
            let expected = callback.call::<f64>(event).unwrap();
            let inputs = [Datum::Number(delta)];
            let Datum::Number(actual) = machine.run(&inputs, |_, _| panic!("no action")).unwrap() else {
                panic!("number result")
            };
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        let Some(Datum::Number(n)) = untouched.cell(0) else {
            panic!("number cell")
        };
        assert_eq!(n.to_bits(), initial.to_bits());
    }
}

#[test]
fn unsupported_behavior_is_rejected_without_running_a_prefix() {
    let lua = Lua::new();
    lua.globals().set("hits", 0).unwrap();
    for source in [
        "hits=hits+1; return e.button",
        "for i=1,3 do end; return true",
        "return tostring(e.button)",
        "return function() return 1 end",
        "return e[e.button]",
        "return e==e",
        "return e.level+1",
    ] {
        let f: Function = lua
            .load(format!("return function(e) {source} end"))
            .eval()
            .unwrap();
        let bindings = Bindings {
            arguments: vec![Binding::Object(0)],
            fields: vec![input("button", 0, Types::BYTES), input("level", 1, Types::BYTES)],
            ..Bindings::default()
        };
        assert!(program::compile(&f, &bindings).is_err(), "{source}");
        assert_eq!(lua.globals().get::<i32>("hits").unwrap(), 0);
    }
    let f: Function = lua.load("return function(...) return ... end").eval().unwrap();
    assert!(program::compile(&f, &Bindings::default()).is_err());
}

#[test]
fn input_contract_errors_and_static_schema_errors_are_checked() {
    let lua = Lua::new();
    let f: Function = lua.load("return function(e) return e.button end").eval().unwrap();
    let mut bindings = Bindings {
        arguments: vec![Binding::Object(0)],
        fields: vec![input("button", 0, Types::BYTES)],
        ..Bindings::default()
    };
    let mut machine = program::compile(&f, &bindings).unwrap().machine();
    assert_eq!(
        machine.run(&[], |_, _| {}),
        Err(program::RunError::MissingInput(0))
    );
    assert_eq!(
        machine.run(&[Datum::Nil], |_, _| {}),
        Err(program::RunError::InputType(0))
    );
    bindings.fields.push(input("button", 1, Types::BYTES));
    assert!(program::compile(&f, &bindings).is_err());
    bindings.fields.pop();
    bindings.upvalues.push(Binding::Cell(0));
    assert!(program::compile(&f, &bindings).is_err());
}

#[test]
fn numeric_operations_and_branches_match_authored_lua() {
    let lua = Lua::new();
    let bindings = Bindings {
        arguments: vec![Binding::Object(0)],
        fields: vec![input("a", 0, Types::NUMBER), input("b", 1, Types::NUMBER)],
        ..Bindings::default()
    };
    for expression in [
        "e.a+e.b",
        "e.a-e.b",
        "e.a*e.b",
        "e.a/e.b",
        "-e.a",
        "e.a<e.b",
        "e.a<=e.b",
        "e.a>e.b",
        "e.a>=e.b",
        "e.a==e.b",
        "e.a==e.b and 1 or 2",
    ] {
        let callback: Function = lua
            .load(format!("return function(e) return {expression} end"))
            .eval()
            .unwrap();
        let mut machine = program::compile(&callback, &bindings).unwrap().machine();
        for (a, b) in [
            (0.0, -0.0),
            (-0.0, 0.0),
            (1.5, 2.75),
            (3.0, -2.0),
            (f64::INFINITY, 3.0),
            (f64::from_bits(0x7ff8_0000_0000_1234), 1.0),
        ] {
            let event = lua.create_table().unwrap();
            event.set("a", a).unwrap();
            event.set("b", b).unwrap();
            let expected = callback.call::<Value>(event).unwrap();
            let inputs = [Datum::Number(a), Datum::Number(b)];
            match (expected, machine.run(&inputs, |_, _| {}).unwrap()) {
                (Value::Boolean(a), Datum::Boolean(b)) => assert_eq!(a, b),
                (Value::Number(a), Datum::Number(b)) => assert_eq!(a.to_bits(), b.to_bits(), "{expression}"),
                (Value::Integer(a), Datum::Number(b)) => assert_eq!(a as f64, b),
                pair => panic!("different arithmetic result: {pair:?}"),
            }
        }
    }
}

#[test]
fn method_chains_tail_calls_and_receiver_identity_are_verified() {
    let lua = Lua::new();
    for body in [
        "return actor:visible(true)",
        "actor:visible(true):visible(false); return false",
        "local f=actor.visible; f(actor,true); return true",
    ] {
        let callback: Function = lua
            .load(format!("local actor={{}};return function(e) {body} end"))
            .eval()
            .unwrap();
        let bindings = Bindings {
            arguments: vec![Binding::Object(0)],
            upvalues: vec![Binding::Object(1)],
            fields: vec![visible()],
            ..Bindings::default()
        };
        let mut machine = program::compile(&callback, &bindings).unwrap().machine();
        let mut actions = Vec::new();
        let returned = machine
            .run(&[], |tag, args| {
                assert_eq!(tag, 7);
                assert_eq!(args.get(0), Some(Datum::Object(1)));
                actions.push(matches!(args.get(1), Some(Datum::Boolean(true))));
            })
            .unwrap();
        assert_eq!(
            actions,
            if body.contains("visible(false)") {
                vec![true, false]
            } else {
                vec![true]
            }
        );
        assert_eq!(returned.is_truthy(), !body.contains("return false"));
    }
    let callback: Function = lua
        .load("local actor,other={},{};return function() actor.visible(other,true) end")
        .eval()
        .unwrap();
    let mut bindings = Bindings {
        fields: vec![visible()],
        ..Bindings::default()
    };
    callback
        .for_each_upvalue(|_, name, _| {
            bindings
                .upvalues
                .push(Binding::Object(if name == b"actor" { 1 } else { 2 }));
            Ok(())
        })
        .unwrap();
    assert!(
        program::compile(&callback, &bindings).is_err(),
        "a different method receiver must not inherit a bound action tag"
    );
}

#[test]
fn equal_scalar_values_do_not_merge_distinct_captured_cells() {
    let lua = Lua::new();
    let callback: Function = lua
        .load("local a,b=false,false;return function() a=not a;return b end")
        .eval()
        .unwrap();
    let mut bindings = Bindings {
        cells: vec![Cell::Boolean(false), Cell::Boolean(false)],
        ..Bindings::default()
    };
    callback
        .for_each_upvalue(|_, name, _| {
            bindings
                .upvalues
                .push(Binding::Cell(if name == b"a" { 0 } else { 1 }));
            Ok(())
        })
        .unwrap();
    let mut machine = program::compile(&callback, &bindings).unwrap().machine();
    for expected in [true, false, true, false] {
        assert!(!machine.run(&[], |_, _| {}).unwrap().is_truthy());
        assert_eq!(machine.cell(0), Some(Datum::Boolean(expected)));
        assert_eq!(machine.cell(1), Some(Datum::Boolean(false)));
    }
}

#[test]
fn malformed_or_mutated_dumps_never_need_a_lua_vm_to_validate_or_execute() {
    let bytes = {
        let lua = Lua::new();
        lua.load("return true and 7 or 'fallback'")
            .into_function()
            .unwrap()
            .dump(false)
    };
    // A top-level Lua chunk is vararg. Use an ordinary callback for executable mutations.
    let bytes2 = {
        let lua = Lua::new();
        let f: Function = lua.load("return function() return 7 end").eval().unwrap();
        f.dump(false)
    };
    for end in 0..bytes2.len() {
        assert!(program::compile_bytecode(&bytes2[..end], &Bindings::default()).is_err());
    }
    for original in [bytes, bytes2] {
        for index in 0..original.len() {
            let mut mutated = original.clone();
            mutated[index] ^= 0x81;
            if let Ok(program) = program::compile_bytecode(&mutated, &Bindings::default()) {
                let mut machine = program.machine();
                let _ = machine.run(&[], |_, _| panic!("no bound action"));
            }
        }
    }
}

#[test]
fn borrowed_instruction_decoding_preserves_endianness_and_outlives_the_dump() {
    fn integer(bytes: &mut Vec<u8>, value: u64, width: usize, little: bool) {
        let encoded = if little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        bytes.extend_from_slice(if little {
            &encoded[..width]
        } else {
            &encoded[8 - width..]
        });
    }
    for little in [false, true] {
        for size_t in [4, 8] {
            for bits in [
                0x8000_0000_0000_0000,
                0x7ff8_0000_0000_1234,
                123.5_f64.to_bits(),
            ] {
                let program = {
                    let mut bytes = b"\x1bLua\x51\0".to_vec();
                    bytes.extend_from_slice(&[u8::from(little), 4, size_t as u8, 4, 8, 0]);
                    integer(&mut bytes, 0, size_t, little); // no source name
                    integer(&mut bytes, 0, 4, little);
                    integer(&mut bytes, 0, 4, little);
                    bytes.extend_from_slice(&[0, 0, 0, 2]); // captures, args, vararg, registers
                    integer(&mut bytes, 2, 4, little);
                    integer(&mut bytes, 1, 4, little); // LOADK R0 K0
                    integer(&mut bytes, 30 | (2 << 23), 4, little); // RETURN R0, one result
                    integer(&mut bytes, 1, 4, little);
                    bytes.push(3);
                    integer(&mut bytes, bits, 8, little);
                    for _ in 0..4 {
                        integer(&mut bytes, 0, 4, little);
                    }
                    program::compile_bytecode(&bytes, &Bindings::default()).unwrap()
                };
                let mut machine = program.machine();
                let Datum::Number(value) = machine.run(&[], |_, _| panic!("no action")).unwrap()
                else {
                    panic!("numeric return");
                };
                assert_eq!(value.to_bits(), bits);
            }
        }
    }
}

#[test]
fn noncanonical_boolean_constants_are_rejected() {
    let lua = Lua::new();
    let f: Function = lua
        .load("local t={};return function() t.held=true;return t.held end")
        .eval()
        .unwrap();
    let mut bytes = f.dump(false);
    let marker = b"held\0\x01\x01";
    let offsets = bytes
        .windows(marker.len())
        .enumerate()
        .filter_map(|(i, b)| (b == marker).then_some(i))
        .collect::<Vec<_>>();
    assert_eq!(offsets.len(), 1, "change only the serialized boolean constant");
    let bindings = Bindings {
        upvalues: vec![Binding::Object(1)],
        cells: vec![Cell::Boolean(false)],
        fields: vec![property(1, "held", 0)],
        ..Bindings::default()
    };
    assert!(program::compile_bytecode(&bytes, &bindings).is_ok());
    bytes[offsets[0] + marker.len() - 1] = 2;
    assert!(matches!(
        program::compile_bytecode(&bytes, &bindings),
        Err(program::CompileError::InvalidChunk)
    ));
}

#[test]
fn reused_registers_do_not_leak_previous_inputs_after_branches_or_failed_reads() {
    let lua = Lua::new();
    let callback: Function = lua.load("return function(e) if e.path=='skip' then return false end;local a=e.a;if e.path=='missing' then return e.b end;return a end").eval().unwrap();
    let bindings = Bindings {
        arguments: vec![Binding::Object(0)],
        fields: vec![
            input("path", 0, Types::BYTES),
            input("a", 1, Types::NIL | Types::BOOLEAN | Types::NUMBER | Types::BYTES),
            input("b", 2, Types::BYTES),
        ],
        ..Bindings::default()
    };
    let mut machine = program::compile(&callback, &bindings).unwrap().machine();
    for _ in 0..8 {
        {
            let owned = vec![0, 255, 17];
            assert_eq!(
                machine
                    .run(&[Datum::Bytes(b"read"), Datum::Bytes(&owned)], |_, _| {})
                    .unwrap(),
                Datum::Bytes(&owned)
            );
        }
        assert_eq!(
            machine.run(&[Datum::Bytes(b"missing"), Datum::Boolean(true)], |_, _| {}),
            Err(program::RunError::MissingInput(2))
        );
        assert_eq!(
            machine.run(&[Datum::Bytes(b"skip")], |_, _| {}).unwrap(),
            Datum::Boolean(false)
        );
        assert_eq!(
            machine
                .run(&[Datum::Bytes(b"read"), Datum::Nil], |_, _| {})
                .unwrap(),
            Datum::Nil
        );
    }
}

#[test]
fn implicit_entry_nils_are_restored_only_where_control_flow_can_read_them() {
    let lua = Lua::new();
    let locals = (0..190).map(|i| format!("n{i}")).collect::<Vec<_>>().join(",");
    for body in [
        "return nil".to_owned(),
        "local value;if e.pick then value=e.value end;return value".to_owned(),
        "local value;return not value".to_owned(),
        format!("local {locals};if e.pick then n0=e.value end;return n0"),
    ] {
        let callback: Function = lua.load(format!("return function(e) {body} end")).eval().unwrap();
        let bindings = Bindings {
            arguments: vec![Binding::Object(0)],
            fields: vec![input("pick", 0, Types::BOOLEAN), input("value", 1, Types::BYTES)],
            ..Bindings::default()
        };
        let mut machine = program::compile(&callback, &bindings).unwrap().machine();
        for pick in [true, false, true, false, false, true] {
            let event = lua.create_table().unwrap();
            event.set("pick", pick).unwrap();
            event
                .set("value", lua.create_string([0, 255, 9]).unwrap())
                .unwrap();
            let expected = callback.call::<Value>(event).unwrap();
            let inputs = [Datum::Boolean(pick), Datum::Bytes(&[0, 255, 9])];
            let actual = machine.run(&inputs, |_, _| {}).unwrap();
            match (expected, actual) {
                (Value::Nil, Datum::Nil) => {}
                (Value::Boolean(a), Datum::Boolean(b)) => assert_eq!(a, b),
                (Value::String(a), Datum::Bytes(b)) => assert_eq!(a.as_bytes().as_ref(), b),
                pair => panic!("entry nil differs: {body}: {pair:?}"),
            }
        }
    }
}
