//! Exercise the production implementation, including private parsing paths.
pub use deadsync_noteskin::{TweenType, actor, bright_tap_explosion_key, script};
#[path = "../../../tests/support/safe_parsing.rs"]
mod perf;

// This test-only copy omits runtime.rs, which uses the other crate-private paths.
#[expect(
    dead_code,
    reason = "production runtime entry points are not used by this test crate"
)]
pub mod explosion {
    include!("../src/explosion.rs");

    #[test]
    fn explosion_name_boundaries() {
        for len in [0, 1, 60, 61, 62, 63, 64, 65, 128] {
            for unit in ["a", "é", "東京", "🙂"] {
                let base = unit.repeat(len);
                for blank_mask in 0..64_u8 {
                    let mut checked = Vec::new();
                    let actual =
                        itg_direct_tap_explosion_elements(&base, blank_mask & 1 != 0, |key| {
                            checked.push(key.to_owned());
                            let digit = key.as_bytes()[key.len() - 1] - b'0';
                            blank_mask & (1 << digit) != 0
                        });
                    let variants: Vec<_> = (1..=5).map(|i| format!("{base} W{i}")).collect();
                    assert_eq!(checked, variants);
                    let expected: Vec<_> = std::iter::once(base.clone())
                        .chain(variants)
                        .enumerate()
                        .filter(|(i, _)| blank_mask & (1 << i) == 0)
                        .map(|(_, key)| key)
                        .collect();
                    assert_eq!(actual, expected);
                }
            }
        }
    }

    #[test]
    #[ignore = "manual performance measurement"]
    fn safe_bench() {
        use std::hint::black_box;
        for base in [
            "Tap Explosion Dim".to_owned(),
            "Tap Explosion Bright".to_owned(),
            "東京é".to_owned(),
            "x".repeat(61),
            "x".repeat(62),
            "x".repeat(128),
        ] {
            crate::perf::measure(&format!("explosion_visit_{}", base.len()), 6, || {
                for_each_direct_tap_explosion_element(
                    black_box(&base),
                    black_box(false),
                    &mut |key| {
                        black_box(key);
                        false
                    },
                    &mut |key| {
                        black_box(key);
                    },
                );
            });
            crate::perf::measure(&format!("explosion_collect_{}", base.len()), 6, || {
                black_box(itg_direct_tap_explosion_elements(
                    black_box(&base),
                    false,
                    |key| {
                        black_box(key);
                        false
                    },
                ));
            });
        }
    }
}
