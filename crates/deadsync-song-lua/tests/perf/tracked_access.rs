use super::*;
use std::cell::RefCell;
use std::hint::black_box;
use std::rc::Rc;

#[path = "tracked_access_baseline.rs"]
mod baseline;

fn actors(lua: &Lua, count: usize, populated: usize) -> Vec<SongLuaTrackedActor> {
    (0..count)
        .map(|index| {
            let table = lua.create_table().unwrap();
            reset_actor_capture(lua, &table).unwrap();
            if populated > 0 && (populated == 2 || index % 32 == 0) {
                let block = lua.create_table().unwrap();
                block.raw_set("start", 0.25).unwrap();
                block.raw_set("duration", 0.5).unwrap();
                block.raw_set("x", index as f32).unwrap();
                block.raw_set("easing", "linear").unwrap();
                table
                    .raw_get::<Table>("__songlua_capture_blocks")
                    .unwrap()
                    .raw_set(1, block)
                    .unwrap();
            }
            SongLuaTrackedActor {
                table,
                actor: Default::default(),
                target: SongLuaTrackedActorTarget::Player(index % 2),
            }
        })
        .collect()
}

fn collect(
    actors: &[SongLuaTrackedActor],
    indices: &[usize],
    old: bool,
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    if old {
        baseline::collect_tracked_capture_blocks_for_indices(actors, indices)
    } else {
        collect_tracked_capture_blocks_for_indices(actors, indices)
    }
}

fn reset(lua: &Lua, actors: &[SongLuaTrackedActor], old: bool) -> Result<(), String> {
    if old {
        baseline::reset_tracked_capture_tables(lua, actors)
    } else {
        reset_tracked_capture_tables(lua, actors)
    }
}

#[test]
fn lua_access_tracked_collection_preserves_order_duplicates_and_flushes() {
    for count in [0, 1, 4, 64] {
        for populated in 0..=2 {
            let mut results = Vec::new();
            for old in [true, false] {
                let lua = Lua::new();
                let actors = actors(&lua, count, populated);
                if let Some(actor) = actors.first() {
                    let block = lua.create_table().unwrap();
                    block.raw_set("start", 0.75).unwrap();
                    block.raw_set("duration", 1.5).unwrap();
                    block.raw_set("__songlua_has_changes", true).unwrap();
                    block.raw_set("y", -32.0).unwrap();
                    actor
                        .table
                        .raw_set("__songlua_capture_block", block)
                        .unwrap();
                }
                let indices: Vec<_> = (0..count)
                    .rev()
                    .chain([usize::MAX, count, 0, 0])
                    .chain(0..count)
                    .collect();
                let out = collect(&actors, &indices, old).unwrap();
                let indexed: Vec<_> = indices
                    .iter()
                    .filter_map(|&index| {
                        actors.get(index).map(|actor| (index, actor.table.clone()))
                    })
                    .collect();
                let direct = if old {
                    baseline::collect_indexed_actor_capture_blocks(&indexed)
                } else {
                    collect_indexed_actor_capture_blocks(&indexed)
                }
                .unwrap();
                assert_eq!(direct, out);
                if count > 0 {
                    assert_eq!(out.iter().filter(|(index, _)| *index == 0).count(), 4);
                }
                let cursors: Vec<_> = actors
                    .iter()
                    .map(|a| {
                        (
                            a.table
                                .raw_get::<f32>("__songlua_capture_cursor")
                                .unwrap()
                                .to_bits(),
                            a.table
                                .raw_get::<Table>("__songlua_capture_blocks")
                                .unwrap()
                                .raw_len(),
                        )
                    })
                    .collect();
                results.push((out, cursors));
            }
            assert_eq!(results[0], results[1]);
        }
    }
}

#[test]
fn lua_access_tracked_reset_preserves_aliases_and_stops_after_partial_error() {
    for failure in [None, Some(0), Some(1), Some(2)] {
        let mut results = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            let actors = actors(&lua, 3, 2);
            let aliases: Vec<Table> = actors
                .iter()
                .map(|a| a.table.raw_get("__songlua_capture_blocks").unwrap())
                .collect();
            for actor in &actors {
                actor
                    .table
                    .raw_set("__songlua_capture_cursor", 99.0)
                    .unwrap();
                actor.table.raw_set("keep", 42).unwrap();
            }
            if let Some(index) = failure {
                actors[index]
                    .table
                    .raw_remove("__songlua_capture_blocks")
                    .unwrap();
                let mt=lua.load("return {__newindex=function(t,k,v) if k == '__songlua_capture_blocks' then error('reset failed') end rawset(t,k,v) end}").eval::<Table>().unwrap();
                actors[index].table.set_metatable(Some(mt)).unwrap();
            }
            let result = reset(&lua, &actors, old);
            assert_eq!(result.is_err(), failure.is_some());
            let mut state = Vec::new();
            for (index, actor) in actors.iter().enumerate() {
                let blocks = actor
                    .table
                    .raw_get::<Option<Table>>("__songlua_capture_blocks")
                    .unwrap();
                assert_eq!(aliases[index].raw_len(), 1);
                let changed = failure.is_none_or(|failed| index < failed);
                if changed {
                    assert_ne!(
                        blocks.as_ref().unwrap().to_pointer(),
                        aliases[index].to_pointer()
                    );
                }
                assert_eq!(actor.table.raw_get::<i32>("keep").unwrap(), 42);
                state.push((
                    actor
                        .table
                        .raw_get::<f32>("__songlua_capture_cursor")
                        .unwrap()
                        .to_bits(),
                    blocks.map(|b| b.raw_len()),
                ));
            }
            results.push((result, state));
        }
        assert_eq!(results[0], results[1]);
    }
}

#[test]
fn lua_access_tracked_collection_preserves_lookup_and_error_order() {
    let mut results = Vec::new();
    for old in [true, false] {
        let lua = Lua::new();
        let actors = actors(&lua, 3, 0);
        let trace = Rc::new(RefCell::new(Vec::<(usize, String)>::new()));
        for (index, actor) in actors.iter().enumerate() {
            actor.table.raw_remove("__songlua_capture_blocks").unwrap();
            let mt = lua.create_table().unwrap();
            let log = Rc::clone(&trace);
            mt.set(
                "__index",
                lua.create_function(move |_, (_, key): (Table, String)| {
                    log.borrow_mut().push((index, key.clone()));
                    if index == 1 && key == "__songlua_capture_blocks" {
                        return Err(mlua::Error::runtime("collect failed"));
                    }
                    Ok(Value::Nil)
                })
                .unwrap(),
            )
            .unwrap();
            actor.table.set_metatable(Some(mt)).unwrap();
        }
        let result = collect(&actors, &[2, usize::MAX, 0, 2, 1, 0], old);
        assert!(result.as_ref().unwrap_err().contains("collect failed"));
        results.push((result, trace.borrow().clone()));
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn lua_access_empty_tracked_collection_has_no_churn_even_with_unique_handles() {
    let lua = Lua::new();
    let actors = actors(&lua, 64, 0);
    let indices: Vec<_> = (0..64).chain([usize::MAX, 0, 0]).collect();
    lua.gc_stop();
    crate::perf::assert_no_churn(|| {
        black_box(collect(&actors, &indices, false).unwrap());
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_access_bench_tracked() {
    for count in [0, 2, 16, 64] {
        let lua = Lua::new();
        let actors = actors(&lua, count, 0);
        reset(&lua, &actors, true).unwrap();
        let before: Vec<_> = actors
            .iter()
            .map(|actor| {
                (
                    actor
                        .table
                        .raw_get::<f32>("__songlua_capture_cursor")
                        .unwrap()
                        .to_bits(),
                    actor
                        .table
                        .raw_get::<f32>("__songlua_capture_duration")
                        .unwrap()
                        .to_bits(),
                    actor
                        .table
                        .raw_get::<Table>("__songlua_capture_blocks")
                        .unwrap()
                        .raw_len(),
                )
            })
            .collect();
        reset(&lua, &actors, false).unwrap();
        let after: Vec<_> = actors
            .iter()
            .map(|actor| {
                (
                    actor
                        .table
                        .raw_get::<f32>("__songlua_capture_cursor")
                        .unwrap()
                        .to_bits(),
                    actor
                        .table
                        .raw_get::<f32>("__songlua_capture_duration")
                        .unwrap()
                        .to_bits(),
                    actor
                        .table
                        .raw_get::<Table>("__songlua_capture_blocks")
                        .unwrap()
                        .raw_len(),
                )
            })
            .collect();
        assert_eq!(before, after);
        lua.gc_stop();
        for old in order() {
            crate::perf::measure_sampled(
                &format!("tracked_reset_{count}/{}", if old { "old" } else { "new" }),
                64,
                count.max(1),
                || {
                    reset(black_box(&lua), black_box(&actors), old).unwrap();
                    black_box(&actors);
                },
            );
        }
    }
    for (count, populated) in [(0, 0), (2, 0), (16, 0), (64, 0), (64, 1), (16, 2)] {
        let lua = Lua::new();
        let actors = actors(&lua, count, populated);
        let indices: Vec<_> = (0..count).rev().chain([usize::MAX]).collect();
        assert_eq!(
            collect(&actors, &indices, true).unwrap(),
            collect(&actors, &indices, false).unwrap()
        );
        lua.gc_stop();
        for old in order() {
            crate::perf::measure_sampled(
                &format!(
                    "tracked_collect_{count}_density_{populated}/{}",
                    if old { "old" } else { "new" }
                ),
                64,
                count.max(1),
                || {
                    black_box(collect(black_box(&actors), black_box(&indices), old).unwrap());
                },
            );
        }
    }
}

fn order() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    }
}
