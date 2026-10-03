use std::hint::black_box;
include!("time_signatures/baseline.rs");

fn signature(beat: f32, index: usize) -> TimeSignatureSegment {
    TimeSignatureSegment {
        beat,
        numerator: (index % 7) as i32,
        denominator: (index % 5) as i32,
    }
}

fn assert_equal(old: &[TimeSignatureSegment], new: &[TimeSignatureSegment]) {
    assert_eq!(old.len(), new.len());
    for (old, new) in old.iter().zip(new) {
        assert_eq!(
            (old.beat.to_bits(), old.numerator, old.denominator),
            (new.beat.to_bits(), new.numerator, new.denominator)
        );
    }
}

#[test]
fn signatures_match_original_sorting_and_default_insertion() {
    for beats in [
        vec![],
        vec![0.0],
        vec![-0.0, 0.0, 4.0, 4.0],
        vec![0.001, 4.0],
        vec![1.0, 4.0],
        vec![4.0, -2.0, 0.0, 4.0],
        vec![f32::NAN, f32::NEG_INFINITY, -0.0, 0.0, f32::INFINITY],
        vec![f32::from_bits(0xffc00001), f32::from_bits(0x7fc00002)],
    ] {
        let segments = TimingSegments {
            time_signatures: beats
                .iter()
                .enumerate()
                .map(|(i, &beat)| signature(beat, i))
                .collect(),
            ..TimingSegments::default()
        };
        let before = format!("{:?}", segments.time_signatures);
        assert_equal(
            &original_time_signatures(&segments),
            &normalized_time_signatures(&segments),
        );
        assert_eq!(before, format!("{:?}", segments.time_signatures));
    }
}

#[test]
fn normalized_signatures_borrow_input_without_churn() {
    for count in [0, 1, 128] {
        let segments = TimingSegments {
            time_signatures: (0..count).map(|i| signature(i as f32 * 4.0, i)).collect(),
            ..TimingSegments::default()
        };
        let normalized = normalized_time_signatures(&segments);
        assert!(matches!(normalized, Cow::Borrowed(_)));
        if count > 0 {
            assert_eq!(normalized.as_ptr(), segments.time_signatures.as_ptr());
        }
        crate::perf::assert_no_churn(|| {
            black_box(normalized_time_signatures(&segments));
        });
        crate::perf::assert_reduced_churn(
            || {
                black_box(original_time_signatures(&segments));
            },
            || {
                black_box(normalized_time_signatures(&segments));
            },
        );
    }
}

#[test]
fn background_rows_match_with_borrowed_and_original_signatures() {
    let segments = TimingSegments {
        bpms: (0..64).map(|i| (i as f32 * 4.0, 120.0)).collect(),
        time_signatures: vec![signature(0.0, 3), signature(24.0, 5)],
        ..TimingSegments::default()
    };
    let old = original_time_signatures(&segments);
    let new = normalized_time_signatures(&segments);
    let run = |sigs: &[TimeSignatureSegment]| {
        let mut cycle =
            MovieCycle::new(vec![PathBuf::from("a.avi"), PathBuf::from("b.avi")], "song");
        let mut used_rows = UsedRows::new(-48, 16384, 64);
        let mut expansion = RandomExpansion {
            timing_segments: &segments,
            time_sigs: sigs,
            used_rows: &mut used_rows,
            cycle: &mut cycle,
        };
        let mut out = Vec::new();
        let mut template = SongBackgroundChange::new(0.0, SongBackgroundChangeTarget::Random);
        template.effect = "effect".into();
        template.transition = "transition".into();
        push_random_segment(&mut out, -1.0, 256.0, &mut expansion, &template);
        format!("{out:?}")
    };
    assert_eq!(run(&old), run(&new));
}

#[test]
#[ignore = "manual original/current time signature normalization benchmark; run in release"]
fn signature_normalization_benchmark() {
    for (count, shape) in [
        (0, "empty"),
        (1, "sorted"),
        (128, "sorted"),
        (128, "reversed"),
        (128, "positive"),
    ] {
        let mut signatures: Vec<_> = (0..count)
            .map(|i| signature((i + usize::from(shape == "positive")) as f32 * 4.0, i))
            .collect();
        if shape == "reversed" {
            signatures.reverse();
        }
        let segments = TimingSegments {
            time_signatures: signatures,
            ..TimingSegments::default()
        };
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for new in order {
            let variant = if new { "new" } else { "old" };
            let label = format!("signatures/{count}/{shape}/{variant}");
            if new {
                crate::perf::measure_sampled(&label, 8192, 1, || {
                    normalized_time_signatures(black_box(&segments))
                });
            } else {
                crate::perf::measure_sampled(&label, 8192, 1, || {
                    original_time_signatures(black_box(&segments))
                });
            }
        }
    }
}
