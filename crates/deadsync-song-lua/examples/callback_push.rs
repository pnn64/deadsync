//! Setter API projection: identical callbacks published through three input types.
use mlua::{Function, Lua, MultiValue, Table, Value};
#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;

fn publish<const MODE: u8>(table: &Table, key: &str, function: Function) -> mlua::Result<()> {
    match MODE {
        0 => table.set(key, function),
        1 => table.set(key, Value::Function(function)),
        _ => table.set(key, &function),
    }
}

fn install<const MODE: u8>(lua: &Lua, table: &Table, key: &'static str) -> mlua::Result<()> {
    let function = lua.create_function(move |_, args: MultiValue| Ok((key, args.len())))?;
    publish::<MODE>(table, key, function)
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
        for publish in [publish::<0>, publish::<1>, publish::<2>] {
            let lua = Lua::new();
            let table = lua.create_table().unwrap();
            let native = lua.create_function(|_, args: MultiValue| Ok(args)).unwrap();
            publish(&table, "callback", native.clone()).unwrap();
            let restored = table.raw_get::<Function>("callback").unwrap();
            assert_eq!(restored, native);
            assert!(restored.environment().is_none());
            let value = lua.create_table().unwrap();
            assert_eq!(restored.call::<Table>(&value).unwrap(), value);
            let replacement = lua.create_function(|_, ()| Ok(42)).unwrap();
            publish(&table, "callback", replacement.clone()).unwrap();
            assert_eq!(table.raw_get::<Function>("callback").unwrap(), replacement);
        }
        println!(
            "Validated callback identity, native classification, arguments and replacement for all three setter inputs."
        );
        return;
    }
    assert_eq!(args.len(), 2, "callback_push REPORT ORDER | validate");
    let order: usize = args[1].parse().unwrap();
    let variants: [(&str, fn(&Lua, &Table, &'static str) -> mlua::Result<()>); 3] = [
        ("owned", install::<0>),
        ("value", install::<1>),
        ("borrowed", install::<2>),
    ];
    let names: [&'static str; 16] = [
        "x",
        "y",
        "aux",
        "linear",
        "GetWidth",
        "visible",
        "diffusealpha",
        "SetUpdateFunction",
        "__index",
        "__newindex",
        "__concat",
        "__tostring",
        "zoom",
        "rotationz",
        "effectclock",
        "name",
    ];
    let mut records = Vec::new();
    for (workload, count) in [("class16", 16), ("metatable4", 4)] {
        for offset in 0..3 {
            let index = if order < 3 {
                (offset + order) % 3
            } else {
                (2 - offset + order - 3) % 3
            };
            let (variant, install) = variants[index];
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let tables: Vec<_> = (0..256)
                    .map(|_| lua.create_table_with_capacity(0, count).unwrap())
                    .collect();
                lua.gc_stop();
                let measurement = metrics::Measurement::start();
                for table in &tables {
                    for name in &names[..count] {
                        install(&lua, table, name).unwrap();
                    }
                }
                let result = measurement.finish();
                if sample > 0 {
                    samples.push(result);
                }
            }
            records.push(serde_json::json!({"workload":workload,"variant":variant,"operations_per_sample":256,"samples":samples}));
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
