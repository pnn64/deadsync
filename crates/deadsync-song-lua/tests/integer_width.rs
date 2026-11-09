use deadlib_lua::{Value, compile};
use deadsync_song_lua::{SongLuaCompileContext, rust_engine::RustSongRuntime};

fn runtime() -> RustSongRuntime {
    RustSongRuntime::new(&SongLuaCompileContext::new(
        ".",
        "integer boundary fixtures",
    ))
    .unwrap()
}

#[test]
fn json_integers_preserve_values_beyond_i32() {
    let mut runtime = runtime();
    for value in [
        i64::MIN,
        i64::from(i32::MIN) - 1,
        i64::from(i32::MIN),
        0,
        i64::from(i32::MAX),
        i64::from(i32::MAX) + 1,
        i64::MAX,
    ] {
        let vm = runtime.vm_mut();
        let decode = vm.get_global(b"JsonDecode").unwrap();
        let json = vm.intern(serde_json::json!(value).to_string().as_bytes());
        assert_eq!(
            vm.call(decode, &[json]).unwrap(),
            &[Value::Number(value as f64)]
        );
    }
}

#[test]
fn runtime_numbers_preserve_values_beyond_native_integer_range() {
    let mut runtime = runtime();
    let read = compile(b"local r=__songlua_compile_song_runtime;return r.__songlua_song_beat,r.__songlua_music_seconds").unwrap();
    for value in boundary_values() {
        runtime.set_values(value, value).unwrap();
        assert_eq!(
            runtime.vm_mut().execute(&read).unwrap(),
            &[
                Value::Number(f64::from(value)),
                Value::Number(f64::from(value))
            ]
        );
    }
}

#[test]
fn range_numbers_preserve_values_beyond_native_integer_range() {
    let mut runtime = runtime();
    for value in boundary_values() {
        let args = [
            Value::Number(f64::from(value)),
            Value::Number(f64::from(value)),
            Value::Number(f64::from(value.abs().max(1.0))),
        ];
        let vm = runtime.vm_mut();
        let range = vm.get_global(b"range").unwrap();
        let [Value::Table(table)] = *vm.call(range, &args).unwrap() else {
            panic!("range should return a table");
        };
        assert_eq!(vm.raw_table_len(table).unwrap(), 1);
        assert_eq!(
            vm.raw_table_get(table, Value::Number(1.)).unwrap(),
            Value::Number(f64::from(value))
        );
    }
}

fn boundary_values() -> [f32; 11] {
    [
        0.0,
        -1.0,
        1.0,
        0.5,
        -2_147_483_648.0,
        -2_147_483_904.0,
        2_147_483_648.0,
        -9_223_372_036_854_775_808.0,
        9_223_372_036_854_775_808.0,
        -18_446_744_073_709_551_616.0,
        f32::MAX,
    ]
}
