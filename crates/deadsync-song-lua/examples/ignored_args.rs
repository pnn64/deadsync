//! Prototype for retaining checked callback inputs without a receiver buffer allocation.
use mlua::{FromLua, FromLuaMulti, Function, Lua, MultiValue, Value};
use std::{hint::black_box, os::raw::c_int};

#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;

enum IgnoredArgs {
    Empty,
    One { _value: Value },
    Many { _values: MultiValue },
}

impl FromLuaMulti for IgnoredArgs {
    fn from_lua_multi(mut values: MultiValue, _: &Lua) -> mlua::Result<Self> {
        Ok(match values.len() {
            0 => Self::Empty,
            1 => Self::One {
                _value: values.pop_front().unwrap(),
            },
            _ => Self::Many { _values: values },
        })
    }

    unsafe fn from_stack_multi(nvals: c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        // SAFETY: FromLuaMulti's caller supplies nvals live argument slots.
        // Keep the same checked conversions and callback input lifetime. Larger
        // lists use mlua's original implementation, including conversion order.
        Ok(match nvals {
            0 => Self::Empty,
            1 => Self::One {
                _value: unsafe { Value::from_stack(-1, lua)? },
            },
            _ => Self::Many {
                _values: unsafe { MultiValue::from_stack_multi(nvals, lua)? },
            },
        })
    }
}

fn reader(lua: &Lua, current: bool) -> Function {
    if current {
        lua.create_function(|_, _args: IgnoredArgs| Ok(17)).unwrap()
    } else {
        lua.create_function(|_, _args: MultiValue| Ok(17)).unwrap()
    }
}

fn validate() {
    let mut pressure = Vec::new();
    for current in [false, true] {
        let lua = Lua::new();
        let capture = if current {
            lua.create_function(|lua, args: IgnoredArgs| {
                let args = match args {
                    IgnoredArgs::Empty => Vec::new(),
                    IgnoredArgs::One { _value } => vec![_value],
                    IgnoredArgs::Many { _values } => _values.into_vec(),
                };
                let count = args.len();
                Ok((count, lua.create_sequence_from(args)?))
            })
        } else {
            lua.create_function(|lua, args: MultiValue| {
                let count = args.len();
                Ok((count, lua.create_sequence_from(args)?))
            })
        }
        .unwrap();
        lua.globals().set("capture", capture).unwrap();
        lua.load(r#"
            local t, f, co, ud = {}, function() end, coroutine.create(function() end), newproxy(true)
            local args = {nil, false, 17, -0.0, string.char(255,0), t, f, co, ud, nil}
            for n=0,10 do
                local count, values = capture(unpack(args,1,n))
                assert(count==n)
                for i=1,n do assert(rawequal(values[i],args[i])) end
            end
            local ok, n, values = coroutine.resume(coroutine.create(function() return capture(t, nil, ud) end))
            assert(ok and n==3 and values[1]==t and values[2]==nil and values[3]==ud)
        "#).exec().unwrap();
        let target = lua.create_table().unwrap();
        let getter = reader(&lua, current);
        assert_eq!(getter.call::<i64>(&target).unwrap(), 17);
        let mut held = Vec::new();
        for _ in 0..5000 {
            if let Ok(table) = lua.create_table() {
                held.push(table);
            } else {
                break;
            }
        }
        let args = MultiValue::from_vec((0..4000).map(|_| Value::Table(target.clone())).collect());
        pressure.push(getter.call::<i64>(args).map_err(|error| error.to_string()));
        drop(held);
        assert_eq!(getter.call::<i64>(&target).unwrap(), 17);
    }
    assert_eq!(pressure[0], pressure[1]);
    // Lua's default pcall can catch a Rust panic payload. Ignored extra inputs
    // must still be converted so both variants resume that payload identically.
    #[cfg(panic = "unwind")]
    for current in [false, true] {
        let lua = Lua::new();
        lua.globals().set("read", reader(&lua, current)).unwrap();
        lua.globals()
            .set(
                "panic_arg",
                lua.create_function(|_, ()| -> mlua::Result<()> {
                    panic!("ignored callback argument panic")
                })
                .unwrap(),
            )
            .unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            lua.load("local _, p=pcall(panic_arg); return read({},p)")
                .eval::<i64>()
        }))
        .unwrap_err();
        assert_eq!(
            panic.downcast_ref::<&str>().copied(),
            Some("ignored callback argument panic")
        );
        assert_eq!(lua.load("return 6*7").eval::<i64>().unwrap(), 42);
    }
    println!(
        "Validated values/order, nils, binary strings, coroutine inputs and reference-pressure parity/recovery (panic tests also run when built with unwinding)."
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
    assert_eq!(args.len(), 2, "ignored_args REPORT REVERSE | validate");
    let variants = if args[1] == "1" {
        [true, false]
    } else {
        [false, true]
    };
    let mut records = Vec::new();
    for (workload, arguments) in [
        ("empty0", ""),
        ("receiver1", "self"),
        ("scalar1", "17"),
        ("extra3", "self,nil,other"),
        ("extra8", "self,nil,other,17,'binary',false,other,nil"),
    ] {
        for current in variants {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                lua.globals().set("READ", reader(&lua, current)).unwrap();
                let function=lua.load(format!("local read,self,other=READ,{{}},{{}}; return function() local sum=0; for i=1,8192 do sum=sum+read({arguments}) end; return sum end"))
                    .eval::<Function>().unwrap();
                assert_eq!(function.call::<i64>(()).unwrap(), 8192 * 17);
                lua.gc_stop();
                let measurement = metrics::Measurement::start();
                black_box(function.call::<i64>(()).unwrap());
                let result = measurement.finish();
                if sample > 0 {
                    samples.push(result);
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
