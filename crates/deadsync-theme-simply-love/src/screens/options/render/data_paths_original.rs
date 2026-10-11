// Frozen from b4d4a8b49c21b96b3be66cf05cdfda37b74f9b2f; only names changed.
fn original_truncate_help_block(block: RenderedHelpBlock, max_lines: usize) -> RenderedHelpBlock {
    let (text, is_bullet) = match block {
        RenderedHelpBlock::Paragraph { text, .. } => (text, false),
        RenderedHelpBlock::Bullet { text, .. } => (text, true),
    };
    let mut truncated = text
        .lines()
        .take(max_lines.saturating_sub(1))
        .collect::<Vec<_>>()
        .join("\n");
    if !truncated.is_empty() {
        truncated.push('\n');
    }
    if is_bullet && max_lines == 1 {
        truncated.push_str("\u{2022} ...");
    } else {
        truncated.push_str("...");
    }
    let text = Arc::from(truncated);
    if is_bullet {
        RenderedHelpBlock::Bullet {
            text,
            line_count: max_lines,
        }
    } else {
        RenderedHelpBlock::Paragraph {
            text,
            line_count: max_lines,
        }
    }
}
