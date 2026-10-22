use super::*;

#[path = "method_storage_before.rs"]
mod before;

fn same_fields(left: &Table, right: &Table) {
    let mut count = 0;
    left.for_each::<Value, Value>(|key, value| {
        if matches!(&key,Value::String(name) if name.as_bytes().as_ref()==METHODS_KEY.as_bytes()) {
            return Ok(());
        }
        assert_eq!(right.raw_get::<Value>(key)?, value);
        count += 1;
        Ok(())
    })
    .unwrap();
    let mut other = 0;
    right.for_each::<Value,Value>(|key,_| {
        if !matches!(&key,Value::String(name) if name.as_bytes().as_ref()==METHODS_KEY.as_bytes()) {other+=1;}
        Ok(())
    }).unwrap();
    assert_eq!(count, other);
}

#[test]
fn typed_bind_entries_keep_identities_aliases_filters_and_error_atomicity() {
    let lua = Lua::new();
    let native = lua.create_function(|_, ()| Ok(17)).unwrap();
    let script = lua
        .load("return function() return 23 end")
        .eval::<mlua::Function>()
        .unwrap();
    let opaque = lua.create_table().unwrap();
    for bound_kind in 0..4 {
        for invalid in [false, true] {
            let actors: [Table; 2] = std::array::from_fn(|_| {
                let actor = lua.create_table().unwrap();
                for name in ["native", "native\0suffix", "OnCommand", "__songlua_private"] {
                    actor.raw_set(name, &native).unwrap();
                }
                actor.raw_set("script", &script).unwrap();
                actor.raw_set("number", 42).unwrap();
                actor.raw_set(&opaque, &native).unwrap();
                if invalid {
                    actor
                        .raw_set(lua.create_string([255, 0]).unwrap(), &native)
                        .unwrap();
                }
                match bound_kind {
                    1 => actor
                        .raw_set(METHODS_KEY, lua.create_table().unwrap())
                        .unwrap(),
                    2 => actor.raw_set(METHODS_KEY, &actor).unwrap(),
                    3 => actor.raw_set(METHODS_KEY, false).unwrap(),
                    _ => (),
                }
                actor
            });
            let old = before::bind(&lua, &actors[0]).map_err(|e| e.to_string());
            let new = bind(&lua, &actors[1]).map_err(|e| e.to_string());
            assert_eq!(old, new);
            assert_eq!(old.is_err(), invalid || bound_kind == 3);
            same_fields(&actors[0], &actors[1]);
            match (
                actors[0].raw_get::<Value>(METHODS_KEY).unwrap(),
                actors[1].raw_get::<Value>(METHODS_KEY).unwrap(),
            ) {
                (Value::Table(left), Value::Table(right)) => same_fields(&left, &right),
                (left, right) => assert_eq!(left, right),
            }
        }
    }
}

#[test]
fn reserved_class_tables_keep_inventory_inheritance_native_calls_and_errors() {
    let lua = Lua::new();
    let names = [
        "Actor",
        "ActorFrame",
        "Sprite",
        "ActorFrameTexture",
        "ActorMultiVertex",
        "ActorProxy",
        "BitmapText",
    ];
    before::install_native(&lua).unwrap();
    let old: Vec<Table> = names
        .iter()
        .map(|name| lua.globals().get(*name).unwrap())
        .collect();
    install_native(&lua).unwrap();
    let new: Vec<Table> = names
        .iter()
        .map(|name| lua.globals().get(*name).unwrap())
        .collect();
    let actor = lua.create_table().unwrap();
    let bound = lua.create_table().unwrap();
    actor.raw_set(METHODS_KEY, &bound).unwrap();
    // Use the same Lua call site and restore each function's own inventory
    // before capturing its traceback; Lua prefers registered global names.
    let bad_call = lua
        .load("return function(method) return method() end")
        .eval::<mlua::Function>()
        .unwrap();
    for methods in [
        ACTOR,
        FRAME,
        SPRITE,
        AFT,
        AMV,
        &["SetTarget", "GetTarget"][..],
        BITMAP,
    ] {
        for &name in methods {
            bound
                .raw_set(
                    name,
                    lua.create_function(move |_, args: MultiValue| Ok((name, args.len())))
                        .unwrap(),
                )
                .unwrap();
        }
    }
    for (left, right) in old.iter().zip(&new) {
        let mut left_keys: Vec<_> = left
            .pairs::<String, Value>()
            .map(|entry| entry.unwrap().0)
            .collect();
        let mut right_keys: Vec<_> = right
            .pairs::<String, Value>()
            .map(|entry| entry.unwrap().0)
            .collect();
        left_keys.sort();
        right_keys.sort();
        assert_eq!(left_keys, right_keys);
        assert_eq!(left.raw_len(), right.raw_len());
        for key in left_keys
            .into_iter()
            .chain(["x".into(), "__missing__".into()])
        {
            let (left, right) = (
                left.get::<Value>(key.as_str()).unwrap(),
                right.get::<Value>(key.as_str()).unwrap(),
            );
            if let (Value::Function(left), Value::Function(right)) = (&left, &right) {
                assert!(left.environment().is_none() && right.environment().is_none());
                assert_eq!(
                    left.call::<(String, usize)>((actor.clone(), 17)).unwrap(),
                    right.call::<(String, usize)>((actor.clone(), 17)).unwrap()
                );
                for (name, table) in names.iter().zip(&old) {
                    lua.globals().raw_set(*name, table).unwrap();
                }
                let left = bad_call
                    .call::<Value>(left.clone())
                    .unwrap_err()
                    .to_string();
                for (name, table) in names.iter().zip(&new) {
                    lua.globals().raw_set(*name, table).unwrap();
                }
                let right = bad_call
                    .call::<Value>(right.clone())
                    .unwrap_err()
                    .to_string();
                assert_eq!(left, right);
            } else {
                assert_eq!(left, right);
            }
        }
    }
    for index in 0..names.len() {
        let parent = if index == 0 {
            None
        } else if index == 3 {
            Some(1)
        } else {
            Some(0)
        };
        for tables in [&old, &new] {
            assert_eq!(
                tables[index]
                    .metatable()
                    .map(|mt| mt.raw_get::<Table>("__index").unwrap()),
                parent.map(|p| tables[p].clone())
            );
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_method_storage() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, count, existing) in [
        ("bind0", 0, false),
        ("bind8", 8, false),
        ("bind128", 128, false),
        ("bind512", 512, false),
        ("bind128_existing", 128, true),
        ("native_classes", 0, false),
    ] {
        for current in variants {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let function = lua.create_function(|_, ()| Ok(17)).unwrap();
                let keys: Vec<_> = (0..count)
                    .map(|i| lua.create_string(format!("method{i}")).unwrap())
                    .collect();
                let units = if name == "native_classes" { 16 } else { 64 };
                let actors: Vec<_> = (0..units)
                    .map(|_| {
                        let actor = lua.create_table().unwrap();
                        for key in &keys {
                            actor.raw_set(key, &function).unwrap();
                        }
                        if existing {
                            actor
                                .raw_set(METHODS_KEY, lua.create_table().unwrap())
                                .unwrap();
                        }
                        actor
                    })
                    .collect();
                lua.gc_stop();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for actor in &actors {
                        if name == "native_classes" {
                            if current {
                                install_native(&lua)
                            } else {
                                before::install_native(&lua)
                            }
                        } else if current {
                            bind(&lua, actor)
                        } else {
                            before::bind(&lua, actor)
                        }
                        .unwrap();
                        black_box(actor);
                    }
                });
                let used = cycles() - begin;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":units,
                    "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},
                "unit":if name=="native_classes"{"class inventories"}else{"actors"},"baseline_commit":"626d9f1f7","samples":samples,
                "value_entry_bytes":std::mem::size_of::<(Value,Value)>(),"typed_entry_bytes":std::mem::size_of::<(mlua::LuaString,mlua::Function)>()})
            );
        }
    }
}
