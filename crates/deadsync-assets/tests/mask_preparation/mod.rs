use super::*;
use crate::{asset_discovery_support::Tree, noteskin::texture, perf};
use std::hint::black_box;

mod baseline;

fn compare_mesh(old: &ModelMesh, new: &ModelMesh) {
    assert_eq!(old.bounds.map(f32::to_bits), new.bounds.map(f32::to_bits));
    assert_eq!(old.vertices.len(), new.vertices.len());
    for (a, b) in old.vertices.iter().zip(new.vertices.iter()) {
        assert_eq!(a.normal.map(f32::to_bits), b.normal.map(f32::to_bits));
        assert_eq!(a.pos.map(f32::to_bits), b.pos.map(f32::to_bits));
        assert_eq!(a.uv.map(f32::to_bits), b.uv.map(f32::to_bits));
        assert_eq!(
            a.tex_matrix_scale.map(f32::to_bits),
            b.tex_matrix_scale.map(f32::to_bits)
        );
    }
}

fn pattern(w: u32, h: u32, kind: usize) -> image::RgbaImage {
    image::RgbaImage::from_fn(w, h, |x, y| {
        let transparent = match kind {
            0 => false,
            1 => true,
            2 => x >= w / 4 && x < w * 3 / 4 && y >= h / 4 && y < h * 3 / 4,
            3 => x % 4 < 2,
            4 => (x + y) % 2 == 0,
            5 => (x * 37 + y * 17 + x * y) % 11 < 5,
            _ => (x / 3 + y / 2) % 3 == 0,
        };
        image::Rgba([
            x as u8,
            y as u8,
            (x ^ y) as u8,
            if transparent {
                0
            } else {
                ((x + y) % 255 + 1) as u8
            },
        ])
    })
}

fn atlas(cell_w: u32, cell_h: u32, cols: u32, rows: u32, colorful: bool) -> image::RgbaImage {
    image::RgbaImage::from_fn(cell_w * cols, cell_h * rows, |x, y| {
        let transparent = (x % cell_w * 7 + y % cell_h * 3) % 11 < 5;
        let rgb = if colorful {
            [x as u8, y as u8, (x * y) as u8]
        } else {
            [255; 3]
        };
        image::Rgba([
            rgb[0],
            rgb[1],
            rgb[2],
            if transparent {
                0
            } else {
                (x + y) as u8 % 255 + 1
            },
        ])
    })
}

#[test]
fn alpha_cell_validation_matches_parent_across_origins_colors_and_mismatches() {
    for (cw, ch, cols, rows) in [(1, 1, 1, 1), (2, 3, 4, 2), (7, 5, 3, 4), (64, 32, 4, 2)] {
        for colorful in [false, true] {
            let image = atlas(cw, ch, cols, rows, colorful);
            let mut origins = vec![(0, 0), ((cols - 1) * cw, (rows - 1) * ch)];
            if cols > 1 && rows > 1 {
                origins.push((1, 1));
            }
            for (x, y) in origins {
                let region = [x, y, cw, ch];
                assert_eq!(
                    baseline::mask_cells_match(&image, region),
                    mask_cells_match(&image, region)
                );
                perf::assert_no_churn(|| {
                    black_box(mask_cells_match(&image, region));
                });
                for (px, py) in [
                    (0, 0),
                    (image.width() - 1, image.height() - 1),
                    (cw.min(image.width() - 1), 0),
                ] {
                    let mut altered = image.clone();
                    let alpha = &mut altered.get_pixel_mut(px, py)[3];
                    *alpha = if *alpha == 0 { 1 } else { 0 };
                    assert_eq!(
                        baseline::mask_cells_match(&altered, region),
                        mask_cells_match(&altered, region)
                    );
                }
            }
        }
    }
    for (w, h) in [(0, 0), (0, 3), (3, 0)] {
        let image = image::RgbaImage::new(w, h);
        assert!(baseline::mask_cells_match(&image, [0, 0, 1, 1]));
        assert!(mask_cells_match(&image, [0, 0, 1, 1]));
    }
    let valid = atlas(3, 2, 4, 2, true);
    let mut bytes = valid.as_raw().clone();
    bytes.extend(vec![0; 12 * 4 * 3]);
    let oversized = image::RgbaImage::from_raw(12, 4, bytes).unwrap();
    assert!(baseline::mask_cells_match(&oversized, [3, 2, 3, 2]));
    assert!(mask_cells_match(&oversized, [3, 2, 3, 2]));
}

#[test]
fn rectangle_frontier_matches_all_four_by_four_alpha_masks_in_exact_order() {
    let mut image = image::RgbaImage::from_pixel(8, 7, image::Rgba([255; 4]));
    for bits in 0..=u16::MAX {
        for row in 0..4 {
            for col in 0..4 {
                image.get_pixel_mut(col + 2, row + 1)[3] = if bits & (1 << (row * 4 + col)) != 0 {
                    0
                } else {
                    1
                };
            }
        }
        let region = [2, 1, 4, 4];
        let old = baseline::mask_rectangles(&image, region);
        let new = mask_rectangles(&image, region);
        assert_eq!(old, new, "mask {bits:04x}");
        if bits % 127 == 0 {
            compare_mesh(
                &baseline::cutout_mesh(&image, region, [17, -23]),
                &cutout_mesh(&image, region, [17, -23]),
            );
        }
    }
}

#[test]
fn frontier_handles_empty_rows_long_lived_rectangles_and_inline_spill_boundaries() {
    for (w, h) in [
        (0, 0),
        (1, 1),
        (1, 17),
        (2, 31),
        (7, 13),
        (127, 7),
        (128, 7),
        (129, 7),
        (256, 7),
        (641, 7),
    ] {
        for kind in 0..7 {
            let image = pattern(w, h, kind);
            let region = [0, 0, w, h];
            assert_eq!(
                baseline::mask_rectangles(&image, region),
                mask_rectangles(&image, region)
            );
            for size in [[64, 32], [0, 0], [-13, 19], [i32::MIN, i32::MAX]] {
                compare_mesh(
                    &baseline::cutout_mesh(&image, region, size),
                    &cutout_mesh(&image, region, size),
                );
            }
        }
    }
    let image = image::RgbaImage::from_fn(129, 12, |x, y| {
        image::Rgba([
            0,
            0,
            0,
            if y == 3 || y == 8 || (x + y / 4) % 2 != 0 {
                255
            } else {
                0
            },
        ])
    });
    assert_eq!(
        baseline::mask_rectangles(&image, [0, 0, 129, 12]),
        mask_rectangles(&image, [0, 0, 129, 12])
    );
}

#[test]
fn direct_vertex_arrays_preserve_bits_and_remove_staging_allocations() {
    for count in [0, 1, 2, 17, 512] {
        let rects = (0..count)
            .map(|n| [n % 64, n % 32, n % 64 + 1, n % 32 + 1])
            .collect::<Vec<_>>();
        for size in [[64, 32], [-17, 0], [i32::MIN, i32::MAX]] {
            compare_mesh(
                &baseline::mesh_from_rectangles(rects.clone(), [64, 32], size),
                &mesh_from_rectangles(rects.clone(), [64, 32], size),
            );
        }
        let _ = mesh_from_rectangles(rects.clone(), [64, 32], [64, 32]);
        perf::assert_reduced_churn(
            || {
                black_box(baseline::mesh_from_rectangles(
                    rects.clone(),
                    [64, 32],
                    [64, 32],
                ));
            },
            || {
                black_box(mesh_from_rectangles(rects.clone(), [64, 32], [64, 32]));
            },
        );
    }
    let a = mesh_from_rectangles(Vec::new(), [64, 64], [64, 64]);
    let b = mesh_from_rectangles(Vec::new(), [64, 64], [32, 32]);
    assert!(Arc::ptr_eq(&a.vertices, &b.vertices));
    perf::assert_no_churn(|| {
        black_box(mesh_from_rectangles(Vec::new(), [64, 64], [64, 64]));
    });
    let rects = vec![[0, 0, 1, 1]; 512];
    let mut output = None;
    perf::assert_churn_budget(2, 512 * 6 * std::mem::size_of::<ModelVertex>() + 16, || {
        output = Some(mesh_from_rectangles(rects, [64, 64], [64, 64]));
    });
    drop(output);
}

fn slot(path: &Path, region: [i32; 4]) -> SpriteSlot {
    let mut slot = texture::test_model_slot();
    slot.model = None;
    slot.def.src = [region[0], region[1]];
    slot.def.size = [region[2], region[3]];
    slot.source_size = [17, 23];
    if let super::super::texture::SpriteSource::Atlas { texture_key, .. } =
        Arc::get_mut(&mut slot.source).unwrap()
    {
        *texture_key = Arc::from(path.to_str().unwrap());
    }
    slot
}

#[test]
fn decoded_mask_loading_preserves_region_checks_alpha_one_and_model_rejections() {
    crate::noteskin::tests::init_asset_paths();
    let tree = Tree::new();
    let path = tree.path.join("mask 4x2.png");
    let mut image = atlas(7, 5, 4, 2, true);
    for altered in [false, true] {
        if altered {
            let alpha = &mut image.get_pixel_mut(27, 9)[3];
            *alpha = if *alpha == 0 { 1 } else { 0 };
        }
        image.save(&path).unwrap();
        for region in [
            [0, 0, 7, 5],
            [21, 5, 7, 5],
            [1, 1, 7, 5],
            [-1, 0, 7, 5],
            [0, -1, 7, 5],
            [0, 0, 0, 5],
            [0, 0, -7, 5],
            [0, 0, 6, 5],
            [27, 9, 7, 5],
            [i32::MAX, 0, 7, 5],
        ] {
            let initial = slot(&path, region);
            let old = baseline::load_mask_mesh(&initial);
            let new = load_mask_mesh(&initial);
            assert_eq!(old.is_some(), new.is_some());
            if !altered && region == [0, 0, 7, 5] {
                assert!(new.is_some());
            }
            if let (Some(old), Some(new)) = (old, new) {
                compare_mesh(&old, &new);
            }
        }
    }
    for reject in 0..3 {
        let mut initial = slot(&path, [0, 0, 7, 5]);
        match reject {
            0 => initial.model = texture::test_model_slot().model,
            1 => initial.custom_uv = Some([0.0; 4]),
            _ => initial.uv_velocity = [0.0, 0.1],
        }
        perf::assert_no_churn(|| {
            assert!(baseline::load_mask_mesh(&initial).is_none());
            assert!(load_mask_mesh(&initial).is_none());
        });
    }
}

fn resolved(slot: SpriteSlot, script: &str) -> ItgLuaResolvedSprite {
    ItgLuaResolvedSprite {
        element: "mask-fixture".to_owned(),
        slot,
        commands: std::collections::HashMap::from([("initcommand".to_owned(), script.to_owned())]),
    }
}

#[test]
fn complete_mask_application_preserves_depth_order_removal_and_mesh_sharing() {
    crate::noteskin::tests::init_asset_paths();
    let tree = Tree::new();
    let path = tree.path.join("mask.png");
    pattern(8, 8, 2).save(&path).unwrap();
    let base = slot(&path, [0, 0, 8, 8]);
    for script in [
        "clearzbuffer,true;SetTextureFiltering,false;zwrite,1;blend,BlendMode_NoEffect",
        "SetTextureFiltering,true;zwrite,1;blend,BlendMode_NoEffect",
        "blend,BlendMode_NoEffect",
    ] {
        let mut changed = base.clone();
        changed.set_rotation_deg(45);
        let fixture = || {
            vec![
                resolved(base.clone(), script),
                resolved(base.clone(), "ztest,1"),
                resolved(base.clone(), "ztest,true"),
                resolved(changed.clone(), "ztest,1"),
                resolved(base.clone(), "clearzbuffer,1;ztest,1"),
            ]
        };
        let mut old = fixture();
        let mut new = fixture();
        baseline::apply_sprite_masks(&mut old);
        apply_sprite_masks(&mut new);
        assert_eq!(old.len(), new.len());
        for (old, new) in old.iter().zip(new.iter()) {
            assert_eq!(old.element, new.element);
            assert_eq!(old.commands, new.commands);
            assert_eq!(old.slot.sprite_mesh, new.slot.sprite_mesh);
            assert_eq!(old.slot.model.is_some(), new.slot.model.is_some());
            if let (Some(a), Some(b)) = (&old.slot.model, &new.slot.model) {
                compare_mesh(a, b);
            }
        }
        if script.starts_with("clearzbuffer") {
            assert!(new[0].slot.sprite_mesh);
        }
        if new[0].slot.sprite_mesh {
            assert!(Arc::ptr_eq(
                new[0].slot.model.as_ref().unwrap(),
                new[1].slot.model.as_ref().unwrap()
            ));
        }
    }
}

#[test]
fn complete_cutouts_reduce_owning_bytes_and_ordinary_allocation_churn() {
    for kind in 0..6 {
        let image = pattern(64, 64, kind);
        let _ = cutout_mesh(&image, [0, 0, 64, 64], [64, 64]);
        perf::assert_reduced_churn(
            || {
                black_box(baseline::cutout_mesh(&image, [0, 0, 64, 64], [64, 64]));
            },
            || {
                black_box(cutout_mesh(&image, [0, 0, 64, 64], [64, 64]));
            },
        );
    }
}

fn pairs(mut work: impl FnMut(&str, bool)) {
    if std::env::var("DEADSYNC_PERF_ORDER").as_deref() == Ok("new-first") {
        work("new", true);
        work("old", false);
    } else {
        work("old", false);
        work("new", true);
    }
}

#[test]
#[ignore = "manual paired release CPU, throughput and allocation benchmark"]
fn benchmark_mask_preparation() {
    for (name, cw, ch, cols, rows, colorful, mismatch) in [
        ("single", 64, 64, 1, 1, false, 0),
        ("atlas", 32, 32, 8, 4, false, 0),
        ("colorful", 32, 32, 8, 4, true, 0),
        ("early_mismatch", 32, 32, 8, 4, true, 1),
        ("late_mismatch", 32, 32, 8, 4, true, 2),
    ] {
        let mut image = atlas(cw, ch, cols, rows, colorful);
        let region = [
            if cols == 1 { 0 } else { cw },
            if rows == 1 { 0 } else { ch },
            cw,
            ch,
        ];
        if mismatch != 0 {
            let (px, py) = if mismatch == 1 {
                (0, 0)
            } else {
                (image.width() - 1, image.height() - 1)
            };
            let alpha = &mut image.get_pixel_mut(px, py)[3];
            *alpha = if *alpha == 0 { 1 } else { 0 };
        }
        assert_eq!(
            baseline::mask_cells_match(&image, region),
            mask_cells_match(&image, region)
        );
        pairs(|label, new| {
            perf::measure_sampled(&format!("mask_cells_{name}_{label}"), 512, 1, || {
                black_box(if new {
                    mask_cells_match(black_box(&image), black_box(region))
                } else {
                    baseline::mask_cells_match(black_box(&image), black_box(region))
                });
            })
        });
    }
    for (name, w, h, kind, iterations) in [
        ("hole64", 64, 64, 2, 256),
        ("stripes64", 64, 64, 3, 128),
        ("checker64", 64, 64, 4, 16),
        ("wide256", 256, 8, 4, 32),
    ] {
        let image = pattern(w, h, kind);
        let region = [0, 0, w, h];
        assert_eq!(
            baseline::mask_rectangles(&image, region),
            mask_rectangles(&image, region)
        );
        pairs(|label, new| {
            perf::measure_sampled(&format!("mask_rects_{name}_{label}"), iterations, 1, || {
                black_box(if new {
                    mask_rectangles(black_box(&image), black_box(region))
                } else {
                    baseline::mask_rectangles(black_box(&image), black_box(region))
                });
            })
        });
    }
    for (name, count) in [("small", 1), ("fragmented", 512), ("empty", 0)] {
        let rects = (0..count)
            .map(|n| [n % 64, n % 32, n % 64 + 1, n % 32 + 1])
            .collect::<Vec<_>>();
        compare_mesh(
            &baseline::mesh_from_rectangles(rects.clone(), [64, 32], [64, 32]),
            &mesh_from_rectangles(rects.clone(), [64, 32], [64, 32]),
        );
        pairs(|label, new| {
            perf::measure_sampled(&format!("mask_vertices_{name}_{label}"), 512, 1, || {
                let input = black_box(&rects).clone();
                black_box(if new {
                    mesh_from_rectangles(input, black_box([64, 32]), black_box([64, 32]))
                } else {
                    baseline::mesh_from_rectangles(input, black_box([64, 32]), black_box([64, 32]))
                });
            })
        });
    }
    for (name, w, h, kind, iterations) in [
        ("hole64", 64, 64, 2, 256),
        ("checker64", 64, 64, 4, 16),
        ("wide256", 256, 8, 4, 32),
        ("opaque64", 64, 64, 0, 256),
    ] {
        let image = pattern(w, h, kind);
        let region = [0, 0, w, h];
        let size = [w as i32, h as i32];
        compare_mesh(
            &baseline::cutout_mesh(&image, region, size),
            &cutout_mesh(&image, region, size),
        );
        pairs(|label, new| {
            perf::measure_sampled(&format!("mask_full_{name}_{label}"), iterations, 1, || {
                black_box(if new {
                    cutout_mesh(black_box(&image), black_box(region), black_box(size))
                } else {
                    baseline::cutout_mesh(black_box(&image), black_box(region), black_box(size))
                });
            })
        });
    }
}
