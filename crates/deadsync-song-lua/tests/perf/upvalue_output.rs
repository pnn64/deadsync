use super::*;
use std::hint::black_box;

#[path = "upvalue_output_baseline.rs"]
mod baseline;

fn getupvalue(lua: &Lua) -> Function {
    create_debug_table(lua).unwrap().get("getupvalue").unwrap()
}

fn chain(lua: &Lua, count: usize, postorder: bool, shared: bool) -> Function {
    let factory: Function = lua
        .load(if postorder {
            "return function(target, child) return function() return child, target end end"
        } else {
            "return function(target, child) return function() return target, child end end"
        })
        .eval()
        .unwrap();
    let common = lua.create_table().unwrap();
    common.set("id", 0).unwrap();
    let mut child = Value::Nil;
    for index in 0..count {
        let target = if shared {
            common.clone()
        } else {
            let target = lua.create_table().unwrap();
            target.set("id", index).unwrap();
            target
        };
        child = Value::Function(factory.call((target, child)).unwrap());
    }
    match child {
        Value::Function(function) => function,
        _ => lua.load("return function() end").eval().unwrap(),
    }
}

fn discover(
    get: &Function,
    function: &Function,
    names: &[&str],
    tables: &mut HashSet<usize>,
    functions: &mut HashSet<usize>,
    old: bool,
) -> Result<Vec<Table>, String> {
    if old {
        baseline::nested_function_named_upvalue_tables(get, function, names, tables, functions)
    } else {
        nested_function_named_upvalue_tables(get, function, names, tables, functions)
    }
}

fn pointers(tables: &[Table]) -> Vec<usize> {
    tables
        .iter()
        .map(|table| table.to_pointer() as usize)
        .collect()
}

#[test]
fn lua_buffer_pass_upvalue_output_preserves_depth_first_order_and_retained_tables() {
    let lua = Lua::new();
    let get = getupvalue(&lua);
    for postorder in [false, true] {
        for count in [0, 1, 4, 16, 64, 128] {
            let function = chain(&lua, count, postorder, false);
            let (mut old_tables, mut old_functions) = (HashSet::new(), HashSet::new());
            let (mut new_tables, mut new_functions) = (HashSet::new(), HashSet::new());
            let old = discover(
                &get,
                &function,
                &["target"],
                &mut old_tables,
                &mut old_functions,
                true,
            )
            .unwrap();
            let new = discover(
                &get,
                &function,
                &["target"],
                &mut new_tables,
                &mut new_functions,
                false,
            )
            .unwrap();
            assert_eq!(pointers(&new), pointers(&old));
            assert_eq!(old_tables, new_tables);
            assert_eq!(old_functions, new_functions);
            let expected: Vec<_> = if postorder {
                (0..count).collect()
            } else {
                (0..count).rev().collect()
            };
            drop(old);
            drop(function);
            lua.gc_collect().unwrap();
            let actual: Vec<usize> = new.iter().map(|table| table.get("id").unwrap()).collect();
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn lua_buffer_pass_upvalue_output_preserves_cycles_shared_nodes_and_existing_seen_sets() {
    let lua = Lua::new();
    let get = getupvalue(&lua);
    let (root, shared): (Function, Table) = lua
        .load(
            r#"
        local root, left, right
        local target = {id = 'root'}
        local shared = {id = 'shared'}
        left = function() return shared, root end
        right = function() return shared, left end
        root = function() return target, left, right end
        return root, shared
    "#,
        )
        .eval()
        .unwrap();
    for preseen in [false, true] {
        let mut expected = None;
        for old in [true, false] {
            let mut tables = HashSet::new();
            let mut functions = HashSet::new();
            if preseen {
                tables.insert(shared.to_pointer() as usize);
            }
            let result = discover(
                &get,
                &root,
                &["target", "shared"],
                &mut tables,
                &mut functions,
                old,
            )
            .unwrap();
            assert_eq!(result.len(), if preseen { 1 } else { 2 });
            assert_eq!(functions.len(), 3);
            let actual = (pointers(&result), tables.clone(), functions.clone());
            if let Some(expected) = &expected {
                assert_eq!(&actual, expected);
            } else {
                expected = Some(actual);
            }
            assert!(
                discover(
                    &get,
                    &root,
                    &["target", "shared"],
                    &mut tables,
                    &mut functions,
                    old
                )
                .unwrap()
                .is_empty()
            );
        }
    }
}

#[test]
fn lua_buffer_pass_upvalue_output_preserves_callback_order_and_partial_error_state() {
    let lua = Lua::new();
    let real_get = getupvalue(&lua);
    let root = chain(&lua, 8, false, false);
    for invalid_name in [false, true] {
        let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let calls_for_callback = calls.clone();
        let real_get = real_get.clone();
        let callback = lua
            .create_function(move |lua, (function, index): (Function, i64)| {
                let mut calls = calls_for_callback.borrow_mut();
                calls.push((function.to_pointer() as usize, index));
                if calls.len() == 5 {
                    if invalid_name {
                        return Ok((
                            Value::String(lua.create_string([255])?),
                            Value::Boolean(false),
                        ));
                    }
                    return Err(mlua::Error::RuntimeError("upvalue failure".to_owned()));
                }
                drop(calls);
                real_get.call::<(Value, Value)>((&function, index))
            })
            .unwrap();
        let mut expected = None;
        for old in [true, false] {
            calls.borrow_mut().clear();
            let mut tables = HashSet::new();
            let mut functions = HashSet::new();
            let error = discover(
                &callback,
                &root,
                &["target"],
                &mut tables,
                &mut functions,
                old,
            )
            .unwrap_err();
            let actual = (error, calls.borrow().clone(), tables, functions);
            if let Some(expected) = &expected {
                assert_eq!(&actual, expected);
            } else {
                expected = Some(actual);
            }
        }
    }
}

#[test]
fn lua_buffer_pass_upvalue_output_traverses_function_values_with_nonstring_names() {
    let lua = Lua::new();
    let real_get = getupvalue(&lua);
    let root = chain(&lua, 8, false, false);
    let callback = lua
        .create_function(move |_, (function, index): (Function, i64)| {
            let (name, value) = real_get.call::<(Value, Value)>((&function, index))?;
            Ok((
                if matches!(value, Value::Function(_)) {
                    Value::Integer(7)
                } else {
                    name
                },
                value,
            ))
        })
        .unwrap();
    let mut expected = None;
    for old in [true, false] {
        let result = discover(
            &callback,
            &root,
            &["target"],
            &mut HashSet::new(),
            &mut HashSet::new(),
            old,
        )
        .unwrap();
        assert_eq!(result.len(), 8);
        if let Some(expected) = &expected {
            assert_eq!(&pointers(&result), expected);
        } else {
            expected = Some(pointers(&result));
        }
    }
}

#[test]
fn lua_buffer_pass_upvalue_output_skips_seen_roots_without_churn() {
    let lua = Lua::new();
    let get = getupvalue(&lua);
    let root = chain(&lua, 128, false, false);
    let mut tables = HashSet::new();
    let mut functions = HashSet::new();
    functions.insert(root.to_pointer() as usize);
    crate::perf::assert_no_churn(|| {
        drop(black_box(
            discover(&get, &root, &["target"], &mut tables, &mut functions, false).unwrap(),
        ));
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_buffer_pass_bench_upvalue_output() {
    for (kind, count) in [
        ("chain", 0),
        ("chain", 1),
        ("chain", 4),
        ("chain", 16),
        ("chain", 64),
        ("chain", 128),
        ("postorder", 128),
        ("shared", 128),
        ("unmatched", 128),
        ("seen", 128),
    ] {
        let lua = Lua::new();
        let get = getupvalue(&lua);
        let function = chain(&lua, count, kind == "postorder", kind == "shared");
        let names: &[&str] = if kind == "unmatched" {
            &[]
        } else {
            &["target"]
        };
        let mut old_tables = HashSet::new();
        let mut old_functions = HashSet::new();
        let mut new_tables = HashSet::new();
        let mut new_functions = HashSet::new();
        assert_eq!(
            pointers(
                &discover(
                    &get,
                    &function,
                    names,
                    &mut old_tables,
                    &mut old_functions,
                    true
                )
                .unwrap()
            ),
            pointers(
                &discover(
                    &get,
                    &function,
                    names,
                    &mut new_tables,
                    &mut new_functions,
                    false
                )
                .unwrap()
            )
        );
        lua.gc_collect().unwrap();
        lua.gc_stop();
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            let mut tables = HashSet::with_capacity(count);
            let mut functions = HashSet::with_capacity(count.max(1));
            crate::perf::measure_sampled(
                &format!(
                    "upvalue_output_{kind}_{count}/{}",
                    if old { "old" } else { "new" }
                ),
                32,
                count.max(1),
                || {
                    tables.clear();
                    functions.clear();
                    if kind == "seen" {
                        functions.insert(function.to_pointer() as usize);
                    }
                    drop(black_box(
                        discover(
                            black_box(&get),
                            black_box(&function),
                            black_box(names),
                            &mut tables,
                            &mut functions,
                            black_box(old),
                        )
                        .unwrap(),
                    ));
                },
            );
        }
    }
}
