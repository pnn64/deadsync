//! Prototype for checked, inline numeric callback prefixes.
use mlua::{FromLuaMulti, Function, Lua, MultiValue, Value};
use std::{hint::black_box, os::raw::c_int};

#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;

// Use tuple decoding without its argument-error wrapper: MultiValue originally
// propagated checked stack errors directly. A final MultiValue checks all extras.
struct CheckedPrefix<T>(T);

impl<T: FromLuaMulti> FromLuaMulti for CheckedPrefix<T> {
    fn from_lua_multi(values: MultiValue, lua: &Lua) -> mlua::Result<Self> {
        T::from_lua_multi(values, lua).map(Self)
    }

    unsafe fn from_stack_multi(nvals: c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        // SAFETY: forward the unchanged live argument range to mlua's decoder.
        unsafe { T::from_stack_multi(nvals, lua).map(Self) }
    }
}

fn reader(lua: &Lua, current: bool) -> Function {
    if current {
        lua.create_function(
            |_,
             CheckedPrefix((first, second, third, _rest)): CheckedPrefix<(
                Value,
                Value,
                Value,
                MultiValue,
            )>| {
                Ok([&first, &second, &third]
                    .iter()
                    .filter(|v| matches!(v, Value::Number(_) | Value::Integer(_)))
                    .count())
            },
        )
        .unwrap()
    } else {
        lua.create_function(|_, args: MultiValue| {
            Ok(args
                .iter()
                .take(3)
                .filter(|v| matches!(v, Value::Number(_) | Value::Integer(_)))
                .count())
        })
        .unwrap()
    }
}

fn validate() {
    let mut pressure = Vec::new();
    for current in [false, true] {
        let lua = Lua::new();
        let capture = if current {
            lua.create_function(
                |lua,
                 CheckedPrefix((first, second, third, rest)): CheckedPrefix<(
                    Value,
                    Value,
                    Value,
                    MultiValue,
                )>| {
                    let values = [first, second, third].into_iter().chain(rest);
                    lua.create_sequence_from(values)
                },
            )
        } else {
            lua.create_function(|lua, mut args: MultiValue| {
                while args.len() < 3 {
                    args.push_back(Value::Nil);
                }
                lua.create_sequence_from(args)
            })
        }
        .unwrap();
        lua.globals().set("capture", capture).unwrap();
        lua.load(r#"
            local t,f,co,ud={},function() end,coroutine.create(function() end),newproxy(true)
            local args={nil,false,17,-0.0,string.char(255,0),t,f,co,ud,nil}
            for n=0,10 do
                local values=capture(unpack(args,1,n))
                for i=1,math.max(n,3) do
                    local expected=args[i]; if i>n then expected=nil end
                    assert(rawequal(values[i],expected))
                end
            end
            local ok,values=coroutine.resume(coroutine.create(function() return capture(t,nil,ud) end))
            assert(ok and values[1]==t and values[2]==nil and values[3]==ud)
        "#).exec().unwrap();
        let target = lua.create_table().unwrap();
        let read = reader(&lua, current);
        let mut held = Vec::new();
        for _ in 0..5000 {
            if let Ok(t) = lua.create_table() {
                held.push(t);
            } else {
                break;
            }
        }
        let args = MultiValue::from_vec((0..4000).map(|_| Value::Table(target.clone())).collect());
        pressure.push(read.call::<usize>(args).map_err(|e| e.to_string()));
        drop(held);
        assert_eq!(read.call::<usize>((17, 23, 42)).unwrap(), 3);
    }
    assert_eq!(pressure[0], pressure[1]);
    #[cfg(panic = "unwind")]
    for current in [false, true] {
        for position in [1, 2, 3, 4, 8] {
            let lua = Lua::new();
            lua.globals().set("read", reader(&lua, current)).unwrap();
            lua.globals()
                .set(
                    "panic_arg",
                    lua.create_function(|_, ()| -> mlua::Result<()> {
                        panic!("prefix callback argument panic")
                    })
                    .unwrap(),
                )
                .unwrap();
            let inputs = (1..=8)
                .map(|i| if i == position { "p" } else { "nil" })
                .collect::<Vec<_>>()
                .join(",");
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                lua.load(format!("local _,p=pcall(panic_arg); return read({inputs})"))
                    .eval::<usize>()
            }))
            .unwrap_err();
            assert_eq!(
                panic.downcast_ref::<&str>().copied(),
                Some("prefix callback argument panic")
            );
            assert_eq!(lua.load("return 6*7").eval::<usize>().unwrap(), 42);
        }
    }
    println!(
        "Validated prefix/extras, nils, identity, binary strings, coroutines, reference-pressure parity/recovery and panic propagation (unwind builds)."
    );
}

fn run() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut std::ffi::c_void;
            fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
        }
        // SAFETY: adjust only this disposable benchmark worker's affinity.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
            0
        );
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "validate") {
        validate();
        return;
    }
    assert_eq!(args.len(), 2, "prefix_args REPORT REVERSE | validate");
    let variants = if args[1] == "1" {
        [true, false]
    } else {
        [false, true]
    };
    let mut records = Vec::new();
    for (workload, arguments, expected) in [
        ("empty0", "", 0),
        ("receiver1", "self", 0),
        ("scalar2", "self,17", 1),
        ("size3", "self,17,23", 2),
        ("extra8", "self,17,23,other,'binary',false,other,nil", 2),
    ] {
        for current in variants {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                lua.globals().set("READ", reader(&lua, current)).unwrap();
                let function=lua.load(format!("local read,self,other=READ,{{}},{{}}; return function() local sum=0; for i=1,8192 do sum=sum+read({arguments}) end; return sum end")).eval::<Function>().unwrap();
                assert_eq!(function.call::<usize>(()).unwrap(), 8192 * expected);
                lua.gc_stop();
                let measurement = metrics::Measurement::start();
                black_box(function.call::<usize>(()).unwrap());
                let result = measurement.finish();
                if sample > 0 {
                    samples.push(result);
                }
            }
            records.push(serde_json::json!({"workload":workload,"variant":if current{"after"}else{"before"},"operations_per_sample":8192,"samples":samples}));
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
