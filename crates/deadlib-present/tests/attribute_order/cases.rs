use super::*;

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
                let mut new = TextAttrCursor::new(&attrs, &mut new_scratch);
                assert_eq!(new.is_some(), !attrs.is_empty());
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
                    for cursor in [&mut new] {
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
