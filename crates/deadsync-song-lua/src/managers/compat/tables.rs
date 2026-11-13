//! Existing sequence helpers and table extensions from the song host.
use super::*;
use std::collections::HashSet;

#[derive(Clone, Copy)]
enum Operation {
    Rotate(bool),
    Map,
    Deduplicate,
    Stringify,
    Describe,
    Find,
    Copy,
    Split(bool),
    Clean,
    Basename,
    Round,
    Clamp,
    Mod,
}

fn member(vm: &mut Vm, table: TableId, name: &str, value: Value) -> Result<(), Error> {
    let root = vm.pin(value)?;
    let result = crate::put(vm, table, name, value);
    vm.unpin(root)?;
    result
}
pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    let globals = vm.get_global(b"_G")?;
    for (name, op) in [
        ("map", Operation::Map),
        ("deduplicate", Operation::Deduplicate),
        ("stringify", Operation::Stringify),
        ("TableToString", Operation::Describe),
        ("FindInTable", Operation::Find),
        ("DeepCopy", Operation::Copy),
        ("split", Operation::Split(true)),
        ("cleanGSub", Operation::Clean),
        ("Basename", Operation::Basename),
    ] {
        let value = vm.new_host_function_with_data(dispatch, &[globals], op)?;
        bind(vm, name, value)?;
    }
    for (group, entries) in [
        (
            "table",
            vec![
                ("rotate_left", Operation::Rotate(true)),
                ("rotate_right", Operation::Rotate(false)),
            ],
        ),
        ("string", vec![("split", Operation::Split(false))]),
        (
            "math",
            vec![
                ("round", Operation::Round),
                ("clamp", Operation::Clamp),
                ("mod", Operation::Mod),
            ],
        ),
    ] {
        let Value::Table(table) = vm.get_global(group.as_bytes())? else {
            unreachable!()
        };
        for (name, op) in entries {
            let key = vm.try_intern(name.as_bytes())?;
            if group == "math" && vm.raw_table_get(table, key)? != Value::Nil {
                continue;
            }
            let value = vm.new_host_function_with_data(dispatch, &[globals], op)?;
            member(vm, table, name, value)?;
        }
    }
    let Value::Table(table) = vm.get_global(b"table")? else {
        unreachable!()
    };
    let find = vm.execute(&deadlib_lua::compile_named(
        br#"return function(t,sFind)
        for key,value in pairs(t) do if value==sFind then return key end end
        return nil
    end"#,
        b"=existing song table.find",
    )?)?[0];
    member(vm, table, "find", find)?;
    let shuffle = vm.execute(&deadlib_lua::compile_named(
        br#"return function(input)
        local result={}
        for i=1,#input do table.insert(result,math.random(i),input[i]) end
        return result
    end"#,
        b"=existing song table.shuffle",
    )?)?[0];
    member(vm, table, "shuffle", shuffle)?;
    bind(vm, "tableshuffle", shuffle)?;
    let Value::Table(string) = vm.get_global(b"string")? else {
        unreachable!()
    };
    let key = vm.try_intern(b"gfind")?;
    if vm.raw_table_get(string, key)? == Value::Nil {
        let gmatch = vm.try_intern(b"gmatch")?;
        let value = vm.raw_table_get(string, gmatch)?;
        member(vm, string, "gfind", value)?;
    }
    Ok(())
}
fn equal(call: &HostCall<'_>, a: Value, b: Value) -> bool {
    match (a, b) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::Table(a), Value::Table(b)) => a == b,
        (Value::String(a), Value::String(b)) => {
            let a = call.string_bytes(a).unwrap();
            let b = call.string_bytes(b).unwrap();
            std::str::from_utf8(a).is_ok() && std::str::from_utf8(b).is_ok() && a == b
        }
        _ => false,
    }
}
#[derive(Hash, Eq, PartialEq)]
enum Key {
    Boolean(bool),
    Number(u64),
    Text(Vec<u8>),
    Table(TableId),
}
fn key(call: &HostCall<'_>, value: Value) -> Option<Key> {
    match value {
        Value::Boolean(v) => Some(Key::Boolean(v)),
        Value::Number(v) if !v.is_nan() => Some(Key::Number(if v == 0. { 0 } else { v.to_bits() })),
        Value::String(v) => {
            let bytes = call.string_bytes(v).unwrap();
            std::str::from_utf8(bytes)
                .ok()
                .map(|_| Key::Text(bytes.to_vec()))
        }
        Value::Table(v) => Some(Key::Table(v)),
        _ => None,
    }
}
fn dispatch(call: &mut HostCall<'_>) -> Result<(), Error> {
    let op = *call.function_data::<Operation>().unwrap();
    let value = match op {
        Operation::Rotate(left) => {
            let Some(input) = (match arg(call, 0) {
                Value::Table(t) => Some(t),
                _ => None,
            }) else {
                let output = call.try_new_table()?;
                return call.push(Value::Table(output));
            };
            let len = call.raw_table_len(input)? as i64;
            let shift = i64::from(super::super::catalog::integer(call, arg(call, 1)).unwrap_or(1));
            Value::Table(object(call, |call, out| {
                for index in 1..=len {
                    let source = if left {
                        (index + shift - 1).rem_euclid(len) + 1
                    } else {
                        (index - shift - 1).rem_euclid(len) + 1
                    };
                    let value = call.raw_table_get(input, Value::Number(source as f64))?;
                    call.raw_table_set(out, Value::Number(index as f64), value)?;
                }
                Ok(())
            })?)
        }
        Operation::Map => {
            let function = arg(call, 0);
            if !matches!(function, Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"function expected"));
            }
            let input = table(call, arg(call, 1))?;
            Value::Table(object(call, |call, out| {
                let mut index = 1;
                loop {
                    let value = call.raw_table_get(input, Value::Number(index as f64))?;
                    if value == Value::Nil {
                        break;
                    }
                    let mapped = call
                        .call(function, &[value])?
                        .first()
                        .copied()
                        .unwrap_or(Value::Nil);
                    call.raw_table_set(out, Value::Number(index as f64), mapped)?;
                    index += 1;
                }
                Ok(())
            })?)
        }
        Operation::Deduplicate => {
            let input = table(call, arg(call, 0))?;
            let mut seen = HashSet::new();
            Value::Table(object(call, |call, out| {
                let (mut input_index, mut output_index) = (1, 1);
                loop {
                    let value = call.raw_table_get(input, Value::Number(input_index as f64))?;
                    if value == Value::Nil {
                        break;
                    }
                    if key(call, value).is_none_or(|key| seen.insert(key)) {
                        call.raw_table_set(out, Value::Number(output_index as f64), value)?;
                        output_index += 1;
                    }
                    input_index += 1;
                }
                Ok(())
            })?)
        }
        Operation::Stringify => {
            let Value::Table(input) = arg(call, 0) else {
                return call.push(Value::Nil);
            };
            let form = text(call, arg(call, 1));
            let globals = call.captures()[0];
            let key = call.try_intern(b"string")?;
            let library = call.table_get(globals, key)?;
            let library = table(call, library)?;
            let format = call.with_roots(&[Value::Table(library)], |call| {
                let key = call.try_intern(b"format")?;
                call.table_get(Value::Table(library), key)
            })?;
            if !matches!(format, Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"format function expected"));
            }
            call.with_roots(&[format], |call| {
                let out = object(call, |call, out| {
                    let mut index = 1;
                    loop {
                        let mut value = call.raw_table_get(input, Value::Number(index as f64))?;
                        if value == Value::Nil {
                            break;
                        }
                        if let Some(form) = &form
                            && matches!(value, Value::Number(_))
                        {
                            let form = call.try_intern(form.as_bytes())?;
                            value = call
                                .call(format, &[form, value])?
                                .first()
                                .copied()
                                .unwrap_or(Value::Nil);
                        }
                        let value = if let Value::String(id) = value {
                            std::str::from_utf8(call.string_bytes(id).unwrap())
                                .map_err(|e| call.error(e.to_string().as_bytes()))?;
                            value
                        } else {
                            call.try_intern(
                                value_text(call, value, false)?
                                    .unwrap_or_default()
                                    .as_bytes(),
                            )?
                        };
                        call.raw_table_set(out, Value::Number(index as f64), value)?;
                        index += 1;
                    }
                    Ok(())
                })?;
                call.push(Value::Table(out))
            })?;
            return Ok(());
        }
        Operation::Describe => {
            let name = text(call, arg(call, 1)).unwrap_or_else(|| "Value".to_owned());
            let description = match call.arguments().first().copied() {
                Some(Value::Table(t)) => {
                    let len = call.raw_table_len(t)?;
                    if len == 0 {
                        "{}".to_owned()
                    } else {
                        format!("{{...{len} item(s)}}")
                    }
                }
                Some(v) => value_text(call, v, false)
                    .unwrap_or_default()
                    .unwrap_or_default(),
                None => "nil".to_owned(),
            };
            call.try_intern(format!("{name} = {description}").as_bytes())?
        }
        Operation::Find => {
            let needle = arg(call, 0);
            let Value::Table(haystack) = arg(call, 1) else {
                return call.push(Value::Nil);
            };
            rows(call, haystack)?
                .into_iter()
                .find(|(_, value)| equal(call, needle, *value))
                .map_or(Value::Nil, |(key, _)| key)
        }
        Operation::Copy => clone_value(call, arg(call, 0), &|_, _, _| Ok(()))?,
        Operation::Split(reverse) => {
            let source = text(call, arg(call, usize::from(reverse))).unwrap_or_default();
            let separator = text(call, arg(call, usize::from(!reverse))).unwrap_or_default();
            let parts = if separator.is_empty() {
                if source.is_empty() {
                    vec![String::new()]
                } else {
                    source.chars().map(|v| v.to_string()).collect()
                }
            } else {
                source.split(&separator).map(str::to_owned).collect()
            };
            Value::Table(array(call, parts)?)
        }
        Operation::Clean => {
            let source = text(call, arg(call, 0)).unwrap_or_default();
            let needle = text(call, arg(call, 1)).unwrap_or_default();
            let replacement = text(call, arg(call, 2)).unwrap_or_default();
            call.try_intern(source.replace(&needle, &replacement).as_bytes())?
        }
        Operation::Basename => {
            let value = text(call, arg(call, 0)).unwrap_or_default();
            call.try_intern(crate::path_basename(&value).as_bytes())?
        }
        Operation::Round => {
            let v = call.number(0)?;
            Value::Number(if v >= 0. {
                (v + 0.5).floor()
            } else {
                (v - 0.5).ceil()
            })
        }
        Operation::Clamp => {
            let v = call.number(0)?;
            let min = call.number(1)?;
            let max = call.number(2)?;
            if min.is_nan() || max.is_nan() || min > max {
                return Err(call.error(b"invalid clamp bounds"));
            }
            Value::Number(v.clamp(min, max))
        }
        Operation::Mod => {
            let a = call.number(0)?;
            let b = call.number(1)?;
            Value::Number(if b == 0. { f64::NAN } else { a % b })
        }
    };
    call.push(value)
}
pub(super) fn clone_value(
    call: &mut HostCall<'_>,
    value: Value,
    after: &impl Fn(&mut HostCall<'_>, TableId, TableId) -> Result<(), Error>,
) -> Result<Value, Error> {
    let Value::Table(input) = value else {
        return Ok(value);
    };
    let entries = rows(call, input)?;
    let mut roots = Vec::with_capacity(entries.len() * 2 + 2);
    roots.push(value);
    roots.extend(entries.iter().flat_map(|&(k, v)| [k, v]));
    call.with_roots(&roots, |call| {
        let output = object(call, |call, out| {
            for &(key, value) in &entries {
                let key = clone_value(call, key, after)?;
                call.with_roots(&[key], |call| {
                    let value = clone_value(call, value, after)?;
                    call.raw_table_set(out, key, value)
                })?;
            }
            after(call, input, out)
        })?;
        Ok(Value::Table(output))
    })
}
