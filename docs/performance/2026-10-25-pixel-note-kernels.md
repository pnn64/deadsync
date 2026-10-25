# Pixel cleanup, chart parsing, and movie seeds - 2026-10-25

Three changes preserve exact values, note ordering and capacity, hold invalidation, random movie ordering, image backing-buffer semantics, and allocation behavior. Neither `deadlib-lua` nor `deadlib-lua-legacy` is changed by this work.

1. **Hidden-alpha cleanup:** replace a per-pixel alpha branch and three individual RGB writes with safe whole-word bit selection. Compare the packed alpha lane directly, then construct an all-zero or all-one visibility mask with wrapping subtraction. Explicit little-endian conversions make the alpha mask independent of host endianness. First/last visible-color selection, the fully transparent fast path, and partial trailing bytes retain their original behavior. Byte-extraction masks and block-level guards were tried and discarded because they slowed opaque images.
2. **Chart notes:** use the existing `memchr` implementation to find newline offsets in bulk instead of the scalar byte-predicate split. Process slices directly between offsets; a final end-of-input offset preserves unterminated last rows and trailing empty lines. Capacity counting, note classification, row numbering, and hold/tail processing are unchanged.
3. **Movie seeds:** remove the handwritten 14-line CRC routine and call `crc32fast::hash` directly through the existing local name. The library supplies the same IEEE CRC-32, including initial/final inversion, using its optimized implementations. `crc32fast` 1.5.3 and `memchr` 2.8.3 already existed in the lockfile; only direct dependency edges are added.

The three production edits remove seven net lines, including the CRC bit loop, per-pixel branch, and separate RGB stores. Test-only allocator accounting is shared with the existing metadata tests. Frozen reference code and differential tests are test-only. No unsafe code is introduced into production or test infrastructure; the benchmarks reuse existing allocation and Windows cycle-counting helpers.

Release assembly inspection confirms SSE2 selection over eight pixels per loop: packed comparisons (`pcmpgtd`) feed `pand`, `pandn`, and `por`, with unaligned vector loads/stores. The scalar tail uses a conditional move. This SIMD is generated from safe Rust; no intrinsics or architecture dispatch are added to the application.

## Method

Intel Xeon E5-2696 v4, Windows x86_64, rustc 1.99.1 / LLVM 23.1.1. Both versions use release optimization level 3, `--config profile.release.lto=false`, and the repository's static MSVC CRT flag. No new CPU feature requirement is imposed on the application.

The baseline is `f6d3ae65c` in an isolated worktree, with the final benchmark harness, allocator probes, manifests, and lockfile. The three production algorithms remain unchanged there. The optimized build uses the current production implementations. Both executable sets passed the differential tests. Cargo reused stale artifacts when changing between source trees with a shared target directory; the two affected packages were explicitly cleaned and rebuilt before archiving the final optimized binaries. Distinct executable SHA-256 hashes are recorded below.

Four fresh-process runs per version alternate before/after order. Each process pins its benchmark thread to logical processor 2, discards one warmup batch, and reports the median of eleven measured batches. The table gives the median of four process measurements. Throughput is before/after **thread CPU cycles** from Windows `QueryThreadCycleTime`; `Instant` elapsed time is secondary. The ratio range is the minimum and maximum of the four paired process ratios, not a confidence interval. No compilation launched by this task overlapped these measurements.

Fixture construction stays outside timing, except that the movie-cycle fixture deliberately includes cloning and dropping 16 owned paths as well as hashing, shuffling, and truncation. This provides an integrated measurement that includes allocation and ownership costs. Note parser timing includes building and dropping its returned vector. Hidden-alpha cleanup repeatedly mutates a warmed, idempotently cleaned buffer with unchanged alpha values. Allocation probes run separately from timed batches; returned results are dropped after tracking ends.

- `hidden-alpha/SIZE/kind=N` processes one square RGBA image. Kinds are opaque (0), fully transparent (1), alternating alpha (2), irregular mixed alpha (3), and a visible middle half with transparent edges (4). RGB varies by pixel. The fully transparent algorithm is unchanged and serves as a control.
- `note-parser/ROWS/LANES/kind=N` processes 64 or 8,192 rows with a measure delimiter every 64 rows. Kind 0 is empty; kind 1 places notes on every fourth row, cycling taps, holds, rolls, tails, mines, fakes, and lifts across columns. It includes unmatched/overwritten holds and therefore exercises invalidation and compaction.
- `movie-crc/bytes=N` hashes an ASCII seed. `movie-cycle/bytes=N/paths=16` additionally clones 16 paths, shuffles them, and retains ten entries.

These are local kernel and setup measurements, not whole-game FPS claims. The retained changes improve the targeted work: large-image cleanup gains 1.25-14.91x outside its unchanged fully transparent path; ten-lane large-chart parsing gains 1.15-1.16x; and movie-cycle setup gains 1.61x for 256-byte seeds. Short CRC calls have higher fixed overhead: empty and 12-byte hashes regress, and the 12-byte integrated movie-cycle fixture regresses 7.5% (about 210 cycles / 98 ns). The 64-byte integrated fixture gains only 4.4% in this run set. The eight-lane mixed-note fixtures regress 0.8% at 64 rows and 2.1% at 8,192 rows; the other parser fixture medians improve. These controls and the full process-ratio ranges are included below. A preliminary four-run set showed the same large pixel and long-seed wins, with more variation in small parser/setup gains; no confidence interval is claimed.

## CPU cycles and throughput

| Workload | Before cycles/op | After cycles/op | CPU throughput | Process ratio range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `hidden-alpha/512/kind=0` | 196440.07 | 157272.17 | 1.249x | 1.096-1.333x | 90558.79 | 72131.64 |
| `hidden-alpha/512/kind=1` | 378532.91 | 356169.19 | 1.063x | 0.955-1.150x | 173700.39 | 163467.19 |
| `hidden-alpha/512/kind=2` | 294536.41 | 151169.05 | 1.948x | 1.902-2.165x | 135225.00 | 69462.30 |
| `hidden-alpha/512/kind=3` | 2342218.47 | 157055.25 | 14.913x | 13.953-16.718x | 1076237.50 | 72222.08 |
| `hidden-alpha/512/kind=4` | 513404.07 | 284482.72 | 1.805x | 1.665-1.912x | 235714.26 | 130632.42 |
| `hidden-alpha/64/kind=0` | 3012.99 | 2076.24 | 1.451x | 1.295-1.524x | 1369.72 | 943.16 |
| `hidden-alpha/64/kind=1` | 4883.01 | 4608.65 | 1.060x | 0.949-1.110x | 2226.95 | 2101.18 |
| `hidden-alpha/64/kind=2` | 4673.81 | 1990.51 | 2.348x | 2.119-4.337x | 2129.30 | 906.64 |
| `hidden-alpha/64/kind=3` | 6582.43 | 2006.37 | 3.281x | 2.809-3.689x | 3000.19 | 914.06 |
| `hidden-alpha/64/kind=4` | 8320.85 | 3899.12 | 2.134x | 1.538-2.396x | 3792.97 | 1778.32 |
| `movie-crc/bytes=0` | 2.37 | 35.80 | 0.066x | 0.065-0.068x | 1.06 | 16.31 |
| `movie-crc/bytes=12` | 37.10 | 112.37 | 0.330x | 0.313-0.387x | 16.91 | 52.16 |
| `movie-crc/bytes=256` | 1443.26 | 110.12 | 13.106x | 12.022-16.093x | 662.09 | 50.25 |
| `movie-crc/bytes=64` | 326.75 | 84.20 | 3.881x | 3.552-4.484x | 150.20 | 38.39 |
| `movie-cycle/bytes=0/paths=16` | 2796.59 | 2745.89 | 1.018x | 0.937-1.113x | 1285.38 | 1273.14 |
| `movie-cycle/bytes=12/paths=16` | 2821.02 | 3031.47 | 0.931x | 0.921-1.075x | 1295.80 | 1393.76 |
| `movie-cycle/bytes=256/paths=16` | 4564.75 | 2835.12 | 1.610x | 1.461-1.776x | 2102.97 | 1303.65 |
| `movie-cycle/bytes=64/paths=16` | 3038.25 | 2911.11 | 1.044x | 0.975-1.151x | 1393.34 | 1335.73 |
| `note-parser/64/10/kind=0` | 3950.99 | 3397.97 | 1.163x | 0.992-1.440x | 1799.61 | 1542.58 |
| `note-parser/64/10/kind=1` | 6231.74 | 5567.24 | 1.119x | 1.015-1.282x | 2837.89 | 2530.86 |
| `note-parser/64/4/kind=0` | 2141.84 | 1941.20 | 1.103x | 1.003-1.394x | 971.10 | 879.30 |
| `note-parser/64/4/kind=1` | 3771.80 | 3511.14 | 1.074x | 1.018-1.103x | 1717.18 | 1599.22 |
| `note-parser/64/5/kind=0` | 2452.22 | 2368.20 | 1.035x | 0.958-1.188x | 1122.26 | 1073.44 |
| `note-parser/64/5/kind=1` | 4081.33 | 3986.15 | 1.024x | 0.776-1.164x | 1854.69 | 1810.93 |
| `note-parser/64/8/kind=0` | 3284.78 | 2985.55 | 1.100x | 0.970-1.288x | 1491.40 | 1354.68 |
| `note-parser/64/8/kind=1` | 5243.14 | 5284.29 | 0.992x | 0.894-1.508x | 2383.98 | 2402.34 |
| `note-parser/8192/10/kind=0` | 513706.32 | 443990.20 | 1.157x | 1.016-1.340x | 235765.23 | 204353.12 |
| `note-parser/8192/10/kind=1` | 723478.84 | 628250.14 | 1.152x | 0.958-1.293x | 332317.58 | 288462.11 |
| `note-parser/8192/4/kind=0` | 264216.28 | 260219.84 | 1.015x | 0.917-1.209x | 121194.92 | 119583.60 |
| `note-parser/8192/4/kind=1` | 397306.14 | 372299.43 | 1.067x | 1.011-1.093x | 182741.79 | 171216.40 |
| `note-parser/8192/5/kind=0` | 317801.69 | 270714.68 | 1.174x | 1.087-1.229x | 146085.16 | 124196.88 |
| `note-parser/8192/5/kind=1` | 468414.72 | 429869.30 | 1.090x | 0.994-1.218x | 215223.44 | 198298.04 |
| `note-parser/8192/8/kind=0` | 474554.70 | 363758.66 | 1.305x | 1.077-1.437x | 217933.60 | 167289.84 |
| `note-parser/8192/8/kind=1` | 596997.98 | 609293.43 | 0.980x | 0.956-1.144x | 274118.36 | 280450.39 |

## Allocations

Every allocation metric matches before/after across all four runs. Hidden-alpha cleanup and CRC hashing allocate and free nothing. Empty note data also allocates nothing. Mixed note fixtures allocate their output vector and one small invalid-head scratch vector, with no reallocations; output capacities remain identical. Movie-cycle setup, including the explicit input clone, allocates 17 times (16 path strings and their vector), with no reallocations. This change improves CPU execution without changing allocation policy. Raw allocation counts and byte totals accompany every measurement in the TSV.

## Correctness and reproduction

The release suite passed 1,358 tests across `deadlib-assets`, `deadsync-chart`, `deadsync-simfile`, `deadsync-gameplay`, and `deadsync-rules` (34 ignored benchmarks). Clippy completed for all targets of the two modified packages with existing warnings. Validation commands:

```powershell
cargo test -p deadlib-assets -p deadsync-chart -p deadsync-simfile -p deadsync-gameplay -p deadsync-rules --release --config profile.release.lto=false -- --test-threads=1
cargo clippy -p deadlib-assets -p deadsync-chart --all-targets --release --config profile.release.lto=false
```

Differential tests compare every output image byte, including extra complete backing pixels and incomplete trailing bytes, zero-sized images, every alpha value, equal/unequal edge colors, and transparent/opaque/mixed data. Parser tests cover 3,584 deterministic arbitrary-byte inputs over lane counts 0, 1, 4, 5, 8, 10, and 12, comparing all note fields and vector capacity against the frozen original. CRC tests compare all 4,097 prefixes of a deterministic byte corpus and verify exact movie order for empty, ASCII, Japanese, and embedded-NUL seeds; the existing ITG CRC vector is retained.

Build the baseline and optimized trees with the same commands, archiving each version's three test executables under the names used by the runner. When sharing a Cargo target directory between worktrees, clean the two measured packages before the optimized build and verify fresh compilation.

```powershell
cargo test -p deadlib-assets --test hidden_alpha -p deadsync-chart --lib --test note_parser --release --config profile.release.lto=false --no-run
python scripts/benchmark_pixel_note_kernels.py --before target/pixel-note-kernels/before --after target/pixel-note-kernels/after --output docs/performance/2026-10-25-pixel-note-kernels.tsv
```

Raw results: [272 measurements and allocation probes](2026-10-25-pixel-note-kernels.tsv). Per-process stdout remains in `target/pixel-note-kernels/runs/`.

## Executable hashes

| Version | Executable | SHA-256 |
| --- | --- | --- |
| before | `deadsync_chart.exe` | `cbd73c25ad1fd0acd4ae7c92004b0f58be6bed7b7c8c6226ae42c40bfe0cc7eb` |
| before | `hidden_alpha.exe` | `7a9aa0f8458d274297d3d8a8222c693e313fcc88ce3bdc4465af72903cf2bec5` |
| before | `note_parser.exe` | `0b401a509447ab6e32e63ea837c6de1733e27312c7dc4419d3274b99d799c86e` |
| after | `deadsync_chart.exe` | `24542d791aa8a9ff870b5ad1e6c3e6da1fcb77ae238ef845e2baa1b0ce4cc6c5` |
| after | `hidden_alpha.exe` | `d3883df65097e36046cc99c0aa82eb1929d941a88f5c5e2b84bab940b18bebd7` |
| after | `note_parser.exe` | `f71d1b1b0d0f0225530587f0dcacf4620577561dc50cdc0a091a034eea254b89` |
