use super::*;
use crate::metadata_perf::{compare, measure};
use std::hint::black_box;
mod original {
    use super::*;
    include!("timing_parse_original.rs");
}

fn old_signatures(tag: Option<&str>) -> Vec<TimeSignatureSegment> {
    original::parse_time_signatures_as(
        tag,
        |beat, numerator, denominator| TimeSignatureSegment {
            beat,
            numerator,
            denominator,
        },
        |s| s.beat,
    )
}
fn old_ticks(tag: Option<&str>) -> Vec<TickcountSegment> {
    original::parse_tickcounts_as(
        tag,
        |beat, ticks| TickcountSegment { beat, ticks },
        |s| s.beat,
    )
}
fn old_combos(tag: Option<&str>) -> Vec<ComboSegment> {
    original::parse_combos_as(
        tag,
        |beat, combo, miss_combo| ComboSegment {
            beat,
            combo,
            miss_combo,
        },
        |s| s.beat,
    )
}

fn assert_parity(tag: Option<&str>) {
    let old = old_signatures(tag);
    let new = parse_time_signatures(tag);
    assert_eq!(
        old.iter()
            .map(|s| (s.beat.to_bits(), s.numerator, s.denominator))
            .collect::<Vec<_>>(),
        new.iter()
            .map(|s| (s.beat.to_bits(), s.numerator, s.denominator))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        parse_cached_time_signatures(tag)
            .iter()
            .map(|s| (s.0.to_bits(), s.1, s.2))
            .collect::<Vec<_>>(),
        old.iter()
            .map(|s| (s.beat.to_bits(), s.numerator, s.denominator))
            .collect::<Vec<_>>()
    );
    let old = old_ticks(tag);
    let new = parse_tickcounts(tag);
    assert_eq!(
        old.iter()
            .map(|s| (s.beat.to_bits(), s.ticks))
            .collect::<Vec<_>>(),
        new.iter()
            .map(|s| (s.beat.to_bits(), s.ticks))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        parse_cached_tickcounts(tag)
            .iter()
            .map(|s| (s.0.to_bits(), s.1))
            .collect::<Vec<_>>(),
        old.iter()
            .map(|s| (s.beat.to_bits(), s.ticks))
            .collect::<Vec<_>>()
    );
    let old = old_combos(tag);
    let new = parse_combos(tag);
    assert_eq!(
        old.iter()
            .map(|s| (s.beat.to_bits(), s.combo, s.miss_combo))
            .collect::<Vec<_>>(),
        new.iter()
            .map(|s| (s.beat.to_bits(), s.combo, s.miss_combo))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        parse_cached_combos(tag)
            .iter()
            .map(|s| (s.0.to_bits(), s.1, s.2))
            .collect::<Vec<_>>(),
        old.iter()
            .map(|s| (s.beat.to_bits(), s.combo, s.miss_combo))
            .collect::<Vec<_>>()
    );
}

#[test]
fn timing_sort_preserves_row_collisions_defaults_and_stable_winners() {
    for tag in [
        None,
        Some(""),
        Some(" \t "),
        Some("bad,NaN=1=1,inf=2=3"),
        Some("0=8=4,0=16=8,-0=24=16,0.001=32=32,-0.001=48=48"),
        Some("3.4028235e38=3=4,1e30=5=6,-3.4028235e38=7=8,-1e30=9=10"),
        Some("2=8=4,1.001=6=7,1=4=5,2.001=12=13,1.001=16=17"),
        Some("4=+8.25=+16.5, 8=-2=-3, -1=0=4, 2=2147483648=8"),
    ] {
        assert_parity(tag);
    }
}

#[test]
fn timing_sort_matches_original_across_generated_float_bits() {
    let mut seed = 0x518a6931u32;
    for _ in 0..128 {
        let mut fields = Vec::new();
        for i in 0..257 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let beat = f32::from_bits(seed);
            fields.push(format!("{beat}={}={}", i % 49, i % 17 + 1));
        }
        assert_parity(Some(&fields.join(",")));
        fields.reverse();
        assert_parity(Some(&fields.join(",")));
    }
}

#[test]
fn finite_beat_comparator_matches_row_first_order_at_rounding_and_saturation_edges() {
    let mut values = vec![
        -f32::MAX,
        -1e30,
        -0.0,
        0.0,
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        1e30,
        f32::MAX,
    ];
    for row in [-2147483647i32, -1024, -48, -1, 0, 1, 48, 1024, 2147483647] {
        let beat = (row as f32 + 0.5) / 48.0;
        values.extend([beat.next_down(), beat, beat.next_up()]);
    }
    let mut seed = 0x739115adu32;
    for _ in 0..1024 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let value = f32::from_bits(seed);
        if value.is_finite() {
            values.push(value);
        }
    }
    for &a in &values {
        for &b in &values {
            assert_eq!(
                beat_to_note_row(a)
                    .cmp(&beat_to_note_row(b))
                    .then_with(|| a.total_cmp(&b)),
                a.total_cmp(&b)
            );
        }
    }
}

fn fixture(count: usize, order: &str) -> String {
    let mut fields: Vec<_> = (0..count)
        .map(|i| {
            let beat = if order == "duplicates" {
                (i / 8) as f32 + (i % 4) as f32 * 0.001
            } else {
                i as f32 * 0.25
            };
            format!("{beat}={}={}", i % 16 + 1, i % 8 + 1)
        })
        .collect();
    if order == "reverse" {
        fields.reverse();
    }
    if order == "shuffled" {
        let mut seed = 0x74917933usize;
        for i in (1..fields.len()).rev() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            fields.swap(i, seed % (i + 1));
        }
    }
    fields.join(",")
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture"]
fn benchmark_parse_timing_sort() {
    for (count, order) in [
        (0, "sorted"),
        (1, "sorted"),
        (16, "sorted"),
        (256, "shuffled"),
        (4096, "sorted"),
        (4096, "reverse"),
        (4096, "duplicates"),
    ] {
        let tag = fixture(count, order);
        macro_rules! bench {
            ($kind:literal,$old:ident,$new:ident) => {{
                let label = format!("timing/{}/{count}/{order}", $kind);
                let (_, a) = measure(|| $old(Some(&tag)));
                let (_, b) = measure(|| $new(Some(&tag)));
                println!("ALLOC {label}: original {a:?}, current {b:?}");
                compare(
                    &label,
                    4,
                    || {
                        black_box($old(Some(black_box(&tag))));
                    },
                    || {
                        black_box($new(Some(black_box(&tag))));
                    },
                );
            }};
        }
        bench!("signatures", old_signatures, parse_time_signatures);
        bench!("ticks", old_ticks, parse_tickcounts);
        bench!("combos", old_combos, parse_combos);
    }
}
