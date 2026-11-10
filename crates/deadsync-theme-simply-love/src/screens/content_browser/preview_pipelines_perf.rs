use super::*;


fn chart(difficulty: &str, doubles: bool) -> PreviewChart {
    PreviewChart {
        difficulty: difficulty.into(),
        doubles,
        meter: 12,
        lanes: if doubles { 8 } else { 4 },
        notes: Arc::from([]),
        holds: Arc::from([]),
    }
}


#[test]
fn preview_selection_keeps_priority_case_and_fallbacks() {
    for word in ["challenge", "expert"] {
        for mask in 0..1 << word.len() {
            let mixed: String = word
                .bytes()
                .enumerate()
                .map(|(i, byte)| {
                    char::from(if mask & (1 << i) != 0 {
                        byte.to_ascii_uppercase()
                    } else {
                        byte
                    })
                })
                .collect();
            for doubles in [false, true] {
                let charts = [
                    chart("Hard", false),
                    chart(&mixed, doubles),
                    chart("Edit", false),
                ];
                assert_eq!(default_chart(&charts), Some(1));

            }
        }
    }
    let charts = [
        chart("Expert", true),
        chart("Challenge", false),
        chart("Expert", false),
    ];
    assert_eq!(default_chart(&charts), Some(1));
    assert_eq!(default_chart(&[]), None);
    assert_eq!(
        default_chart(&[chart("Edit", true), chart("Hard", true)]),
        Some(1)
    );
}
