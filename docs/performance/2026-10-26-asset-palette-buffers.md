# Asset palette and cache allocation measurements - 2026-10-26

Three retained changes remove transient work in `deadlib-assets` while preserving
pixels, errors, filenames, and public signatures:

1. GIF decoding borrows the decoder's local/global palette instead of cloning the
   global table before reading the frame header. Transparency is copied before
   selecting the borrow. This also removes a clone when a local table overrides it.
2. Indexed BMP decoding reads the palette as one block and reuses its 1 KiB stack
   scratch buffer for scanlines that fit. Larger scanlines retain the heap fallback.
   This removes the per-entry read loop and the short-row `Vec` allocation.
3. Dynamic cache paths encode the fixed option-hash suffix into 38 stack bytes
   and reserve the final `PathBuf` once. The temporary formatted filename and
   repeated path growth disappear. The returned path hash string is unchanged.

No new production helpers, dependencies, or `unsafe` were added. The BMP change
trades about 1 KiB of additional stack space for fewer heap operations; the heap
counters below do not include stack usage. Large BMP rows still allocate a row.
The cache filename adds a 38-byte stack array. GIF palette storage stays borrowed
only until the color table is built, before the decoder is mutated again.

## Separate before/after measurements

Each candidate was benchmarked before editing it and compared with an archived
after binary. The figures below are medians of three alternating process runs.
Each process uses eleven CPU batch samples after warmup. Requested bytes count
all Rust allocations/reallocations during the call, including decoder/OS wrappers.
Returned images and paths are dropped within the measured operation.

| Operation | Allocations | Reallocations | Requested bytes | CPU cycles/op (throughput ratio) |
| --- | ---: | ---: | ---: | ---: |
| GIF, global 256-color table, 1x1 frame | 12 -> 11 | 0 -> 0 | 47225 -> 46457 | 184601.22 -> 178230.57 (1.036x) |
| GIF, local overrides 256-color global table | 13 -> 12 | 0 -> 0 | 47993 -> 47225 | 177191.38 -> 159771.99 (1.109x) |
| BMP8, 1x1 | 4 -> 3 | 0 -> 0 | 8416 -> 8412 | 216855.72 -> 210020.34 (1.033x) |
| BMP8, 256x256 | 4 -> 3 | 0 -> 0 | 270808 -> 270552 | 484532.56 -> 440042.62 (1.101x) |
| Cache path, empty destination | 5 -> 4 | 3 -> 0 | 559 -> 419 | 283860.66 -> 263320.26 (1.078x) |
| Cache path, short destination | 5 -> 4 | 5 -> 0 | 580 -> 424 | 277941.02 -> 275067.80 (1.010x) |
| Cache path, long destination | 5 -> 4 | 4 -> 0 | 1286 -> 675 | 275688.57 -> 282330.16 (0.976x) |

GIF fixtures with a 768-byte global table save one allocation/free and 768 requested
bytes for all measured frame sizes (1, 16, 256). The fixture named `local` supplies
an empty global palette to the encoder; gif 0.14.2 still emits a six-byte padded
global table, so that case saves six bytes. A separate public test removes this
table and verifies genuinely local-only decoding, including transparency.

All 18 measured BMP variants save one allocation/free, with savings equal to their
padded scanline pitch. Cache construction saves one allocation and all 3-5 measured
reallocations, for both option values and all three destination lengths.

The allocation savings are the demonstrated reason to retain these changes.
CPU results are mixed and include substantial movement in unchanged controls.
For example, the unchanged BMP control improves up to 18% during the GIF comparison;
the unchanged GIF control regresses up to 15% during the BMP comparison. BMP4 at
256x256 regresses about 6% in one pair, and the long cache path regresses about 2%.
These runs do not establish an overall CPU, startup-time, or frame-rate improvement.
All controls and regressions are retained:
[GIF](2026-10-26-asset-palette-gif.tsv),
[BMP](2026-10-26-asset-palette-bmp.tsv),
[cache paths](2026-10-26-asset-palette-cache.tsv).
There are 432 process/workload rows for the accepted changes.

## Rejected queue experiment

Returning the inserted upload entry directly was intended to remove a second
lookup in `push_prepared`. Cross-binary timings were inconsistent with a stronger
in-process comparison against the frozen original. Across three paired runs,
small/medium queues range roughly from 0.96x to 1.02x; the largest queue varies
from 1.00x to 1.13x. The before/before calibration already varied by about 4%.
That evidence does not establish a reliable gain, so the production change was
reverted. `upload.rs` matches its original content. The standalone upload fixtures
are retained only for reproducing this experiment and controls; Cargo does not
load them as crate tests.

[Cross-binary results](2026-10-26-asset-palette-queue.tsv),
[paired results](2026-10-26-asset-palette-upload-paired.tsv),
[calibration](2026-10-26-asset-palette-queue-before-paired.log), and
[rejected patch](2026-10-26-asset-palette-queue-rejected.patch).

## Method and correctness

- Windows x86-64; Xeon E5-2696 v4; rustc 1.99.1 (`58cf8f9c3`, LLVM 23.1.1).
- Optimization level 3, fat LTO, one codegen unit, static CRT, generic CPU target.
- `QueryThreadCycleTime` on logical processor 2; fixture setup is outside timing
  and allocation probes. File opening/reading and result destruction are included.
  Nanoseconds are retained but were not used to select these changes.
- Focused test binaries compile complete, unchanged production function/method
  bodies and the same decoder/queue helpers. They use fixed prebuilt dependencies,
  whose [hashes](2026-10-26-asset-palette-dependencies.json) match across all builds.
  These are production-body benchmarks with focused callers, not full application
  benchmarks. Source hashes, extracted-body hashes, exact compiler arguments,
  and binary hashes are retained in eight `.build.json` manifests.
- Both frozen decoder bodies and all three frozen upload methods match commit
  `f2dd892e7` after applying rustfmt. The new cache reference freezes that same
  commit's implementation. Shared pixel and hash helpers remain unchanged.
- Each distinct benchmark archive passes its differential checks. Successful
  check output is cached by executable SHA-256 to avoid repeating the expensive
  truncation corpus for an unchanged executable.
- Decoder checks compare pixels and error text at every truncated byte length
  across global/local GIF palettes, offsets, color-key modes, BMP1/4/8,
  Windows/OS2 headers, and top-down/bottom-up rows. Full crate tests also retain
  the existing native image corpus and decoder validations.
- Public BMP tests check complete pixels at widths 1023, 1024, 1025, 2049, and
  8193 for all three bit depths and both row orientations. They cross both the
  scratch and heap branches. Public GIF tests cover a truly absent global table.
- Cache checks compare path values and hashes for both enabled values, Unicode,
  drive/UNC/relative/root forms, embedded-NUL destination data, directory sources,
  and missing sources. Both possible option-hash values are covered.

Validation:

- `cargo test -p deadlib-assets --locked`: **113 passed**, 18 ignored, 0 failed;
  [log](2026-10-26-asset-palette-final-cargo-tests.log).
- Additional `cargo test -p deadlib-assets --test gif_palette --locked`:
  **1 passed**; [log](2026-10-26-asset-palette-gif-no-global-check.log).
- Final optimized decoder/control snapshot: **2 passed**;
  [log](2026-10-26-asset-palette-final-optimized-checks.log).
- `cargo clippy -p deadlib-assets --locked --lib --tests --no-deps`: passed,
  with existing decoder/DSL/benchmark warnings plus the frozen reference's
  constant-chunk warning; [log](2026-10-26-asset-palette-final-clippy.log).
- Scoped rustfmt, Python compilation, source/dependency hash audit, and staged
  whitespace checks pass. Unrelated worktree changes are excluded.

## Reproduction

Use `scripts/benchmark_asset_palette_upload.py`. Restore the relevant production
bodies from `f2dd892e7`, then build a baseline and apply the recorded patches in
order. The GIF and BMP snapshots include decoder and upload controls; cache
snapshots use `--cache` and only the three path/hash functions. The script copies
complete bodies, never rewritten stand-ins. The `decoder-final-format.patch`
records subsequent formatting, the scratch comment, and borrow coercions; it also
adds the crate test module. The final optimized snapshot checks that final source.

```powershell
python scripts/benchmark_asset_palette_upload.py build NAME --dependencies docs/performance/2026-10-26-asset-palette-dependencies.json
python scripts/benchmark_asset_palette_upload.py compare --before BEFORE.exe --after AFTER.exe --output results.tsv --runs 3
```

Use `--cache` for cache builds and `--expected 6` for their comparisons. Decoder
comparisons expect 33 workloads. Use `--paired` when building the rejected queue
experiment, then run its `benchmark_paired` ignored test for the in-process pair.
The compiler/dependency paths in manifests refer to this machine; on another
machine, first build matching dependencies with Cargo and supply their paths and
SHA-256 values in the dependency JSON.

Apply saved patches with `git apply --unidiff-zero`:
[GIF](2026-10-26-asset-palette-gif.patch),
[BMP](2026-10-26-asset-palette-bmp.patch),
[final decoder source](2026-10-26-asset-palette-decoder-final-format.patch), and
[cache construction](2026-10-26-asset-palette-cache.patch).
Do not apply the rejected queue patch to the final implementation.
