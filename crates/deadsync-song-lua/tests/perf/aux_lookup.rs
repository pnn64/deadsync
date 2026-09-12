use super::*;
use std::hint::black_box;

#[path = "aux_lookup_baseline.rs"]
mod baseline;

type AuxSnapshot = (Table, Vec<(String, Value)>);

fn batch(
    queries: &[Table],
    snapshots: &[(Table, Vec<(String, Value)>)],
    old: bool,
) -> Vec<Option<u32>> {
    if old {
        queries
            .iter()
            .map(|a| baseline::captured_actor_aux_change(a, snapshots).map(f32::to_bits))
            .collect()
    } else {
        let lookup = ActorAuxSnapshots::new(snapshots, queries.len());
        queries
            .iter()
            .map(|a| captured_actor_aux_change(a, &lookup).map(f32::to_bits))
            .collect()
    }
}

fn fixture(
    lua: &Lua,
    count: usize,
    fields: usize,
) -> (Vec<Table>, Vec<AuxSnapshot>) {
    let actors: Vec<_> = (0..count)
        .map(|i| {
            let actor = lua.create_table().unwrap();
            actor.raw_set("__songlua_aux", (i + 1) as f32).unwrap();
            actor
        })
        .collect();
    let snapshots = actors
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let mut state: Vec<_> = (0..fields)
                .map(|j| (format!("field_{j}"), Value::Integer(j as i64)))
                .collect();
            state.push(("__songlua_aux".to_owned(), Value::Number(i as f64)));
            (a.clone(), state)
        })
        .collect();
    (actors, snapshots)
}

#[test]
fn lua_command_aux_lookup_preserves_first_matches_missing_actors_and_numeric_bits() {
    let lua = Lua::new();
    for count in [0, 1, 8, 31, 32, 33, 128] {
        let (mut actors, mut snapshots) = fixture(&lua, count, 8);
        let missing = lua.create_table().unwrap();
        missing.raw_set("__songlua_aux", 19).unwrap();
        actors.push(missing);
        for (i, (actor, state)) in snapshots.iter_mut().enumerate() {
            let value = match i % 8 {
                0 => Value::String(lua.create_string(" NaN ").unwrap()),
                1 => Value::String(lua.create_string(" inf ").unwrap()),
                2 => Value::String(lua.create_string([255]).unwrap()),
                3 => Value::Number(f64::INFINITY),
                4 => Value::Number(-0.0),
                5 => Value::Boolean(true),
                6 => Value::Integer(i64::MAX),
                _ => Value::Nil,
            };
            state.insert(0, ("__songlua_aux".to_owned(), value));
            if i % 3 == 0 {
                actor.raw_set("__songlua_aux", -0.0).unwrap();
            }
            if i % 7 == 0 {
                actor.raw_set("__songlua_aux", f64::NAN).unwrap();
            }
        }
        if let Some(first) = snapshots.first().cloned() {
            snapshots.push((
                first.0,
                vec![("__songlua_aux".to_owned(), Value::Number(999.0))],
            ));
        }
        actors.reverse();
        assert_eq!(
            batch(&actors, &snapshots, true),
            batch(&actors, &snapshots, false)
        );
        assert_eq!(
            batch(&actors[..1], &snapshots, true),
            batch(&actors[..1], &snapshots, false)
        );
        snapshots.reverse();
        assert_eq!(
            batch(&actors, &snapshots, true),
            batch(&actors, &snapshots, false)
        );
    }
}

#[test]
fn lua_command_aux_lookup_preserves_getter_order_errors_and_mutation() {
    let lua = Lua::new();
    let (actors, snapshots) = fixture(&lua, 64, 0);
    let events = lua.create_table().unwrap();
    for (i, actor) in actors.iter().enumerate() {
        actor.raw_set("__songlua_aux", Value::Nil).unwrap();
        let mt = lua.create_table().unwrap();
        let events = events.clone();
        let next = actors[(i + 1) % actors.len()].clone();
        mt.raw_set(
            "__index",
            lua.create_function(move |_, (_table, key): (Table, String)| {
                assert_eq!(key, "__songlua_aux");
                events.raw_set(events.raw_len() + 1, i)?;
                next.raw_set("changed", true)?;
                if i % 2 == 0 {
                    Err(mlua::Error::runtime("aux getter sentinel"))
                } else {
                    Ok(i as f32)
                }
            })
            .unwrap(),
        )
        .unwrap();
        actor.set_metatable(Some(mt)).unwrap();
    }
    let mut outcomes = Vec::new();
    for old in [true, false] {
        events.clear().unwrap();
        outcomes.push((
            batch(&actors, &snapshots, old),
            events
                .sequence_values::<usize>()
                .collect::<mlua::Result<Vec<_>>>()
                .unwrap(),
        ));
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(outcomes[1].1, (0..64).collect::<Vec<_>>());
}

#[test]
fn lua_command_aux_lookup_small_batches_have_no_churn_and_large_index_allocates_once() {
    let lua = Lua::new();
    for count in [0, 1, 8, 31, 32, 128] {
        let (actors, snapshots) = fixture(&lua, count, 0);
        let work = || {
            let lookup = ActorAuxSnapshots::new(&snapshots, actors.len());
            for actor in &actors {
                black_box(captured_actor_aux_change(actor, &lookup));
            }
        };
        work();
        if count < 32 {
            crate::perf::assert_no_churn(work);
        } else {
            crate::perf::assert_churn_budget(1, count * 64, work);
        }
    }
}

struct CaptureFixture {
    lua: Lua,
    overlays: Vec<(usize, Table)>,
    tracked: Vec<SongLuaTrackedActor>,
    function: Function,
}

impl CaptureFixture {
    fn new(count: usize, fail: bool) -> Self {
        let lua = Lua::new();
        let runtime =
            crate::create_song_runtime_table(&lua, &SongLuaCompileContext::new("", "")).unwrap();
        lua.globals()
            .raw_set(crate::SONG_LUA_RUNTIME_KEY, runtime)
            .unwrap();
        set_compile_song_runtime_values(&lua, 7.0, 3.0).unwrap();
        let (actors, _) = fixture(&lua, count, 0);
        let overlays = actors
            .iter()
            .take(count / 2)
            .enumerate()
            .map(|(i, a)| (i, a.clone()))
            .collect();
        let tracked = actors
            .iter()
            .skip(count / 2)
            .map(|a| SongLuaTrackedActor {
                table: a.clone(),
                actor: Default::default(),
                target: SongLuaTrackedActorTarget::SongForeground,
            })
            .collect();
        let function = lua
            .create_function(move |lua, ()| {
                for (i, actor) in actors.iter().enumerate() {
                    prepare_capture_scope_actor(lua, actor)?;
                    actor.raw_set("__songlua_aux", (i + 10) as f32)?;
                }
                if fail {
                    Err(mlua::Error::runtime("capture sentinel"))
                } else {
                    Ok(())
                }
            })
            .unwrap();
        Self {
            lua,
            overlays,
            tracked,
            function,
        }
    }

    fn capture(&self, old: bool) -> Result<SongLuaFunctionActionCapture, String> {
        if old {
            baseline::capture_function_action_blocks_inner(
                &self.lua,
                &self.overlays,
                &self.tracked,
                &self.function,
                9.0,
                false,
            )
        } else {
            capture_function_action_blocks_inner(
                &self.lua,
                &self.overlays,
                &self.tracked,
                &self.function,
                9.0,
                false,
            )
        }
    }
}

#[test]
fn lua_command_aux_lookup_preserves_overlay_tracked_capture_and_error_restoration() {
    for count in [0, 1, 8, 31, 32, 64] {
        for fail in [false, true] {
            let fixture = CaptureFixture::new(count, fail);
            let old = fixture.capture(true);
            let new = fixture.capture(false);
            assert_eq!(old, new);
            if fail {
                assert!(new.unwrap_err().contains("capture sentinel"));
            } else {
                let capture = new.unwrap();
                assert_eq!(capture.overlay_aux.len(), count / 2);
                assert_eq!(capture.tracked_aux.len(), count - count / 2);
            }
            assert_eq!(
                compile_song_runtime_values(&fixture.lua).unwrap(),
                (7.0, 3.0)
            );
            for (i, actor) in fixture
                .overlays
                .iter()
                .map(|(_, a)| a)
                .chain(fixture.tracked.iter().map(|a| &a.table))
                .enumerate()
            {
                assert_eq!(actor.get::<f32>("__songlua_aux").unwrap(), (i + 1) as f32);
            }
        }
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_command_bench_aux_lookup() {
    for (count, queries, fields) in [
        (0, 0, 0),
        (1, 1, 0),
        (8, 8, 0),
        (31, 31, 0),
        (32, 32, 0),
        (33, 33, 0),
        (64, 64, 0),
        (256, 256, 0),
        (512, 512, 0),
        (512, 1, 0),
        (512, 8, 0),
        (256, 256, 16),
    ] {
        let lua = Lua::new();
        let (actors, snapshots) = fixture(&lua, count, fields);
        let queries: Vec<_> = actors.iter().rev().cycle().take(queries).cloned().collect();
        assert_eq!(
            batch(&queries, &snapshots, true),
            batch(&queries, &snapshots, false)
        );
        lua.gc_collect().unwrap();
        lua.gc_stop();
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "aux_{count}_{}_fields_{fields}/{}",
                    queries.len(),
                    if old { "old" } else { "new" }
                ),
                if count < 64 { 256 } else { 32 },
                queries.len().max(1),
                || {
                    drop(black_box(batch(
                        black_box(&queries),
                        black_box(&snapshots),
                        black_box(old),
                    )));
                },
            );
        }
    }
    for count in [8, 128] {
        let fixture = CaptureFixture::new(count, false);
        assert_eq!(
            fixture.capture(true).unwrap(),
            fixture.capture(false).unwrap()
        );
        fixture.lua.gc_collect().unwrap();
        fixture.lua.gc_stop();
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!("aux_capture_{count}/{}", if old { "old" } else { "new" }),
                8,
                count,
                || {
                    drop(black_box(fixture.capture(black_box(old)).unwrap()));
                },
            );
        }
    }
}
