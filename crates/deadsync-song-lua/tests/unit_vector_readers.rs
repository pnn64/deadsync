//! Original vector batch comparisons on native Rust readers and ordinary Lua raw reads.
use deadlib_lua::{Error, HostCall, NativeCallStatus, Root, Value};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{RustSongRuntime, test_support},
};
use std::sync::{Arc, Mutex};
#[path = "support/baked_program.rs"]
mod api;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Vectors {
    v2: Option<[u32; 2]>,
    v3: Option<[u32; 3]>,
    v4: Option<[u32; 4]>,
    v5: Option<[u32; 5]>,
}
type VertexBits = Option<[[u32; 4]; 4]>;
fn runtime() -> RustSongRuntime {
    RustSongRuntime::new(&SongLuaCompileContext::new(".", "vector reader unit")).unwrap()
}
fn vectors_reader(call: &mut HostCall<'_>) -> Result<(), Error> {
    let output = call
        .function_data::<Arc<Mutex<Option<Vectors>>>>()
        .unwrap()
        .clone();
    let Value::Table(t) = call.captures()[2] else {
        unreachable!()
    };
    let value = Vectors {
        v2: test_support::fixture_vector_components::<2>(call, t)?.map(|a| a.map(f32::to_bits)),
        v3: test_support::fixture_vector_components::<3>(call, t)?.map(|a| a.map(f32::to_bits)),
        v4: test_support::fixture_vector_components::<4>(call, t)?.map(|a| a.map(f32::to_bits)),
        v5: test_support::fixture_vector_components::<5>(call, t)?.map(|a| a.map(f32::to_bits)),
    };
    *output.lock().unwrap() = Some(value);
    Ok(())
}
fn colors_reader(call: &mut HostCall<'_>) -> Result<(), Error> {
    let output = call
        .function_data::<Arc<Mutex<Option<VertexBits>>>>()
        .unwrap()
        .clone();
    let Value::Table(t) = call.captures()[2] else {
        unreachable!()
    };
    let value = test_support::fixture_vertex_colors(call, t)?
        .map(|rows| rows.map(|row| row.map(f32::to_bits)));
    *output.lock().unwrap() = Some(value);
    Ok(())
}
fn model(host: &mut RustSongRuntime) -> Value {
    api::eval(
        host.vm_mut(),
        r#"return function(t,n)
    local out={}
    for i=1,n do local value=tonumber(rawget(t,i));if value==nil then return false end;out[i]=value end
    return out
end"#,
    )[0]
}
fn index(host: &mut RustSongRuntime, owner: Value, index: usize) -> Value {
    let Value::Table(t) = owner else {
        panic!("fixture table expected")
    };
    let value = host
        .vm_mut()
        .raw_table_get(t, Value::Number(index as f64))
        .unwrap();
    host.vm_mut().pin(value).unwrap();
    value
}
fn set_index(host: &mut RustSongRuntime, owner: Value, index: usize, value: Value) {
    let Value::Table(t) = owner else {
        panic!("fixture table expected")
    };
    host.vm_mut()
        .raw_table_set(t, Value::Number(index as f64), value)
        .unwrap();
}
fn model_components<const N: usize>(
    host: &mut RustSongRuntime,
    model: Value,
    table: Value,
) -> Option<[u32; N]> {
    let value = api::call_one(host.vm_mut(), model, &[table, Value::Number(N as f64)]);
    if value == Value::Boolean(false) {
        return None;
    }
    Some(std::array::from_fn(|i| {
        (api::as_number(index(host, value, i + 1)) as f32).to_bits()
    }))
}
fn expected_vectors(host: &mut RustSongRuntime, model: Value, table: Value) -> Vectors {
    Vectors {
        v2: model_components(host, model, table),
        v3: model_components(host, model, table),
        v4: model_components(host, model, table),
        v5: model_components(host, model, table),
    }
}
fn expected_colors(host: &mut RustSongRuntime, model: Value, colors: Value) -> VertexBits {
    let mut out = [[0; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        let value = index(host, colors, i + 1);
        if !matches!(value, Value::Table(_)) {
            return None;
        }
        *row = model_components(host, model, value)?;
    }
    Some(out)
}
fn native(host: &mut RustSongRuntime, function: Root) {
    let vm = host.vm_mut();
    let function = vm.rooted(function).unwrap();
    vm.call(function, &[]).unwrap();
}
fn compare_vectors(
    host: &mut RustSongRuntime,
    function: Root,
    output: &Arc<Mutex<Option<Vectors>>>,
    model: Value,
    table: Value,
) {
    let expected = expected_vectors(host, model, table);
    native(host, function);
    assert_eq!(output.lock().unwrap().unwrap(), expected);
}
fn compare_colors(
    host: &mut RustSongRuntime,
    function: Root,
    output: &Arc<Mutex<Option<VertexBits>>>,
    model: Value,
    table: Value,
) {
    let expected = expected_colors(host, model, table);
    native(host, function);
    assert_eq!(output.lock().unwrap().unwrap(), expected);
}
fn vector_probe(host: &mut RustSongRuntime, table: Value) -> (Root, Arc<Mutex<Option<Vectors>>>) {
    let output = Arc::new(Mutex::new(None));
    let function =
        test_support::prepare_actor_reader(host, table, vectors_reader, output.clone()).unwrap();
    (function, output)
}
fn colors_probe(
    host: &mut RustSongRuntime,
    table: Value,
) -> (Root, Arc<Mutex<Option<VertexBits>>>) {
    let output = Arc::new(Mutex::new(None));
    let function =
        test_support::prepare_actor_reader(host, table, colors_reader, output.clone()).unwrap();
    (function, output)
}
fn vector_coroutine_reader(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Value::Table(t) = call.arguments()[0] else {
        return Err(call.error(b"table expected"));
    };
    let Some(values) = test_support::fixture_vector_components::<4>(call, t)? else {
        return call.push(Value::Nil);
    };
    let table = call.try_new_table()?;
    call.with_roots(&[Value::Table(table)], |call| {
        for (i, value) in values.into_iter().enumerate() {
            call.raw_table_set(
                table,
                Value::Number((i + 1) as f64),
                Value::Number(value as f64),
            )?;
        }
        call.push(Value::Table(table))
    })
}

#[test]
fn vector_batches_preserve_bits_coercions_holes_and_raw_lookup() {
    let mut host = runtime();
    let table = api::eval(host.vm_mut(), "return {1.0,2.0,3.0,4.0,5.0}")[0];
    let model = model(&mut host);
    let (function, output) = vector_probe(&mut host, table);
    let vm = host.vm_mut();
    let values = [
        Value::Nil,
        Value::Boolean(false),
        api::table(vm),
        api::string(vm, " -0.0 "),
        api::string(vm, "0x10"),
        api::string(vm, "not a number"),
        api::string(vm, [b'1', 0, b'x']),
        api::string(vm, [0xff]),
        Value::Number(f64::INFINITY),
        Value::Number(f64::NEG_INFINITY),
        Value::Number(f64::from_bits(0xfff8_2468_0000_1234)),
        Value::Number(-0.0),
    ];
    for value in values {
        for position in 1..=6 {
            set_index(&mut host, table, position, value);
            compare_vectors(&mut host, function, &output, model, table);
            set_index(&mut host, table, position, Value::Number(position as f64));
        }
    }
    let missing = api::eval(host.vm_mut(), ORIGINAL_MISSING_VECTOR)[0];
    let (missing_function, missing_output) = vector_probe(&mut host, missing);
    compare_vectors(&mut host, missing_function, &missing_output, model, missing);
    assert_eq!(
        api::as_number(host.vm_mut().get_global(b"lookups").unwrap()) as i32,
        0
    );
    let bad = api::eval(host.vm_mut(), "return {false}")[0];
    let (bad_function, bad_output) = vector_probe(&mut host, bad);
    let expected = expected_vectors(&mut host, model, table);
    let bad_expected = expected_vectors(&mut host, model, bad);
    let function_value = host.vm_mut().rooted(function).unwrap();
    let bad_value = host.vm_mut().rooted(bad_function).unwrap();
    host.vm_mut()
        .with_native_stack(|stack| {
            stack.set_top(0)?;
            assert!(stack.check_stack(8));
            stack.push(Value::Number(77.))?;
            let top = stack.top();
            stack.push(function_value)?;
            assert_eq!(
                stack.protected_call(0, Some(0), None)?,
                NativeCallStatus::Ok
            );
            assert_eq!(output.lock().unwrap().unwrap(), expected);
            stack.push(bad_value)?;
            assert_eq!(
                stack.protected_call(0, Some(0), None)?,
                NativeCallStatus::Ok
            );
            assert_eq!(bad_output.lock().unwrap().unwrap(), bad_expected);
            assert_eq!(stack.top(), top);
            assert_eq!(stack.value(-1), Some(Value::Number(77.)));
            stack.set_top(top - 1)?;
            Ok(())
        })
        .unwrap();
    let callback =
        test_support::prepare_actor_reader(&mut host, Value::Nil, vector_coroutine_reader, ())
            .unwrap();
    let value = host.vm_mut().rooted(callback).unwrap();
    host.vm_mut().set_global(b"read_vector", value).unwrap();
    let result = api::eval(
        host.vm_mut(),
        "return coroutine.resume(coroutine.create(function() return read_vector{1,2,3,4} end))",
    );
    let ok = api::as_bool(result[0]);
    let result = result[1];
    assert!(ok);
    assert_eq!(
        model_components::<4>(&mut host, model, result),
        Some([1.0_f32, 2.0, 3.0, 4.0].map(f32::to_bits))
    );
}
#[test]
fn color_batches_preserve_nested_failures_aliases_and_stack() {
    let mut host = runtime();
    let row = api::eval(host.vm_mut(), "return {1.0,0.5,0.25,0.75,99.0}")[0];
    let colors = api::table(host.vm_mut());
    for i in 1..=5 {
        set_index(&mut host, colors, i, row);
    }
    let model = model(&mut host);
    let (function, output) = colors_probe(&mut host, colors);
    compare_colors(&mut host, function, &output, model, colors);
    for position in 1..=5 {
        let empty = api::table(host.vm_mut());
        for value in [Value::Nil, Value::Boolean(false), Value::Number(2.0), empty] {
            set_index(&mut host, colors, position, value);
            compare_colors(&mut host, function, &output, model, colors);
        }
        set_index(&mut host, colors, position, row);
    }
    for position in 1..=5 {
        let text = api::string(host.vm_mut(), " 0.25 ");
        for value in [
            Value::Nil,
            Value::Boolean(true),
            text,
            Value::Number(f64::NAN),
            Value::Number(f64::INFINITY),
            Value::Number(-0.0),
        ] {
            set_index(&mut host, row, position, value);
            compare_colors(&mut host, function, &output, model, colors);
        }
        set_index(&mut host, row, position, Value::Number(0.5));
    }
    set_index(&mut host, colors, 3, Value::Boolean(false));
    let expected = expected_colors(&mut host, model, colors);
    let function_value = host.vm_mut().rooted(function).unwrap();
    host.vm_mut()
        .with_native_stack(|stack| {
            stack.set_top(0)?;
            assert!(stack.check_stack(8));
            stack.push(Value::Number(42.))?;
            let top = stack.top();
            stack.push(function_value)?;
            assert_eq!(
                stack.protected_call(0, Some(0), None)?,
                NativeCallStatus::Ok
            );
            assert_eq!(output.lock().unwrap().unwrap(), expected);
            assert_eq!(stack.top(), top);
            assert_eq!(stack.value(-1), Some(Value::Number(42.)));
            stack.set_top(top - 1)?;
            Ok(())
        })
        .unwrap();
    host.vm_mut()
        .set_global(b"color_lookups", Value::Number(0.))
        .unwrap();
    let missing = api::eval(host.vm_mut(), ORIGINAL_MISSING_COLOR)[0];
    let (missing_function, missing_output) = colors_probe(&mut host, missing);
    let expected = expected_colors(&mut host, model, missing);
    native(&mut host, missing_function);
    assert_eq!(missing_output.lock().unwrap().unwrap(), expected);
    set_index(&mut host, colors, 3, missing);
    compare_colors(&mut host, function, &output, model, colors);
    assert_eq!(
        api::as_number(host.vm_mut().get_global(b"color_lookups").unwrap()) as i32,
        0
    );
    assert_eq!(
        api::as_number(api::eval(host.vm_mut(), "return 6*7")[0]) as i32,
        42
    );
}

const ORIGINAL_MISSING_VECTOR: &str = r#"lookups=0; return setmetatable({}, {__index=function()
            lookups=lookups+1; error('raw reads must not call __index') end})"#;
const ORIGINAL_MISSING_COLOR: &str = r#"return setmetatable({}, {__index=function()
            color_lookups=color_lookups+1; error('unexpected color lookup') end})"#;
