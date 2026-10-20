//! Prototypes for eliminating temporary wrappers in read-only VM operations.
use mlua::{FromLua, Function, Lua, LuaString, Table, Value, ffi};
use std::hint::black_box;

#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;
#[path = "../src/raw_vm.rs"]
mod raw_vm;

fn flags_before(name: &LuaString) -> mlua::Result<(bool, bool)> {
    let text = name.to_str()?;
    Ok((
        !text.starts_with("__songlua_") && !text.ends_with("Command"),
        matches!(
            text.as_ref(),
            "SetUpdateFunction" | "SetDrawFunction" | "GetDrawFunction"
        ),
    ))
}

fn flags_after(lua: &Lua, name: &LuaString) -> mlua::Result<(bool, bool)> {
    // SAFETY: the checked string stays rooted at base+1. Only validated flags
    // leave the read scope; neither borrowed bytes nor a pointer escape.
    unsafe {
        raw_vm::with_read_stack(lua, 1, |raw, base| {
            raw.push(name)?;
            let mut len = 0;
            let data = ffi::lua_tolstring(raw.state(), base + 1, &mut len);
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

fn contains_before(registry: &Table, actor: &Table) -> mlua::Result<bool> {
    let pointer = actor.to_pointer();
    for value in registry.sequence_values::<Value>() {
        if let Value::Table(existing) = value? {
            if existing.to_pointer() == pointer {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn contains_after(lua: &Lua, registry: &Table, actor: &Table) -> mlua::Result<bool> {
    // SAFETY: registry/actor are rooted in two reserved slots. A third holds
    // one raw sequence value. Reads and identity checks cannot invoke Lua.
    unsafe {
        raw_vm::with_read_stack(lua, 3, |raw, base| {
            raw.push(registry)?;
            raw.push(actor)?;
            let state = raw.state();
            for index in 1.. {
                ffi::lua_rawgeti(state, base + 1, index);
                match ffi::lua_type(state, base + 3) {
                    ffi::LUA_TNIL => return Ok(false),
                    ffi::LUA_TTABLE => {
                        if ffi::lua_rawequal(state, base + 2, base + 3) != 0 {
                            return Ok(true);
                        }
                    }
                    // Preserve checked conversion of unusual registry values.
                    _ => {
                        let _ = Value::from_stack(base + 3, raw)?;
                    }
                }
                ffi::lua_pop(state, 1);
            }
            unreachable!()
        })
    }
}

fn upvalues_after(lua: &Lua, function: &Function) -> mlua::Result<u8> {
    // SAFETY: >u consumes a rooted function and fills only debug metadata.
    // It does not execute the function or allocate/throw in Lua 5.1.
    unsafe {
        raw_vm::with_read_stack(lua, 1, |raw, _| {
            raw.push(function)?;
            let mut info: ffi::lua_Debug = std::mem::zeroed();
            assert_ne!(ffi::lua_getinfo(raw.state(), c">u".as_ptr(), &mut info), 0);
            Ok(info.nups as u8)
        })
    }
}

#[cfg(windows)]
fn pin() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    // SAFETY: change only this disposable benchmark worker's affinity.
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
        0
    );
}
#[cfg(not(windows))]
fn pin() {}

fn run() {
    pin();
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "vm_reads REPORT REVERSE");
    let variants = if args[1] == "1" {
        [true, false]
    } else {
        [false, true]
    };
    let lua = Lua::new();
    let names = lua.create_table().unwrap();
    for index in 1..=128 {
        names.raw_set(index, format!("method{index}")).unwrap();
    }
    let registry = lua.create_table().unwrap();
    for index in 1..=1024 {
        registry
            .raw_set(index, lua.create_table().unwrap())
            .unwrap();
    }
    let actor = registry.raw_get::<Table>(1024).unwrap();
    let locals: Vec<_> = (1..=32).map(|index| format!("v{index}")).collect();
    let values: Vec<_> = (1..=32).map(|index| index.to_string()).collect();
    let function = lua
        .load(format!(
            "local {} = {}; return function() return {} end",
            locals.join(","),
            values.join(","),
            locals.join("+")
        ))
        .eval::<Function>()
        .unwrap();
    assert_eq!(function.info().num_upvalues, 32);
    let mut records = Vec::new();
    for workload in ["name128", "registry1024", "upvalues32"] {
        let units = if workload == "registry1024" {
            128
        } else {
            8192
        };
        for current in variants {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let measurement = metrics::Measurement::start();
                let mut observed = 0;
                for index in 0..units {
                    observed += match workload {
                        "name128" => {
                            let key = names.raw_get::<LuaString>(index % 128 + 1).unwrap();
                            let flags = if current {
                                flags_after(&lua, &key)
                            } else {
                                flags_before(&key)
                            }
                            .unwrap();
                            assert!(!flags.1);
                            flags.0 as u64
                        }
                        "registry1024" => if current {
                            contains_after(&lua, &registry, &actor)
                        } else {
                            contains_before(&registry, &actor)
                        }
                        .unwrap() as u64,
                        _ => {
                            (if current {
                                upvalues_after(&lua, &function).unwrap()
                            } else {
                                function.info().num_upvalues
                            }) as u64
                        }
                    };
                }
                black_box(observed);
                let data = measurement.finish();
                assert_eq!(
                    observed,
                    units as u64 * if workload == "upvalues32" { 32 } else { 1 }
                );
                if sample > 0 {
                    samples.push(data);
                }
            }
            records.push(serde_json::json!({"workload":workload,"variant":if current {"after"} else {"before"},
                "operations_per_sample":units,"samples":samples}));
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
