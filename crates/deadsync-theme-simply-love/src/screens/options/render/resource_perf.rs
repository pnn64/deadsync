// Frozen originals from 3b0f61b73348db057f8d2a189ac86d46da9b6513; paired benchmarks exercise the production functions.
use super::*;
use crate::{perf::measure, resource_perf_support::compare};
use std::hint::black_box;
fn original(blocks: Vec<RenderedHelpBlock>) -> Vec<RenderedHelpBlock> {
    let max_height = DESC_H - DESC_TITLE_TOP_PAD_PX - DESC_BOTTOM_PAD_PX;
    if help_blocks_height(&blocks) <= max_height {
        return blocks;
    }

    let mut fitted = Vec::with_capacity(blocks.len());
    let mut remaining = max_height;
    let block_count = blocks.len();
    for (idx, block) in blocks.into_iter().enumerate() {
        let line_height = match &block {
            RenderedHelpBlock::Paragraph { .. } => DESC_TITLE_LINE_H_PX,
            RenderedHelpBlock::Bullet { .. } => DESC_BODY_LINE_H_PX,
        };
        let paragraph_gap =
            if idx + 1 < block_count && matches!(&block, RenderedHelpBlock::Paragraph { .. }) {
                DESC_BULLET_TOP_PAD_PX
            } else {
                0.0
            };
        let full_height = help_block_height(&block) + paragraph_gap;
        let more_blocks = idx + 1 < block_count;
        let ellipsis_height = DESC_TITLE_LINE_H_PX.max(DESC_BODY_LINE_H_PX);
        let fits = if more_blocks {
            full_height + ellipsis_height <= remaining
        } else {
            full_height <= remaining
        };
        if fits {
            remaining -= full_height;
            fitted.push(block);
            continue;
        }

        let max_lines = (remaining / line_height).floor() as usize;
        if max_lines > 0 {
            fitted.push(truncate_help_block(block, max_lines));
        }
        break;
    }
    fitted
}

fn block(bullet: bool, lines: usize, text: &str) -> RenderedHelpBlock {
    if bullet {
        RenderedHelpBlock::Bullet {
            text: Arc::from(text),
            line_count: lines,
        }
    } else {
        RenderedHelpBlock::Paragraph {
            text: Arc::from(text),
            line_count: lines,
        }
    }
}

#[test]
fn resource_help_matches_original_at_every_height_boundary() {
    for count in [0, 1, 2, 8, 19, 30, 256] {
        for first_lines in 0..=22 {
            for bullet in [false, true] {
                let blocks: Vec<_> = (0..count)
                    .map(|i| {
                        block(
                            if i == 0 { bullet } else { i % 3 != 0 },
                            if i == 0 { first_lines } else { i % 4 },
                            "\r\n\u{2022} \u{e9}\r\n\nlast\r",
                        )
                    })
                    .collect();
                let before = original(blocks.clone());
                let after = fit_help_blocks(blocks);
                assert_eq!(
                    format!("{before:?}"),
                    format!("{after:?}"),
                    "{count}/{first_lines}/{bullet}"
                );
            }
        }
    }
}

#[test]
fn resource_help_retains_buffer_and_removes_output_allocation() {
    for count in [1, 30, 256] {
        let input: Vec<_> = (0..count).map(|_| block(true, 30, "a\nb\nc\nd")).collect();
        let other = input.clone();
        let ptr = other.as_ptr();
        let (before, old) = measure(|| original(input));
        let (after, new) = measure(|| fit_help_blocks(other));
        assert_eq!(format!("{before:?}"), format!("{after:?}"));
        assert_eq!(ptr, after.as_ptr());
        assert_eq!(old.allocs, new.allocs + 1);
        assert_eq!(
            old.allocated_bytes - new.allocated_bytes,
            count * std::mem::size_of::<RenderedHelpBlock>()
        );
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_resources_help() {
    for (count, lines, label) in [
        (0, 1, "empty"),
        (2, 1, "fits"),
        (1, 30, "one-overflow"),
        (30, 1, "30-overflow"),
        (256, 2, "256-overflow"),
    ] {
        let blocks: Vec<_> = (0..count)
            .map(|_| block(true, lines, "a\nb\nc\nd\ne\nf"))
            .collect();
        assert_eq!(
            format!("{:?}", original(blocks.clone())),
            format!("{:?}", fit_help_blocks(blocks.clone()))
        );
        // Both owned-input paths include the same fixture clone and output drop.
        compare(
            &format!("help/{label}"),
            || {
                black_box(original(black_box(&blocks).clone()));
            },
            || {
                black_box(fit_help_blocks(black_box(&blocks).clone()));
            },
        );
    }
}
