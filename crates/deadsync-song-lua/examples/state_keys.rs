//! Prototypes for retaining only the actor/global key text a snapshot needs.
use mlua::{Lua, LuaString, ffi};
use std::{collections::HashSet, hint::black_box};

#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;
#[path = "../src/raw_vm.rs"]
mod raw_vm;

fn state_key(lua: &Lua, key: &LuaString, keep: fn(&str) -> bool) -> mlua::Result<Option<String>> {
    // SAFETY: one rooted, checked Lua string; UTF-8 validation and the fixed
    // Rust predicate do not call Lua. Only owned text leaves the read scope.
    unsafe {
        raw_vm::with_read_stack(lua, 1, |raw, base| {
            raw.push(key)?;
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
            Ok(keep(text).then(|| text.to_owned()))
        })
    }
}

fn remove_global(
    lua: &Lua,
    key: &LuaString,
    scalar: bool,
    retained: &HashSet<&str>,
) -> mlua::Result<bool> {
    // SAFETY: same rooted immutable-string read; hashing is pure Rust.
    unsafe {
        raw_vm::with_read_stack(lua, 1, |raw, base| {
            raw.push(key)?;
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
            Ok(scalar && !retained.contains(text))
        })
    }
}

fn keep(text: &str) -> bool {
    text.starts_with("__songlua_state_") || text == "Text"
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
    assert_eq!(args.len(), 2, "state_keys REPORT REVERSE");
    let variants = if args[1] == "1" {
        [true, false]
    } else {
        [false, true]
    };
    let lua = Lua::new();
    let names = lua.create_table().unwrap();
    for index in 1..=128 {
        let key = if index % 8 == 0 {
            format!("__songlua_state_{index}")
        } else {
            format!("method{index}")
        };
        names.raw_set(index, key).unwrap();
    }
    let retained = HashSet::from(["method1", "__songlua_state_8"]);
    let mut records = Vec::new();
    for workload in ["snapshot_keys", "restore_global_keys"] {
        for current in variants {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let measurement = metrics::Measurement::start();
                for index in 0..8192 {
                    let key = names.raw_get::<LuaString>(index % 128 + 1).unwrap();
                    if workload == "snapshot_keys" {
                        black_box(
                            if current {
                                state_key(&lua, &key, keep)
                            } else {
                                key.to_str()
                                    .map(|text| keep(&text).then(|| text.to_string()))
                            }
                            .unwrap(),
                        );
                    } else {
                        black_box(
                            if current {
                                remove_global(&lua, &key, index % 2 == 0, &retained)
                            } else {
                                key.to_str()
                                    .map(|text| index % 2 == 0 && !retained.contains(text.as_ref()))
                            }
                            .unwrap(),
                        );
                    }
                }
                let data = measurement.finish();
                if sample > 0 {
                    samples.push(data);
                }
            }
            records.push(serde_json::json!({"workload":workload,"variant":if current {"after"} else {"before"},
                "operations_per_sample":8192,"samples":samples}));
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
