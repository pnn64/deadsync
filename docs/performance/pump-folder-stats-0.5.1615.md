# Cache folder stats for pump styles

Baseline: `2806aa4f7` (0.5.1615). Whole-codebase review of per-frame paths.

## Finding and change

Select Music draws a folder-stats overlay every frame while a pack header or a
song inside a pack is selected, if folder stats are enabled and the wheel is
sorted by group or series. `build_folder_stats_summary` memoized its result per
(pack, side, chart type, difficulty), but only for `dance-single` and
`dance-double`. For `pump-single` and `pump-double`, it neither read nor
filled the cache. Each frame it rescanned every chart in the pack, compared
chart types and difficulties as strings, and looked up history for each match.
This happened once per shown side, twice in versus.

The cache now indexes all four play-style chart types. Any other chart type
string still takes the uncached path. Invalidation is unchanged: history
replacement already clears every entry of the widened array. This follows
`rust-performance.md`'s M-HOTPATH and M-THROUGHPUT guidance. No allocation or
dependency is added.

## Behavior validation

The existing memoization test now also checks that pump-single and pump-double
summaries are stored and returned. A temporary benchmark asserted that the
cached result equals a fresh pack scan for a pump pack with passes before
timing either path. All 1,141 non-ignored theme library tests except one pass.
The failure, `cyber_model_tap_scale_uses_model_height_not_logical_height`,
loads the installed cyber noteskin. It fails its model-height regression guard
(logical and model height are both 60.16) in gameplay notefield code that this
change does not touch.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.1 / LLVM 22.1.8, release
profile, pinned to logical CPU 6. The fixture is one pack of 100 songs with
five pump-single charts each; half of the Hard charts have a passing score.

- **Before:** the scan the baseline ran every frame, `score_data::folder_stats_summary`.
- **After:** `build_folder_stats_summary` on a warm cache.
- **Timing:** both variants run in the same binary. Each run takes seven batches
  of 4,096 calls and reports the median. There were three runs, with the order
  reversed in run 2.
- **Allocations:** counted over one call.

## Results

| Variant | Median ns/frame (per side) | Thread cycles | Allocations |
| --- | ---: | ---: | ---: |
| Before (pack scan) | 14,636.9 | 32,050 | 0 |
| After (cache hit) | 24.5 | 54 | 0 |

The work drops about 600-fold per shown side in every run. At 144 frames per
second that is roughly 2 ms of main-thread CPU saved per second per side for a
100-song pack; larger packs save proportionally more. Neither variant
allocates. Raw results are in the [CSV](pump-folder-stats-0.5.1615.csv).
