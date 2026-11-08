use super::*;

#[path = "ignored_host_before.rs"]
mod before;

fn manager(lua: &Lua, current: bool) -> (Table, Table) {
    let screen = lua.create_table().unwrap();
    let players = [lua.create_table().unwrap(), lua.create_table().unwrap()];
    if current {
        install_screen_manager_globals(lua, screen.clone(), players)
    } else {
        before::install_screen_manager_globals(lua, screen.clone(), players)
    }
    .unwrap();
    (lua.globals().get("SCREENMAN").unwrap(), screen)
}

#[test]
fn host_callbacks_keep_screen_identity_loading_policy_and_side_effects() {
    let mut outcomes = Vec::new();
    for current in [false, true] {
        let lua = Lua::new();
        let (manager, screen) = manager(&lua, current);
        let getter = manager.get::<Function>("GetTopScreen").unwrap();
        for args in crate::callback_args::perf::argument_cases(&lua, &manager) {
            assert_eq!(getter.call::<Table>(args).unwrap(), screen);
        }
        lua.globals().set("__songlua_screen_loading", true).unwrap();
        assert!(getter.call::<Value>(&manager).unwrap().is_nil());
        lua.globals()
            .set("__songlua_screen_loading", false)
            .unwrap();
        for method in ["SystemMessage", "set_input_redirected"] {
            manager
                .get::<Function>(method)
                .unwrap()
                .call::<()>((&manager, "ignored", Value::Nil))
                .unwrap();
        }
        outcomes.push(
            lua.globals()
                .get::<i64>(SONG_LUA_SIDE_EFFECT_COUNT_KEY)
                .unwrap(),
        );
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert!(outcomes[0] > 0);
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_ignored_host() {
    crate::callback_args::perf::measure_calls("host_top_screen", |lua, current| {
        let (manager, _) = manager(lua, current);
        let getter = manager.get::<Function>("GetTopScreen").unwrap();
        (getter, manager)
    });
}
