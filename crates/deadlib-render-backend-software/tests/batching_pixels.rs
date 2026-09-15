//! Compile the backend unchanged to compare its headless pixel paths.
#[allow(dead_code)]
mod backend {
    include!("../src/lib.rs");

    #[cfg(test)]
    mod batching {
        use super::*;
        use crate::fixtures;

        struct Textures(Texture);
        impl TextureLookup for Textures {
            fn software_texture(&self, handle: TextureHandle) -> Option<&Texture> {
                (handle == 7).then_some(&self.0)
            }
        }

        fn render(
            pass: SoftwarePass<'_>,
            textures: &(impl TextureLookup + Sync),
            stage: bool,
        ) -> Vec<u32> {
            let mut objects = Vec::new();
            let mut meshes = Vec::with_capacity(64);
            let mut tmeshes = Vec::with_capacity(64);
            let fixed = prepare_objects(
                pass,
                Matrix4::IDENTITY,
                textures,
                96,
                96,
                &mut objects,
                &mut meshes,
                &mut tmeshes,
                stage,
            );
            let mut pixels = vec![0xff101820; 96 * 96];
            let mut bins = StripeBins::warmed();
            bins.build(&objects, &meshes, &tmeshes, 96);
            for (index, rows) in pixels.chunks_mut(96 * SOFTWARE_ROW_CHUNK).enumerate() {
                let start = index * SOFTWARE_ROW_CHUNK;
                draw_rows(
                    pass,
                    &objects,
                    Some(bins.stripe(index)),
                    &meshes,
                    &tmeshes,
                    textures,
                    96,
                    96,
                    start,
                    start + rows.len() / 96,
                    rows,
                    fixed,
                );
            }
            pixels
        }

        #[test]
        fn joining_incomplete_mesh_runs_changes_pixels() {
            let texture = create_texture(&RgbaImage::new(1, 1), SamplerDesc::default()).unwrap();
            let textures = Textures(texture);
            let mut split = fixtures::fixture(1, BlendMode::Alpha, false, false);
            split.mesh_vertices.truncate(3);
            split.ops = vec![
                DrawOp::Mesh(deadlib_render_core::MeshRun {
                    vertex_start: 0,
                    vertex_count: 1,
                    blend: BlendMode::Alpha,
                    camera: 0,
                }),
                DrawOp::Mesh(deadlib_render_core::MeshRun {
                    vertex_start: 1,
                    vertex_count: 2,
                    blend: BlendMode::Alpha,
                    camera: 0,
                }),
            ];
            let mut joined = split.clone();
            let DrawOp::Mesh(run) = &mut joined.ops[0] else {
                unreachable!()
            };
            run.vertex_count = 3;
            joined.ops.pop();
            assert!(
                render((&split).into(), &textures, false)
                    != render((&joined).into(), &textures, false),
                "merging one and two unused vertices creates a visible triangle"
            );
        }

        #[test]
        fn batch_coalescing_preserves_direct_and_staged_software_pixels() {
            let texture = create_texture(
                &RgbaImage::from_fn(4, 4, |x, y| {
                    image::Rgba([
                        180 + x as u8 * 20,
                        160 + y as u8 * 20,
                        230,
                        80 + (x + y) as u8 * 25,
                    ])
                }),
                SamplerDesc::default(),
            )
            .unwrap();
            let textures = Textures(texture);
            for kind in 0..3 {
                for blend in [BlendMode::Alpha, BlendMode::Add] {
                    for glow in [false, true] {
                        for stage in [false, true] {
                            let split = fixtures::fixture(kind, blend, false, glow);
                            let merged = fixtures::coalesce(&split);
                            let expected = render((&split).into(), &textures, stage);
                            let actual = render((&merged).into(), &textures, stage);
                            assert!(
                                expected.iter().any(|p| *p != expected[0]),
                                "visible fixture"
                            );
                            assert_eq!(
                                expected, actual,
                                "kind={kind} blend={blend:?} glow={glow} stage={stage}"
                            );
                            let split = fixtures::offscreen(split);
                            let merged = fixtures::offscreen(merged);
                            assert_eq!(
                                render((&split.render_targets[0]).into(), &textures, stage),
                                render((&merged.render_targets[0]).into(), &textures, stage)
                            );
                        }
                    }
                }
            }
        }
    }
}

#[path = "../../../tests/support/draw_batching.rs"]
mod fixtures;
