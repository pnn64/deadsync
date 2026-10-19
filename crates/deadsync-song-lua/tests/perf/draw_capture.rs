use super::*;

#[path = "../../../../tests/support/perf.rs"]
#[expect(
    dead_code,
    reason = "this benchmark uses only scoped allocation measurement"
)]
mod allocations;
#[path = "draw_capture/baseline.rs"]
#[expect(
    dead_code,
    reason = "frozen module retains unmodified production entry points"
)]
mod baseline;

// The baseline is frozen from add695545. It owns independent capture storage
// and runs the same callbacks, then compares every emitted frame and operation.
fn actor(lua: &Lua, kind: &str, index: usize) -> Table {
    let actor = lua.create_table().unwrap();
    actor.raw_set("__songlua_actor_type", kind).unwrap();
    actor.raw_set("Name", format!("actor-{index}")).unwrap();
    actor
        .raw_set("__songlua_state_x", index as f32 * 3.25)
        .unwrap();
    actor
}

fn fixture(
    current: bool,
    owners: usize,
    leaves: usize,
    wrappers: usize,
) -> (Lua, Table, FxHashMap<usize, DrawSource>) {
    let lua = Lua::new();
    let runtime = lua.create_table().unwrap();
    runtime
        .raw_set(crate::SONG_LUA_RUNTIME_SECONDS_KEY, 1.25)
        .unwrap();
    runtime
        .raw_set(crate::SONG_LUA_RUNTIME_RATE_KEY, 1.0)
        .unwrap();
    lua.globals()
        .raw_set(crate::SONG_LUA_RUNTIME_KEY, runtime)
        .unwrap();
    lua.globals().raw_set(DRAW_INSTALLED_KEY, true).unwrap();
    let root = actor(&lua, "ActorFrame", 0);
    let screen = actor(&lua, "ActorFrame", 1);
    screen.raw_set("__songlua_state_fov", 45.0).unwrap();
    lua.globals()
        .raw_set("__songlua_top_screen", screen)
        .unwrap();
    let mut sources = FxHashMap::default();
    for owner in 0..owners {
        let frame = actor(&lua, "ActorFrame", owner + 2);
        sources.insert(frame.to_pointer() as usize, DrawSource::Overlay(owner));
        let mut children = Vec::new();
        for leaf in 0..leaves {
            let index = owners + owner * leaves + leaf;
            let child = actor(
                &lua,
                if leaf % 3 == 0 { "ActorProxy" } else { "Quad" },
                index,
            );
            // Exercise external proxies and both String-compatible name reads.
            if leaf % 3 == 0 {
                child
                    .raw_set("__songlua_top_screen_child_name", "PlayerP1")
                    .unwrap();
            }
            if leaf % 3 == 1 {
                child
                    .raw_set("__songlua_player_child_name", "Combo")
                    .unwrap();
            }
            let states = lua.create_table().unwrap();
            for wrapper in 0..wrappers {
                states
                    .raw_set(wrapper + 1, actor(&lua, "WrapperState", index + wrapper))
                    .unwrap();
            }
            child.raw_set("__songlua_wrappers", states).unwrap();
            sources.insert(child.to_pointer() as usize, DrawSource::Overlay(index));
            children.push(child);
        }
        frame
            .raw_set(
                "__songlua_draw_function",
                lua.create_function(move |lua, _: Table| {
                    for child in &children {
                        if current {
                            draw(lua, child)?;
                        } else {
                            baseline::draw(lua, child)?;
                        }
                    }
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
        root.raw_set(owner + 1, frame).unwrap();
    }
    (lua, root, sources)
}

fn capture(current: bool, lua: &Lua, root: &Table, frames: usize) {
    for _ in 0..frames {
        if current {
            run(lua, root).unwrap();
        } else {
            baseline::run(lua, root).unwrap();
        }
    }
}

fn finish(
    current: bool,
    lua: &Lua,
    sources: &FxHashMap<usize, DrawSource>,
) -> Result<Vec<(usize, DrawFrame)>, String> {
    if current {
        convert(lua.remove_app_data::<Capture>().unwrap(), sources)
    } else {
        baseline::finish_for_test(lua, sources)
    }
}

#[test]
fn captured_frames_match_frozen_baseline() {
    for (owners, leaves, wrappers) in [
        (1, 1, 0),
        (1, 128, 0),
        (3, 12, 4),
        (8, 3, 1),
        (9, 3, 1),
        (32, 2, 1),
    ] {
        let variants = [false, true].map(|current| {
            let (lua, root, sources) = fixture(current, owners, leaves, wrappers);
            capture(current, &lua, &root, 3);
            // Removing a callback must still append its quiet tail. Retain
            // duplicate owner visits and their separate output frame ordering.
            let first = root.raw_get::<Table>(1).unwrap();
            root.raw_set(owners + 1, first.clone()).unwrap();
            capture(current, &lua, &root, 1);
            first
                .raw_set("__songlua_draw_function", mlua::Value::Nil)
                .unwrap();
            capture(current, &lua, &root, 1);
            finish(current, &lua, &sources).unwrap()
        });
        assert_eq!(
            variants[0], variants[1],
            "owners={owners}, leaves={leaves}, wrappers={wrappers}"
        );
        assert!(variants[1].iter().any(|(_, frame)| !frame.ops.is_empty()));
    }
}

#[test]
fn draw_state_preserves_sprite_lookup_mutations_and_errors() {
    for script in [
        "return {__songlua_actor_type='Quad', __songlua_state_x='1.25', __songlua_visible=0}",
        "return {__songlua_actor_type='Model', __songlua_state_x=2.5}",
        r#"local a={__songlua_actor_type='sPrItE'}
            return setmetatable(a, {__index=function(_, key)
                events[#events+1]=key
                if key=='Texture' then a.__songlua_state_x=23; a.__songlua_visible=false; return 'sprite.png' end
            end})"#,
        r#"local a={__songlua_actor_type='Sprite'}
            return setmetatable(a, {__index=function(_, key)
                events[#events+1]=key
                if key=='__songlua_aft_capture_name' then a.__songlua_state_z=31; return 'capture' end
            end})"#,
        r#"return setmetatable({__songlua_actor_type='Sprite'}, {__index=function(_, key)
                events[#events+1]=key; error('authored lookup failed') end})"#,
        r#"return setmetatable({__songlua_actor_type='Sprite', __songlua_state_sprite_texture=true},
            {__index=function(_, key) events[#events+1]=key; error('unexpected lookup') end})"#,
        "return {__songlua_actor_type='Quad', __songlua_state_x={}}",
    ] {
        let outputs = [false, true].map(|current| {
            let lua = Lua::new();
            lua.globals()
                .raw_set("events", lua.create_table().unwrap())
                .unwrap();
            let actor = lua
                .load(script)
                .set_name("state-reader-control")
                .eval::<Table>()
                .unwrap();
            let result = if current {
                state(&lua, &actor)
            } else {
                baseline::state_for_test(&lua, &actor)
            }
            .map_err(|error| error.to_string());
            let events = lua
                .globals()
                .get::<Table>("events")
                .unwrap()
                .sequence_values::<String>()
                .collect::<mlua::Result<Vec<_>>>()
                .unwrap();
            (result, events)
        });
        assert_eq!(outputs[0], outputs[1], "{script}");
    }
}

#[test]
fn draw_type_coercions_and_errors_match_frozen_baseline() {
    for code in [
        "nil",
        "123",
        "true",
        "{}",
        "string.char(255)",
        "string.rep('x', 100)",
        "'aCtOr'",
        "'ActorProxy'",
    ] {
        for field in [
            "__songlua_actor_type",
            "__songlua_top_screen_child_name",
            "__songlua_player_child_name",
        ] {
            let outputs = [false, true].map(|current| {
                let (lua, root, sources) = fixture(current, 1, 1, 0);
                let value = lua
                    .load(format!("return {code}"))
                    .eval::<mlua::Value>()
                    .unwrap();
                // A direct immediate Draw exercises the leaf classification.
                let leaf = actor(&lua, "Quad", 42);
                leaf.raw_set(field, value).unwrap();
                let result = if current {
                    draw(&lua, &leaf)
                } else {
                    baseline::draw(&lua, &leaf)
                };
                let _ = (root, sources);
                result.map_err(|error| error.to_string())
            });
            assert_eq!(outputs[0], outputs[1], "field={field}, value={code}");
        }
    }
}

#[test]
fn source_binding_errors_match_frozen_baseline() {
    for missing in [0, 1] {
        let outputs = [false, true].map(|current| {
            let (lua, root, mut sources) = fixture(current, 1, 1, 0);
            sources.retain(|_, source| *source != DrawSource::Overlay(missing));
            capture(current, &lua, &root, 1);
            finish(current, &lua, &sources).unwrap_err()
        });
        assert_eq!(outputs[0], outputs[1]);
    }
}

#[test]
fn immediate_draws_and_nested_targets_match_frozen_baseline() {
    let outputs = [false, true].map(|current| {
        let (lua, root, mut sources) = fixture(current, 1, 1, 2);
        let first = root.raw_get::<Table>(1).unwrap();
        if current {
            draw(&lua, &first).unwrap();
        } else {
            baseline::draw(&lua, &first).unwrap();
        }
        let target = actor(&lua, "ActorFrameTexture", 100);
        sources.insert(target.to_pointer() as usize, DrawSource::Overlay(100));
        let leaf = actor(&lua, "Quad", 101);
        sources.insert(leaf.to_pointer() as usize, DrawSource::Overlay(101));
        first
            .raw_set(
                "__songlua_draw_function",
                lua.create_function(move |lua, _: Table| {
                    if current {
                        begin(lua, &target, true)?;
                        begin(lua, &target, false)?;
                        draw(lua, &leaf)?;
                        finish_target(lua, &target)?;
                        finish_target(lua, &target)?;
                    } else {
                        baseline::begin(lua, &target, true)?;
                        baseline::begin(lua, &target, false)?;
                        baseline::draw(lua, &leaf)?;
                        baseline::finish(lua, &target)?;
                        baseline::finish(lua, &target)?;
                    }
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
        capture(current, &lua, &root, 1);
        finish(current, &lua, &sources).unwrap()
    });
    assert_eq!(outputs[0], outputs[1]);
    assert!(outputs[1].iter().any(|(_, frame)| frame.immediate));
    assert!(
        outputs[1]
            .iter()
            .flat_map(|(_, frame)| &frame.ops)
            .any(|op| matches!(op, DrawOp::Begin { preserve: true, .. }))
    );
}

fn finish_target(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    super::finish(lua, actor)
}

#[cfg(windows)]
fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut value = 0;
    // SAFETY: the pseudo-handle refers to this live thread and the output
    // pointer is valid for the synchronous Windows call.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) },
        0
    );
    value
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_draw_capture() {
    use std::{hint::black_box, time::Instant};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    struct Affinity(usize);
    impl Drop for Affinity {
        fn drop(&mut self) {
            // SAFETY: restore the affinity returned for this same live thread.
            unsafe {
                SetThreadAffinityMask(GetCurrentThread(), self.0);
            }
        }
    }
    // SAFETY: CPU 2 is present on the measurement host. Windows rejects an
    // unavailable processor rather than accepting an invalid mask.
    let previous = unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) };
    assert_ne!(previous, 0, "benchmark requires logical CPU 2");
    let _affinity = Affinity(previous);
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, owners, leaves, wrappers, frames) in [
        ("capture_shallow", 1, 4, 0, 32),
        ("capture_wrapped", 3, 12, 4, 12),
        ("capture_dense", 1, 128, 0, 8),
        ("convert_one_owner", 1, 4, 0, 64),
        ("convert_dense_owner", 1, 128, 0, 32),
        ("convert_many_owners", 64, 2, 0, 32),
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let (lua, root, sources) = fixture(current, owners, leaves, wrappers);
                if name.starts_with("convert") {
                    capture(current, &lua, &root, frames);
                }
                let started = Instant::now();
                let cycle_start = cycles();
                let (_, churn) = allocations::measure(|| {
                    if name.starts_with("convert") {
                        black_box(finish(current, &lua, &sources).unwrap());
                    } else {
                        capture(current, &lua, &root, frames);
                    }
                });
                let used_cycles = cycles() - cycle_start;
                let nanos = started.elapsed().as_nanos();
                if sample != 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used_cycles,
                        "frames":frames,"allocations":churn.allocs,"reallocations":churn.reallocs,
                        "allocated_bytes":churn.allocated_bytes,"frees":churn.frees,
                        "frames_per_second": frames as f64 * 1e9 / nanos as f64}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,
                "variant":if current {"after"} else {"before"},"samples":samples,
                "baseline_commit":"add695545"})
            );
        }
    }
}
