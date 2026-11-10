use super::*;

fn assert_decimal_groups(value: usize) {
    let text = commify(value);
    assert_eq!(text.replace(',', ""), value.to_string());
    let groups: Vec<_> = text.split(',').collect();
    assert!((1..=3).contains(&groups[0].len()));
    assert!(groups.iter().skip(1).all(|group| group.len() == 3));
}

#[test]
fn comma_formatting_preserves_decimal_boundaries_and_large_values() {
    for (n, expected) in [
        (0, "0"),
        (999, "999"),
        (1000, "1,000"),
        (1_234_567, "1,234,567"),
    ] {
        assert_eq!(commify(n), expected);
    }
    for n in 0..20_000 {
        assert_decimal_groups(n);
    }
    let mut value = 1usize;
    loop {
        for n in [value - 1, value, value.saturating_add(1)] {
            assert_decimal_groups(n);
        }
        let Some(next) = value.checked_mul(10) else {
            break;
        };
        value = next;
    }
    let mut random = 0x1234_5678_9abc_def0u64;
    for _ in 0..10_000 {
        random = random
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let n = random as usize;
        assert_decimal_groups(n);
    }
    assert_decimal_groups(usize::MAX);
}
