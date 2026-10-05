use super::*;
use std::hint::black_box;

#[path = "update_dispatch_compile_baseline.rs"]
mod baseline;

struct Fixture {
    lua: Lua,
    context: SongLuaCompileContext,
    root: Value,
    overlays: Vec<SongLuaOverlayCompileActor<()>>,
}

impl Fixture {
    fn new(count: usize, seconds: f32) -> Self {
        let lua = Lua::new();
        let mut context = SongLuaCompileContext::new("", "Update dispatch");
        context.music_length_seconds = seconds;
        context.song_timing_bpms = vec![(0.0, 120.0), (1.0, 180.0)];
        context.song_music_rate = 1.25;
        let runtime = crate::create_song_runtime_table(&lua, &context).unwrap();
        lua.globals()
            .raw_set(crate::SONG_LUA_RUNTIME_KEY, runtime)
            .unwrap();
        for key in SONG_LUA_PLAYER_OPTIONS_KEYS {
            lua.globals()
                .raw_set(key, lua.create_table().unwrap())
                .unwrap();
        }
        let root = lua.create_table().unwrap();
        root.raw_set("__songlua_actor_type", "ActorFrame").unwrap();
        root.raw_set("__songlua_update_rate", 0.75).unwrap();
        let overlays = (0..count)
            .map(|index| {
                let table = lua.create_table().unwrap();
                table.raw_set("__songlua_actor_type", "Actor").unwrap();
                table
                    .raw_set("__songlua_recurring_update_command", "TickCommand")
                    .unwrap();
                table
                    .raw_set("__songlua_recurring_update_interval", 0.05)
                    .unwrap();
                table
                    .raw_set("__songlua_update_rate", 1.0 + (index % 3) as f64 * 0.25)
                    .unwrap();
                table
                    .raw_set(
                        "__songlua_update_function",
                        lua.create_function(|lua, (actor, dt): (Table, f64)| {
                            let (beat, _) = compile_song_runtime_values(lua)?;
                            crate::lua_util::capture_block_set_f32(
                                lua,
                                &actor,
                                "x",
                                beat + dt as f32,
                            )?;
                            Ok(())
                        })
                        .unwrap(),
                    )
                    .unwrap();
                table
                    .raw_set(
                        "TickCommand",
                        lua.create_function(|lua, actor: Table| {
                            let (beat, _) = compile_song_runtime_values(lua)?;
                            crate::lua_util::capture_block_set_f32(lua, &actor, "y", beat * 2.0)?;
                            // Synthetic actors schedule their next cycle directly.
                            actor.raw_set("__songlua_recurring_update_command", "TickCommand")?;
                            Ok(())
                        })
                        .unwrap(),
                    )
                    .unwrap();
                reset_actor_capture(&lua, &table).unwrap();
                root.raw_set(index + 1, &table).unwrap();
                SongLuaOverlayCompileActor {
                    table,
                    actor: crate::SongLuaOverlayActor {
                        kind: (),
                        name: Some(format!("actor{index}")),
                        parent_index: None,
                        initial_state: SongLuaOverlayState::default(),
                        message_commands: vec![],
                    },
                    message_sounds: vec![],
                }
            })
            .collect();
        Self {
            lua,
            context,
            root: Value::Table(root),
            overlays,
        }
    }

    fn compile(
        &mut self,
        old: bool,
    ) -> (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        Vec<SongLuaOverlayUpdateTrack>,
        Vec<SongLuaColumnOffsetWindow>,
        Vec<SongLuaStatefulMessageCapture>,
        Vec<(f32, String, bool)>,
    ) {
        if old {
            baseline::compile_update_functions(
                &self.lua,
                &self.root,
                &self.context,
                &mut self.overlays,
                &[],
                &[],
            )
        } else {
            compile_update_functions(
                &self.lua,
                &self.root,
                &self.context,
                &mut self.overlays,
                &mut [],
                &[],
                &mut Vec::new(),
                &mut Vec::new(),
                &mut Vec::new(),
                &mut Vec::new(),
            )
        }
        .unwrap()
    }
}

#[test]
fn update_dispatch_complete_compiler_preserves_tracks_timing_and_recurring_commands() {
    for count in [0, 1, 16] {
        for seconds in [0.0, 0.03, 1.01] {
            let mut old = Fixture::new(count, seconds).compile(true);
            let mut new = Fixture::new(count, seconds).compile(false);
            old.2
                .sort_by_key(|track| (track.overlay_index, track.target));
            new.2
                .sort_by_key(|track| (track.overlay_index, track.target));
            assert_eq!(
                format!("{new:?}"),
                format!("{old:?}"),
                "count={count}, seconds={seconds}"
            );
        }
    }
}

#[test]
#[ignore = "manual release benchmark; run serially"]
fn update_dispatch_complete_compiler_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for count in [1, 16, 64] {
        for old in order {
            crate::perf::measure_sampled_with_setup(
                &format!(
                    "compile_dispatch_{count}/{}",
                    if old { "old" } else { "new" }
                ),
                3,
                1,
                || Fixture::new(count, 2.0),
                |fixture| drop(black_box(fixture.compile(old))),
            );
        }
    }
}
