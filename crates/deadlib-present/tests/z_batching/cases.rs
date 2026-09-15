use super::*;
use crate::frame_compare::compare_render_frames_semantic;
use crate::perf;
use std::hint::black_box;

mod baseline;

struct Fixture {
    builder: FrameBuilder,
    frame: RenderFrame,
    geometries: HashMap<TMeshGeomKey, u32, rustc_hash::FxBuildHasher>,
    // Actors retain shared source geometry while their frame is finalized.
    _mesh_source: Arc<[renderer::MeshVertex]>,
}

fn fixture(kind: &str, count: usize) -> Fixture {
    let triangle: Arc<[renderer::MeshVertex]> = Arc::from([
        renderer::MeshVertex {
            pos: [-4.0, -4.0],
            color: [1.0, 0.2, 0.4, 0.5],
        },
        renderer::MeshVertex {
            pos: [4.0, -4.0],
            color: [0.2, 1.0, 0.4, 0.5],
        },
        renderer::MeshVertex {
            pos: [0.0, 4.0],
            color: [0.2, 0.4, 1.0, 0.5],
        },
    ]);
    let textured: Arc<[renderer::TexturedMeshVertex]> = Arc::from(
        triangle
            .iter()
            .map(|v| renderer::TexturedMeshVertex {
                pos: [v.pos[0], v.pos[1], 0.0],
                color: v.color,
                uv: [0.5; 2],
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    );
    let mut result = Fixture {
        _mesh_source: triangle.clone(),
        builder: FrameBuilder::default(),
        frame: RenderFrame {
            clear_color: [0.1, 0.2, 0.3, 1.0],
            render_targets: Vec::new(),
            cameras: vec![Matrix4::IDENTITY, Matrix4::from_rotation_z(0.1)],
            sprite_instances: Vec::with_capacity(count),
            mesh_vertices: Vec::with_capacity(count * 3),
            tmesh_instances: Vec::with_capacity(count),
            tmesh_geometries: Vec::with_capacity(count),
            ops: Vec::with_capacity(count),
        },
        geometries: HashMap::with_capacity_and_hasher(FRAME_TMESH_GEOMS_MAX, Default::default()),
    };
    for i in 0..count {
        let z = if kind.ends_with("same_z") {
            0
        } else if kind.ends_with("16layers") {
            (i / (count / 16).max(1)) as i16
        } else {
            (i / 4) as i16
        };
        let texture = if kind.ends_with("textures") {
            7 + (i % 2) as u64
        } else {
            7
        };
        let camera = u8::from(kind.ends_with("cameras") && i % 2 == 1);
        let blend = if kind == "mixed" && i % 5 == 0 {
            BlendMode::Add
        } else {
            BlendMode::Alpha
        };
        let draw_kind = if kind.starts_with("sprites") {
            0
        } else if kind.starts_with("meshes") {
            1
        } else if kind.starts_with("tmesh") {
            2
        } else {
            (i / 16) % 3
        };
        let transform =
            Matrix4::from_translation(Vector3::new((i % 8) as f32, 0.0, (i % 3) as f32 * 0.1));
        match draw_kind {
            0 => {
                let index = result.frame.sprite_instances.len() as u32;
                result
                    .frame
                    .sprite_instances
                    .push(renderer::SpriteInstanceRaw {
                        center: [i as f32, 0.0, 0.0, 1.0],
                        size: [8.0; 2],
                        rot_sin_cos: [0.0, 1.0],
                        tint: [0.7, 0.2, 0.4, 0.5],
                        uv_scale: [1.0; 2],
                        uv_offset: [0.0; 2],
                        local_offset: [0.0; 2],
                        local_offset_rot_sin_cos: [0.0, 1.0],
                        edge_fade: [0.0; 4],
                        texture_mask: (i % 2) as f32,
                    });
                let payload = if kind == "sprites_fragmented" {
                    index ^ 1
                } else {
                    index
                };
                result
                    .builder
                    .push_sprite(texture, i as u32, z, blend, camera, payload);
            }
            1 => result.builder.push_mesh(
                0,
                i as u32,
                z,
                blend,
                camera,
                MeshPayload {
                    transform,
                    tint: [1.0, 0.8, 0.6, 0.5],
                    vertices: MeshVertices::Shared(triangle.clone()),
                },
            ),
            _ => result.builder.push_textured_mesh(
                texture,
                i as u32,
                z,
                blend,
                camera,
                TexturedMeshPayload {
                    instance: renderer::TexturedMeshInstanceRaw::new(
                        transform,
                        [1.0, 0.8, 0.6, 0.5],
                        [1.0; 2],
                        [0.0; 2],
                        [0.0; 2],
                        i % 2 == 1,
                    ),
                    vertices: renderer::TexturedMeshVertices::Shared(if kind == "tmesh_geometry" {
                        Arc::from(textured.to_vec())
                    } else {
                        textured.clone()
                    }),
                    geom_cache_key: 0,
                    depth_test: kind == "tmesh_depth"
                        || (kind == "tmesh_depth_boundary" && i % 2 == 1)
                        || (kind == "mixed" && i % 7 == 0),
                },
            ),
        }
    }
    result
}

fn finish<const TRACK: bool>(f: &mut Fixture, old: bool) -> SpriteGatherStats {
    let finish = if old {
        baseline::finish_frame::<TRACK>
    } else {
        finish_frame::<TRACK>
    };
    finish(
        &mut f.builder,
        &mut f.frame.mesh_vertices,
        &mut f.frame.tmesh_instances,
        &mut f.frame.tmesh_geometries,
        &mut f.frame.ops,
        &mut f.geometries,
    )
}

const CASES: &[&str] = &[
    "sprites_layers",
    "sprites_16layers",
    "sprites_same_z",
    "sprites_textures",
    "sprites_fragmented",
    "meshes_layers",
    "meshes_same_z",
    "meshes_cameras",
    "tmesh_layers",
    "tmesh_16layers",
    "tmesh_depth",
    "tmesh_cameras",
    "mixed",
];

#[test]
fn batching_preserves_expanded_draws_across_z_and_state_boundaries() {
    compare_cases::<false>();
    compare_cases::<true>();
}

fn compare_cases<const TRACK: bool>() {
    for kind in CASES.iter().chain(&[
        "sprites_cameras",
        "tmesh_textures",
        "tmesh_geometry",
        "tmesh_depth_boundary",
    ]) {
        let mut old = fixture(kind, 128);
        let mut new = fixture(kind, 128);
        finish::<TRACK>(&mut old, true);
        finish::<TRACK>(&mut new, false);
        compare_render_frames_semantic(&old.frame, &new.frame)
            .unwrap_or_else(|e| panic!("{kind}: {e}"));
        if matches!(*kind, "sprites_layers" | "tmesh_layers" | "tmesh_depth") {
            assert_eq!(old.frame.ops.len(), 32, "{kind}");
            assert_eq!(new.frame.ops.len(), 1, "{kind}");
        } else if kind.ends_with("16layers") {
            assert_eq!(old.frame.ops.len(), 16, "{kind}");
            assert_eq!(new.frame.ops.len(), 1, "{kind}");
        } else if *kind != "mixed" {
            assert_eq!(old.frame.ops.len(), new.frame.ops.len(), "{kind}");
        }
    }
}

#[test]
fn mesh_z_boundaries_do_not_join_incomplete_triangles() {
    for vertex_counts in [[1, 2], [4, 4], [4, 2], [3, 4]] {
        let complete = |old| {
            let mut f = fixture("meshes_layers", 2);
            for (i, count) in vertex_counts.into_iter().enumerate() {
                f.builder.items[i].z = i as i16;
                let mesh = f.builder.meshes[i].as_mut().unwrap();
                mesh.vertices = MeshVertices::Shared(Arc::from(
                    mesh.vertices
                        .as_ref()
                        .iter()
                        .cycle()
                        .take(count)
                        .copied()
                        .collect::<Vec<_>>(),
                ));
            }
            finish::<false>(&mut f, old);
            f.frame
        };
        compare_render_frames_semantic(&complete(true), &complete(false))
            .unwrap_or_else(|error| panic!("{vertex_counts:?}: {error}"));
    }
}

#[test]
fn sorting_and_optional_sprite_gather_preserve_painter_output() {
    for kind in ["sprites_layers", "sprites_fragmented", "mixed"] {
        let complete = |old| {
            let mut f = fixture(kind, 128);
            for (i, item) in f.builder.items.iter_mut().enumerate() {
                item.z = i as i16;
            }
            f.builder.items.reverse();
            sort_composed_draw_items(&mut f.builder.items, &mut ComposeScratch::default());
            let plan = finish::<true>(&mut f, old);
            let gather = sprite_gather_is_profitable(plan);
            if gather {
                gather_finalized_sprites(
                    &mut f.frame.ops,
                    &mut f.frame.sprite_instances,
                    &mut Vec::new(),
                    plan,
                );
            }
            (f.frame, gather)
        };
        let (old, old_gather) = complete(true);
        let (new, new_gather) = complete(false);
        compare_render_frames_semantic(&old, &new).unwrap();
        if kind == "sprites_layers" {
            assert!(old_gather);
            assert!(!new_gather);
        } else if kind == "sprites_fragmented" {
            assert!(old_gather && new_gather);
        }
    }
}

#[test]
fn batch_comparison_detects_changes_to_order_geometry_camera_and_depth() {
    for kind in ["sprites_layers", "meshes_layers", "tmesh_layers"] {
        let mut f = fixture(kind, 8);
        finish::<false>(&mut f, false);
        let expected = f.frame.clone();
        match kind {
            "sprites_layers" => f.frame.sprite_instances.swap(0, 1),
            "meshes_layers" => f.frame.mesh_vertices.swap(0, 3),
            _ => f.frame.tmesh_instances.swap(0, 1),
        }
        assert!(
            compare_render_frames_semantic(&expected, &f.frame).is_err(),
            "{kind}"
        );
        let mut changed = expected.clone();
        match &mut changed.ops[0] {
            renderer::DrawOp::Sprite(run) => run.camera = 1,
            renderer::DrawOp::Mesh(run) => run.blend = BlendMode::Add,
            renderer::DrawOp::TexturedMesh(run) => run.depth_test = !run.depth_test,
        }
        assert!(
            compare_render_frames_semantic(&expected, &changed).is_err(),
            "{kind}"
        );
    }
}

#[test]
#[ignore = "manual old/new release cycle, throughput, and allocation benchmark"]
fn z_batching_bench() {
    benchmark::<false>();
    benchmark::<true>();
}

fn benchmark<const TRACK: bool>() {
    let mode = if TRACK { "tracked" } else { "untracked" };
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for kind in CASES {
        let mut old = fixture(kind, 2048);
        let mut new = fixture(kind, 2048);
        finish::<TRACK>(&mut old, true);
        finish::<TRACK>(&mut new, false);
        compare_render_frames_semantic(&old.frame, &new.frame).unwrap();
        eprintln!(
            "{mode}/{kind}: draw runs {} -> {}",
            old.frame.ops.len(),
            new.frame.ops.len()
        );
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled_with_setup(
                &format!("{mode}/{kind}/{}", if old { "old" } else { "new" }),
                256,
                2048,
                || fixture(kind, 2048),
                |fixture| {
                    black_box(finish::<TRACK>(fixture, old));
                },
            );
        }
    }
}
