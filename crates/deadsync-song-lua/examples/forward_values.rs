//! Prototype: checked exact-arity forwarding without short argument/result buffers.
use mlua::{FromLua, FromLuaMulti, Function, IntoLuaMulti, Lua, MultiValue, Table, Value};
use std::{hint::black_box, os::raw::c_int};
#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;

enum MethodValues {
    Empty,
    One(Value),
    Two([Value; 2]),
    Three([Value; 3]),
    Many(MultiValue),
}

impl MethodValues {
    fn front(&self) -> Option<&Value> {
        match self {
            Self::Empty => None,
            Self::One(value) => Some(value),
            Self::Two(values) => Some(&values[0]),
            Self::Three(values) => Some(&values[0]),
            Self::Many(values) => values.front(),
        }
    }
}

impl FromLuaMulti for MethodValues {
    fn from_lua_multi(mut values: MultiValue, _: &Lua) -> mlua::Result<Self> {
        Ok(match values.len() {
            0 => Self::Empty,
            1 => Self::One(values.pop_front().unwrap()),
            2 => Self::Two(std::array::from_fn(|_| values.pop_front().unwrap())),
            3 => Self::Three(std::array::from_fn(|_| values.pop_front().unwrap())),
            _ => Self::Many(values),
        })
    }

    unsafe fn from_stack_multi(nvals: c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        // SAFETY: the caller supplies nvals live slots. Use the same checked
        // left-to-right Value conversions, retaining every input and exact arity.
        unsafe {
            Ok(match nvals {
                0 => Self::Empty,
                1 => Self::One(Value::from_stack(-1, lua)?),
                2 => Self::Two([Value::from_stack(-2, lua)?, Value::from_stack(-1, lua)?]),
                3 => Self::Three([
                    Value::from_stack(-3, lua)?,
                    Value::from_stack(-2, lua)?,
                    Value::from_stack(-1, lua)?,
                ]),
                _ => Self::Many(MultiValue::from_stack_multi(nvals, lua)?),
            })
        }
    }
}

impl IntoLuaMulti for MethodValues {
    fn into_lua_multi(self, _: &Lua) -> mlua::Result<MultiValue> {
        Ok(match self {
            Self::Empty => MultiValue::new(),
            Self::One(value) => MultiValue::from_vec(vec![value]),
            Self::Two(values) => MultiValue::from_vec(Vec::from(values)),
            Self::Three(values) => MultiValue::from_vec(Vec::from(values)),
            Self::Many(values) => values,
        })
    }

    unsafe fn push_into_stack_multi(self, lua: &mlua::state::RawLua) -> mlua::Result<c_int> {
        // SAFETY: delegate stack reservation and writes to mlua. The trailing
        // unit preserves MultiValue's n+1 reservation, without adding a value.
        // Borrow each value so all handles survive until every push finishes.
        unsafe {
            match self {
                Self::Empty => ((),).push_into_stack_multi(lua),
                Self::One(value) => (&value, ()).push_into_stack_multi(lua),
                Self::Two(values) => (&values[0], &values[1], ()).push_into_stack_multi(lua),
                Self::Three(values) => {
                    (&values[0], &values[1], &values[2], ()).push_into_stack_multi(lua)
                }
                Self::Many(values) => values.push_into_stack_multi(lua),
            }
        }
    }
}

enum ReturnValues {
    Empty,
    One { _value: Value },
    Many { _values: MultiValue },
}

impl FromLuaMulti for ReturnValues {
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

impl IntoLuaMulti for ReturnValues {
    fn into_lua_multi(self, _: &Lua) -> mlua::Result<MultiValue> {
        Ok(match self {
            Self::Empty => MultiValue::new(),
            Self::One { _value } => MultiValue::from_vec(vec![_value]),
            Self::Many { _values } => _values,
        })
    }
    unsafe fn push_into_stack_multi(self, lua: &mlua::state::RawLua) -> mlua::Result<c_int> {
        // SAFETY: delegated n+1 reservation and checked pushes. Keep handles
        // through all pushes, exactly as MultiValue's borrowed traversal does.
        unsafe {
            match self {
                Self::Empty => ((),).push_into_stack_multi(lua),
                Self::One { _value } => (&_value, ()).push_into_stack_multi(lua),
                Self::Many { _values } => _values.push_into_stack_multi(lua),
            }
        }
    }
}

const METHODS_KEY: &str = "__songlua_bound_methods";

fn forward(lua: &Lua, variant: usize) -> Function {
    let name = "probe";
    macro_rules! make {
        ($args:ty,$result:ty) => {
            lua.create_function(move |_, args: $args| {
                let Some(Value::Table(actor)) = args.front() else {
                    return Err(mlua::Error::runtime(format!(
                        "actor method {name} requires an actor"
                    )));
                };
                let bound = actor.raw_get::<Option<Table>>(METHODS_KEY)?;
                let method = match bound {
                    Some(bound) => bound.raw_get::<Value>(name)?,
                    None => actor.raw_get::<Value>(name)?,
                };
                let Value::Function(method) = method else {
                    return Err(mlua::Error::runtime(format!(
                        "native actor method {name} has no semantic implementation"
                    )));
                };
                method.call::<$result>(args)
            })
            .unwrap()
        };
    }
    match variant {
        0 => make!(MultiValue, MultiValue),
        1 => make!(MethodValues, MultiValue),
        2 => make!(MultiValue, MethodValues),
        3 => make!(MethodValues, MethodValues),
        _ => make!(MethodValues, ReturnValues),
    }
}

fn actor(lua: &Lua, semantic: Function) -> Table {
    let actor = lua.create_table().unwrap();
    let bound = lua.create_table().unwrap();
    bound.raw_set("probe", semantic).unwrap();
    actor.raw_set(METHODS_KEY, bound).unwrap();
    actor
}

fn validate() {
    let mut pressure = Vec::new();
    for variant in 0..5 {
        let lua = Lua::new();
        let semantic = lua
            .load("return function(...) return ... end")
            .eval::<Function>()
            .unwrap();
        let receiver = actor(&lua, semantic);
        lua.globals().raw_set("actor", &receiver).unwrap();
        lua.globals()
            .raw_set("FORWARD", forward(&lua, variant))
            .unwrap();
        lua.load(r#"
            local t,f,co,ud={},function()end,coroutine.create(function()end),newproxy(true)
            local args={actor,nil,false,-0.0,string.char(255,0),t,f,co,ud,nil}
            for n=1,10 do
                local count=select('#',FORWARD(unpack(args,1,n)))
                local values={FORWARD(unpack(args,1,n))}
                assert(count==n)
                for i=1,n do assert(rawequal(values[i],args[i])) end
            end
            local ok,a,b,c=coroutine.resume(coroutine.create(function() return FORWARD(actor,nil,ud) end))
            assert(ok and a==actor and b==nil and c==ud)
            assert(not pcall(FORWARD)); assert(not pcall(FORWARD,false))
        "#).exec().unwrap();
        for expression in [
            "return",
            "return nil",
            "return 17,nil,-0.0",
            "return nil,false,17,{},function()end,string.char(255,0),newproxy(true),nil",
        ] {
            let semantic = lua
                .load(format!("return function(...) {expression} end"))
                .eval::<Function>()
                .unwrap();
            receiver
                .raw_get::<Table>(METHODS_KEY)
                .unwrap()
                .raw_set("probe", &semantic)
                .unwrap();
            let expected = semantic.call::<MultiValue>(()).unwrap();
            let actual = forward(&lua, variant)
                .call::<MultiValue>(&receiver)
                .unwrap();
            assert_eq!(actual.len(), expected.len());
            // Identity checks for freshly-created values are covered by echo.
            for (left, right) in actual.iter().zip(&expected) {
                if let (Value::Number(left), Value::Number(right)) = (left, right) {
                    assert_eq!(left.to_bits(), right.to_bits());
                } else if !matches!(
                    left,
                    Value::Table(_) | Value::Function(_) | Value::UserData(_)
                ) {
                    assert_eq!(left, right);
                }
            }
        }
        let semantic = lua
            .load("return function(...) return ... end")
            .eval::<Function>()
            .unwrap();
        receiver
            .raw_get::<Table>(METHODS_KEY)
            .unwrap()
            .raw_set("probe", semantic)
            .unwrap();
        let held: Vec<_> = (0..5000).filter_map(|_| lua.create_table().ok()).collect();
        let mut args = MultiValue::with_capacity(4000);
        for _ in 0..4000 {
            args.push_back(Value::Table(receiver.clone()));
        }
        pressure.push(
            forward(&lua, variant)
                .call::<MultiValue>(args)
                .map(|values| values.len())
                .map_err(|e| e.to_string()),
        );
        drop(held);
        assert_eq!(
            forward(&lua, variant)
                .call::<MultiValue>(&receiver)
                .unwrap()
                .len(),
            1
        );
    }
    assert!(pressure.windows(2).all(|pair| pair[0] == pair[1]));
    #[cfg(panic = "unwind")]
    for variant in 0..5 {
        for results in [false, true] {
            for position in [1, 2, 3, 4, 8] {
                let lua = Lua::new();
                let semantic = lua
                    .load("return function(...) return ... end")
                    .eval::<Function>()
                    .unwrap();
                let receiver = actor(&lua, semantic);
                lua.globals().raw_set("actor", &receiver).unwrap();
                lua.globals()
                    .raw_set("FORWARD", forward(&lua, variant))
                    .unwrap();
                lua.globals()
                    .raw_set(
                        "panic_arg",
                        lua.create_function(|_, ()| -> mlua::Result<()> {
                            panic!("forward value panic")
                        })
                        .unwrap(),
                    )
                    .unwrap();
                let items = (1..=8)
                    .map(|i| {
                        if i == position {
                            "p"
                        } else if i == 1 {
                            "actor"
                        } else {
                            "nil"
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let script = if results {
                    format!(
                        "local _,p=pcall(panic_arg); actor.__songlua_bound_methods.probe=function() return {items} end; return FORWARD(actor)"
                    )
                } else {
                    format!("local _,p=pcall(panic_arg); return FORWARD({items})")
                };
                let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    lua.load(script).eval::<MultiValue>()
                }))
                .unwrap_err();
                assert_eq!(
                    panic.downcast_ref::<&str>().copied(),
                    Some("forward value panic")
                );
                assert_eq!(lua.load("return 6*7").eval::<i64>().unwrap(), 42);
            }
        }
    }
    println!(
        "Validated exact arity/order, nils, binary/reference identities, coroutine calls, zero/one/three/eight returns, bounded reference-pressure parity/recovery and argument/result panic propagation."
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
        // SAFETY: pin only this disposable worker.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
            0
        );
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "validate") {
        validate();
        return;
    }
    assert_eq!(args.len(), 2, "forward_values REPORT ORDER | validate");
    let order: usize = args[1].parse().unwrap();
    let mut records = Vec::new();
    for (workload, arguments, body, count) in [
        ("receiver1_return1", "receiver", "return 17", 1),
        ("scalar2_return1", "receiver,17", "return (...) ", 1),
        ("size3_return1", "receiver,17,23", "return (...) ", 1),
        (
            "extra8_return1",
            "receiver,17,nil,false,other,other,other,nil",
            "return (...) ",
            1,
        ),
        ("scalar2_return0", "receiver,17", "return", 0),
        ("scalar2_return3", "receiver,17", "return 17,nil,-0.0", 3),
        (
            "extra8_return8",
            "receiver,17,nil,false,other,other,other,nil",
            "return ...",
            8,
        ),
    ] {
        for offset in 0..5 {
            let variant = if order < 5 {
                (offset + order) % 5
            } else {
                (4 - offset + order - 5) % 5
            };
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let semantic = lua
                    .load(format!("return function(...) {body} end"))
                    .eval::<Function>()
                    .unwrap();
                let receiver = actor(&lua, semantic);
                let runner=lua.load(format!("local call,receiver=...; local other={{}}; return function() local n=0; for i=1,8192 do n=n+select('#',call({arguments})) end; return n end"))
                    .call::<Function>((forward(&lua,variant),receiver)).unwrap();
                assert_eq!(runner.call::<usize>(()).unwrap(), 8192 * count);
                lua.gc_stop();
                let measurement = metrics::Measurement::start();
                black_box(runner.call::<usize>(()).unwrap());
                let result = measurement.finish();
                if sample > 0 {
                    samples.push(result);
                }
            }
            records.push(serde_json::json!({"workload":workload,"variant":(["before","args","results","both","narrow"][variant]),"operations_per_sample":8192,"samples":samples}));
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
