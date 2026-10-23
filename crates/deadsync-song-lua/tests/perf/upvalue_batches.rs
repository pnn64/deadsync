use super::*;

// Read-only prototype measured before adding a safe library operation.
fn prototype(
    lua: &Lua,
    function: &Function,
    mut visit: impl FnMut(u8, &[u8], Value) -> mlua::Result<()>,
) -> mlua::Result<()> {
    // SAFETY: the function stays rooted, getupvalue only reads existing cells,
    // and checked mlua conversion roots each value before the callback. Names
    // belong to the rooted function's prototype. The scope restores the stack.
    unsafe {
        crate::raw_vm::with_read_stack(lua, 3, |raw, base| {
            raw.push(function)?;
            if ffi::lua_iscfunction(raw.state(), base + 1) != 0 {
                return Ok(());
            }
            for index in 1..=u8::MAX {
                let name = ffi::lua_getupvalue(raw.state(), base + 1, index.into());
                if name.is_null() {
                    break;
                }
                let value = raw.pop::<Value>()?;
                visit(index, std::ffi::CStr::from_ptr(name).to_bytes(), value)?;
            }
            Ok(())
        })
    }
}

fn collect(lua: &Lua, function: &Function, current: bool) -> Vec<(u8, Vec<u8>, Value)> {
    let mut out = Vec::new();
    if current {
        function
            .for_each_upvalue(|index, name, value| {
                out.push((index, name.to_vec(), value));
                Ok(())
            })
            .unwrap();
    } else {
        for index in 1..=function_upvalue_count(lua, function).unwrap() {
            let (name, value) = read_function_upvalue(lua, function.clone(), index.into()).unwrap();
            let Value::String(name) = name else {
                panic!("missing upvalue name")
            };
            out.push((index, name.as_bytes().to_vec(), value));
        }
    }
    out
}

fn fixture(lua: &Lua, count: usize) -> Function {
    let names: Vec<_> = (0..count).map(|index| format!("v{index}")).collect();
    let source = if count == 0 {
        "return function() return 7 end".into()
    } else {
        format!(
            "local {} = {}; return function() return {} end",
            names.join(","),
            vec!["{}"; count].join(","),
            names.join(",")
        )
    };
    lua.load(source).eval().unwrap()
}

#[test]
fn batched_upvalues_match_names_values_order_and_aliases() {
    let lua = Lua::new();
    for count in [0, 1, 8, 32, 60] {
        let function = fixture(&lua, count);
        assert_eq!(
            collect(&lua, &function, false),
            collect(&lua, &function, true)
        );
        let mut original = Vec::new();
        prototype(&lua, &function, |index, name, value| {
            original.push((index, name.to_vec(), value));
            Ok(())
        })
        .unwrap();
        assert_eq!(original, collect(&lua, &function, true));
    }
    let function: Function = lua
        .load("local a={}; local b=a; return function() return a,b end")
        .eval()
        .unwrap();
    let values = collect(&lua, &function, true);
    assert_eq!(values[0].2, values[1].2);
}

#[test]
fn snapshot_preserves_invalid_upvalue_name_errors() {
    let lua = Lua::new();
    let function: Function = lua
        .load("local captured_slot=7; return function() return captured_slot end")
        .set_name("@invalid-upvalue-name.lua")
        .eval()
        .unwrap();
    let mut bytecode = function.dump(false);
    let marker = b"captured_slot";
    let positions: Vec<_> = bytecode
        .windows(marker.len())
        .enumerate()
        .filter_map(|(index, bytes)| (bytes == marker).then_some(index))
        .collect();
    assert_eq!(
        positions.len(),
        1,
        "change only the debug name, retaining valid bytecode"
    );
    bytecode[positions[0]] = 0xff;
    let function = lua.load(bytecode).into_function().unwrap();
    let (name, _) = read_function_upvalue(&lua, function.clone(), 1).unwrap();
    let Value::String(name) = name else {
        panic!("missing upvalue name");
    };
    let expected = name.to_str().unwrap_err().to_string();
    let functions = lua.create_sequence_from([function]).unwrap();
    let actual = snapshot_function_action_tables(&lua, &functions)
        .err()
        .unwrap();
    assert_eq!(actual.to_string(), expected);
}

#[cfg(windows)]
#[test]
#[ignore = "paired release benchmark; run alone"]
fn benchmark_upvalue_batches() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::hint::black_box;
    let _affinity = pin_cpu2();
    let lua = Lua::new();
    for count in [0, 1, 8, 32, 60] {
        let function = fixture(&lua, count);
        assert_eq!(
            collect(&lua, &function, false),
            collect(&lua, &function, true)
        );
        lua.gc_stop();
        for sample in 0..10 {
            for current in [sample % 2 != 0, sample % 2 == 0] {
                let first = cycles();
                let start = std::time::Instant::now();
                let (_, counts) = allocations::measure(|| {
                    for _ in 0..1024 {
                        if current {
                            function
                                .for_each_upvalue(|index, name, value| {
                                    black_box((index, name, value));
                                    Ok(())
                                })
                                .unwrap();
                        } else {
                            for index in 1..=function_upvalue_count(&lua, &function).unwrap() {
                                black_box(
                                    read_function_upvalue(&lua, function.clone(), index.into())
                                        .unwrap(),
                                );
                            }
                        }
                    }
                });
                let ns = start.elapsed().as_nanos() as u64;
                let elapsed_cycles = cycles() - first;
                if sample != 0 {
                    println!(
                        "LUA_BOUNDARY_METRIC {}",
                        serde_json::json!({
                            "workload": format!("upvalues_{count}"), "current": current,
                            "sample": sample, "operations":1024, "ns":ns,
                            "cycles":elapsed_cycles, "allocations":counts.allocs,
                            "reallocations":counts.reallocs,"bytes":counts.allocated_bytes,
                        })
                    );
                }
            }
        }
    }
}
