# Asset filename metadata CPU measurements - 2026-10-26

Three changes in `deadlib-assets` preserve the public API and filename semantics:

1. `parse_texture_resolution_hint` searches for opening parentheses with `memchr`,
   using a scalar scan below 16 remaining bytes. It folds the empty-parenthetical
   branch into the parse guard and removes redundant cursor clamps.
2. `parse_sprite_sheet_dims` skips spans without `(`, `x`, or `X` with `memchr3`.
   Adjacent candidates stay on the direct path and short tails use a scalar scan.
   It reuses `hint_starts_with`, removes `is_res_tag`, and drops `left < i`, which
   is guaranteed by the preceding digit check.
3. `texture_hint_doubleres` compares the first eight bytes in a safe `u64` plus
   the ninth byte, folding ASCII case by setting bit 5. Every target character is
   an ASCII letter, so only its upper/lowercase pair can match. It removes the
   generic substring helper and the unnecessary Unicode trim: a matching word
   cannot overlap whitespace removed by trimming.

No production `unsafe`, dependencies, public signatures, or allocations were added.
Two private helpers were removed. Excluding the three-line test-module declaration,
`lib.rs` has 31 inserted and 31 removed lines. The extra scan guards prevent the
short/dense regressions seen in the initial plain-vector-search experiment.

## Separate before/after gates

Each change was built, checked, and benchmarked before proceeding to the next.
These are medians of three process measurements; each process reports the median
of eleven measured batches after warmup. CPU throughput is before/after cycles.
All values below are **CPU cycles per filename**, including the call/loop overhead.

| Isolated change, 982 shipped image paths | Before | After | CPU throughput |
| --- | ---: | ---: | ---: |
| Resolution scan | 135.54 | 59.41 | 2.281x |
| Sprite-grid scan | 233.17 | 71.58 | 3.257x |
| Doubleres scan | 181.21 | 82.71 | 2.191x |

The raw per-step measurements include unmodified routines as controls:
[resolution](2026-10-26-asset-path-resolution.tsv),
[sprite grids](2026-10-26-asset-path-sheet.tsv), and
[doubleres](2026-10-26-asset-path-double.tsv).

## Original versus combined result

This is a fresh alternating comparison of the original and final archives,
separate from the three attribution gates above.

| Workload | Before | After | CPU throughput |
| --- | ---: | ---: | ---: |
| TextureStore::bind_texture, shipped | 419.31 | 241.98 | 1.733x |
| Source/frame dimensions plus full hint parsing, shipped | 1267.54 | 886.64 | 1.430x |
| Resolution, shipped | 137.58 | 49.06 | 2.804x |
| Sprite grids, shipped | 224.97 | 63.86 | 3.523x |
| Doubleres, shipped | 178.25 | 79.29 | 2.248x |
| Combined, short | 142.76 | 141.54 | 1.009x |
| Resolution, short | 22.88 | 25.18 | 0.909x |
| Sprite grids, short | 22.85 | 23.70 | 0.964x |
| Doubleres, short | 19.86 | 5.74 | 3.458x |
| Combined, dense candidates | 450.59 | 444.95 | 1.013x |
| Combined, long absent hints | 4000.67 | 2733.02 | 1.464x |
| Combined, long present hints | 4564.36 | 3285.81 | 1.389x |

Every allocation probe in all four comparisons records zero allocations,
reallocations, frees, requested bytes, freed bytes, and peak live bytes for both
versions. Fixture construction and store population are outside those probes and
outside timing. Allocation behavior is preserved; there is no allocation saving.

Small-input results are mixed: the final resolution scan costs about 2.30 extra
cycles per short filename, and the sprite scan about 0.85 extra cycles. Combined
short-input throughput is essentially unchanged. Unmodified controls also move
between archives, so small differences should not be treated as precise isolated
effects. All 504 process/workload measurements are retained, including regressions,
in the four TSVs; [final results](2026-10-26-asset-path-final.tsv).

## Method and scope

- Windows x86-64, Intel Xeon E5-2696 v4, 22 cores / 44 logical processors.
- rustc 1.99.1 (`58cf8f9c3`, LLVM 23.1.1), optimization level 3, full LTO,
  static CRT, generic CPU target, matching the repository release settings.
- Four full crate unit-test binaries: baseline, resolution, sheet, double.
  Direct `rustc` invocations reuse Cargo's release dependencies to avoid the
  shared Cargo lock. Exact successful arguments are in the shared
  [build arguments](2026-10-26-asset-path-build-args.json); append the `argv_suffix`
  from each of the four `.build.json` files, which also record source hashes. Dependency hashes are recorded in
  [dependencies](2026-10-26-asset-path-dependencies.json).
- `QueryThreadCycleTime` measures current-thread CPU cycles. The measuring thread
  is pinned to logical processor 2. Archive order alternates across three runs.
  Nanoseconds are also retained, but decisions use CPU cycles and allocations.
- The frozen corpus contains all 982 repository image paths: mean 60.35 UTF-8
  bytes, range 23-88 bytes; 390 contain `doubleres`, 431 contain parentheses.
  These are repository-relative paths including `assets/`, not a measured
  distribution of every runtime key or request. Short, long present/absent,
  and dense-candidate synthetic inputs provide additional coverage.
- The combined parser workload calls `texture_source_frame_dims_from_real` and
  `parse_texture_hints`. The binding workload calls the actual populated store.
  Neither is a full decode, startup, or game-frame benchmark. Other work ran on
  the machine during this session; alternating CPU measurements reduce, but do
  not eliminate, cache, scheduling, and frequency effects.

## Correctness and verification

The frozen reference bodies match starting commit
`434ea40298d3339611ebf4d197a6670f3f8247ba` apart from formatting. Every measured
archive passes the same three differential checks, covering all corpus names,
case/slash variants, nested/unclosed parentheses, malformed tags, integer overflow,
Unicode, dense candidates, and candidate offsets 0-127. This includes 262,144
Latin-1 scalar/offset cases, 20,000 generated token strings, 3,364 token combinations, and
2,946 corpus/case/slash variants. Existing malformed-tag behavior is retained.

Public integration checks cover all 512 case combinations, offsets 0-63, every
U+0000-U+00FF scalar substituted at each target position, and texture binding order for resolution,
doubleres, and sheet sizing. The packed comparison uses safe fixed-size windows;
its eight-byte prefix always exists and requires no aligned load or pointer cast.

Validation:

- `cargo test -p deadlib-assets --locked`: **110 passed**, 16 ignored, 0 failed,
  plus doc tests; [log](2026-10-26-asset-path-cargo-tests.log).
- Original and final full-LTO archives: **99 passed** each, 14 ignored, 0 failed;
  [baseline](2026-10-26-asset-path-baseline-tests.log),
  [final](2026-10-26-asset-path-double-suite.log).
- `cargo clippy -p deadlib-assets --locked --lib --tests --no-deps`: passed.
  Existing warnings concern duplicate benchmark modules, decoder code,
  `present_dsl`, and frozen image-decoder references; [log](2026-10-26-asset-path-clippy.log).
- Scoped rustfmt checks for all changed functions and new Rust files,
  Python compilation, and `git diff --check`: passed. Existing unrelated
  formatting and worktree edits were left alone.
- All three patches were replayed from the starting source plus the unchanged
  test module. Each stage matches its compiled source SHA-256. The harness,
  reference, corpus, and shared support hashes match across all four builds.
  [Audit](2026-10-26-asset-path-audit.json).

## Reproduction

Keep the final test module, reference, corpus, and support files fixed. Start with
`lib.rs` from `434ea40298d3339611ebf4d197a6670f3f8247ba` and insert
`#[cfg(test)] mod path_scan_perf;` after `mod perf_fixture;` with the same blank
lines as this commit. Build/archive the baseline, then apply the
[resolution patch](2026-10-26-asset-path-resolution.patch),
[sheet patch](2026-10-26-asset-path-sheet.patch), and
[double patch](2026-10-26-asset-path-double.patch) in order with `git apply --unidiff-zero`, building/archive after
each. Use `cargo test -p deadlib-assets --release --lib --no-run --locked`, or the
recorded direct build arguments with `CARGO_MANIFEST_DIR` set to the crate path.

For each adjacent pair (and baseline/final), run:

```powershell
python scripts/benchmark_asset_paths.py --before BEFORE.exe --after AFTER.exe --output results.tsv --runs 3
```

The runner verifies differential checks in both archives, alternates execution,
checks all 21 workloads and their allocation probes, and records binary SHA-256
and check output beside each TSV. Detailed process logs remain in
`target/asset-path-scans/<output-stem>/`.
