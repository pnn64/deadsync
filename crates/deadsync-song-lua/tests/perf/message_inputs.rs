use super::*;
use crate::{SONG_LUA_RUNTIME_KEY, create_song_runtime_table};

#[path = "message_inputs_before.rs"]
mod before;

fn prepare(
    count: usize,
    messages: usize,
    aliases: bool,
    fail: bool,
) -> (Lua, Vec<SongLuaOverlayCompileActor<()>>, Function) {
    let lua = Lua::new();
    let runtime = create_song_runtime_table(&lua, &SongLuaCompileContext::new("", "")).unwrap();
    lua.globals().set(SONG_LUA_RUNTIME_KEY, runtime).unwrap();
    set_compile_song_runtime_values(&lua, 2.0, 3.0).unwrap();
    let mut actors: Vec<SongLuaOverlayCompileActor<()>> = (0..count)
        .map(|i| {
            let table = lua.create_table().unwrap();
            table.raw_set("Name", format!("Actor{i}")).unwrap();
            table.raw_set("__songlua_actor_type", "Quad").unwrap();
            table.raw_set("__songlua_visible", true).unwrap();
            table.raw_set("__songlua_state_x", i as f32).unwrap();
            reset_actor_capture(&lua, &table).unwrap();
            SongLuaOverlayCompileActor {
                table,
                actor: crate::SongLuaOverlayActor {
                    kind: (),
                    name: Some(format!("Actor{i}")),
                    parent_index: None,
                    initial_state: SongLuaOverlayState::default(),
                    message_commands: Vec::new(),
                },
                message_sounds: Vec::new(),
            }
        })
        .collect();
    for i in 0..count {
        for command_index in 0..messages {
            let target = actors[(i + 1) % count].table.clone();
            let command = lua
                .create_function(move |lua, _: mlua::MultiValue| {
                    capture_block_set_f32(lua, &target, "x", 12.0 + command_index as f32)?;
                    target.raw_set("__songlua_visible", false)?;
                    if fail {
                        return Err(mlua::Error::runtime("message input oracle failure"));
                    }
                    Ok(())
                })
                .unwrap();
            actors[i]
                .table
                .raw_set(format!("Probe{command_index}MessageCommand"), command)
                .unwrap();
        }
    }
    if aliases && count > 0 {
        actors.push(SongLuaOverlayCompileActor {
            table: actors[0].table.clone(),
            actor: crate::SongLuaOverlayActor {
                kind: (),
                name: Some("Alias".into()),
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            },
            message_sounds: Vec::new(),
        });
    }
    let targets: Vec<_> = actors.iter().map(|a| a.table.clone()).collect();
    let function = lua
        .create_function(move |lua, ()| {
            for target in &targets {
                capture_block_set_f32(lua, target, "x", 17.0)?;
            }
            if fail {
                return Err(mlua::Error::runtime("action input oracle failure"));
            }
            Ok(())
        })
        .unwrap();
    (lua, actors, function)
}

fn action_case(current: bool, count: usize, aliases: bool, fail: bool) -> String {
    let (lua, actors, function) = prepare(count, 0, aliases, fail);
    let result = if current {
        super::capture_overlay_compile_actor_function_action_blocks(
            &lua,
            &actors,
            &[],
            &function,
            9.0,
        )
    } else {
        before::capture_overlay_compile_actor_function_action_blocks(
            &lua,
            &actors,
            &[],
            &function,
            9.0,
        )
    };
    assert_eq!(compile_song_runtime_values(&lua).unwrap(), (2.0, 3.0));
    let states: Vec<_> = actors
        .iter()
        .map(|a| a.table.raw_get::<Option<f32>>("__songlua_state_x").unwrap())
        .collect();
    format!("{result:?} {states:?}")
}

fn message_case(current: bool, count: usize, messages: usize, aliases: bool, fail: bool) -> String {
    let (lua, mut actors, _) = prepare(count, messages, aliases, fail);
    let mut dynamic = Vec::new();
    let result = if current {
        super::capture_stable_cross_actor_message_commands(&lua, &mut actors, |v| dynamic.push(v))
    } else {
        before::capture_stable_cross_actor_message_commands(&lua, &mut actors, |v| dynamic.push(v))
    };
    assert_eq!(compile_song_runtime_values(&lua).unwrap(), (2.0, 3.0));
    let outputs: Vec<_> = actors
        .iter()
        .map(|a| {
            let mut commands = a.actor.message_commands.clone();
            commands.sort_by(|a, b| a.message.cmp(&b.message));
            (
                commands,
                a.table.raw_get::<Option<f32>>("__songlua_state_x").unwrap(),
                a.table.raw_get::<bool>("__songlua_visible").unwrap(),
            )
        })
        .collect();
    dynamic.sort();
    format!("{result:?} {outputs:?} {dynamic:?}")
}

#[test]
fn direct_action_actor_inputs_keep_indices_aliases_state_and_errors() {
    for count in [0, 1, 8, 64] {
        for aliases in [false, true] {
            for fail in [false, true] {
                assert_eq!(
                    action_case(false, count, aliases, fail),
                    action_case(true, count, aliases, fail)
                );
            }
        }
    }
}

#[test]
fn borrowed_message_sources_keep_targets_aliases_commands_and_errors() {
    for count in [0, 1, 8] {
        for messages in [0, 1, 4] {
            for aliases in [false, true] {
                for fail in [false, true] {
                    assert_eq!(
                        message_case(false, count, messages, aliases, fail),
                        message_case(true, count, messages, aliases, fail)
                    );
                }
            }
        }
    }
}

#[cfg(windows)]
#[path = "message_inputs_bench.rs"]
mod bench;
