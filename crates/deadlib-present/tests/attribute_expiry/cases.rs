use super::*;
use std::hint::black_box;

#[allow(dead_code)]
mod baseline {
    use super::*;
    include!("baseline.rs");
}

fn attributes(count: usize, pattern: usize) -> Vec<actors::TextAttribute> {
    (0..count)
        .map(|i| {
            let (start, length) = match pattern {
                0 => (i * 2, 2),
                1 => ((i * 37) % 96, (i * 17) % 70),
                2 => ((i % 4) * 16, 48),
                3 => (0, count - i),
                4 => (7, 128),
                _ => (0, 256),
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
fn expired_ranges_preserve_exact_colors_and_precedence() {
    let mut before_scratch = TextAttrScratch::default();
    let mut after_scratch = TextAttrScratch::default();
    for count in [0, 1, 8, 9, 32, 128, 257] {
        for pattern in 0..6 {
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
                let mut before = baseline::TextAttrCursor::new(&attrs, &mut before_scratch);
                let mut after = TextAttrCursor::new(&attrs, &mut after_scratch);
                assert_eq!(before.is_some(), after.is_some());
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
                    let before = before
                        .as_mut()
                        .map_or([[1.0; 4]; 4], |c| c.colors_for(index));
                    let after = after
                        .as_mut()
                        .map_or([[1.0; 4]; 4], |c| c.colors_for(index));
                    assert_eq!(before.map(|c| c.map(f32::to_bits)), expected);
                    assert_eq!(
                        after.map(|c| c.map(f32::to_bits)),
                        expected,
                        "count={count}, pattern={pattern}, permutation={permutation}, index={index}"
                    );
                }
            }
        }
    }
}

#[test]
fn removing_the_maximum_keeps_survivor_precedence_before_new_starts() {
    let mut attrs = attributes(4, 4);
    // Index 3 expires at the next query; index 2 survives and must still
    // override newly starting index 0. Index 1 is skipped without activation.
    for (attr, (start, length)) in attrs.iter_mut().zip([(4, 12), (1, 1), (0, 9), (0, 3)]) {
        attr.start = start;
        attr.length = length;
    }
    let mut scratch = TextAttrScratch::default();
    let mut cursor = TextAttrCursor::new(&attrs, &mut scratch).unwrap();
    for (index, winner) in [(0, Some(3)), (4, Some(2)), (9, Some(0)), (16, None)] {
        let expected = winner.map_or([[1.0; 4]; 4], |i| attrs[i].colors());
        assert_eq!(cursor.colors_for(index), expected);
        assert_eq!(cursor.colors_for(index), expected, "repeated glyph query");
    }
}

fn before(attrs: &[actors::TextAttribute], scratch: &mut TextAttrScratch, step: usize) {
    let mut cursor = baseline::TextAttrCursor::new(attrs, scratch).unwrap();
    for index in (0..256).step_by(step) {
        black_box(cursor.colors_for(black_box(index)));
    }
}

fn after(attrs: &[actors::TextAttribute], scratch: &mut TextAttrScratch, step: usize) {
    let mut cursor = TextAttrCursor::new(attrs, scratch).unwrap();
    for index in (0..256).step_by(step) {
        black_box(cursor.colors_for(black_box(index)));
    }
}

#[test]
#[ignore = "manual release benchmark"]
fn benchmark_attribute_expiry() {
    type Walk = fn(&[actors::TextAttribute], &mut TextAttrScratch, usize);
    let mut variants: [(&str, Walk); 2] = [("before", before), ("after", after)];
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        variants.reverse();
    }
    for (name, count, pattern, step) in [
        ("no_expiry_1", 1, 5, 1),
        ("ordered_8", 8, 0, 1),
        ("overlap_8", 8, 1, 1),
        ("overlap_32", 32, 1, 4),
        ("tied_groups_32", 32, 2, 4),
        ("nested_32", 32, 3, 4),
        ("nested_32_single", 32, 3, 1),
        ("identical_128", 128, 4, 4),
        ("overlap_128", 128, 1, 4),
        ("nested_128", 128, 3, 4),
    ] {
        let attrs = attributes(count, pattern);
        for (variant, walk) in variants {
            let mut scratch = TextAttrScratch::default();
            let walk = black_box(walk);
            crate::perf::measure_sampled(&format!("{name}/{variant}"), 8192, 1, || {
                walk(black_box(&attrs), black_box(&mut scratch), black_box(step));
            });
        }
    }
}
