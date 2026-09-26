use super::*;
use std::hint::black_box;

// Frozen constructor from 2ea0521e9. Both cursors use production event handling.
fn baseline<'a>(
    attributes: &'a [actors::TextAttribute],
    scratch: &'a mut TextAttrScratch,
) -> Option<TextAttrCursor<'a>> {
    if attributes.is_empty() {
        return None;
    }
    let TextAttrScratch {
        start_order,
        end_order,
        active,
    } = scratch;
    start_order.clear();
    end_order.clear();
    active.clear();
    active.reserve(attributes.len());
    start_order.extend(0..attributes.len());
    end_order.extend(0..attributes.len());
    start_order.sort_unstable_by_key(|&index| (attributes[index].start, index));
    end_order.sort_unstable_by_key(|&index| (attr_end(&attributes[index]), index));
    Some(TextAttrCursor {
        attributes,
        scratch,
        active_max: None,
        next_start: 0,
        next_end: 0,
    })
}

fn production<'a>(
    attributes: &'a [actors::TextAttribute],
    scratch: &'a mut TextAttrScratch,
) -> Option<TextAttrCursor<'a>> {
    TextAttrCursor::new(attributes, scratch)
}

fn attributes(count: usize, pattern: usize) -> Vec<actors::TextAttribute> {
    (0..count)
        .map(|i| {
            let (start, length) = match pattern {
                0 => (i * 2, 2),
                1 => ((i * 37) % 96, (i * 17) % 70),
                2 => ((i % 4) * 16, 48),
                _ => (7, 128),
            };
            actors::TextAttribute {
                start,
                length,
                color: [i as f32, -0.0, 0.5, 0.75],
                vertex_colors: (i % 3 == 0).then_some([[i as f32; 4]; 4]),
                glow: None,
            }
        })
        .collect()
}

#[test]
fn attribute_boundaries_preserve_exact_colors_and_slice_precedence() {
    let mut old_scratch = TextAttrScratch::default();
    let mut new_scratch = TextAttrScratch::default();
    for count in [0, 1, 8, 9, 32, 128, 257] {
        for pattern in 0..4 {
            let mut attrs = attributes(count, pattern);
            for permutation in 0..8 {
                if count > 0 {
                    attrs.rotate_left(permutation % count);
                }
                if permutation % 2 != 0 {
                    attrs.reverse();
                }
                if let Some(last) = attrs.last_mut() {
                    last.start = usize::MAX - 3;
                    last.length = if permutation % 3 == 0 { 0 } else { 10 };
                }
                let mut old = baseline(&attrs, &mut old_scratch);
                let mut new = TextAttrCursor::new(&attrs, &mut new_scratch);
                assert_eq!(old.is_some(), new.is_some());
                for index in (0..512).step_by(permutation + 1).chain([
                    usize::MAX - 2,
                    usize::MAX - 1,
                    usize::MAX,
                ]) {
                    let expected = attrs
                        .iter()
                        .rev()
                        .find(|a| a.start <= index && index < attr_end(a))
                        .map_or([[1.0; 4]; 4], |a| a.colors())
                        .map(|c| c.map(f32::to_bits));
                    for cursor in [&mut old, &mut new] {
                        let actual = cursor
                            .as_mut()
                            .map_or([[1.0; 4]; 4], |c| c.colors_for(index));
                        assert_eq!(
                            actual.map(|c| c.map(f32::to_bits)),
                            expected,
                            "count={count}, pattern={pattern}, permutation={permutation}, index={index}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "manual release benchmark"]
fn benchmark_attribute_order() {
    type Constructor = for<'a> fn(
        &'a [actors::TextAttribute],
        &'a mut TextAttrScratch,
    ) -> Option<TextAttrCursor<'a>>;
    let mut variants: [(&str, Constructor); 2] = [("before", baseline), ("after", production)];
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        variants.reverse();
    }
    for (name, count, pattern) in [
        ("ordered_8", 8, 0),
        ("overlap_8", 8, 1),
        ("overlap_32", 32, 1),
        ("tied_groups_32", 32, 2),
        ("identical_128", 128, 3),
        ("overlap_128", 128, 1),
    ] {
        let attrs = attributes(count, pattern);
        for (variant, constructor) in variants {
            let mut scratch = TextAttrScratch::default();
            let constructor = black_box(constructor);
            crate::perf::measure_sampled(&format!("{name}/{variant}"), 8192, 1, || {
                let mut cursor = constructor(black_box(&attrs), black_box(&mut scratch)).unwrap();
                for index in (0..256).step_by(4) {
                    black_box(cursor.colors_for(black_box(index)));
                }
            });
        }
    }
}
