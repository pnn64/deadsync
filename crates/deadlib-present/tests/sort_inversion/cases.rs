use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline;

type Sort = fn(&mut [DrawItem], &mut ComposeScratch);

fn order(items: &mut [DrawItem], scratch: &mut ComposeScratch, sort: Sort) {
    if !items
        .windows(2)
        .all(|pair| pair[0].sort_key() <= pair[1].sort_key())
    {
        sort(items, scratch);
    }
}

fn fixture(count: usize, layers: usize, mode: &str) -> Vec<DrawItem> {
    if mode == "shadows" {
        return shadow_fixture(count / 2, i16::MIN);
    }
    let mut items: Vec<_> = (0..count)
        .map(|i| {
            let layer = if mode == "ordered_z" {
                i * layers / count
            } else {
                i * 17 % layers
            };
            let z = if mode.contains("wide") {
                (layer * (u16::MAX as usize / layers)) as i32 + i16::MIN as i32
            } else {
                layer as i32
            } as i16;
            let mut item = DrawItem::synthetic(z, i as u32, i as u32);
            item.order = if mode.contains("unordered") || mode == "ordered_z" {
                (count - i) as u32
            } else {
                i as u32
            };
            item.camera = (i % 3) as u8;
            item.texture_handle = (i % 19) as u64;
            item.blend = if i % 5 == 0 {
                BlendMode::Add
            } else {
                BlendMode::Alpha
            };
            item.kind = match i % 3 {
                0 => DrawKind::Sprite,
                1 => DrawKind::Mesh,
                _ => DrawKind::TexturedMesh,
            };
            item
        })
        .collect();
    if mode == "late" {
        if let Some(last) = items.last_mut() {
            last.order = 0;
        }
    } else if mode == "sorted" {
        items.sort_unstable_by_key(|item| item.sort_key());
    }
    items
}

fn shadow_fixture(count: usize, z: i16) -> Vec<DrawItem> {
    let mut builder = FrameBuilder::default();
    let vertices = Arc::from(
        [renderer::MeshVertex {
            pos: [0.0; 2],
            color: [1.0; 4],
        }; 3],
    );
    for start in (0..count).step_by(4) {
        let first = builder.len();
        for i in start..(start + 4).min(count) {
            builder.push_mesh(
                0,
                i as u32,
                z,
                BlendMode::Alpha,
                0,
                MeshPayload {
                    transform: Matrix4::IDENTITY,
                    tint: [0.5; 4],
                    vertices: MeshVertices::Shared(Arc::clone(&vertices)),
                },
            );
        }
        let end = builder.len();
        push_shadow_objects_for_range(
            &mut builder,
            &mut Vec::new(),
            &mut Vec::new(),
            first,
            end,
            [2.0; 2],
            [0.5; 4],
        );
    }
    builder.items
}

#[test]
fn saturated_shadow_layers_keep_exact_source_and_shadow_order() {
    for count in [2, 7, 65, 512] {
        for z in [i16::MIN, i16::MIN + 1, 0, i16::MAX] {
            let mut old = shadow_fixture(count, z);
            if z == i16::MIN {
                // Shadow Z saturates at the source layer; appending a range of
                // shadows then produces a genuine within-layer inversion.
                assert!(old.windows(2).any(|p| p[0].order > p[1].order));
            }
            let mut new = old.clone();
            order(
                &mut old,
                &mut ComposeScratch::default(),
                baseline::sort_composed_draw_items,
            );
            order(
                &mut new,
                &mut ComposeScratch::default(),
                sort_composed_draw_items,
            );
            assert_eq!(new, old, "count {count}, z {z}");
        }
    }
}

#[test]
fn detected_order_inversion_preserves_exact_sort_and_tie_order() {
    let mut old_scratch = ComposeScratch::default();
    let mut new_scratch = ComposeScratch::default();
    let mut cases = 0;
    // Reuse scratch across changing fallback reasons, key ranges and sizes.
    for count in [0, 1, 2, 17, 64, 65, 257, 1024, 4096] {
        for layers in [1, 2, 7, 64, 65, 128, 257] {
            for mode in [
                "sorted",
                "interleaved",
                "wide",
                "unordered",
                "wide_unordered",
                "ordered_z",
                "late",
            ] {
                let source = fixture(count, layers, mode);
                for ties in [false, true] {
                    let mut old = source.clone();
                    if ties {
                        for item in &mut old {
                            item.order /= 32;
                        }
                    }
                    let mut new = old.clone();
                    order(
                        &mut old,
                        &mut old_scratch,
                        baseline::sort_composed_draw_items,
                    );
                    order(&mut new, &mut new_scratch, sort_composed_draw_items);
                    assert_eq!(
                        new, old,
                        "count {count}, layers {layers}, mode {mode}, ties {ties}"
                    );
                    cases += 1;
                }
            }
        }
    }
    eprintln!("{cases} full draw sequences matched exactly");
}

#[test]
fn detected_order_inversion_keeps_warmed_sort_allocation_free() {
    for sort in [
        baseline::sort_composed_draw_items as Sort,
        sort_composed_draw_items,
    ] {
        let mut scratch = ComposeScratch::default();
        for mode in [
            "interleaved",
            "unordered",
            "wide_unordered",
            "ordered_z",
            "late",
            "wide",
        ] {
            for layers in [7, 65, 128] {
                let source = fixture(4096, layers, mode);
                let mut items = source.clone();
                // Sparse sorting swaps the count and permutation buffers.
                // Warm both allocations before checking steady-state reuse.
                for _ in 0..3 {
                    items.copy_from_slice(&source);
                    order(&mut items, &mut scratch, sort);
                }
                perf::assert_no_churn(|| {
                    for _ in 0..8 {
                        items.copy_from_slice(&source);
                        order(&mut items, &mut scratch, sort);
                    }
                });
            }
        }
    }
}

#[test]
#[ignore = "paired release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_sort_inversion() {
    for (name, count, layers, mode) in [
        ("sorted1024", 1024, 7, "sorted"),
        ("sparse7_1024", 1024, 7, "interleaved"),
        ("dense65_1024", 1024, 65, "interleaved"),
        ("dense128_4096", 4096, 128, "interleaved"),
        ("unordered7_64", 64, 7, "unordered"),
        ("unordered7_1024", 1024, 7, "unordered"),
        ("unordered7_4096", 4096, 7, "unordered"),
        ("wide_unordered7", 1024, 7, "wide_unordered"),
        ("ordered_z1024", 1024, 7, "ordered_z"),
        ("late7_1024", 1024, 7, "late"),
        ("unordered128", 4096, 128, "unordered"),
        ("saturated_shadows64", 64, 1, "shadows"),
        ("saturated_shadows1024", 1024, 1, "shadows"),
    ] {
        let source = fixture(count, layers, mode);
        let mut items = source.clone();
        let mut scratch = ComposeScratch::default();
        let mut variants = [
            ("before", baseline::sort_composed_draw_items as Sort),
            ("after", sort_composed_draw_items as Sort),
        ];
        if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            variants.reverse();
        }
        for (variant, sort) in variants {
            let sort = black_box(sort);
            order(&mut items, &mut scratch, sort);
            perf::measure_sampled(
                &format!("sort_inversion/{name}/{variant}"),
                (4096 * 4096 / count).min(65536),
                count,
                || {
                    items.copy_from_slice(black_box(&source));
                    order(black_box(&mut items), &mut scratch, sort);
                    black_box(&items);
                },
            );
        }
    }
}
