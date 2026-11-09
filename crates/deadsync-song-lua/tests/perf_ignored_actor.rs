//! Original loading size-getter argument fixtures on actual native readers.
use deadlib_lua::{Root, Value};
use deadsync_song_lua::{
    SongLuaCompileContext, {SongRuntime, test_support},
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
const ACTOR: &str = "{__songlua_actor_type='Quad',__songlua_state_size={12,24},__songlua_state_zoom=2,__songlua_state_zoom_x=3}";
fn fixture() -> (SongRuntime, Value, Value) {
    let mut host =
        SongRuntime::new(&SongLuaCompileContext::new(".", "loading getter arguments")).unwrap();
    let original = api::eval(host.vm_mut(), &format!("return {ACTOR}"))[0];
    host.vm_mut()
        .set_global(b"ORIGINAL_ACTOR", original)
        .unwrap();
    let definition = api::eval(host.vm_mut(), "return Def.Quad(ORIGINAL_ACTOR)")[0];
    let scene = host.instantiate_actor(definition).unwrap();
    let actor = host.actor_scene_root(scene).unwrap();
    // Native owned pose setup uses the same authored scale and explicit X axis.
    host.vm_mut().set_global(b"ACTOR", actor).unwrap();
    api::eval(host.vm_mut(), "ACTOR:zoom(2):zoomx(3)");
    (host, actor, original)
}
fn native(host: &mut SongRuntime, actor: Value, name: &'static str) -> Value {
    let root: Root = test_support::prepare_loading_size_getter(host, actor, name).unwrap();
    host.vm_mut().rooted(root).unwrap()
}
fn baseline(host: &mut SongRuntime, actor: Value, name: &str) -> Value {
    let factory = api::eval(
        host.vm_mut(),
        r#"return function(actor,name)
 return function(...)
  if name=='GetNumStates' then return 1 end
  local size=rawget(actor,'__songlua_state_size')
  if type(size)~='table' then error('table expected') end
  local axis=(name=='GetHeight' or name=='GetZoomedHeight') and 2 or 1
  local value=tonumber(rawget(size,axis)) or 0
  if name=='GetZoomedWidth' or name=='GetZoomedHeight' then
   local suffix=axis==1 and 'x' or 'y'
   local zoom=rawget(actor,'__songlua_state_zoom_'..suffix) or rawget(actor,'__songlua_state_zoom') or 1
   local base=rawget(actor,'__songlua_state_basezoom_'..suffix) or rawget(actor,'__songlua_state_basezoom') or 1
   value=value*zoom*base
  end
  return value
 end
end"#,
    )[0];
    let name = api::string(host.vm_mut(), name);
    api::call_one(host.vm_mut(), factory, &[actor, name])
}
fn outcome(host: &mut SongRuntime, f: Value) -> Result<Vec<Value>, String> {
    host.vm_mut().call(f, &[]).map(|v| v.to_vec()).map_err(|e| {
        let error = e.to_string();
        let first = error.lines().next().unwrap_or(&error);
        if first == "table expected"
            || first == "actor table expected"
            || first.ends_with(": table expected")
        {
            "table expected".into()
        } else {
            error
        }
    })
}
#[test]
fn size_getters_keep_values_extra_inputs_errors_and_coroutine_calls() {
    let (mut host, actor, original) = fixture();
    let inputs = ignored_callback::argument_cases(host.vm_mut(), original);
    for name in [
        "GetWidth",
        "GetHeight",
        "GetNumStates",
        "GetZoomedWidth",
        "GetZoomedHeight",
    ] {
        let before = baseline(&mut host, original, name);
        let after = native(&mut host, actor, name);
        for args in &inputs {
            let expected = api::call_one(host.vm_mut(), before, args);
            assert_eq!(
                api::call_one(host.vm_mut(), after, args),
                expected,
                "{name}"
            );
        }
        host.vm_mut().set_global(b"getter", after).unwrap();
        let values = api::eval(
            host.vm_mut(),
            "return coroutine.resume(coroutine.create(function() return getter() end))",
        );
        assert_eq!(values[0], Value::Boolean(true));
    }
    for table in [original, actor] {
        api::set(
            host.vm_mut(),
            table,
            b"__songlua_state_size",
            Value::Boolean(false),
        );
    }
    let before = baseline(&mut host, original, "GetWidth");
    let after = native(&mut host, actor, "GetWidth");
    let before = outcome(&mut host, before);
    let after = outcome(&mut host, after);
    assert!(before.is_err());
    assert_eq!(before, after);
}
#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_ignored_actor() {
    ignored_callback::measure_calls("actor_width", |_| {
        let (mut host, actor, _) = fixture();
        let getter = native(&mut host, actor, "GetWidth");
        (host, getter, actor)
    });
}
