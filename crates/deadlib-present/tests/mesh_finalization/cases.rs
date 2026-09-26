use super::*;
use crate::perf;
use deadlib_render_core::frame_compare::compare_render_frames;
use std::hint::black_box;

mod baseline;

struct Fixture {
    builder: FrameBuilder,
    frame: RenderFrame,
    geometries: HashMap<TMeshGeomKey, u32, rustc_hash::FxBuildHasher>,
    source: MeshVertices,
}

fn fixture(count: usize, vertices: usize, reusable: bool, group: usize, general: bool) -> Fixture {
    let vertices: Vec<_> = (0..vertices)
        .map(|i| renderer::MeshVertex {
            pos: [(i % 17) as f32 - 8.0, (i % 7) as f32 - 3.0],
            color: [0.25, -0.0, 0.75, 0.5],
        })
        .collect();
    let source = if reusable {
        MeshVertices::Reusable(Arc::new(vertices))
    } else {
        MeshVertices::Shared(Arc::from(vertices))
    };
    let mut builder = FrameBuilder::default();
    for i in 0..count {
        let transform = if general {
            Matrix4::from_rotation_z(i as f32 * 0.03125)
        } else {
            Matrix4::from_cols(Vector4::X, -Vector4::Y, Vector4::Z, Vector4::W)
        };
        builder.push_mesh(
            0,
            i as u32,
            (i / group) as i16,
            if i / group % 2 == 0 {
                BlendMode::Alpha
            } else {
                BlendMode::Add
            },
            (i / group % 3) as u8,
            MeshPayload {
                transform,
                tint: if general {
                    [0.5, 1.0, 0.25, 0.75]
                } else {
                    [1.0; 4]
                },
                vertices: source.clone(),
            },
        );
    }
    Fixture {
        frame: RenderFrame {
            clear_color: [0.0; 4],
            render_targets: Vec::new(),
            cameras: vec![Matrix4::IDENTITY; 3],
            sprite_instances: Vec::new(),
            mesh_vertices: Vec::with_capacity(count * source.len()),
            tmesh_instances: Vec::new(),
            tmesh_geometries: Vec::new(),
            ops: Vec::with_capacity(count),
        },
        builder,
        geometries: HashMap::default(),
        source,
    }
}

fn finish<const TRACK: bool>(f: &mut Fixture, old: bool) -> SpriteGatherStats {
    let finish = if old {
        baseline::finish_frame::<TRACK>
    } else {
        finish_frame::<TRACK>
    };
    black_box(finish)(
        &mut f.builder,
        &mut f.frame.mesh_vertices,
        &mut f.frame.tmesh_instances,
        &mut f.frame.tmesh_geometries,
        &mut f.frame.ops,
        &mut f.geometries,
    )
}

fn strong_count(source: &MeshVertices) -> usize {
    match source {
        MeshVertices::Shared(vertices) => Arc::strong_count(vertices),
        MeshVertices::Reusable(vertices) => Arc::strong_count(vertices),
    }
}

fn compare<const TRACK: bool>() {
    for reusable in [false, true] {
        for general in [false, true] {
            for count in [0, 1, 2, 17, 128] {
                for vertices in [0, 1, 2, 3, 4, 6, 63] {
                    for group in [1, 4, 128] {
                        let mut before = fixture(count, vertices, reusable, group, general);
                        let mut after = fixture(count, vertices, reusable, group, general);
                        let old_stats = finish::<TRACK>(&mut before, true);
                        let new_stats = finish::<TRACK>(&mut after, false);
                        compare_render_frames(&before.frame, &after.frame).unwrap();
                        assert_eq!(
                            (
                                old_stats.sprites,
                                old_stats.runs_before,
                                old_stats.runs_after
                            ),
                            (
                                new_stats.sprites,
                                new_stats.runs_before,
                                new_stats.runs_after
                            )
                        );
                        assert_eq!(strong_count(&before.source), strong_count(&after.source));
                        assert_eq!(
                            before
                                .builder
                                .meshes
                                .iter()
                                .map(Option::is_some)
                                .collect::<Vec<_>>(),
                            after
                                .builder
                                .meshes
                                .iter()
                                .map(Option::is_some)
                                .collect::<Vec<_>>()
                        );
                        before.builder.clear();
                        after.builder.clear();
                        assert_eq!(strong_count(&before.source), 1);
                        assert_eq!(strong_count(&after.source), 1);
                    }
                }
            }
        }
    }
}

#[test]
fn mesh_finalization_preserves_vertices_runs_and_source_lifetimes() {
    compare::<false>();
    compare::<true>();
}

#[test]
fn mesh_finalization_preserves_special_float_bits() {
    for reusable in [false, true] {
        for value in [
            -0.0,
            f32::MIN_POSITIVE,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ] {
            let mut before = fixture(16, 4, reusable, 4, true);
            let mut after = fixture(16, 4, reusable, 4, true);
            for f in [&mut before, &mut after] {
                for (i, payload) in f.builder.meshes.iter_mut().enumerate() {
                    let payload = payload.as_mut().unwrap();
                    payload.transform.w_axis.x = value;
                    payload.tint[i % 4] = value;
                }
            }
            finish::<true>(&mut before, true);
            finish::<true>(&mut after, false);
            compare_render_frames(&before.frame, &after.frame).unwrap();
        }
    }
}

#[test]
#[ignore = "manual release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_mesh_finalization() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, vertices, group, general) in [
        ("empty", 0, 1, false),
        ("triangle-runs", 3, 1, false),
        ("triangle-runs-general", 3, 1, true),
        ("triangle-groups", 3, 4, false),
        ("triangle-one-run", 3, 256, false),
        ("large-runs", 192, 1, true),
    ] {
        for reusable in [false, true] {
            let storage = if reusable { "reusable" } else { "shared" };
            // Both variants reuse the same allocations and source addresses.
            let mut f = fixture(256, vertices, reusable, group, general);
            let items = f.builder.items.clone();
            let template = f.builder.meshes.clone();
            f.builder.clear();
            for old in if reverse {
                [false, true]
            } else {
                [true, false]
            } {
                let variant = if old { "before" } else { "after" };
                perf::measure_sampled(
                    &format!("{name}/{storage}/{variant}"),
                    if vertices > 3 { 512 } else { 8192 },
                    256,
                    || {
                        f.builder.items.extend_from_slice(&items);
                        f.builder.meshes.extend(template.iter().cloned());
                        f.frame.mesh_vertices.clear();
                        f.frame.ops.clear();
                        black_box(finish::<false>(&mut f, old));
                    },
                );
                assert_eq!(f.frame.mesh_vertices.len(), 256 * vertices);
                assert_eq!(
                    f.frame.ops.len(),
                    if vertices == 0 { 0 } else { 256 / group }
                );
                assert!(f.builder.items.is_empty());
                assert!(f.builder.meshes.is_empty());
            }
        }
    }
}
