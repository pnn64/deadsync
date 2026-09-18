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
        fn mesh_colors_match_unity_tinted_reference() {
            let texture = create_texture(&RgbaImage::new(1, 1), SamplerDesc::default()).unwrap();
            let textures = Textures(texture);
            let colors = [
                [0.1, 0.4, 0.8, 0.5],
                [-0.0, 0.0, f32::from_bits(1), 1.0],
                [f32::MIN, f32::MAX, -1.0, 2.0],
                [f32::NEG_INFINITY, f32::INFINITY, 0.5, 1.0],
                [f32::from_bits(0x7f800001), 0.5, 1.0, 0.5],
                [0.25, f32::from_bits(0xffc12345), 0.75, 1.0],
                [0.25, 0.5, 0.75, f32::from_bits(0xff800001)],
            ];
            for blend in [BlendMode::Alpha, BlendMode::Add] {
                for (case, color) in colors.iter().enumerate() {
                    let mut frame = fixtures::fixture(1, blend, false, false);
                    for (index, vertex) in frame.mesh_vertices.iter_mut().enumerate() {
                        vertex.color = if index % 3 == 0 {
                            *color
                        } else {
                            colors[(case + index) % colors.len()]
                        };
                    }
                    let mut reference = frame.clone();
                    for vertex in &mut reference.mesh_vertices {
                        // Preserve the old arithmetic, including NaN quieting.
                        for channel in &mut vertex.color {
                            *channel *= std::hint::black_box(1.0);
                        }
                    }
                    for stage in [false, true] {
                        assert_eq!(
                            render((&frame).into(), &textures, stage),
                            render((&reference).into(), &textures, stage),
                            "blend={blend:?} case={case} stage={stage}"
                        );
                    }
                }
            }
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
