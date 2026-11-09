//! Original ScreenManager ignored-argument fixtures on the current Rust host.
use deadlib_lua::Value;
use deadsync_song_lua::{
    SONG_LUA_SIDE_EFFECT_COUNT_KEY, SongLuaCompileContext,
    rust_engine::{RustSongRuntime, test_support},
};
#[path = "../../../tests/support/perf.rs"]
mod allocations;
#[path = "support/baked_program.rs"]
mod api;
#[path = "support/bench_clock.rs"]
mod bench_clock;
#[path = "support/ignored_callback.rs"]
mod ignored_callback;
fn manager(current: bool) -> (RustSongRuntime, Value, Value) {
    let mut host =
        RustSongRuntime::new(&SongLuaCompileContext::new(".", "ignored host callback")).unwrap();
    let screen = api::table(host.vm_mut());
    let players = [api::table(host.vm_mut()), api::table(host.vm_mut())];
    test_support::expose_screen_publication(&mut host).unwrap();
    let publish = host.vm_mut().get_global(b"PUBLISH").unwrap();
    api::call(host.vm_mut(), publish, &[screen, players[0], players[1]]);
    let native = host.vm_mut().get_global(b"SCREENMAN").unwrap();
    if current {
        return (host, native, screen);
    }
    // Independent ordinary Lua model of the original getter/loading and two
    // side-effect-only callbacks; the real publisher supplies the same identities.
    let model = api::eval(
        host.vm_mut(),
        &format!(
            "local function side(...) {key}=({key} or 0)+1 end;return {{GetTopScreen=function(...) if __songlua_screen_loading then return nil end;return __songlua_top_screen end,SystemMessage=side,set_input_redirected=side}}",
            key = SONG_LUA_SIDE_EFFECT_COUNT_KEY
        ),
    )[0];
    (host, model, screen)
}
fn count(host: &mut RustSongRuntime) -> f64 {
    match host
        .vm_mut()
        .get_global(SONG_LUA_SIDE_EFFECT_COUNT_KEY.as_bytes())
        .unwrap()
    {
        Value::Nil => 0.,
        value => api::as_number(value),
    }
}
#[test]
fn host_callbacks_keep_screen_identity_loading_policy_and_side_effects() {
    let mut outcomes = Vec::new();
    for current in [false, true] {
        let (mut host, manager, screen) = manager(current);
        let getter = api::get(host.vm_mut(), manager, b"GetTopScreen");
        for args in ignored_callback::argument_cases(host.vm_mut(), manager) {
            assert_eq!(api::call_one(host.vm_mut(), getter, &args), screen);
        }
        host.vm_mut()
            .set_global(b"__songlua_screen_loading", Value::Boolean(true))
            .unwrap();
        assert_eq!(api::call_one(host.vm_mut(), getter, &[manager]), Value::Nil);
        host.vm_mut()
            .set_global(b"__songlua_screen_loading", Value::Boolean(false))
            .unwrap();
        let before = count(&mut host);
        for method in ["SystemMessage", "set_input_redirected"] {
            let method = api::get(host.vm_mut(), manager, method.as_bytes());
            let text = api::string(host.vm_mut(), "ignored");
            api::call(host.vm_mut(), method, &[manager, text, Value::Nil]);
        }
        outcomes.push(count(&mut host) - before);
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert!(outcomes[0] > 0.);
}
#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_ignored_host() {
    ignored_callback::measure_calls("host_top_screen", |_| {
        let (mut host, manager, _) = manager(true);
        let getter = api::get(host.vm_mut(), manager, b"GetTopScreen");
        (host, getter, manager)
    });
}
