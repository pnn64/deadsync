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
        name: name.as_bytes().to_vec().into(),
        value: FieldValue::Value(Binding::Input { slot, types }),
    }
}
fn property(object: u32, name: &str, cell: usize) -> Field {
    Field {
        object,
        name: name.as_bytes().to_vec().into(),
        value: FieldValue::Value(Binding::Cell(cell)),
    }
}
fn visible() -> Field {
    Field {
        object: 1,
        name: b"visible".as_slice().into(),
        value: FieldValue::Action(Action {
            tag: 7,
            parameters: vec![Parameter::Object(1), Parameter::Type(Types::BOOLEAN)].into(),
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
fn shared_byte_cells_keep_pool_meaning_copy_semantics_and_binary_bytes_after_vm_destruction() {
    let dropped = Arc::new(AtomicBool::new(false));
    let (mut group, mut separate, expected) = {
        let lua = Lua::new();
        lua.set_app_data(Dropped(dropped.clone()));
        let actions = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let actor = lua.create_table().unwrap();
        actor
            .set(
                "mode",
                lua.create_function({
                    let actions = actions.clone();
                    move |lua, (_, value): (Table, mlua::LuaString)| {
                        actions.lock().unwrap().push(value.as_bytes().to_vec());
                        lua.create_string(b"result")
                    }
                })
                .unwrap(),
            )
            .unwrap();
        let factory: Function = lua
            .load(
                r#"return function(actor)
            local state={mode='idle',copy='idle'}
            return function(e)
                local old=state.mode
                if e.button=='Right' then state.mode='\000armed'
                elseif e.button=='Empty' then state.mode='' end
                state.copy=old
                actor:mode(state.mode)
                return state.mode
            end,function(e)
                if state.mode=='\000armed' then state.mode='\255done' end
                return actor:mode(state.mode)
            end,function(e)
                local old=state.mode
                state.mode='idle'
                state.copy=old
                return state.copy
            end,state
        end"#,
            )
            .eval()
            .unwrap();
        let (a, b, c, state): (Function, Function, Function, Table) =
            factory.call(actor.clone()).unwrap();
        let mut bindings = Bindings {
            arguments: vec![Binding::Object(0)],
            cells: vec![Cell::Bytes(0), Cell::Bytes(0)],
            strings: vec![Box::from(&b"idle"[..])],
            fields: vec![
                input("button", 0, Types::BYTES),
                property(2, "mode", 0),
                property(2, "copy", 1),
                Field {
                    object: 1,
                    name: b"mode".as_slice().into(),
                    value: FieldValue::Action(Action {
                        tag: 7,
                        parameters: vec![Parameter::Object(1), Parameter::Type(Types::BYTES)].into(),
                        result: Some(Binding::Literal(program::Literal::Bytes(Box::from(
                            &b"result"[..],
                        )))),
                    }),
                },
            ],
            ..Bindings::default()
        };
        let mut programs = Vec::new();
        for callback in [&a, &b, &c] {
            bindings.upvalues.clear();
            callback
                .for_each_upvalue_as::<mlua::InspectionValue>(|_, _, value| {
                    let mlua::InspectionValue::Table(table) = value else {
                        panic!("table capture")
                    };
                    let object = if table.to_pointer() == actor.to_pointer() {
                        1
                    } else {
                        assert_eq!(table.to_pointer(), state.to_pointer());
                        2
                    };
                    bindings.upvalues.push(Binding::Object(object));
                    Ok(())
                })
                .unwrap();
            programs.push(program::compile(callback, &bindings).unwrap());
        }
        let separate = programs[0].machine();
        let single = program::Group::new(vec![programs[0].clone()])
            .unwrap()
            .retained_bytes();
        let repeated = program::Group::new(vec![programs[0].clone(); 32])
            .unwrap()
            .retained_bytes();
        assert!(
            repeated < single * 2,
            "identical code and byte arenas remain shared"
        );
        // An external machine and a repeated code Arc must retain their own
        // original pools while the group normalizes constants across callbacks.
        programs.push(programs[0].clone());
        let group = program::Group::new(programs).unwrap();
        let mut expected = Vec::new();
        for index in 0..24 {
            let event = lua.create_table().unwrap();
            event
                .set("button", ["Right", "Left", "Empty"][index % 3])
                .unwrap();
            for callback in [&a, &b, &c] {
                let result = callback
                    .call::<mlua::LuaString>(event.clone())
                    .unwrap()
                    .as_bytes()
                    .to_vec();
                expected.push((
                    result,
                    state
                        .get::<mlua::LuaString>("mode")
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                    state
                        .get::<mlua::LuaString>("copy")
                        .unwrap()
                        .as_bytes()
                        .to_vec(),
                    std::mem::take(&mut *actions.lock().unwrap()),
                ));
            }
        }
        (group, separate, expected)
    };
    assert!(dropped.load(Ordering::SeqCst));
    for (step, (result, mode, copy, expected_actions)) in expected.into_iter().enumerate() {
        let inputs = [Datum::Bytes(
            [b"Right".as_slice(), b"Left", b"Empty"][(step / 3) % 3],
        )];
        let mut actions = Vec::new();
        let actual = group
            .run(step % 3, &inputs, |tag, args| {
                assert_eq!(tag, 7);
                let Some(Datum::Bytes(bytes)) = args.get(1) else {
                    panic!("byte action")
                };
                actions.push(bytes.to_vec());
            })
            .unwrap()
            .unwrap();
        assert_eq!(actual, Datum::Bytes(&result));
        assert_eq!(actions, expected_actions);
        assert_eq!(group.cell(0), Some(Datum::Bytes(&mode)));
        assert_eq!(group.cell(1), Some(Datum::Bytes(&copy)));
    }
    assert_eq!(separate.cell(0), Some(Datum::Bytes(b"idle")));
    assert_eq!(
        separate.run(&[Datum::Bytes(b"Right")], |_, _| {}).unwrap(),
        Datum::Bytes(b"\0armed")
    );
    assert_eq!(separate.cell(1), Some(Datum::Bytes(b"idle")));
}

#[test]
fn byte_cells_reject_transient_inputs_type_changes_and_incompatible_initial_contents() {
    let lua = Lua::new();
    let mut bindings = Bindings {
        arguments: vec![Binding::Object(0)],
        upvalues: vec![Binding::Object(2)],
        cells: vec![Cell::Bytes(0)],
        strings: vec![Box::from(&b"initial"[..])],
        fields: vec![input("button", 0, Types::BYTES), property(2, "mode", 0)],
        ..Bindings::default()
    };
    for source in ["state.mode=e.button", "state.mode=17"] {
        let f: Function = lua
            .load(format!("local state={{}};return function(e){source} end"))
            .eval()
            .unwrap();
        assert!(program::compile(&f, &bindings).is_err(), "{source}");
    }
    let f:Function=lua.load("local state={};return function(e)local next=state.mode;if e.button=='Right' then next='next' end;state.mode=next;return next end").eval().unwrap();
    let a = program::compile(&f, &bindings).unwrap();
    bindings.strings[0] = Box::from(&b"different"[..]);
    let b = program::compile(&f, &bindings).unwrap();
    assert!(program::Group::new(vec![a, b]).is_err());
    bindings.cells[0] = Cell::Bytes(usize::MAX);
    assert!(matches!(
        program::compile(&f, &bindings),
        Err(program::CompileError::InvalidBindings("cell string index"))
    ));
}

#[test]
fn borrowed_byte_values_reuse_storage_without_merging_cells_or_changing_equality() {
    for count in [3, 16, 17, 64] {
        let (mut machine, payload) = {
            let lua = Lua::new();
            let callback: Function = lua.load("local s={};return function(e)local same=s.a==s.b;s.a=e.down and 'next' or s.a;return same and s.b==e.button end").eval().unwrap();
            let mut bindings = Bindings {
                arguments: vec![Binding::Object(0)],
                upvalues: vec![Binding::Object(1)],
                cells: vec![Cell::Bytes(0); count],
                strings: vec![Box::from(&b"idle"[..])],
                fields: vec![
                    input("button", 0, Types::BYTES),
                    input("down", 1, Types::BOOLEAN),
                    property(1, "a", 0),
                    property(1, "b", 1),
                ],
                ..Bindings::default()
            };
            let repeated = program::compile(&callback, &bindings).unwrap();
            bindings.strings = (0..count)
                .map(|index| format!("{index:04}").into_bytes().into_boxed_slice())
                .collect();
            bindings.cells = (0..count).map(Cell::Bytes).collect();
            let distinct = program::compile(&callback, &bindings).unwrap();
            (
                repeated.machine(),
                (repeated.retained_bytes(), distinct.retained_bytes()),
            )
        };
        assert!(payload.0 < payload.1);
        for (button, down, expected) in [
            (b"idle".as_slice(), false, true),
            (b"idle", true, true),
            (b"idle", false, false),
            (b"next", false, false),
        ] {
            assert_eq!(
                machine
                    .run(&[Datum::Bytes(button), Datum::Boolean(down)], |_, _| {})
                    .unwrap(),
                Datum::Boolean(expected)
            );
            assert_eq!(machine.cell(1), Some(Datum::Bytes(b"idle")));
        }
    }
}

#[test]
fn prepared_schemas_preserve_binary_fields_cells_errors_and_released_loading_storage() {
    for count in [4, 31, 32, 65, 256] {
        let dropped = Arc::new(AtomicBool::new(false));
        let (mut ordinary, mut prepared) = {
            let lua = Lua::new();
            lua.set_app_data(Dropped(dropped.clone()));
            let callback:Function=lua.load("local t={held=false};return function(e)t.held=not t.held;return e['\\255\\000value'] end").eval().unwrap();
            let mut bindings = Bindings {
                arguments: vec![Binding::Object(0)],
                upvalues: vec![Binding::Object(2)],
                cells: vec![Cell::Boolean(false)],
                fields: vec![
                    property(2, "held", 0),
                    Field {
                        object: 0,
                        name: b"\xff\0value".as_slice().into(),
                        value: FieldValue::Value(Binding::Input {
                            slot: 0,
                            types: Types::NUMBER,
                        }),
                    },
                ],
                ..Bindings::default()
            };
            for index in bindings.fields.len()..count {
                bindings.fields.push(Field {
                    object: index as u32,
                    name: format!("unused{index}").into_bytes().into(),
                    value: FieldValue::Value(Binding::Literal(program::Literal::Nil)),
                });
            }
            bindings.fields.rotate_left(count / 2);
            let bytes = callback.dump(false);
            let normal = program::compile(&callback, &bindings).unwrap();
            let compiler = program::Compiler::new(&bindings).unwrap();
            let current = compiler
                .compile_bytecode(&bytes, &bindings.arguments, &bindings.upvalues)
                .unwrap();
            assert_eq!(
                compiler
                    .compile(&callback, &bindings.arguments, &[])
                    .unwrap_err()
                    .to_string(),
                program::compile(
                    &callback,
                    &Bindings {
                        upvalues: vec![],
                        ..bindings.clone()
                    }
                )
                .unwrap_err()
                .to_string()
            );
            drop(compiler);
            bindings.fields.push(bindings.fields[0].clone());
            assert!(matches!(
                program::Compiler::new(&bindings),
                Err(program::CompileError::InvalidBindings(
                    "duplicate object field"
                ))
            ));
            assert!(matches!(
                program::compile(&callback, &bindings),
                Err(program::CompileError::InvalidBindings(
                    "duplicate object field"
                ))
            ));
            (normal.machine(), current.machine())
        };
        assert!(dropped.load(Ordering::SeqCst));
        for bits in [
            0,
            1,
            (-0.0f64).to_bits(),
            f64::INFINITY.to_bits(),
            0x7ff8_0000_0000_1234,
        ] {
            let inputs = [Datum::Number(f64::from_bits(bits))];
            let a = ordinary.run(&inputs, |_, _| panic!("no actions")).unwrap();
            let b = prepared.run(&inputs, |_, _| panic!("no actions")).unwrap();
            let (Datum::Number(a), Datum::Number(b)) = (a, b) else {
                panic!("number result")
            };
            assert_eq!(a.to_bits(), bits);
            assert_eq!(a.to_bits(), b.to_bits());
            assert_eq!(ordinary.cell(0), prepared.cell(0));
        }
    }
}

#[test]
fn identical_group_code_shares_storage_without_merging_execution_or_machine_state() {
    let closed = Arc::new(AtomicBool::new(false));
    let (mut group, mut separate, expected, unshared_payload) = {
        let lua = Lua::new();
        lua.set_app_data(Dropped(closed.clone()));
        let f: Function = lua
            .load(
                "local s={total=0};return function(delta) s.total=s.total+delta;return s.total end",
            )
            .eval()
            .unwrap();
        let bindings = Bindings {
            arguments: vec![Binding::Input {
                slot: 0,
                types: Types::NUMBER,
            }],
            upvalues: vec![Binding::Object(1)],
            cells: vec![Cell::Number(0.0)],
            fields: vec![property(1, "total", 0)],
            ..Bindings::default()
        };
        let programs = (0..16)
            .map(|_| program::compile(&f, &bindings).unwrap())
            .collect::<Vec<_>>();
        let unshared_payload = programs
            .iter()
            .map(program::Program::retained_bytes)
            .sum::<usize>();
        let separate = programs[7].machine();
        let group = program::Group::new(programs).unwrap();
        let mut expected = Vec::new();
        for step in 0..16 {
            let delta = [-0.0, 0.25, -0.5, 0.125][step % 4];
            for index in 0..16 {
                expected.push((index, delta, f.call::<f64>(delta).unwrap().to_bits()));
            }
        }
        (group, separate, expected, unshared_payload)
    };
    assert!(closed.load(Ordering::SeqCst));
    assert_eq!(group.len(), 16);
    assert!(group.retained_bytes() < unshared_payload);
    for (index, delta, bits) in expected {
        let Datum::Number(value) = group
            .run(index, &[Datum::Number(delta)], |_, _| panic!("no action"))
            .unwrap()
            .unwrap()
        else {
            panic!("number")
        };
        assert_eq!(value.to_bits(), bits);
        let Some(Datum::Number(cell)) = group.cell(0) else {
            panic!("number cell")
        };
        assert_eq!(cell.to_bits(), bits);
    }
    assert_eq!(
        separate
            .run(&[Datum::Number(1.0)], |_, _| panic!("no action"))
            .unwrap(),
        Datum::Number(1.0)
    );
}

#[test]
fn shared_group_code_keeps_mutable_byte_arena_indices_after_vm_destruction() {
    let closed = Arc::new(AtomicBool::new(false));
    let (mut group, expected) = {
        let lua = Lua::new();
        lua.set_app_data(Dropped(closed.clone()));
        let f:Function=lua.load("local s={mode='idle'};return function(e) if e then s.mode='armed' else s.mode='idle' end;return s.mode end").eval().unwrap();
        let bindings = Bindings {
            arguments: vec![Binding::Input {
                slot: 0,
                types: Types::BOOLEAN,
            }],
            upvalues: vec![Binding::Object(1)],
            cells: vec![Cell::Bytes(0)],
            strings: vec![b"idle".as_slice().into()],
            fields: vec![property(1, "mode", 0)],
            ..Bindings::default()
        };
        let programs = (0..16)
            .map(|_| program::compile(&f, &bindings).unwrap())
            .collect::<Vec<_>>();
        let group = program::Group::new(programs).unwrap();
        let expected = (0..64)
            .map(|step| {
                let input = step % 3 == 0;
                let value = f.call::<mlua::String>(input).unwrap();
                (input, value.as_bytes().to_vec())
            })
            .collect::<Vec<_>>();
        (group, expected)
    };
    assert!(closed.load(Ordering::SeqCst));
    for (step, (input, expected)) in expected.into_iter().enumerate() {
        assert_eq!(
            group
                .run(step % 16, &[Datum::Boolean(input)], |_, _| panic!(
                    "no action"
                ))
                .unwrap()
                .unwrap(),
            Datum::Bytes(&expected)
        );
        assert_eq!(group.cell(0), Some(Datum::Bytes(&expected)));
    }
}

#[test]
fn group_code_sharing_preserves_independent_cells_constants_and_object_ids() {
    let closed = Arc::new(AtomicBool::new(false));
    let (mut independent, mut constants, expected) = {
        let lua = Lua::new();
        lua.set_app_data(Dropped(closed.clone()));
        let mut programs = Vec::new();
        for index in 0..16 {
            let f: Function = lua.load("local s={total=0};return function(delta) s.total=s.total+delta;return s.total end").eval().unwrap();
            programs.push(
                program::compile(
                    &f,
                    &Bindings {
                        arguments: vec![Binding::Input {
                            slot: 0,
                            types: Types::NUMBER,
                        }],
                        upvalues: vec![Binding::Object(index as u32 + 1)],
                        cells: vec![Cell::Number(0.0); 16],
                        fields: vec![property(index as u32 + 1, "total", index)],
                        ..Bindings::default()
                    },
                )
                .unwrap(),
            );
        }
        let independent = program::Group::new(programs).unwrap();
        let objects = [lua.create_table().unwrap(), lua.create_table().unwrap()];
        let mut programs = Vec::new();
        let mut expected = Vec::new();
        for index in 0..16 {
            let (value, binding, bits, bytes, object) = match index % 8 {
                kind @ 0..=3 => {
                    let bits = [
                        0x8000_0000_0000_0000,
                        0,
                        0x7ff8_0000_0000_1234,
                        0x7ff8_0000_0000_5678,
                    ][kind];
                    let value = f64::from_bits(bits);
                    (
                        Value::Number(value),
                        Binding::Literal(program::Literal::Number(value)),
                        Some(bits),
                        None,
                        None,
                    )
                }
                kind @ 4..=5 => {
                    let bytes = if kind == 4 {
                        &b"\0\xffalpha"[..]
                    } else {
                        &b"\0\xffbeta"[..]
                    };
                    (
                        Value::String(lua.create_string(bytes).unwrap()),
                        Binding::Literal(program::Literal::Bytes(bytes.into())),
                        None,
                        Some(bytes.to_vec()),
                        None,
                    )
                }
                kind => {
                    let id = kind as u32 + 35;
                    (
                        Value::Table(objects[kind - 6].clone()),
                        Binding::Object(id),
                        None,
                        None,
                        Some(id),
                    )
                }
            };
            lua.globals().set("N", value).unwrap();
            let f: Function = lua
                .load("local n=N;return function() return n end")
                .eval()
                .unwrap();
            if let Some(bits) = bits {
                assert_eq!(f.call::<f64>(()).unwrap().to_bits(), bits);
            }
            programs.push(
                program::compile(
                    &f,
                    &Bindings {
                        upvalues: vec![binding],
                        ..Bindings::default()
                    },
                )
                .unwrap(),
            );
            expected.push((bits, bytes, object));
        }
        (
            independent,
            program::Group::new(programs).unwrap(),
            expected,
        )
    };
    assert!(closed.load(Ordering::SeqCst));
    for delta in [0.25, -0.5] {
        for index in 0..16 {
            assert_eq!(
                independent
                    .run(index, &[Datum::Number(delta)], |_, _| panic!("no action"))
                    .unwrap()
                    .unwrap(),
                Datum::Number(if delta > 0.0 { 0.25 } else { -0.25 })
            );
        }
    }
    for (index, (bits, bytes, object)) in expected.into_iter().enumerate() {
        match constants
            .run(index, &[], |_, _| panic!("no action"))
            .unwrap()
            .unwrap()
        {
            Datum::Number(value) => assert_eq!(Some(value.to_bits()), bits),
            Datum::Bytes(value) => assert_eq!(Some(value.to_vec()), bytes),
            Datum::Object(value) => assert_eq!(Some(value), object),
            other => panic!("constant {other:?}"),
        }
    }
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
fn shared_signatures_check_each_field_receiver_and_preserve_distinct_return_objects() {
    const PARAMETERS: &[Parameter] = &[Parameter::Receiver, Parameter::Type(Types::NUMBER)];
    for borrowed in [false, true] {
        let lua = Lua::new();
        let callback: Function = lua
            .load("local a={};return function(e) return a:step(e):step(-e) end")
            .eval()
            .unwrap();
        let mut bindings = Bindings {
            arguments: vec![Binding::Input {
                slot: 0,
                types: Types::NUMBER,
            }],
            upvalues: vec![Binding::Object(7)],
            fields: [(7, 9, 1), (9, 7, 2)]
                .map(|(object, result, tag)| Field {
                    object,
                    name: b"step".as_slice().into(),
                    value: FieldValue::Action(Action {
                        tag,
                        parameters: if borrowed {
                            PARAMETERS.into()
                        } else {
                            PARAMETERS.to_vec().into()
                        },
                        result: Some(Binding::Object(result)),
                    }),
                })
                .into(),
            ..Bindings::default()
        };
        let mut machine = program::compile(&callback, &bindings).unwrap().machine();
        let wrong: Function = lua
            .load("local a={};return function(e) return a.step(e,1) end")
            .eval()
            .unwrap();
        bindings.arguments = vec![Binding::Object(9)];
        assert!(program::compile(&wrong, &bindings).is_err());
        drop(wrong);
        drop(callback);
        drop(bindings);
        drop(lua);
        let mut actions = Vec::new();
        assert_eq!(
            machine
                .run(&[Datum::Number(-0.0)], |tag, args| {
                    let Some(Datum::Number(value)) = args.get(1) else {
                        panic!("numeric argument")
                    };
                    let Some(Datum::Object(receiver)) = args.get(0) else {
                        panic!("object receiver")
                    };
                    actions.push((tag, receiver, value.to_bits()));
                })
                .unwrap(),
            Datum::Object(7)
        );
        assert_eq!(
            actions,
            vec![
                (1, 7, (-0.0f64).to_bits()),
                (2, 9, 0.0f64.to_bits()),
            ]
        );
    }
}

#[test]
fn reusable_dumps_keep_byte_state_rebinding_independent_of_the_loading_vm() {
    let lua = Lua::new();
    let callback: Function = lua
        .load("local s={};return function() s.mode='armed';return s.mode end")
        .eval()
        .unwrap();
    let bytes = program::dump(&callback).unwrap();
    let native = lua.create_function(|_, ()| Ok(())).unwrap();
    assert!(matches!(
        program::dump(&native),
        Err(program::CompileError::Unsupported {
            reason: "native closure",
            ..
        })
    ));
    drop(native);
    drop(callback);
    drop(lua);
    let mut bindings = Bindings {
        upvalues: vec![Binding::Object(3)],
        fields: vec![Field {
            object: 3,
            name: b"mode".as_slice().into(),
            value: FieldValue::Value(Binding::Literal(program::Literal::Bytes(
                b"idle".as_slice().into(),
            ))),
        }],
        ..Bindings::default()
    };
    assert!(program::compile_bytecode(&bytes, &bindings).is_err());
    bindings.fields[0].value = FieldValue::Value(Binding::Cell(0));
    bindings.cells.push(Cell::Bytes(0));
    bindings.strings.push(b"idle".as_slice().into());
    let mut machine = program::Compiler::new(&bindings)
        .unwrap()
        .compile_bytecode(&bytes, &[], &bindings.upvalues)
        .unwrap()
        .machine();
    drop(bytes);
    drop(bindings);
    for _ in 0..3 {
        assert_eq!(
            machine.run(&[], |_, _| panic!("no action")).unwrap(),
            Datum::Bytes(b"armed")
        );
    }
}

#[test]
fn truthiness_execution_matches_lua_after_vm_destruction_and_keeps_input_errors() {
    let closed = Arc::new(AtomicBool::new(false));
    let (programs, cases) = {
        let lua = Lua::new();
        let marker = Dropped(closed.clone());
        lua.globals()
            .set(
                "close_marker",
                lua.create_function(move |_, ()| {
                    let _ = &marker;
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
        let callback: Function = lua.load("return function(e) return e end").eval().unwrap();
        let truth: Function = lua
            .load("return function(e) return not not e end")
            .eval()
            .unwrap();
        let cases = [
            (Datum::Nil, Value::Nil),
            (Datum::Boolean(false), Value::Boolean(false)),
            (Datum::Boolean(true), Value::Boolean(true)),
            (Datum::Number(-0.0), Value::Number(-0.0)),
            (Datum::Number(f64::NAN), Value::Number(f64::NAN)),
            (
                Datum::Bytes(b""),
                Value::String(lua.create_string(b"").unwrap()),
            ),
            (
                Datum::Bytes(b"\xff\0"),
                Value::String(lua.create_string(b"\xff\0").unwrap()),
            ),
        ]
        .into_iter()
        .map(|(input, value)| (input, truth.call::<bool>(value).unwrap()))
        .collect::<Vec<_>>();
        let programs = [
            Binding::Input {
                slot: 0,
                types: Types::NIL | Types::BOOLEAN | Types::NUMBER | Types::BYTES,
            },
            Binding::Object(42),
            Binding::Literal(program::Literal::Nil),
        ]
        .into_iter()
        .map(|argument| {
            program::compile(
                &callback,
                &Bindings {
                    arguments: vec![argument],
                    ..Bindings::default()
                },
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
        (programs, cases)
    };
    assert!(closed.load(Ordering::SeqCst));
    let mut machine = programs[0].machine();
    let mut group = program::Group::new(programs.clone()).unwrap();
    let mut reference = program::Group::new(programs).unwrap();
    for _ in 0..3 {
        for &(input, expected) in &cases {
            assert_eq!(
                machine.run_truthy(&[input], |_, _| panic!("no action")),
                Ok(expected)
            );
            assert_eq!(
                group.run_truthy(0, &[input], |_, _| panic!("no action")),
                Some(Ok(expected))
            );
            assert_eq!(
                reference
                    .run(0, &[input], |_, _| panic!("no action"))
                    .unwrap()
                    .unwrap()
                    .is_truthy(),
                expected
            );
            assert_eq!(
                group.run_truthy(1, &[], |_, _| panic!("no action")),
                Some(Ok(true))
            );
            assert_eq!(
                group.run_truthy(2, &[], |_, _| panic!("no action")),
                Some(Ok(false))
            );
        }
    }
    for (input, expected) in [
        (&[][..], program::RunError::MissingInput(0)),
        (&[Datum::Object(1)][..], program::RunError::InputType(0)),
    ] {
        assert_eq!(
            machine.run_truthy(input, |_, _| panic!("no action")),
            Err(expected)
        );
        assert_eq!(
            group.run_truthy(0, input, |_, _| panic!("no action")),
            Some(Err(expected))
        );
    }
    assert_eq!(group.run_truthy(3, &[], |_, _| panic!("no action")), None);
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
