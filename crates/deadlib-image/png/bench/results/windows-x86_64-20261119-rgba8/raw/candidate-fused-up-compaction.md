# Inactive fused Up reconstruction experiment

candidate-fused-up-compaction.patch adds a safe block helper, one dispatch branch in the existing reuse path, and two focused independent tests. It is independent of the stride-2 candidates. Context comes from the current source with candidate A applied, but the hunks do not touch A or B.

No production source, benchmark, build, test run, or timing is changed by preparing this patch.

## Scope and correctness

Only later Up-filter rows in 8-bit noninterlaced decoding use the helper. The existing complete filtered-row check executes first, preserving truncation-before-filter-validation behavior. First rows, other filters, 16-bit images, Adam7, allocation limits, checksum validation, and final extra-data checks retain their original paths.

The buffer begins at the compacted destination. The input begins source bytes later. A fixed 32-byte local array reads the entire input block before its earlier destination is written. That store ends before the next input block starts, including source gaps smaller than a block. The previous row comes from the separate prefix returned by split_at_mut and is never overwritten. A forward scalar tail handles the last 0..31 bytes. There is no allocation or unsafe code in the helper.

The decoder normally supplies source = row_index + 1. The independent helper test also exercises source 0 and 1, every overlapping shift for lengths 0..95, large gaps, lengths around block boundaries through 1025, offsets 0..31, three wrapping/data patterns, and unchanged surrounding bytes.

The mixed-filter test independently produces known samples using the PNG predictor definitions, then verifies all five color types at depth 8, tiny/wide/tall dimensions, first-row Up, repeated Up, changes to every other filter, and source gaps larger than a row. It checks output limits, extra data, truncation at each row boundary/interior, invalid filters, and truncated-invalid-row error precedence.

## Measurement target

asset-23.png / assets/graphics/step_stats_gifs/catjam 11x14.png has 1899 Up rows of 2660 (71.4%), 19,719,216 bytes (18.8 MiB) of row payloads. Those rows currently require an add pass followed by copy_within. This candidate writes their reconstructed result directly into the compacted output, removing one full row read/write pass.

asset-22.png has 122 Up rows of 512 (23.8%). kodim18.png has only 12 Up rows of 768 and is a useful low-effect control. First-row Up still follows the existing path.

Full native decoding and equivalent RGBA8 consumption must improve before retaining the change. The stage benchmark includes an inflated-input clone and should be interpreted accordingly. No performance gain is claimed before root's isolated checks and measurements.
