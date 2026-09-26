use super::*;
use crate::perf;
use std::hint::black_box;

mod baseline;

type Sort = fn(&mut [DrawItem], &mut ComposeScratch);

// Match composition's sorted-input guard, so timing includes the complete
// ordering path and never repeatedly sorts an already-sorted fixture.
fn order(items: &mut [DrawItem], scratch: &mut ComposeScratch, sort: Sort) {
    if !items
        .windows(2)
        .all(|pair| pair[0].sort_key() <= pair[1].sort_key())
    {
        sort(items, scratch);
    }
}

fn fixture(count: usize, layers: usize, mode: &str) -> Vec<DrawItem> {
    let mut items = (0..count)
        .map(|i| {
            let z = match mode {
                "wide" => {
                    ((i * 17 % layers) * (u16::MAX as usize / layers)) as i32 + i16::MIN as i32
                }
                "ordered_z" => (i * layers / count) as i32,
                _ => (i * 17 % layers) as i32,
            } as i16;
            let order = if mode == "unordered" || mode == "ordered_z" {
                count - i
            } else {
                i
            };
            let mut item = DrawItem::synthetic(z, order as u32, i as u32);
            item.camera = (i % 3) as u8;
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
        .collect::<Vec<_>>();
    if mode == "sorted" {
        items.sort_unstable_by_key(|item| item.sort_key());
    }
    items
}

#[test]
fn repeated_order_check_removal_preserves_every_draw_header() {
    let mut old_scratch = ComposeScratch::default();
    let mut new_scratch = ComposeScratch::default();
    let mut cases = 0;
    for count in [0, 1, 2, 17, 64, 65, 257, 1024, 4096] {
        for layers in [1, 2, 7, 64, 65, 128, 257] {
            for mode in ["sorted", "interleaved", "wide", "unordered", "ordered_z"] {
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
                    assert!(
                        new.windows(2)
                            .all(|pair| pair[0].sort_key() <= pair[1].sort_key())
                    );
                    cases += 1;
                }
            }
        }
    }
    eprintln!("{cases} ordered draw lists matched exactly");
}

#[test]
fn repeated_order_check_removal_keeps_warmed_sort_allocation_free() {
    for sort in [
        baseline::sort_composed_draw_items as Sort,
        sort_composed_draw_items,
    ] {
        for mode in ["interleaved", "wide", "unordered", "ordered_z"] {
            let source = fixture(4096, 128, mode);
            let mut items = source.clone();
            let mut scratch = ComposeScratch::default();
            order(&mut items, &mut scratch, sort);
            perf::assert_no_churn(|| {
                for _ in 0..16 {
                    items.copy_from_slice(&source);
                    order(&mut items, &mut scratch, sort);
                }
            });
        }
    }
}

#[test]
#[ignore = "manual paired release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_draw_order_recheck() {
    for (name, count, layers, mode) in [
        ("sorted1024", 1024, 128, "sorted"),
        ("sparse7_1024", 1024, 7, "interleaved"),
        ("dense65_1024", 1024, 65, "interleaved"),
        ("dense128_4096", 4096, 128, "interleaved"),
        ("wide65_1024", 1024, 65, "wide"),
        ("unordered7_1024", 1024, 7, "unordered"),
        ("unordered128_4096", 4096, 128, "unordered"),
        ("ordered_z_1024", 1024, 7, "ordered_z"),
    ] {
        let source = fixture(count, layers, mode);
        let mut variants = [
            ("before", baseline::sort_composed_draw_items as Sort),
            ("after", sort_composed_draw_items as Sort),
        ];
        if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            variants.reverse();
        }
        for (variant, sort) in variants {
            let sort = black_box(sort);
            let mut items = source.clone();
            let mut scratch = ComposeScratch::default();
            order(&mut items, &mut scratch, sort);
            perf::measure_sampled(&format!("{name}/{variant}"), 2048, count, || {
                items.copy_from_slice(black_box(&source));
                order(black_box(&mut items), &mut scratch, sort);
                black_box(&items);
            });
        }
    }
}
