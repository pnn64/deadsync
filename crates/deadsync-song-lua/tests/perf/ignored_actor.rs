use super::*;

#[path = "ignored_actor_before.rs"]
mod before;

fn actor(lua: &Lua, current: bool) -> Table {
    let actor=lua.load("{__songlua_actor_type='Quad',__songlua_state_size={12,24},__songlua_state_zoom=2,__songlua_state_zoom_x=3}").eval::<Table>().unwrap();
    if current {
        install_actor_size_getter_methods(lua, &actor)
    } else {
        before::install_actor_size_getter_methods(lua, &actor)
    }
    .unwrap();
    actor
}

#[test]
fn size_getters_keep_values_extra_inputs_errors_and_coroutine_calls() {
    let lua = Lua::new();
    let old = actor(&lua, false);
    let new = actor(&lua, true);
    let inputs = crate::callback_args::perf::argument_cases(&lua, &old);
    for name in [
        "GetWidth",
        "GetHeight",
        "GetNumStates",
        "GetZoomedWidth",
        "GetZoomedHeight",
    ] {
        let before = old.get::<Function>(name).unwrap();
        let after = new.get::<Function>(name).unwrap();
        for args in &inputs {
            assert_eq!(
                before.call::<Value>(args.clone()).unwrap(),
                after.call::<Value>(args.clone()).unwrap(),
                "{name}"
            );
        }
        lua.globals().set("getter", after).unwrap();
        let (ok, _): (bool, Value) = lua
            .load("return coroutine.resume(coroutine.create(function() return getter() end))")
            .eval()
            .unwrap();
        assert!(ok);
    }
    for table in [&old, &new] {
        table.raw_set("__songlua_state_size", false).unwrap();
    }
    let before = old
        .get::<Function>("GetWidth")
        .unwrap()
        .call::<Value>(())
        .map_err(|e| e.to_string());
    let after = new
        .get::<Function>("GetWidth")
        .unwrap()
        .call::<Value>(())
        .map_err(|e| e.to_string());
    assert_eq!(before, after);
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_ignored_actor() {
    crate::callback_args::perf::measure_calls("actor_width", |lua, current| {
        let actor = actor(lua, current);
        let getter = actor.get::<Function>("GetWidth").unwrap();
        (getter, actor)
    });
}
