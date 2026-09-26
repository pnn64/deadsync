//! Exercise and measure the production stripe builder without a window.
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

#[allow(dead_code)]
mod backend {
    include!("../src/lib.rs");

    mod stripe_bins {
        use super::*;
        use crate::perf;
        use std::hint::black_box;

        fn sprite(rows: ScreenRows) -> PreparedObject {
            PreparedObject::Sprite {
                vertices: [ScreenVertex {
                    x: 0.0,
                    y: 0.0,
                    u: 0.0,
                    v: 0.0,
                }; 4],
                rows,
                inv_denom: [None; 2],
                tint: [1.0; 4],
                texture_mask: false,
                blend: BlendMode::Alpha,
                texture_handle: 7,
            }
        }

        #[test]
        fn stripe_boundaries_preserve_empty_ranges_and_partial_last_stripe() {
            let objects: Vec<_> = [
                (0, 0),
                (0, 32),
                (31, 32),
                (32, 33),
                (32, 64),
                (64, 65),
                (96, 97),
            ]
            .into_iter()
            .map(|(start, end)| sprite(ScreenRows { start, end }))
            .collect();
            let mut bins = StripeBins::warmed();
            bins.build(&objects, &[], &[], 65);
            assert_eq!(bins.stripe(0), [StripeItem::whole(1), StripeItem::whole(2)]);
            assert_eq!(bins.stripe(1), [StripeItem::whole(3), StripeItem::whole(4)]);
            assert_eq!(bins.stripe(2), [StripeItem::whole(5)]);
            bins.build(&[], &[], &[], 65);
            assert!(bins.items.is_empty());
            assert_eq!(bins.offsets, [0; 4]);
        }

        #[test]
        fn rebuild_matches_ordered_memberships_across_sizes_and_object_kinds() {
            let mut bins = StripeBins::warmed();
            let mut seed = 123456789u32;
            let mut random = || {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed
            };
            for height in [0, 1, 31, 32, 33, 64, 65, 720, 1080, 2160, 33, 0] {
                for count in [0, 1, 7, 128, 513, 3] {
                    let mut objects = Vec::new();
                    let mut meshes = Vec::new();
                    let mut tmeshes = Vec::new();
                    // Independent ordered membership list, assembled with the fixture.
                    let mut expected = Vec::new();
                    for index in 0..count {
                        let start = random() % (height as u32 + 65);
                        let end = start + random() % (height as u32 + 1) + 1;
                        let rows = ScreenRows { start, end };
                        let setup = RasterSetup {
                            min_x: 0,
                            max_x: 1,
                            min_y: start as i32,
                            max_y: end as i32 - 1,
                            inv_denom: 1.0,
                        };
                        match index % 5 {
                            0 => {
                                objects.push(sprite(rows));
                                expected.push((rows, StripeItem::whole(index)));
                            }
                            1 => {
                                let triangle = meshes.len() as u32;
                                meshes.push(PreparedTriangle {
                                    object: index,
                                    vertices: [ScreenVertexColor {
                                        x: 0.0,
                                        y: 0.0,
                                        color: [1.0; 4],
                                    }; 3],
                                    setup,
                                });
                                objects.push(PreparedObject::Mesh {
                                    triangle_start: triangle,
                                    triangle_count: 1,
                                    rows,
                                    blend: BlendMode::Alpha,
                                });
                                expected.push((rows, StripeItem::mesh(triangle)));
                            }
                            2 => {
                                let triangle = tmeshes.len() as u32;
                                tmeshes.push(PreparedTriangle {
                                    object: index,
                                    vertices: [ScreenVertexTexColor {
                                        x: 0.0,
                                        y: 0.0,
                                        u: 0.0,
                                        v: 0.0,
                                        color: [1.0; 4],
                                    }; 3],
                                    setup,
                                });
                                objects.push(PreparedObject::TexturedMesh {
                                    triangle_start: triangle,
                                    triangle_count: 1,
                                    rows,
                                    texture_mask: false,
                                    blend: BlendMode::Alpha,
                                    texture_handle: 7,
                                });
                                expected.push((rows, StripeItem::tmesh(triangle)));
                            }
                            3 => {
                                objects.push(PreparedObject::DirectMesh {
                                    vertex_start: 0,
                                    vertex_count: 3,
                                    projection: Matrix4::IDENTITY,
                                    blend: BlendMode::Alpha,
                                });
                                expected.push((
                                    ScreenRows {
                                        start: 0,
                                        end: height as u32,
                                    },
                                    StripeItem::whole(index),
                                ));
                            }
                            _ => {
                                objects.push(PreparedObject::DirectTexturedMesh {
                                    geometry: 0,
                                    instance: 0,
                                    mvp: Matrix4::IDENTITY,
                                    blend: BlendMode::Alpha,
                                    texture_handle: 7,
                                });
                                expected.push((
                                    ScreenRows {
                                        start: 0,
                                        end: height as u32,
                                    },
                                    StripeItem::whole(index),
                                ));
                            }
                        }
                    }
                    bins.build(&objects, &meshes, &tmeshes, height);
                    for stripe in 0..height.div_ceil(SOFTWARE_ROW_CHUNK) {
                        let expected: Vec<_> = expected
                            .iter()
                            .filter_map(|(rows, item)| {
                                ((rows.start as usize) < (stripe + 1) * SOFTWARE_ROW_CHUNK
                                    && rows.end as usize > stripe * SOFTWARE_ROW_CHUNK)
                                    .then_some(*item)
                            })
                            .collect();
                        assert_eq!(
                            bins.stripe(stripe),
                            expected,
                            "height={height}, count={count}, stripe={stripe}"
                        );
                    }
                    assert_eq!(bins.offsets.last().copied(), Some(bins.items.len() as u32));
                    perf::assert_no_churn(|| bins.build(&objects, &meshes, &tmeshes, height));
                }
            }
        }

        #[test]
        #[ignore = "manual release benchmark"]
        fn benchmark_stripe_build() {
            for (name, height, count, span) in [
                ("empty", 1080, 0, 1),
                ("small", 1080, 16, 32),
                ("one-stripe", 1080, 1024, 1),
                ("four-stripes", 1080, 1024, 128),
                ("full-height", 1080, 1024, 1080),
                ("mixed", 1080, 1024, 0),
                ("4k-mixed", 2160, 1024, 0),
            ] {
                let objects: Vec<_> = (0..count)
                    .map(|index| {
                        let span = if span == 0 {
                            [1, 32, 128, height][index % 4]
                        } else {
                            span
                        };
                        let start = (index * 37) % (height - span + 1);
                        sprite(ScreenRows {
                            start: start as u32,
                            end: (start + span) as u32,
                        })
                    })
                    .collect();
                let mut bins = StripeBins::warmed();
                bins.build(&objects, &[], &[], height);
                perf::measure_sampled(name, 8192, count.max(1), || {
                    bins.build(black_box(&objects), &[], &[], black_box(height));
                    black_box(&bins);
                });
            }
        }
    }
}
