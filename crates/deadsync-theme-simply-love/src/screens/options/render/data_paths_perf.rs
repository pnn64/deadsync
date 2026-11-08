use super::*;
use crate::{data_paths_perf_support::compare, perf::measure};
use std::hint::black_box;

include!("data_paths_original.rs");

fn block(text: &str, bullet: bool) -> RenderedHelpBlock {
    if bullet {
        RenderedHelpBlock::Bullet {
            text: Arc::from(text),
            line_count: 100,
        }
    } else {
        RenderedHelpBlock::Paragraph {
            text: Arc::from(text),
            line_count: 100,
        }
    }
}

fn value(block: &RenderedHelpBlock) -> (&str, usize, bool) {
    match block {
        RenderedHelpBlock::Paragraph { text, line_count } => (text, *line_count, false),
        RenderedHelpBlock::Bullet { text, line_count } => (text, *line_count, true),
    }
}

#[test]
fn data_paths_truncation_preserves_line_and_bullet_semantics() {
    let parts = ["", "\n", "\r\n", "\r", "a", "• déjà", "日本語", "\u{2028}"];
    for a in parts {
        for b in parts {
            for c in parts {
                let text = format!("{a}{b}{c}");
                for bullet in [false, true] {
                    for limit in [0, 1, 2, 3, 4, 8, usize::MAX] {
                        let before = original_truncate_help_block(block(&text, bullet), limit);
                        let after = truncate_help_block(block(&text, bullet), limit);
                        assert_eq!(value(&before), value(&after), "{text:?}, {limit}, {bullet}");
                    }
                }
            }
        }
    }
}

#[test]
fn data_paths_truncation_removes_line_vector_and_growth() {
    let input = block("One line\nSecond line\nThird line\nFourth line", true);
    let other = input.clone();
    let (before, old) = measure(|| original_truncate_help_block(input, 4));
    let (after, new) = measure(|| truncate_help_block(other, 4));
    assert_eq!(value(&before), value(&after));
    assert!(new.allocs < old.allocs);
    assert_eq!(new.reallocs, 0);
    assert!(new.allocated_bytes < old.allocated_bytes);
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_data_paths_truncation() {
    for (label, text, limit, bullet) in [
        ("empty", String::new(), 10, false),
        ("ellipsis", "unused\ntext".into(), 1, false),
        ("bullet-ellipsis", "unused\ntext".into(), 1, true),
        (
            "three-lines",
            "One line\nSecond line\nThird line\nFourth line".into(),
            4,
            true,
        ),
        ("crlf", "• déjà\r\n\r\n日本語\r\nlast\r".into(), 4, true),
        (
            "long-tail",
            "A line of wrapped help text\n".repeat(256),
            20,
            false,
        ),
    ] {
        let input = block(&text, bullet);
        compare(
            &format!("truncate/{label}"),
            || {
                black_box(original_truncate_help_block(
                    black_box(&input).clone(),
                    black_box(limit),
                ));
            },
            || {
                black_box(truncate_help_block(
                    black_box(&input).clone(),
                    black_box(limit),
                ));
            },
        );
    }
}
