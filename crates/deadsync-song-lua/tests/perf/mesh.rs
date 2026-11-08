use super::*;

#[path = "mesh_before.rs"]
mod before;

fn points(count: usize) -> Arc<[SongLuaActorMultiVertexPoint]> {
    (0..count)
        .map(|i| SongLuaActorMultiVertexPoint {
            pos: [i as f32 * 1.25, (i % 7) as f32 - 3.0],
            z: if i % 5 == 0 { -0.0 } else { i as f32 * 0.125 },
            color: [0.25, 0.5, 0.75, 1.0],
            uv: [i as f32 * 0.01, -0.0],
        })
        .collect()
}

fn actor(lua: &Lua, count: usize, mode: Option<&str>, raw: bool) -> Table {
    let actor = lua.create_table().unwrap();
    let data = points(count);
    if raw {
        let table = lua.create_table().unwrap();
        for (i, p) in data.iter().enumerate() {
            let row = lua.create_table().unwrap();
            row.raw_set(
                1,
                lua.create_sequence_from([p.pos[0], p.pos[1], p.z]).unwrap(),
            )
            .unwrap();
            row.raw_set(2, lua.create_sequence_from(p.color).unwrap())
                .unwrap();
            row.raw_set(3, lua.create_sequence_from(p.uv).unwrap())
                .unwrap();
            table.raw_set(i + 1, row).unwrap();
        }
        actor.raw_set("__songlua_vertices", table).unwrap();
    } else {
        actor
            .raw_set("__songlua_vertices", lua.create_any_userdata(data).unwrap())
            .unwrap();
    }
    actor.raw_set("__songlua_draw_state_mode", mode).unwrap();
    actor.raw_set("__songlua_line_width", 1.25).unwrap();
    actor
}

fn bits(
    result: Result<Option<Arc<[SongLuaOverlayMeshVertex]>>, String>,
) -> Result<Option<Vec<[u32; 9]>>, String> {
    result.map(|mesh| {
        mesh.map(|mesh| {
            mesh.iter()
                .map(|v| {
                    [
                        v.pos[0], v.pos[1], v.z, v.color[0], v.color[1], v.color[2], v.color[3],
                        v.uv[0], v.uv[1],
                    ]
                    .map(f32::to_bits)
                })
                .collect()
        })
    })
}

#[test]
fn mesh_topologies_ranges_and_payload_bits_match_frozen_readers() {
    let lua = Lua::new();
    for count in (0..17).chain([257]) {
        for mode in [
            None,
            Some("DrawMode_Triangles"),
            Some("dRaWmOdE_QuAdS"),
            Some("DrawMode_QuadStrip"),
            Some("DrawMode_Fan"),
            Some("DrawMode_LineStrip"),
            Some("unknown"),
        ] {
            for raw in [false, true] {
                let actor = actor(&lua, count, mode, raw);
                for (first, num) in [(1, -1), (2, 7), (0, 0), (999, 3), (-8, 4)] {
                    let range = lua.create_table().unwrap();
                    range.raw_set("First", first).unwrap();
                    range.raw_set("Num", num).unwrap();
                    actor.raw_set("__songlua_draw_state", range).unwrap();
                    assert_eq!(
                        bits(before::read_actor_multi_vertex_mesh(&actor)),
                        bits(read_actor_multi_vertex_mesh(&actor)),
                        "count={count}, mode={mode:?}, range={first}/{num}, raw={raw}"
                    );
                }
            }
        }
    }
    let data: Arc<[_]> = (0..9)
        .map(|i| SongLuaActorMultiVertexPoint {
            pos: [
                if i % 3 == 0 {
                    f32::from_bits(0x7fc01234)
                } else {
                    -0.0
                },
                f32::INFINITY,
            ],
            z: f32::NEG_INFINITY,
            color: [-0.0, f32::from_bits(0x7fc05678), 1.0, 0.5],
            uv: [f32::NAN, -0.0],
        })
        .collect();
    let a = lua.create_table().unwrap();
    a.raw_set("__songlua_vertices", lua.create_any_userdata(data).unwrap())
        .unwrap();
    for mode in [
        "DrawMode_Triangles",
        "DrawMode_Quads",
        "DrawMode_QuadStrip",
        "DrawMode_Fan",
    ] {
        a.raw_set("__songlua_draw_state_mode", mode).unwrap();
        assert_eq!(
            bits(before::read_actor_multi_vertex_mesh(&a)),
            bits(read_actor_multi_vertex_mesh(&a))
        );
    }
}

#[test]
fn mesh_getter_order_coercions_and_errors_match_frozen_reader() {
    for mode in [
        "nil",
        "'DrawMode_Quads'",
        "123",
        "true",
        "{}",
        "string.char(255)",
        "string.rep('x',128)",
    ] {
        for width in ["nil", "1.5", "'2'", "false", "0/0"] {
            let outputs = [false, true].map(|current| {
                let lua = Lua::new();
                lua.globals()
                    .set("source", lua.create_any_userdata(points(8)).unwrap())
                    .unwrap();
                let a: Table = lua
                    .load(format!(
                        r#"
                    events={{}}
                    local state=setmetatable({{}},{{__index=function(_,key)
                        events[#events+1]=key
                        if key=='First' then return 2 end
                        if key=='Num' then return 5 end
                    end}})
                    return setmetatable({{}},{{__index=function(_,key)
                        events[#events+1]=key
                        if key=='__songlua_vertices' then return source end
                        if key=='__songlua_draw_state' then return state end
                        if key=='__songlua_draw_state_mode' then return {mode} end
                        if key=='__songlua_line_width' then return {width} end
                    end}})
                "#
                    ))
                    .eval()
                    .unwrap();
                let result = if current {
                    read_actor_multi_vertex_mesh(&a)
                } else {
                    before::read_actor_multi_vertex_mesh(&a)
                };
                let events: Table = lua.globals().get("events").unwrap();
                (
                    bits(result),
                    events
                        .sequence_values::<String>()
                        .collect::<mlua::Result<Vec<_>>>()
                        .unwrap(),
                )
            });
            assert_eq!(outputs[0], outputs[1], "mode={mode}, width={width}");
        }
    }
    for value in [
        "nil",
        "false",
        "{}",
        "{true,false}",
        "{{{1,2},'#ff0000',{'0.5',0/0}}}",
    ] {
        let lua = Lua::new();
        let a = lua.create_table().unwrap();
        a.raw_set(
            "__songlua_vertices",
            lua.load(format!("return {value}")).eval::<Value>().unwrap(),
        )
        .unwrap();
        assert_eq!(
            bits(before::read_actor_multi_vertex_mesh(&a)),
            bits(read_actor_multi_vertex_mesh(&a))
        );
    }
    use crate::draw_capture::perf::allocations;
    let lua = Lua::new();
    let a = actor(&lua, 6, Some("DrawMode_Triangles"), false);
    // Warm mlua's reference storage before checking the geometry/string traffic.
    let _ = before::read_actor_multi_vertex_mesh(&a).unwrap();
    let _ = read_actor_multi_vertex_mesh(&a).unwrap();
    let (_, old) = allocations::measure(|| before::read_actor_multi_vertex_mesh(&a).unwrap());
    let (_, new) = allocations::measure(|| read_actor_multi_vertex_mesh(&a).unwrap());
    assert_eq!(new.allocs + 2, old.allocs);
    assert_eq!((new.reallocs, old.reallocs), (0, 0));
    assert!(new.allocated_bytes < old.allocated_bytes);
}

#[test]
fn aft_predicate_preserves_aliases_coercions_errors_and_getters() {
    for value in [
        "nil",
        "true",
        "false",
        "123",
        "0/0",
        "{}",
        "function() end",
        "'ActorFrameTexture'",
        "'aCtOrFrAmEtExTuRe'",
        "'Sprite'",
        "'Banner'",
        "string.rep('x',128)",
        "string.char(255)",
        "'雪'",
        "'ActorFrameTexture'..string.char(0)",
    ] {
        for expected in [
            "ActorFrameTexture",
            "ACTORFRAMETEXTURE",
            "Sprite",
            "Banner",
            "Actor",
            "雪",
            "",
        ] {
            let outputs=[false,true].map(|current| {
                let lua=Lua::new();
                let a:Table=lua.load(format!("calls=0; return setmetatable({{}},{{__index=function() calls=calls+1; return {value} end}})")).eval().unwrap();
                let result=if current {actor_type_is(&a,expected)} else {before::actor_type_is(&a,expected)}.map_err(|e|e.to_string());
                (result,lua.globals().get::<i32>("calls").unwrap())
            });
            assert_eq!(outputs[0], outputs[1], "value={value}, expected={expected}");
        }
    }
}

#[test]
fn mesh_and_predicate_preserve_stack_coroutines_and_reentrant_getters() {
    for current in [false, true] {
        let lua = Lua::new();
        let a = actor(&lua, 6, Some("DrawMode_Triangles"), false);
        a.raw_set("__songlua_actor_type", "ActorFrameTexture")
            .unwrap();
        lua.exec_raw_lua(|raw| unsafe {
            assert_ne!(ffi::lua_checkstack(raw.state(), 8), 0);
            raw.push(314).unwrap();
            let top = ffi::lua_gettop(raw.state());
            let mesh = if current {
                read_actor_multi_vertex_mesh(&a)
            } else {
                before::read_actor_multi_vertex_mesh(&a)
            }
            .unwrap()
            .unwrap();
            assert_eq!(mesh.len(), 6);
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            let check = if current {
                actor_type_is(&a, "ActorFrameTexture")
            } else {
                before::actor_type_is(&a, "ActorFrameTexture")
            };
            assert!(check.unwrap());
            a.raw_set("__songlua_actor_type", lua.create_string([255]).unwrap())
                .unwrap();
            let check = if current {
                actor_type_is(&a, "ActorFrameTexture")
            } else {
                before::actor_type_is(&a, "ActorFrameTexture")
            };
            assert!(check.is_err());
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            assert_eq!(raw.pop::<i32>().unwrap(), 314);
        });
        a.raw_set("__songlua_actor_type", "ActorFrameTexture")
            .unwrap();
        lua.globals().set("actor", a).unwrap();
        lua.globals()
            .set(
                "classify",
                lua.create_function(move |_, a: Table| {
                    if current {
                        actor_type_is(&a, "ActorFrameTexture")
                    } else {
                        before::actor_type_is(&a, "ActorFrameTexture")
                    }
                })
                .unwrap(),
            )
            .unwrap();
        lua.globals()
            .set(
                "mesh_count",
                lua.create_function(move |_, a: Table| {
                    let result = if current {
                        read_actor_multi_vertex_mesh(&a)
                    } else {
                        before::read_actor_multi_vertex_mesh(&a)
                    };
                    result
                        .map(|m| m.map_or(0, |m| m.len()))
                        .map_err(mlua::Error::external)
                })
                .unwrap(),
            )
            .unwrap();
        let result:(bool,usize,bool)=lua.load("return coroutine.resume(coroutine.create(function() return mesh_count(actor),classify(actor) end))").eval().unwrap();
        assert_eq!(result, (true, 6, true));
        lua.load("local source=actor.__songlua_vertices; actor.__songlua_vertices=nil; setmetatable(actor,{__index=function(_,key) if key=='__songlua_vertices' then assert(classify(actor)); return source end end}); assert(mesh_count(actor)==6)").exec().unwrap();
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_mesh() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, size, mode, raw, reads) in [
        ("mesh_small_triangles", 6, None, false, 512),
        (
            "mesh_triangles",
            4096,
            Some("DrawMode_Triangles"),
            false,
            64,
        ),
        ("mesh_quads", 4096, Some("DrawMode_Quads"), false, 32),
        (
            "mesh_quad_strip",
            4096,
            Some("DrawMode_QuadStrip"),
            false,
            32,
        ),
        ("mesh_fan", 4096, Some("DrawMode_Fan"), false, 32),
        ("mesh_lines", 128, Some("DrawMode_LineStrip"), false, 128),
        (
            "mesh_raw_triangles",
            128,
            Some("DrawMode_Triangles"),
            true,
            32,
        ),
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let lua = Lua::new();
            let a = actor(&lua, size, mode, raw);
            let mut samples = Vec::new();
            for sample in 0..10 {
                let start = Instant::now();
                let start_cycles = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..reads {
                        let result = if current {
                            read_actor_multi_vertex_mesh(black_box(&a))
                        } else {
                            before::read_actor_multi_vertex_mesh(black_box(&a))
                        };
                        black_box(result.unwrap());
                    }
                });
                let used = cycles() - start_cycles;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":reads,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":"mesh_reads","baseline_commit":"b86cc66df"})
            );
        }
    }
    for (name, kind, expected) in [
        (
            "predicate_aft_match",
            "ActorFrameTexture",
            "ActorFrameTexture",
        ),
        ("predicate_aft_miss", "Quad", "ActorFrameTexture"),
        (
            "predicate_aft_long",
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            "ActorFrameTexture",
        ),
        ("predicate_sprite_alias", "Banner", "Sprite"),
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let lua = Lua::new();
            let a = lua.create_table().unwrap();
            a.raw_set("__songlua_actor_type", kind).unwrap();
            let mut samples = Vec::new();
            let reads = 20_000;
            for sample in 0..10 {
                let start = Instant::now();
                let start_cycles = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..reads {
                        black_box(
                            if current {
                                actor_type_is(black_box(&a), expected)
                            } else {
                                before::actor_type_is(black_box(&a), expected)
                            }
                            .unwrap(),
                        );
                    }
                });
                let used = cycles() - start_cycles;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":reads,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":"predicate_reads","baseline_commit":"b86cc66df"})
            );
        }
    }
}
