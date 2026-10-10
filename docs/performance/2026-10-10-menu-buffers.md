# Menu buffers performance pass - 2026-10-10

Branch: `perf/1872-menu-buffers-20261010`

Starting committed main: `e3256f0c0a6eed1c88f044d60c7ed7bcda027407`

Worktree: `C:/GitHub/deadsync-perf-1872-menu-buffers-20261010`

Version: **0.5.1871 -> 0.5.1872**, exactly one patch increment in Cargo.toml and all three workspace-version lockfile entries.

Read the local rust-performance.md guidance on removing repeated work, allocation reuse, sufficient capacity, and measured hot paths. Audited **69 unmerged local/remote perf refs (43 distinct heads)** before implementation. Pack menu and browser input had no pending edits. The pending `perf/1872-data-paths-20261010` edit in song_search.rs affects delete-word; this pass changes completion, whose original body is byte-identical on that pending branch. No pending optimization was incorporated. Test allocator support is reused from the prior pass for consistent measurement.

## Changes

1. **Content browser input:** consume printable characters directly from the incoming text iterator and stop at the existing 64 Unicode-scalar query limit. Count available space once, instead of rescanning the growing query per character. Private query editing compares lengths instead of cloning the query: its only callers append, pop, or clear. Printable-text detection remains before focus changes and dialog gates. No-op edits retain debounce/caret times and search results; changed edits retain result rebuilding and sound effects.
2. **Song search completion:** borrow the accepted query's UTF-8 suffix at the original folded character boundary. Build the display String once with its exact byte capacity. This removes a temporary suffix String and possible growth reallocations. Display, typed text, accepted query, accent/combining-mark folding, filters, and 80-character acceptance limit are unchanged.
3. **Noteskin component menu:** determine bundled names and eligible providers once, and reuse normalized bundled selections, labels, and family names across the ten component rows. Borrow variant labels, build base selections without an intermediate ID clone, and format the 191 mine-size labels directly into the existing inline actor text representation. Provider order, duplicates, missing saved selections, row order, help, bindings, and final label storage remain intact.

No Song Lua implementation or compatibility fixtures changed. Temporary menu metadata lives only for add_rows; it adds no persistent cache, invalidation protocol, or new runtime abstraction. The existing owned result/selection data remains owned. Remaining allocations in provider formatting and row construction are included in the benchmarks.

## Validation

Fresh baseline and current release binaries used the same combined package/feature graph, opt-level 3, LTO disabled, and locked dependencies. Frozen original function bodies in the three `menu_buffers_original.rs` files were mechanically verified against the starting commit, allowing only function renames and routing the frozen input handler to the frozen query editor. Six new regression tests compare original/current behavior and verify allocation-free full-query/rejected input. Existing theme tests additionally exercise song-mode completion and input navigation.

| Suite | Passing | Failing (unchanged) | Ignored |
| --- | ---: | ---: | ---: |
| semantic | 144 | 39 | 77 |
| actor | 30 | 1 | 0 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| deadsync_theme_simply_love-tests | 1269 | 1 | 6 |

Failure names and diagnostics match baseline after normalizing thread IDs and only audited nondeterministic diagnostics: ordering of missing Sprite.Load aliases and which of the same missing referenced archives is reported first. A fresh missing-reference audit reruns the saved original and current executables. The baseline includes missing external song directories/references, native recapture requirements, and the existing Cyber model scale assertion. These failures are not reported as passing.

The Song Lua compatibility harness also passed **1,650,933 native comparisons in each build**, with **zero failed comparisons**, using six checked-in recordings from ITGmania (not a newly launched ITGmania engine):

| Capture | Archive | Native comparisons |
| --- | --- | ---: |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst` | 363873 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst` | 363873 |
| 280\|MODS\|[MASTER] Sharkmode | `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst` | 304425 |
| Warp Zone | `ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst` | 212220 |
| Let Me Hear That | `2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst` | 205071 |
| 272\|MODS\|[lv.02] Riddle | `7ffacb89fd95ab788758e273ba878c30380c4622406f1bb22d8c51292ca78d83.tar.zst` | 201471 |

## Paired benchmarks

Windows x86_64, Intel Xeon E5-2696 v4, rustc/cargo 1.98.1. Four fresh processes, pinned to logical CPU 2 with AboveNormal priority, after this pass's builds and compatibility runs finished. Each process alternates old/new order over nine timed batches after three warm-up calls per variant and calibration targeting 25 ms per batch (capped at 1,000,000 iterations); the table reports the median of four process medians, and the range of within-process old/new ratios. Times are elapsed CPU-affined microbenchmarks, not hardware cycle counters or whole-game frame rates.

Completion measures the complete completion function including result destruction. Input measures the complete raw input handler, result rebuilding, and effects; both variants restore the query and clear their effects in existing buffers each iteration. `catalog4096` includes catalog search against one immutable catalog shared by both variants; other input cases isolate input work with an empty catalog. `long-paste` and `blocked` are 24 KiB UTF-8 paste stress cases, distinct from ordinary typing. Pack cases call complete add_rows repeatedly with existing choice-vector capacity: bundled-only, two providers with 16 variants per slot, eight with 64, and hidden providers. Setup, fixture creation, and i18n initialization are outside measurements.

| Case | Original ns/op | Current ns/op | Throughput ratio | Paired ratio range |
| --- | ---: | ---: | ---: | ---: |
| completion/empty | 4.03 | 4.33 | 0.928x | 0.891-0.940x |
| completion/ascii | 1,143.19 | 588.25 | 1.943x | 1.913-1.949x |
| completion/accented | 778.38 | 643.44 | 1.210x | 1.202-1.447x |
| completion/combining | 927.39 | 559.31 | 1.658x | 1.575-1.687x |
| completion/unicode | 1,075.34 | 728.12 | 1.477x | 1.459-1.582x |
| completion/filter | 1,048.17 | 793.73 | 1.321x | 1.294-1.324x |
| completion/exact | 568.26 | 566.36 | 1.003x | 0.971-1.040x |
| completion/reordered | 388.01 | 388.56 | 0.999x | 0.928-1.026x |
| completion/limit | 2,021.91 | 1,061.17 | 1.905x | 1.893-1.918x |
| input/single | 421.86 | 267.80 | 1.575x | 1.519-1.639x |
| input/paste64 | 1,863.00 | 437.20 | 4.261x | 4.109-4.311x |
| input/unicode64 | 4,306.02 | 2,040.37 | 2.110x | 2.104-2.302x |
| input/long-paste | 48,496.10 | 2,517.38 | 19.265x | 18.547-22.258x |
| input/full | 227.50 | 34.64 | 6.569x | 6.405-7.819x |
| input/blocked | 46,561.24 | 17.02 | 2734.875x | 2650.135-3515.097x |
| input/controls | 29.95 | 20.91 | 1.432x | 1.157-1.536x |
| input/catalog4096 | 658,910.00 | 660,458.03 | 0.998x | 0.965-1.276x |
| input/backspace | 772.26 | 761.24 | 1.014x | 0.942-1.182x |
| input/clear | 286.67 | 202.45 | 1.416x | 1.398-1.570x |
| packs/bundled | 57,498.54 | 28,850.46 | 1.993x | 1.935-1.994x |
| packs/two-providers | 402,598.55 | 350,591.67 | 1.148x | 1.072-1.180x |
| packs/large | 6,595,423.33 | 6,209,330.00 | 1.062x | 1.033-1.217x |
| packs/hidden | 37,352.15 | 16,090.98 | 2.321x | 1.818-2.394x |

The scoped thread-local System allocator counts are measured separately, with counting disabled during timed batches and were identical across all four processes. Requested bytes sum allocation/reallocation request sizes; they are **not peak/resident memory**. Allocation counts include the complete measured operation and its buffer restoration/result cleanup.

| Case | Allocations old -> new | Reallocations old -> new | Requested bytes old -> new |
| --- | ---: | ---: | ---: |
| completion/empty | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| completion/ascii | 6 -> 5 | 3 -> 0 | 241 -> 149 |
| completion/accented | 6 -> 5 | 0 -> 0 | 42 -> 33 |
| completion/combining | 6 -> 5 | 2 -> 0 | 89 -> 57 |
| completion/unicode | 6 -> 5 | 2 -> 0 | 101 -> 69 |
| completion/filter | 6 -> 5 | 1 -> 0 | 74 -> 52 |
| completion/exact | 3 -> 3 | 0 -> 0 | 24 -> 24 |
| completion/reordered | 3 -> 3 | 0 -> 0 | 30 -> 30 |
| completion/limit | 6 -> 5 | 3 -> 0 | 1609 -> 1048 |
| input/single | 3 -> 1 | 0 -> 0 | 17 -> 8 |
| input/paste64 | 1 -> 0 | 3 -> 0 | 120 -> 0 |
| input/unicode64 | 1 -> 0 | 5 -> 0 | 504 -> 0 |
| input/long-paste | 3 -> 1 | 17 -> 5 | 66033 -> 504 |
| input/full | 2 -> 0 | 0 -> 0 | 72 -> 0 |
| input/blocked | 1 -> 0 | 12 -> 0 | 65528 -> 0 |
| input/controls | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| input/catalog4096 | 5 -> 4 | 8 -> 8 | 2936 -> 2928 |
| input/backspace | 2 -> 1 | 1 -> 1 | 38 -> 24 |
| input/clear | 1 -> 0 | 0 -> 0 | 14 -> 0 |
| packs/bundled | 341 -> 155 | 50 -> 24 | 12513 -> 11169 |
| packs/two-providers | 1963 -> 1418 | 1160 -> 1134 | 113824 -> 106592 |
| packs/large | 26029 -> 20564 | 19800 -> 19775 | 1713526 -> 1615686 |
| packs/hidden | 235 -> 44 | 10 -> 10 | 8096 -> 6568 |

The empty-query completion control measured 4.03 -> 4.33 ns (about 0.3 ns slower), retaining the identical early-return source and zero allocations. Exact/reordered completion, backspace, and full-catalog input timing ranges cross parity, so no CPU improvement is claimed for those controls; backspace and catalog input still remove one allocation. The catalog median differs by only 0.24%. These are operation-level microbenchmarks, and sub-nanosecond empty-return differences do not establish a frame-time effect.

Initial shorter measurements used two independently allocated catalog fixtures and suggested a 1.4% catalog slowdown. To check that result, the final committed benchmark shares the immutable catalog and uses longer warmed batches. All eight preliminary logs remain under target/initial-final-*.log and target/initial-repeat-*.log, with the first summary in initial-benchmark-summary.json; the final table uses all four runs of the revised protocol. Production code did not change during this refinement. Hash checks proved every native/application executable unchanged; the theme test executable alone rebuilt, and its complete test suite was rerun with the same outcomes.

## Reproduction and evidence

From this worktree, use an isolated target directory (this run reused `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build`), TEMP/TMP under this worktree's target/tmp, and CARGO_BUILD_JOBS=4:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile -p deadsync-theme-simply-love --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run --message-format=json
```

Use the executable paths from Cargo's artifact messages. Run the theme and simfile library executables and semantic/actor harnesses with `--test-threads=1`; run full_song_lua once per archive above. Run the theme test executable with `benchmark_menu_buffers --ignored --nocapture --test-threads=1` for paired measurements. Ordinary library test runs include the six new regression tests and skip the three benchmarks.

Local raw evidence remains under target/: baseline/current artifact manifests, build/test logs, six capture logs per build, saved original executable hashes, `pending-audit.json`, `pending-heads.json`, `compatibility-comparison.json`, `missing-reference-audit.json`, `alias-order-audit.json`, `final-1.log` through `final-4.log`, `benchmark-summary.json`, and `verified-source-hashes.json`. The committed tests contain the benchmark and original implementations. Assets were verified against Git blob hashes before baseline execution. The original checkout was read only; all source changes, builds, temporary files, and the commit belong to this pass's worktree (with the noted separate-worktree build cache). No merge or push was performed. The four excluded filenames are absent from the commit.

At the final checkout audit, main had independently advanced to `49d5874b7f53b3d3a087305d39b3f5ec8489b454` and the original checkout was clean. This perf branch retains the frozen starting main commit above; no original-checkout edits or main commits were made by this pass.

Final theme benchmark/test executable SHA-256: `62480d2798aee4234d0c0fb64bb3f08ea16b9a8236de4b2551300b77fb22481c`.
