use super::*;
use crate::{SONG_LUA_RUNTIME_KEY, create_song_runtime_table};

#[path = "capture_inputs_before.rs"]
mod before;

fn clock() -> Lua {
    let lua = Lua::new();
    let runtime = create_song_runtime_table(&lua, &SongLuaCompileContext::new("", "")).unwrap();
    lua.globals().set(SONG_LUA_RUNTIME_KEY, runtime).unwrap();
    set_compile_song_runtime_values(&lua, 2.0, 3.0).unwrap();
    lua
}

fn prepare_capture(
    count: usize,
    touched: usize,
    aliases: bool,
    fail: bool,
) -> (Lua, Vec<(usize, Table)>, Function) {
    let lua = clock();
    let mut actors = Vec::new();
    for index in 0..count {
        let actor = lua.create_table().unwrap();
        actor.raw_set("__songlua_actor_type", "Quad").unwrap();
        actor.raw_set("__songlua_visible", true).unwrap();
        actor.raw_set("__songlua_state_x", index as f32).unwrap();
        reset_actor_capture(&lua, &actor).unwrap();
        actors.push((index * 3 + 1, actor));
    }
    if aliases && count > 0 {
        actors.push((count * 3 + 1, actors[0].1.clone()));
    }
    let targets: Vec<_> = actors
        .iter()
        .take(touched)
        .map(|(_, actor)| actor.clone())
        .collect();
    let function = lua
        .create_function(move |lua, value: Option<f32>| {
            for actor in &targets {
                capture_block_set_f32(lua, actor, "x", value.unwrap_or(12.0))?;
                actor.raw_set("__songlua_visible", false)?;
            }
            if fail {
                return Err(mlua::Error::runtime("capture input oracle failure"));
            }
            Ok(())
        })
        .unwrap();
    (lua, actors, function)
}

fn capture_case(
    current: bool,
    touched_only: bool,
    count: usize,
    touched: usize,
    aliases: bool,
    fail: bool,
) -> (String, Vec<(bool, Option<f32>)>) {
    let (lua, actors, function) = prepare_capture(count, touched, aliases, fail);
    let result = match (current, touched_only) {
        (false, false) => before::capture_indexed_actor_function_blocks(
            &lua,
            &actors,
            &function,
            Some(12.0),
            Some(9.0),
        ),
        (true, false) => super::capture_indexed_actor_function_blocks(
            &lua,
            &actors,
            &function,
            Some(12.0),
            Some(9.0),
        ),
        (false, true) => {
            before::capture_touched_blocks(&lua, &actors, &function, Some(12.0), Some(9.0))
        }
        (true, true) => {
            super::capture_touched_blocks(&lua, &actors, &function, Some(12.0), Some(9.0))
        }
    };
    assert_eq!(compile_song_runtime_values(&lua).unwrap(), (2.0, 3.0));
    let state = actors
        .iter()
        .map(|(_, actor)| {
            (
                actor.raw_get::<bool>("__songlua_visible").unwrap(),
                actor.raw_get::<Option<f32>>("__songlua_state_x").unwrap(),
            )
        })
        .collect();
    (format!("{:?}", result.map_err(|e| e.to_string())), state)
}

#[test]
fn borrowed_capture_inputs_keep_alias_order_restoration_and_errors() {
    for count in [0, 1, 8, 64] {
        for touched in [0, 1, count / 2, count] {
            for touched_only in [false, true] {
                for aliases in [false, true] {
                    for fail in [false, true] {
                        assert_eq!(
                            capture_case(false, touched_only, count, touched, aliases, fail),
                            capture_case(true, touched_only, count, touched, aliases, fail),
                            "count={count} touched={touched} touched_only={touched_only} aliases={aliases} fail={fail}",
                        );
                    }
                }
            }
        }
    }
}

fn prepare_sounds(count: usize, extra_bytes: usize, fail: bool) -> (Lua, Function, Vec<String>) {
    let lua = clock();
    let paths: Vec<_> = (0..count)
        .map(|i| format!("sounds/é曲-{}{}.wav", i % 3, "x".repeat(extra_bytes)))
        .collect();
    let strings: Vec<_> = paths
        .iter()
        .map(|p| lua.create_string(p).unwrap())
        .collect();
    let function = lua
        .create_function(move |lua, ()| {
            let calls: Table = lua.globals().get(SONG_LUA_SOUND_CALLS_KEY)?;
            for (index, path) in strings.iter().enumerate() {
                calls.raw_set(index + 1, path)?;
                crate::runtime::note_song_lua_side_effect(lua)?;
            }
            if fail {
                return Err(mlua::Error::runtime("sound input oracle failure"));
            }
            Ok(())
        })
        .unwrap();
    (lua, function, paths)
}

fn sound_case(current: bool, count: usize, fail: bool) -> (String, Vec<SongLuaSoundEvent>, usize) {
    let (lua, function, paths) = prepare_sounds(count, 0, fail);
    let mut messages = Vec::new();
    let mut events = Vec::new();
    let mut counter = 0;
    let result = if current {
        super::compile_overlay_compile_actor_function_action::<()>(
            &lua,
            &mut [],
            &mut [],
            &function,
            4.5,
            true,
            &mut counter,
            &mut messages,
            &mut events,
        )
    } else {
        before::compile_overlay_compile_actor_function_action::<()>(
            &lua,
            &mut [],
            &mut [],
            &function,
            4.5,
            true,
            &mut counter,
            &mut messages,
            &mut events,
        )
    };
    assert!(messages.is_empty());
    assert_eq!(compile_song_runtime_values(&lua).unwrap(), (2.0, 3.0));
    if !fail {
        assert_eq!(
            events
                .iter()
                .map(|e| e.path.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            paths
        );
        assert!(
            events
                .iter()
                .all(|e| e.beat == 4.5 && e.source_message.is_none() && !e.queued_startup)
        );
    }
    (format!("{result:?}"), events, counter)
}

#[test]
fn moved_capture_sound_paths_keep_duplicates_order_event_metadata_and_errors() {
    for count in [0, 1, 8, 128] {
        for fail in [false, true] {
            assert_eq!(
                sound_case(false, count, fail),
                sound_case(true, count, fail)
            );
        }
    }
}

#[cfg(windows)]
#[path = "capture_inputs_bench.rs"]
mod bench;
