# Asset dimension and filename query performance

Baseline: `79bc83980`. Three safe-Rust changes preserve dimensions, errors and filename semantics.

1. GIF dimension queries rewind the file already used for the signature and pass it directly to the GIF reader. The shared reader helper retains the reopen fallback when seeking fails. The old path-opening GIF wrapper now exists only in tests. Normal queries remove one file open, one allocation and one free.

2. Multiframe filename scans handle an immediately adjacent space directly and use `memchr` to skip longer gaps. The digit and extension rules retain their original order. This removes byte-by-byte traversal across long filename sections.

3. Texture-hint scanning compares each constant keyword inside its matching branch. This removes the runtime `(needle slice, flag)` pair and the indirect comparison after the match. Constant lengths let LLVM use packed comparisons. Overlapping keywords and every flag bit are preserved. The scanner shrinks from 35 to 29 lines; the tuned multiframe scanner grows from 30 to 42 lines.

## Separate measurements

CPU columns are median cycles per query or per filename, averaging each filename batch. Throughput is before/after CPU. Filenames use 982 shipped names, six short names, two long names, or two space-heavy/keyword-heavy names.

| Change / workload | Before cycles | After cycles | CPU throughput |
|---|---:|---:|---:|
| gif/1 | 379261.70 | 197119.59 | 1.924x |
| gif/256 | 362044.69 | 190714.63 | 1.898x |
| multiframe/long | 2874.08 | 742.52 | 3.871x |
| multiframe/shipped | 241.22 | 154.07 | 1.566x |
| multiframe/short | 58.30 | 56.88 | 1.025x |
| multiframe/spaces | 872.86 | 712.52 | 1.225x |
| flags/keywords | 823.12 | 519.60 | 1.584x |
| flags/long | 7973.17 | 4944.75 | 1.612x |
| flags/shipped | 685.61 | 540.29 | 1.269x |
| flags/short | 72.02 | 54.59 | 1.319x |

GIF allocations fall from **6 to 5** per call, with 226 fewer requested bytes in these fixtures. The byte saving depends on the temporary path length. Scanner workloads remain allocation free.

## Fresh combined comparison

| Workload | Before cycles | After cycles | CPU throughput |
|---|---:|---:|---:|
| bmp/1 | 405068.38 | 401378.04 | 1.009x |
| bmp/256 | 368094.64 | 376269.30 | 0.978x |
| error/1 | 387949.09 | 394793.04 | 0.983x |
| gif/1 | 377382.23 | 197726.62 | 1.909x |
| gif/256 | 369931.24 | 189167.85 | 1.956x |
| jpg/32 | 478916.44 | 474435.54 | 1.009x |
| multiframe/long | 2991.55 | 796.54 | 3.756x |
| multiframe/shipped | 255.02 | 160.30 | 1.591x |
| multiframe/short | 55.45 | 57.73 | 0.960x |
| multiframe/spaces | 878.86 | 728.81 | 1.206x |
| png/1 | 492249.32 | 479929.91 | 1.026x |
| png/256 | 416720.74 | 413762.63 | 1.007x |

The tiny filename mix varied from 0.924x to 1.025x across separate gates, and is 0.960x in the fresh combined comparison. Its few calls are sensitive to call and branch overhead. Shipped names improve 1.57-1.59x in the complete dimension-query harness; long and space-heavy names also improve.

An initial `memchr_iter` candidate was rejected: it improved shipped names but made space-heavy input about twice as expensive and short input slower. Its `iter` and `iter-final` measurements remain archived. The `spaces-near` file records the tuned scanner in a smaller standalone harness; `spaces-final` and `final` measure the retained source in the complete harness.

## Method and limits

Windows x86-64 on Xeon E5-2696 v4; rustc 1.99.1, generic CPU, opt-level 3, fat LTO, one codegen unit, static CRT. Dependency rlib hashes are fixed and verified. Benchmarks compile complete extracted production functions; they are focused kernels, not whole-application or renderer measurements.

Each measurement pins its thread to CPU 2 and uses QueryThreadCycleTime through the existing benchmark helper. Each process takes twelve batches, discards warmup and reports the median of eleven. Three before/after process pairs alternate execution order. Fixture creation and file encoding occur outside measurements. Files are warm. Other workspace jobs can affect clocks and scheduling; wall time is archived but does not drive retention.

The GIF and space-scan gates share unchanged dimension-query controls. The keyword scanner is measured separately without image codec dependencies. Requested allocation counts use the existing System-backed test allocator. There is no new unsafe production code or cache, and no claimed frame-rate gain.

## Correctness and audit

- `cargo test -p deadlib-assets --locked`: **121 passed, 22 ignored, zero failures** on the final source.
- `cargo clippy -p deadlib-assets --locked --lib --tests --no-deps`: passed; existing codec, DSL, frozen-reference and duplicate-test-module warnings remain.
- New dimension checks compare values, error discriminants and error text across PNG/BMP/JPEG content, six extension forms, truncations, GIF logical canvases and every GIF truncation boundary, invalid files, missing paths and directories. Existing real-file/pipe rewind tests pass in the full suite.
- New filename checks compare shipped names, case changes, path separators, malformed fields, Unicode and 25,920 composed names against a frozen scanner.
- Keyword checks compare exact flag bits for each keyword, inserted bytes 0-255 at every position, uppercase variants, adjacent keywords, overlaps, shipped names and 20,000 generated byte sequences.
- Every archived binary, generated snapshot body and dependency hash was audited. Retained function hashes match current production functions. Frozen scanner bodies match the baseline after formatting; frozen dimension code substitutes only the equivalent signature-opening helper.
- New Rust files and modified function bodies pass rustfmt; Python scripts compile; the scoped patch passes whitespace checks.

## Reproduction

`scripts/benchmark_asset_metadata.py` archives build arguments, dependency hashes, source hashes, extracted function hashes and binary hashes. `final.patch` records the production changes from the baseline; apply it with `git apply --unidiff-zero`. For baseline/individual gates, use the corresponding recorded production bodies with these committed test fixtures.

```powershell
python scripts/benchmark_asset_metadata.py build final --dependencies docs/performance/2026-10-26-asset-metadata-dependencies.json
python scripts/benchmark_asset_metadata.py build final --flags --dependencies docs/performance/2026-10-26-asset-metadata-dependencies.json
python scripts/benchmark_asset_metadata.py build scalar --spaces --frozen --dependencies docs/performance/2026-10-26-asset-metadata-dependencies.json
python scripts/benchmark_asset_metadata.py build tuned --spaces --dependencies docs/performance/2026-10-26-asset-metadata-dependencies.json
python scripts/benchmark_asset_metadata.py compare --spaces --before target/asset-metadata-spaces/scalar.exe --after target/asset-metadata-spaces/tuned.exe --output target/asset-metadata-spaces/repeat.tsv --runs 3
```

Raw cycle/allocation rows are in the neighboring TSV files; comparison JSON files contain the exact binary hashes and their frozen-check output. Build JSON and `audit.json` link those results to source bodies. Temporary executables, generated Rust snapshots and per-run logs remain under `target/asset-metadata*`.
