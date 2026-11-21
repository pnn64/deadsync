# Inactive stride-2 experiments

Prepared against the production scanline sources present at baseline b04e37657. No production source has been edited and no new compilation or measurement was performed to prepare these patches.

## Application order

Both candidates apply independently to the same baseline.

1. Measure the baseline.
2. Apply candidate-a-portable-stride2.patch, validate, and measure A.
3. Remove only candidate A's changes before measuring candidate B.
4. Apply candidate-b-sse41-stride2.patch to the baseline, validate, and measure B.

Candidate B deliberately retains the original portable fallback. It does not depend on A. Their production hunks do not overlap, so a combined A+B experiment is possible later if both independent results justify it; measure that combination separately.

The root agent owns patch application, reversion, validation, and timing. Use git apply --check before applying a candidate. Do not reset unrelated working-tree changes.

## Candidate A

Replace the indexed two-byte Average/Paeth recurrence in scanline.rs with the existing unfilter_pixels::<2, PAETH> helper.

This is safe Rust, uses the same exact predictor rules, keeps two preceding byte lanes in registers, and handles odd final tails through the existing generic helper. The caller already provides a previous row at least as long as the current row. Existing scanline specification-oracle tests cover stride 2, all filters, first rows, short rows, and wrapping arithmetic.

No new unsafe code or instruction requirement is introduced. This change affects stride 2 on every architecture, so it needs portable checks in addition to x86-64 timing.

## Candidate B

Enable N=2 in the existing SSE4.1 Average/Paeth dispatcher. The existing length threshold (64 bytes), runtime SSE4.1 check, checked-array loads/stores, and portable fallback remain in force.

The kernel processes N byte lanes per iteration. For N=2, its four-byte load is used only when at least four bytes remain. It writes only two reconstructed bytes, and the final one or two bytes use the existing checked, zero-padded tail path. Unused upper lanes do not mix into the two live predictor lanes.

Tests added by the patch:

- rows::<2, true>() and rows::<2, false>() compare directly with the independent specification predictor. They cover offsets 0..31, lengths 0..139 plus 255/256/257/511/512/513/4095/4096/4097, four input/previous-row patterns, both parity lengths, wrapping reconstruction, and unchanged prefix/suffix guards.
- averages::<2>() covers every a/b input pair with residual values 0, 1, 127, and 255 against integer division.
- The existing exhaustive Paeth predictor test still covers the same SIMD lane algorithm.

No unchecked memory access, pointer load, or new unsafe block is introduced. Existing SIMD safety comments and target-feature contracts remain applicable. Follow rust-performance.md's unsafe validation requirements before retaining the candidate.

## Representative inputs

candidate-stride2-cases.txt contains repository-relative input paths. All six files were confirmed present.

The first four files have a two-byte PNG filter stride: GrayAlpha8 or Gray16, noninterlaced. Their noisy and patterned variants help distinguish fixed setup costs from reconstruction cost. The icons are small-file controls and are not expected to benefit directly from stride-2 reconstruction.

Archived November 17 run 1 ratios (these are not current measurements):

| File | Native / png cycles | Native / image cycles |
| --- | ---: | ---: |
| color-4-depth-08-adam7-0-noise-1.png | 1.709 | 1.624 |
| color-4-depth-08-adam7-0-noise-0.png | 1.421 | 1.412 |
| color-0-depth-16-adam7-0-noise-1.png | 1.539 | 1.524 |
| color-0-depth-16-adam7-0-noise-0.png | 1.426 | 1.428 |
| icon-16.png | 1.316 | 1.335 |
| icon-32.png | 1.861 | 1.780 |

Source: crates/deadlib-image/png/bench/results/windows-x86_64-20261117-pipelined/timing-run1.tsv. Ratios are rounded to three decimals.

Targeted measurements only discriminate candidates. Final selection still needs the representative full corpus, exact decoded samples/metadata, allocation measurements, and independently repeated runs. The unfilter stage includes cloning inflated data; public decode thread cycles are the deciding measurement.
