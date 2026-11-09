//! Original style callback argument fixtures on actual current catalog objects.
use deadlib_lua::Value;
use deadsync_song_lua::{
    SongLuaCompileContext, SongRuntime, song_lua_style_column_name, song_lua_style_column_x,
    song_lua_style_info,
};
#[expect(
    dead_code,
    reason = "this fixture uses scoped measurement from the shared allocation API"
)]
#[path = "../../../tests/support/perf.rs"]
mod allocations;
#[path = "support/baked_program.rs"]
mod api;
#[path = "support/bench_clock.rs"]
mod bench_clock;
#[path = "support/ignored_callback.rs"]
mod ignored_callback;
fn style(name: &str) -> (SongRuntime, Value) {
    let mut context = SongLuaCompileContext::new(".", "ignored style callback");
    context.style_name = name.into();
    let mut host = SongRuntime::new(&context).unwrap();
    let style = api::eval(host.vm_mut(), "return GAMESTATE:GetCurrentStyle()")[0];
    (host, style)
}
#[test]
fn style_callbacks_keep_names_widths_columns_and_consumed_argument_methods() {
    for name in ["single", "double", "solo"] {
        let (mut host, style) = style(name);
        let info = song_lua_style_info(name);
        // The frozen constructor used these same pure metadata helpers. Observe
        // the published native closures independently of their argument registers.
        for method in [
            "GetName",
            "GetStepsType",
            "GetStyleType",
            "ColumnsPerPlayer",
            "GetWidth",
        ] {
            let expected = match method {
                "GetName" => api::string(host.vm_mut(), info.name),
                "GetStepsType" => api::string(host.vm_mut(), info.steps_type),
                "GetStyleType" => api::string(host.vm_mut(), info.style_type),
                "ColumnsPerPlayer" => Value::Number(info.columns as f64),
                "GetWidth" => Value::Number(info.width as f64),
                _ => unreachable!(),
            };
            let getter = api::get(host.vm_mut(), style, method.as_bytes());
            for args in ignored_callback::argument_cases(host.vm_mut(), style) {
                assert_eq!(
                    api::call_one(host.vm_mut(), getter, &args),
                    expected,
                    "{name}:{method}"
                );
            }
        }
        let getter = api::get(host.vm_mut(), style, b"GetColumnInfo");
        let ignored = api::string(host.vm_mut(), "ignored");
        let actual = api::call_one(host.vm_mut(), getter, &[style, ignored, Value::Number(2.)]);
        for (key, expected) in [
            (
                "Name",
                api::string(host.vm_mut(), song_lua_style_column_name(1)),
            ),
            ("Track", Value::Number(1.)),
            (
                "XOffset",
                Value::Number(song_lua_style_column_x(info.name, 1) as f64),
            ),
        ] {
            assert_eq!(api::get(host.vm_mut(), actual, key.as_bytes()), expected);
        }
    }
}
#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_ignored_tables() {
    ignored_callback::measure_calls("style_width", |_| {
        let (mut host, style) = style("single");
        let getter = api::get(host.vm_mut(), style, b"GetWidth");
        (host, getter, style)
    });
}
