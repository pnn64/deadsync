use super::super::{merge_routine, project};
use super::{cases, chart, compare, simfile};
use std::borrow::Cow;
use std::hint::black_box;

#[path = "projection_original.rs"]
mod original;

fn mixed_file() -> Vec<u8> {
    let mut data = Vec::new();
    for (kind, width) in [
        ("unsupported", 8),
        ("dance-single", 4),
        ("dance-couple", 8),
        ("dance-routine", 8),
    ] {
        data.extend_from_slice(&simfile(kind, &chart(width, 2, 16, 2), 4, "sm"));
    }
    data
}

fn check_projection(data: &[u8], entries: &[rssp::parse::ParsedChartEntry<'_>]) {
    let old = original::project(data, entries);
    let new = project(data, entries);
    assert_eq!(new.bytes, old.bytes);
    assert_eq!(new.owners, old.owners);
    assert_eq!(new.hash_notes, old.hash_notes);
    assert_eq!(
        matches!(new.bytes, Cow::Borrowed(_)),
        matches!(old.bytes, Cow::Borrowed(_))
    );
    if matches!(new.bytes, Cow::Borrowed(_)) {
        assert_eq!(new.bytes.as_ptr(), data.as_ptr());
    }
}

#[test]
fn partner_dispatch_preserves_mixed_ordinals_and_field_count_boundaries() {
    let data = mixed_file();
    for count in [0, 4, 5, u8::MAX] {
        let mut parsed = rssp::parse::extract_sections(&data, "sm").unwrap();
        assert_eq!(parsed.notes_list.len(), 16);
        for (index, entry) in parsed.notes_list.iter_mut().enumerate() {
            if index.is_multiple_of(3) {
                entry.field_count = count;
            }
        }
        check_projection(&data, &parsed.notes_list);
    }
    let parsed = rssp::parse::extract_sections(&data, "sm").unwrap();
    let projection = project(&data, &parsed.notes_list);
    assert!(!projection.owners.is_empty());
    assert!(
        projection
            .owners
            .keys()
            .all(|&(chart, _, _)| (8..12).contains(&chart))
    );
}

#[test]
fn generated_routine_hash_inputs_preserve_every_grid_and_owner() {
    for (index, (body, width)) in cases().into_iter().enumerate() {
        let extension = ["sm", "ssc"][index % 2];
        let kind = match (width, index % 3) {
            (8, 0) => "DANCE-ROUTINE",
            (8, 1) => " dance-routine ",
            (8, _) => "dance-routine",
            (_, 0) => "PUMP-ROUTINE",
            (_, 1) => " pump-routine ",
            _ => "pump-routine",
        };
        let data = simfile(kind, &body, 1, extension);
        let parsed = rssp::parse::extract_sections(&data, extension).unwrap();
        check_projection(&data, &parsed.notes_list);
    }
}

#[test]
fn comment_bypass_preserves_empty_parts_and_trailing_measure_boundaries() {
    for width in [8, 10] {
        for prefix in [
            b"".as_slice(),
            b"1000000000",
            b"2000000000\n3000000000",
            b"1000000000\n&",
            b"// & comment",
            b"x/y",
            b"1000000000\n// ",
            b" \t\r",
        ] {
            for suffix in [
                b"".as_slice(),
                b"\n",
                b"\r",
                b",",
                b"&",
                b" \t",
                b",\n",
                b"&\n",
                b"//ignored",
                b"\n//&\n",
            ] {
                let data = [prefix, suffix].concat();
                assert_eq!(
                    merge_routine(&data, width),
                    original::merge_routine(&data, width),
                    "{data:?}"
                );
            }
        }
    }
    assert_eq!(merge_routine(b"", 8).0.len(), 192 * 9);
}

fn compare_project(label: &str, data: &[u8], iterations: usize) {
    let parsed = rssp::parse::extract_sections(data, "sm").unwrap();
    compare(label, iterations, |current| {
        if current {
            black_box(project(black_box(data), black_box(&parsed.notes_list)));
        } else {
            black_box(original::project(
                black_box(data),
                black_box(&parsed.notes_list),
            ));
        }
    });
}

#[test]
#[ignore = "optimized paired CPU and allocation benchmark"]
fn benchmark_partner_hashes() {
    let body = chart(8, 2, 16, 2);
    compare_project(
        "project/couple/16",
        &simfile("dance-couple", &body, 16, "sm"),
        1_000,
    );
    compare_project(
        "project/native/16",
        &simfile("dance-single", &chart(4, 2, 16, 2), 16, "sm"),
        1_000,
    );
    compare_project("project/mixed/16", &mixed_file(), 15);
    for (label, pattern) in [
        ("project/routine_sparse/4", 0),
        ("project/routine_dense/4", 1),
        ("project/routine_holds/4", 2),
    ] {
        compare_project(
            label,
            &simfile("dance-routine", &chart(8, 2, 16, pattern), 4, "sm"),
            15,
        );
    }
    let plain = chart(8, 16, 48, 0);
    let mut commented = Vec::new();
    for line in plain.split(|&b| b == b'\n') {
        commented.extend_from_slice(line);
        commented.extend_from_slice(b" // comment containing & and enough text to expose excessive input-sized reservations\n");
    }
    let mut trailing = plain.clone();
    trailing.push(b',');
    for (label, data) in [
        ("merge/plain/16", &plain),
        ("merge/comments/16", &commented),
        ("merge/trailing/16", &trailing),
    ] {
        compare(label, 15, |current| {
            black_box(if current {
                merge_routine(black_box(data), 8)
            } else {
                original::merge_routine(black_box(data), 8)
            });
        });
    }
}
