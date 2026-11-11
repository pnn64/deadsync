//! Opt-in measurements of mesh expansion and native PlayerOptions operations.
use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaActorMultiVertexPoint, SongLuaCompileContext, SongLuaOverlayMeshVertex, SongRuntime,
    test_support,
};
use std::{hint::black_box, path::Path, time::Instant};

#[path = "../../../tests/support/allocations.rs"]
#[expect(
    dead_code,
    reason = "shared allocation probe also supports no-churn assertions"
)]
mod allocations;
#[path = "../src/actors/mesh_geometry.rs"]
mod geometry;

fn execute(runtime: &mut SongRuntime, text: &str) -> Value {
    runtime
        .vm_mut()
        .execute(&compile_named(text.as_bytes(), b"=performance fixture").unwrap())
        .unwrap()
        .first()
        .copied()
        .unwrap_or(Value::Nil)
}

fn runtime(dense: bool) -> SongRuntime {
    let context = SongLuaCompileContext::new(Path::new("."), "performance fixture");
    let mut runtime = SongRuntime::new(&context).unwrap();
    test_support::expose_catalog(&mut runtime).unwrap();
    execute(
        &mut runtime,
        "options=catalog.player_options[1][4]; state=catalog.player_states[1]",
    );
    if dense {
        let names: Vec<_> = deadsync_song_lua::SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES
            .iter()
            .flat_map(|prefix| (1..=16).map(move |col| format!("'{prefix}{col}'")))
            .collect();
        execute(
            &mut runtime,
            &format!(
                "for _,o in ipairs(catalog.player_options[1]) do for _,name in ipairs{{{}}} do o.__songlua_player_option_state[string.lower(name)]=0.375 end end",
                names.join(",")
            ),
        );
    }
    runtime
}

fn points(count: usize) -> Vec<SongLuaActorMultiVertexPoint> {
    (0..count)
        .map(|i| SongLuaActorMultiVertexPoint {
            pos: [i as f32, -(i as f32)],
            z: i as f32 * 0.25,
            color: [0.25, 0.5, 0.75, 1.0],
            uv: [i as f32 * 0.125, 0.0],
        })
        .collect()
}

fn bits(vertex: &SongLuaOverlayMeshVertex) -> [u32; 9] {
    [
        vertex.pos[0],
        vertex.pos[1],
        vertex.z,
        vertex.color[0],
        vertex.color[1],
        vertex.color[2],
        vertex.color[3],
        vertex.uv[0],
        vertex.uv[1],
    ]
    .map(f32::to_bits)
}

#[test]
fn mesh_expansion_preserves_order_remainders_and_float_bits() {
    let mut seed = 0x1357_2468_u32;
    let mut scalar = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        f32::from_bits(seed)
    };
    let points: Vec<_> = (0..128)
        .map(|_| SongLuaActorMultiVertexPoint {
            pos: [scalar(), scalar()],
            z: scalar(),
            color: [-0.0, f32::from_bits(0xff80_1234), scalar(), scalar()],
            uv: [scalar(), scalar()],
        })
        .collect();
    for count in 0..=points.len() {
        for mode in [
            "DrawMode_Quads",
            "drawmode_quadstrip",
            "DRAWMODE_FAN",
            "DrawMode_Triangles",
            "unknown",
        ] {
            let indices: Vec<_> = if mode.eq_ignore_ascii_case("DrawMode_Quads") {
                (0..count / 4)
                    .flat_map(|group| [0, 1, 2, 2, 3, 0].map(|i| group * 4 + i))
                    .collect()
            } else if mode.eq_ignore_ascii_case("DrawMode_QuadStrip") {
                (0..count.saturating_sub(2) / 2)
                    .flat_map(|group| [0, 1, 3, 3, 2, 0].map(|i| group * 2 + i))
                    .collect()
            } else if mode.eq_ignore_ascii_case("DrawMode_Fan") {
                (1..count.saturating_sub(1))
                    .flat_map(|i| [0, i, i + 1])
                    .collect()
            } else {
                (0..count / 3 * 3).collect()
            };
            let expected: Vec<_> = indices
                .into_iter()
                .map(|i| {
                    let p = points[i];
                    bits(&SongLuaOverlayMeshVertex {
                        pos: p.pos,
                        z: p.z,
                        color: p.color,
                        uv: p.uv,
                    })
                })
                .collect();
            let actual = geometry::actor_multi_vertex_mesh_arc(&points[..count], mode, 1.0);
            assert_eq!(actual.is_none(), expected.is_empty(), "{mode}/{count}");
            assert_eq!(
                actual
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(bits)
                    .collect::<Vec<_>>(),
                expected,
                "{mode}/{count}"
            );
        }
    }
}

// OS accounting counts this thread's executed cycles, excluding descheduling.
#[cfg(windows)]
fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut cycles = 0;
    // SAFETY: the pseudo-handle is valid and the output is writable during the call.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) },
        0
    );
    cycles
}
#[cfg(not(windows))]
fn cycles() -> u64 {
    0
}

fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    for _ in 0..32 {
        work();
    }
    // Count separately so allocator instrumentation is excluded from cycle samples.
    let (_, churn) = allocations::measure(&mut work);
    println!(
        "alloc,{label},{},{},{}",
        churn.allocs, churn.reallocs, churn.allocated_bytes
    );
    for round in 0..10 {
        let start_cycles = cycles();
        let start = Instant::now();
        for _ in 0..iterations {
            work();
        }
        let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
        let cpu = (cycles() - start_cycles) as f64 / iterations as f64;
        println!("sample,{label},{round},{iterations},{cpu:.3},{ns:.3}");
    }
}

#[test]
#[ignore = "CPU/allocation benchmark; run serially in release with --nocapture"]
fn benchmark_hot_paths() {
    println!();
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut std::ffi::c_void;
            fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
        }
        // SAFETY: only changes the current benchmark thread's affinity.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
            0
        );
    }
    for count in [4, 96, 3072] {
        let points = points(count);
        for mode in [
            "DrawMode_Quads",
            "DrawMode_QuadStrip",
            "DrawMode_Fan",
            "DrawMode_Triangles",
        ] {
            measure(
                &format!("mesh/{mode}/{count}"),
                (300_000 / count).max(100),
                || {
                    black_box(geometry::actor_multi_vertex_mesh_arc(
                        black_box(&points),
                        black_box(mode),
                        1.0,
                    ));
                },
            );
        }
    }
    for count in [96, 3072] {
        let mut runtime = runtime(false);
        let definition = execute(&mut runtime, "return Def.ActorMultiVertex{}");
        let scene = runtime.instantiate_actor(definition).unwrap();
        let actor = runtime.actor_scene_root(scene).unwrap();
        runtime.vm_mut().set_global(b"mesh", actor).unwrap();
        execute(
            &mut runtime,
            &format!(
                "local v={{}}; for i=1,{count} do v[i]={{{{i,-i,i/4}},{{0.25,0.5,0.75,1}},{{i/8,0}}}} end; mesh:SetVertices(v); mesh:SetDrawState{{Mode='DrawMode_Quads'}}"
            ),
        );
        assert_eq!(
            runtime
                .actor_multi_vertex_mesh(actor)
                .unwrap()
                .unwrap()
                .vertices
                .len(),
            count / 4 * 6
        );
        measure(
            &format!("mesh/read/{count}"),
            (300_000 / count).max(100),
            || {
                black_box(runtime.actor_multi_vertex_mesh(black_box(actor)).unwrap());
            },
        );
    }
    for dense in [false, true] {
        let mut runtime = runtime(dense);
        execute(
            &mut runtime,
            "function output() return state:GetPlayerOptionsString(3) end",
        );
        let function = runtime.vm_mut().get_global(b"output").unwrap();
        let result = runtime.vm_mut().call(function, &[]).unwrap()[0];
        let Value::String(id) = result else {
            panic!("options string")
        };
        println!(
            "output/{dense}: {}",
            String::from_utf8_lossy(runtime.vm_mut().string_bytes(id).unwrap())
        );
        measure(&format!("options/text/{dense}"), 300, || {
            black_box(runtime.vm_mut().call(function, &[]).unwrap());
        });
        measure(&format!("options/advance/{dense}"), 150, || {
            runtime
                .advance_options(black_box(1.0), Some([1.0 / 60.0; 3]))
                .unwrap();
        });
        measure(&format!("options/restart/{dense}"), 100, || {
            runtime.restart_options().unwrap();
        });
    }
    let mut runtime = runtime(false);
    execute(
        &mut runtime,
        "PS=GAMESTATE:GetPlayerState(PLAYER_1); PO=PS:GetCurrentPlayerOptions(); PO:Tipsy(0.3); PO:TanTipsy(0.2); PO:Tiny2(0.25); PO:Stealth2(0.25); function arrow_x() return ArrowEffects.GetXPos(PS,2,64) end; function arrow_y() return ArrowEffects.GetYPos(PS,2,64) end; function arrow_zoom() return ArrowEffects.GetZoom(PS,64,2) end; function arrow_alpha() return ArrowEffects.GetAlpha(PS,2,64,nil,nil,1000,0.3) end; function arrow_rotation() return ArrowEffects.GetRotationZ(PS,0,false,2) end",
    );
    for (label, function_name) in [
        ("arrows/xpos", b"arrow_x".as_slice()),
        ("arrows/ypos", b"arrow_y".as_slice()),
        ("arrows/zoom", b"arrow_zoom".as_slice()),
        ("arrows/alpha", b"arrow_alpha".as_slice()),
        ("arrows/rotation", b"arrow_rotation".as_slice()),
    ] {
        let function = runtime.vm_mut().get_global(function_name).unwrap();
        measure(label, 500, || {
            black_box(runtime.vm_mut().call(function, &[]).unwrap());
        });
    }
    measure("arrows/update/tipsy", 300, || {
        runtime.update_arrow_effects().unwrap();
    });
}
