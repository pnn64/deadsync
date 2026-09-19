use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline;

#[derive(Default)]
struct Textures(Cell<usize>);

impl TextureContext for Textures {
    fn texture_registry_generation(&self) -> u64 {
        1
    }
    fn texture_dims(&self, _: &str) -> Option<TextureMeta> {
        None
    }
    fn sprite_sheet_dims(&self, _: &str) -> (u32, u32) {
        (1, 1)
    }
    fn texture_handle(&self, _: &str) -> u64 {
        self.0.set(self.0.get() + 1);
        7
    }
}

fn mesh<'a>(
    texture: &'a Arc<str>,
    vertices: &'a Arc<[renderer::TexturedMeshVertex]>,
    alpha: f32,
    glow: f32,
) -> TexturedMeshActorView<'a> {
    TexturedMeshActorView {
        align: [0.5, 0.25],
        offset: [12.0, -7.0],
        world_z: 3.0,
        size: [SizeSpec::Px(80.0), SizeSpec::Px(40.0)],
        local_transform: Matrix4::from_rotation_z(0.25),
        texture,
        tint: [0.2, 0.4, 0.8, alpha],
        glow: [0.6, 0.3, 0.1, glow],
        vertices: TexturedMeshActorVertices::Shared(vertices),
        geom_cache_key: 42,
        uv_scale: [0.5, 0.75],
        uv_offset: [0.1, 0.2],
        uv_tex_shift: [0.2, 0.3],
        depth_test: true,
        visible: true,
        blend: BlendMode::Add,
        z: 11,
    }
}

fn compose<const OLD: bool>(
    mesh: TexturedMeshActorView<'_>,
    order: &mut u32,
    out: &mut FrameBuilder,
    cache: &mut TextureLookupCache,
    textures: &Textures,
) {
    let build = if OLD {
        baseline::build_textured_mesh_actor
    } else {
        build_textured_mesh_actor
    };
    build(
        mesh,
        SmRect {
            x: 0.0,
            y: 0.0,
            w: 640.0,
            h: 480.0,
        },
        &Metrics {
            left: 0.0,
            right: 640.0,
            bottom: 0.0,
            top: 480.0,
        },
        7,
        0,
        ComposeStyle {
            tint: [0.8, 0.7, 0.6, 0.5],
            blend: None,
        },
        Some(ActorXFold::new(320.0, 0.8)),
        order,
        out,
        cache,
        textures,
    );
}

#[test]
fn invisible_meshes_preserve_output_and_order_without_texture_lookups() {
    let texture = Arc::from("mesh.png");
    let vertices: Arc<[renderer::TexturedMeshVertex]> =
        Arc::from([renderer::TexturedMeshVertex::default(); 3]);
    let reusable = Arc::new(vertices.to_vec());
    for alpha in [0.0, -0.0, -1.0, 0.5, f32::NAN, f32::INFINITY] {
        for glow in [0.0, -1.0, 0.0001, 0.00010001, 0.5, f32::NAN] {
            for start in [0, 7, u32::MAX - 1, u32::MAX] {
                for use_reusable in [false, true] {
                    let mut frames = Vec::new();
                    let mut orders = Vec::new();
                    for old in [true, false] {
                        let mut out = FrameBuilder::default();
                        let mut cache = TextureLookupCache::default();
                        let textures = Textures::default();
                        cache.begin_frame(&textures);
                        let mut order = start;
                        let mut view = mesh(&texture, &vertices, alpha, glow);
                        if use_reusable {
                            view.vertices = TexturedMeshActorVertices::Reusable(&reusable);
                        }
                        if old {
                            compose::<true>(view, &mut order, &mut out, &mut cache, &textures);
                        } else {
                            compose::<false>(view, &mut order, &mut out, &mut cache, &textures);
                        }
                        if !old && !(alpha > 0.0 || glow > 0.0001) {
                            assert_eq!(textures.0.get(), 0);
                        }
                        // A following visible actor must keep the same order and resolve its texture.
                        compose::<false>(
                            mesh(&texture, &vertices, 1.0, 0.0),
                            &mut order,
                            &mut out,
                            &mut cache,
                            &textures,
                        );
                        assert_eq!(textures.0.get(), 1);
                        orders.push((
                            order,
                            out.items.iter().map(|item| item.order).collect::<Vec<_>>(),
                        ));
                        frames.push(out);
                    }
                    assert_eq!(orders[0], orders[1]);
                    assert_eq!(frames[0].items.len(), frames[1].items.len());
                    for (a, b) in frames[0].items.iter().zip(&frames[1].items) {
                        assert_eq!(
                            (
                                a.texture_handle,
                                a.order,
                                a.z,
                                a.blend,
                                a.camera,
                                a.kind,
                                a.payload_index
                            ),
                            (
                                b.texture_handle,
                                b.order,
                                b.z,
                                b.blend,
                                b.camera,
                                b.kind,
                                b.payload_index
                            )
                        );
                    }
                    for (a, b) in frames[0]
                        .textured_meshes
                        .iter()
                        .zip(&frames[1].textured_meshes)
                    {
                        let (a, b) = (a.as_ref().unwrap(), b.as_ref().unwrap());
                        assert_eq!(a.instance, b.instance);
                        assert_eq!(a.vertices.as_ref(), b.vertices.as_ref());
                        assert_eq!(
                            (a.geom_cache_key, a.depth_test),
                            (b.geom_cache_key, b.depth_test)
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "manual release CPU benchmark; --ignored --nocapture --test-threads=1"]
fn mesh_visibility_bench() {
    let texture = Arc::from("mesh.png");
    let vertices = Arc::from([renderer::TexturedMeshVertex::default(); 3]);
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, alpha, glow) in [
        ("hidden", 0.0, 0.0),
        ("diffuse", 0.5, 0.0),
        ("glow", 0.0, 0.5),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let mut out = FrameBuilder::default();
            let mut cache = TextureLookupCache::default();
            let textures = Textures::default();
            cache.begin_frame(&textures);
            perf::measure_sampled(
                &format!("{name}/{}", if old { "old" } else { "new" }),
                2048,
                256,
                || {
                    let mut order = 0;
                    for _ in 0..256 {
                        let view = black_box(mesh(&texture, &vertices, alpha, glow));
                        if old {
                            compose::<true>(view, &mut order, &mut out, &mut cache, &textures);
                        } else {
                            compose::<false>(view, &mut order, &mut out, &mut cache, &textures);
                        }
                    }
                    black_box((&out, order));
                    out.items.clear();
                    out.textured_meshes.clear();
                },
            );
        }
    }
}
