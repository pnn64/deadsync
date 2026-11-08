//! Compare collected method representations and mlua push paths.
use mlua::{Lua, Table, Value};
use std::hint::black_box;
#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;
#[path = "../src/raw_vm.rs"]
mod raw_vm;
const METHODS_KEY: &str = "__songlua_bound_methods";

fn method_name_flags(lua: &Lua, name: &mlua::LuaString) -> mlua::Result<(bool, bool)> {
    // SAFETY: the checked string stays rooted at base+1. Only validated flags
    // leave the read scope; neither borrowed bytes nor a pointer escape.
    unsafe {
        crate::raw_vm::with_read_stack(lua, 1, |raw, base| {
            raw.push(name)?;
            let mut len = 0;
            let data = mlua::ffi::lua_tolstring(raw.state(), base + 1, &mut len);
            let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len);
            let text = std::str::from_utf8(bytes).map_err(|error| {
                mlua::Error::FromLuaConversionError {
                    from: "string",
                    to: "&str".into(),
                    message: Some(error.to_string()),
                }
            })?;
            Ok((
                !text.starts_with("__songlua_") && !text.ends_with("Command"),
                matches!(
                    text,
                    "SetUpdateFunction" | "SetDrawFunction" | "GetDrawFunction"
                ),
            ))
        })
    }
}

mod before {
    use super::*;
    pub(crate) fn bind(lua: &Lua, actor: &Table) -> mlua::Result<()> {
        let bound = match actor.raw_get::<Option<Table>>(METHODS_KEY)? {
            Some(bound) => bound,
            None => lua.create_table()?,
        };
        let mut methods = Vec::new();
        actor.for_each::<Value, Value>(|key, value| {
            if let (Value::String(name), Value::Function(function)) = (&key, &value) {
                let (native, _) = method_name_flags(lua, name)?;
                if native && function.environment().is_none() {
                    methods.push((key, value));
                }
            }
            Ok(())
        })?;
        for (key, value) in methods {
            bound.raw_set(&key, value)?;
            actor.raw_set(key, Value::Nil)?;
        }
        actor.raw_set(METHODS_KEY, bound)
    }
}

mod owned {
    use super::*;
    pub(crate) fn bind(lua: &Lua, actor: &Table) -> mlua::Result<()> {
        let bound = match actor.raw_get::<Option<Table>>(METHODS_KEY)? {
            Some(bound) => bound,
            None => lua.create_table()?,
        };
        let mut methods = Vec::new();
        actor.for_each::<Value, Value>(|key, value| {
            if let (Value::String(name), Value::Function(function)) = (key, value) {
                let (native, _) = method_name_flags(lua, &name)?;
                if native && function.environment().is_none() {
                    methods.push((name, function));
                }
            }
            Ok(())
        })?;
        for (key, value) in methods {
            bound.raw_set(&key, value)?;
            actor.raw_set(key, Value::Nil)?;
        }
        actor.raw_set(METHODS_KEY, bound)
    }
}

mod borrowed {
    use super::*;
    pub(crate) fn bind(lua: &Lua, actor: &Table) -> mlua::Result<()> {
        let bound = match actor.raw_get::<Option<Table>>(METHODS_KEY)? {
            Some(bound) => bound,
            None => lua.create_table()?,
        };
        let mut methods = Vec::new();
        actor.for_each::<Value, Value>(|key, value| {
            if let (Value::String(name), Value::Function(function)) = (key, value) {
                let (native, _) = method_name_flags(lua, &name)?;
                if native && function.environment().is_none() {
                    methods.push((name, function));
                }
            }
            Ok(())
        })?;
        for (key, value) in methods {
            bound.raw_set(&key, &value)?;
            actor.raw_set(&key, Value::Nil)?;
        }
        actor.raw_set(METHODS_KEY, bound)
    }
}

mod reboxed {
    use super::*;
    pub(crate) fn bind(lua: &Lua, actor: &Table) -> mlua::Result<()> {
        let bound = match actor.raw_get::<Option<Table>>(METHODS_KEY)? {
            Some(bound) => bound,
            None => lua.create_table()?,
        };
        let mut methods = Vec::new();
        actor.for_each::<Value, Value>(|key, value| {
            if let (Value::String(name), Value::Function(function)) = (key, value) {
                let (native, _) = method_name_flags(lua, &name)?;
                if native && function.environment().is_none() {
                    methods.push((name, function));
                }
            }
            Ok(())
        })?;
        for (key, value) in methods {
            let key = Value::String(key);
            let value = Value::Function(value);
            bound.raw_set(&key, value)?;
            actor.raw_set(key, Value::Nil)?;
        }
        actor.raw_set(METHODS_KEY, bound)
    }
}

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

static VALIDATE_VARIANT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn bind(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    match VALIDATE_VARIANT.load(std::sync::atomic::Ordering::Relaxed) {
        0 => before::bind(lua, actor),
        1 => owned::bind(lua, actor),
        2 => borrowed::bind(lua, actor),
        _ => reboxed::bind(lua, actor),
    }
}

fn run() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut std::ffi::c_void;
            fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
        }
        // SAFETY: change only this disposable worker's affinity.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
            0
        );
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "validate") {
        for index in 0..4 {
            VALIDATE_VARIANT.store(index, std::sync::atomic::Ordering::Relaxed);
            typed_bind_entries_keep_identities_aliases_filters_and_error_atomicity();
            let lua = Lua::new();
            let actor = lua.create_table().unwrap();
            let native = lua.create_function(|_, ()| Ok(17)).unwrap();
            actor.raw_set("native", &native).unwrap();
            lua.globals().raw_set("actor", &actor).unwrap();
            lua.globals()
                .raw_set(
                    "panic_arg",
                    lua.create_function(|_, ()| -> mlua::Result<()> {
                        panic!("stored binding panic")
                    })
                    .unwrap(),
                )
                .unwrap();
            lua.load("local _,p=pcall(panic_arg); actor.caught=p")
                .exec()
                .unwrap();
            let panic =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| bind(&lua, &actor)))
                    .unwrap_err();
            assert_eq!(
                panic.downcast_ref::<&str>().copied(),
                Some("stored binding panic")
            );
            assert_eq!(actor.raw_get::<mlua::Function>("native").unwrap(), native);
            actor.raw_set("caught", Value::Nil).unwrap();
            assert_eq!(lua.load("return 6*7").eval::<i64>().unwrap(), 42);
        }
        println!(
            "All four variants preserve bindings, identities, aliases, filtering, error atomicity and caught-panic propagation/recovery."
        );
        return;
    }
    assert_eq!(args.len(), 2, "method_storage REPORT ORDER | validate");
    let order: usize = args[1].parse().unwrap();
    let functions: [(&str, fn(&Lua, &Table) -> mlua::Result<()>); 4] = [
        ("before", before::bind),
        ("owned", owned::bind),
        ("borrowed", borrowed::bind),
        ("reboxed", reboxed::bind),
    ];
    let mut records = Vec::new();
    for (name, count, existing) in [
        ("bind0", 0, false),
        ("bind8", 8, false),
        ("bind128", 128, false),
        ("bind512", 512, false),
        ("bind128_existing", 128, true),
    ] {
        for offset in 0..4 {
            let index = if order < 4 {
                (offset + order) % 4
            } else {
                (3 - offset + order - 4) % 4
            };
            let (variant, bind) = functions[index];
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let function = lua.create_function(|_, ()| Ok(17)).unwrap();
                let keys: Vec<_> = (0..count)
                    .map(|i| lua.create_string(format!("method{i}")).unwrap())
                    .collect();
                let actors: Vec<_> = (0..64)
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
                let measurement = metrics::Measurement::start();
                for actor in &actors {
                    bind(&lua, actor).unwrap();
                    black_box(actor);
                }
                let result = measurement.finish();
                if sample > 0 {
                    samples.push(result);
                }
            }
            records.push(serde_json::json!({"workload":name,"variant":variant,"operations_per_sample":64,"samples":samples}));
        }
    }
    std::fs::write(&args[0], serde_json::to_vec_pretty(&records).unwrap()).unwrap();
}
fn main() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}
