use super::super::project;
use super::{chart, simfile};

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


#[test]
fn partner_dispatch_preserves_mixed_ordinals_and_field_count_boundaries() {
    let data = mixed_file();
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
