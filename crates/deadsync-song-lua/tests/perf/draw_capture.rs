use super::*;

#[path = "../../../../tests/support/perf.rs"]
#[expect(
    dead_code,
    reason = "this benchmark uses only scoped allocation measurement"
)]
pub(crate) mod allocations;
#[path = "draw_capture/baseline.rs"]
#[expect(
    dead_code,
    reason = "frozen module retains unmodified production entry points"
)]
mod baseline;

// The baseline is frozen from 8c2c68c77. It owns independent capture storage
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

fn actor_with_properties(lua: &Lua, kind: &str, index: usize, properties: bool) -> Table {
    let actor = actor(lua, kind, index);
    if properties {
        for key in [
            "__songlua_state_diffuse",
            "__songlua_state_glow",
            "__songlua_state_shadow_color",
            "__songlua_state_effect_color1",
            "__songlua_state_effect_color2",
            "__songlua_state_custom_texture_rect",
            "__songlua_state_stretch_rect",
        ] {
            actor
                .raw_set(
                    key,
                    lua.create_sequence_from([0.25_f32, 0.5, 0.75, 1.0])
                        .unwrap(),
                )
                .unwrap();
        }
        for key in [
            "__songlua_state_shadow_len",
            "__songlua_state_vanishpoint",
            "__songlua_state_texcoord_offset",
            "__songlua_state_texcoord_velocity",
            "__songlua_state_size",
        ] {
            actor
                .raw_set(key, lua.create_sequence_from([1.25_f32, 2.5]).unwrap())
                .unwrap();
        }
        for key in [
            "__songlua_state_base_rotation",
            "__songlua_state_effect_magnitude",
        ] {
            actor
                .raw_set(key, lua.create_sequence_from([0.5_f32, 1.0, 1.5]).unwrap())
                .unwrap();
        }
        actor
            .raw_set(
                "__songlua_state_effect_timing",
                lua.create_sequence_from([0.1_f32, 0.2, 0.3, 0.4, 0.5])
                    .unwrap(),
            )
            .unwrap();
        let rows = (0..4).map(|_| {
            lua.create_sequence_from([0.25_f32, 0.5, 0.75, 1.0])
                .unwrap()
        });
        actor
            .raw_set(
                "__songlua_state_vertex_colors",
                lua.create_sequence_from(rows).unwrap(),
            )
            .unwrap();
    }
    actor
}

fn fixture(
    current: bool,
    owners: usize,
    leaves: usize,
    wrappers: usize,
) -> (Lua, Table, FxHashMap<usize, DrawSource>) {
    fixture_with_properties(current, owners, leaves, wrappers, false)
}

fn fixture_with_properties(
    current: bool,
    owners: usize,
    leaves: usize,
    wrappers: usize,
    properties: bool,
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
    let root = actor_with_properties(&lua, "ActorFrame", 0, properties);
    let screen = actor_with_properties(&lua, "ActorFrame", 1, properties);
    screen.raw_set("__songlua_state_fov", 45.0).unwrap();
    lua.globals()
        .raw_set("__songlua_top_screen", screen)
        .unwrap();
    let mut sources = FxHashMap::default();
    for owner in 0..owners {
        let frame = actor_with_properties(&lua, "ActorFrame", owner + 2, properties);
        sources.insert(frame.to_pointer() as usize, DrawSource::Overlay(owner));
        let mut children = Vec::new();
        for leaf in 0..leaves {
            let index = owners + owner * leaves + leaf;
            let child = actor_with_properties(
                &lua,
                if leaf % 3 == 0 { "ActorProxy" } else { "Quad" },
                index,
                properties,
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
                    .raw_set(
                        wrapper + 1,
                        actor_with_properties(&lua, "WrapperState", index + wrapper, properties),
                    )
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
fn populated_vector_and_color_frames_match_frozen_baseline() {
    let frames = [false, true].map(|current| {
        let (lua, root, sources) = fixture_with_properties(current, 3, 12, 4, true);
        capture(current, &lua, &root, 3);
        finish(current, &lua, &sources).unwrap()
    });
    assert_eq!(frames[0], frames[1]);
    assert!(frames[1].iter().flat_map(|(_,f)| &f.ops).any(|op| matches!(op,
        DrawOp::Draw {state, ..} if state.vertex_colors.is_some() && state.effect_timing.is_some())));
}

#[test]
fn sprite_predicate_preserves_all_value_types_and_binary_strings() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    for code in [
        "nil",
        "false",
        "123",
        "1.25",
        "{}",
        "function() end",
        "coroutine.create(function() end)",
        "newproxy(true)",
        "string.char(255)",
        "'sPrItE'",
        "'Sprite\\0'",
        "string.rep('x',200)",
    ] {
        let value = lua
            .load(format!("return {code}"))
            .eval::<mlua::Value>()
            .unwrap();
        actor
            .raw_set("__songlua_actor_type", value.clone())
            .unwrap();
        let before = baseline::sprite_for_test(&actor).unwrap();
        assert_eq!(
            actor.raw_get::<IsSprite>("__songlua_actor_type").unwrap().0,
            before,
            "{code}"
        );
        assert_eq!(
            <IsSprite as mlua::FromLua>::from_lua(value, &lua)
                .unwrap()
                .0,
            before,
            "{code}"
        );
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
pub(crate) fn cycles() -> u64 {
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
pub(crate) fn pin_cpu2() -> impl Drop {
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
    Affinity(previous)
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_draw_capture() {
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, owners, leaves, wrappers, frames, properties) in [
        ("capture_shallow", 1, 4, 0, 32, false),
        ("capture_wrapped", 3, 12, 4, 12, false),
        ("capture_dense", 1, 128, 0, 8, false),
        ("capture_properties", 3, 12, 4, 12, true),
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let (lua, root, sources) =
                    fixture_with_properties(current, owners, leaves, wrappers, properties);
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
                        "frames":frames,"units":frames,"allocations":churn.allocs,"reallocations":churn.reallocs,
                        "allocated_bytes":churn.allocated_bytes,"frees":churn.frees,
                        "frames_per_second": frames as f64 * 1e9 / nanos as f64}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,
                "variant":if current {"after"} else {"before"},"samples":samples,
                "unit":"scene_frames", "baseline_commit":"8c2c68c77"})
            );
        }
    }
    for name in [
        "sprite_predicate",
        "vector_2",
        "vector_3",
        "vector_4",
        "vector_5",
        "vertex_colors",
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let lua = Lua::new();
            let vector = lua
                .create_sequence_from([1.25_f32, 2.5, 3.75, 4.125, 5.75])
                .unwrap();
            let matrix = lua
                .create_sequence_from([
                    vector.clone(),
                    vector.clone(),
                    vector.clone(),
                    vector.clone(),
                ])
                .unwrap();
            let actor = actor(&lua, "Quad", 0);
            let count = 20_000;
            let mut samples = Vec::new();
            for sample in 0..10 {
                let started = Instant::now();
                let start_cycles = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..count {
                        let vector = black_box(&vector);
                        match name {
                            "sprite_predicate" => {
                                black_box(if current {
                                    actor.raw_get::<IsSprite>("__songlua_actor_type").unwrap().0
                                } else {
                                    baseline::sprite_for_test(&actor).unwrap()
                                });
                            }
                            "vector_2" => {
                                black_box(if current {
                                    crate::table_vec2(vector)
                                } else {
                                    crate::lua_util::draw_state_baseline::table_vec2(vector)
                                });
                            }
                            "vector_3" => {
                                black_box(if current {
                                    crate::table_vec3(vector)
                                } else {
                                    crate::lua_util::draw_state_baseline::table_vec3(vector)
                                });
                            }
                            "vector_4" => {
                                black_box(if current {
                                    crate::table_vec4(vector)
                                } else {
                                    crate::lua_util::draw_state_baseline::table_vec4(vector)
                                });
                            }
                            "vector_5" => {
                                black_box(if current {
                                    crate::table_vec5(vector)
                                } else {
                                    crate::lua_util::draw_state_baseline::table_vec5(vector)
                                });
                            }
                            _ => {
                                black_box(if current {
                                    crate::table_vertex_colors(black_box(&matrix))
                                } else {
                                    crate::lua_util::draw_state_baseline::table_vertex_colors(
                                        black_box(&matrix),
                                    )
                                });
                            }
                        }
                    }
                });
                let used_cycles = cycles() - start_cycles;
                let nanos = started.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used_cycles,"units":count,
                    "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,
                    "frees":churn.frees,"units_per_second":count as f64*1e9/nanos as f64}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,
                "variant":if current {"after"} else {"before"},"samples":samples,
                "unit":"reads", "baseline_commit":"8c2c68c77"})
            );
        }
    }
}
