# Golf metadata ownership and lookup — 2026-10-26

Three safe Rust optimizations in `deadsync-chart::golf`, following the allocation-reuse, hot-path, and fast-hasher guidance in `rust-performance.md`. The model is loaded during library scanning and queried by the music wheel and gameplay golf display. The production diff adds 17 lines and removes 13, including formatting and comments. No dependency, public API, production function/type, or unsafe block is added. Lua and unrelated worktree changes are excluded.

## Execution simplified

1. **Reuse song keys on override updates.** A recognized chart override first looks up the existing entry with a borrowed `str`. Successful updates write its value and presence bit directly, avoiding a temporary owned key and its destruction. Default sections keep the existing entry path; previously unseen override-only songs still create an owned key.
2. **Index difficulty slots directly.** Reuse the existing length/case dispatch for five standard difficulties, with Edit handled separately. Lookup classifies the requested difficulty once and tests its presence bit, replacing the loop over stored overrides and removing the duplicated difficulty-name table. Default-only songs skip classification.
3. **Use the existing Fx hasher for private song metadata.** Local pack-authored song-folder keys use `rustc_hash::FxHashMap`, already a dependency of this crate. The outer pack registry retains its existing hasher, locking, replacement policy, and path handling. No new cache or lifetime policy is introduced.

## Isolated before/after measurements

Consecutive stages are original → borrowed override keys → indexed slots → Fx hashing. Every archive uses the repository's **default release profile, including LTO**. Results are median thread CPU cycles per fixture from six fresh processes per version for key reuse and eight for the other stages.

| Change / workload | Before cycles | After cycles | CPU throughput | Paired ratio range |
| --- | ---: | ---: | ---: | ---: |
| Key reuse, complete-override parse | 1,121,020.75 | 825,155.41 | 1.359x | 1.207–1.481x |
| Key reuse, complete parse plus queries | 1,317,836.88 | 979,820.62 | 1.345x | 1.292–1.805x |
| Indexed slots, complete direct queries | 138,399.90 | 101,661.09 | 1.361x | 1.191–1.405x |
| Indexed slots, complete parse plus queries | 854,184.25 | 768,407.78 | 1.112x | 0.895–1.145x |
| Fx hashing, default-only direct queries | 89,362.23 | 51,453.89 | 1.737x | 1.547–1.951x |
| Fx hashing, complete direct queries | 102,036.62 | 70,937.93 | 1.438x | 1.396–1.596x |
| Fx hashing, complete parse plus queries | 789,370.06 | 668,013.94 | 1.182x | 1.121–1.271x |

Raw data with all controls: [key reuse](2026-10-26-golf-borrowed.tsv), [slot lookup](2026-10-26-golf-slots.tsv), [hashing](2026-10-26-golf-hash.tsv). Ranges are minimum/maximum ratios of corresponding process runs, not confidence intervals. The first comparison has substantial variation, including changes in untouched direct-query controls. It also shows a default-only parse regression (0.874x), although that path retains the same allocation counts and has no material gain in the combined comparison. Slot indexing alone regresses sparse direct and public-pack queries by about 5%. These costs are included in the retained raw records.

An attempted single-override specialization made direct queries slower, and an inline hint did not recover their throughput. Both were discarded. The smaller indexed implementation is retained for its complete-override benefit and the combined results below.

## Combined results and allocations

The [combined comparison](2026-10-26-golf-final.tsv) uses twelve fresh processes per version, comparing the original model with all three changes.

| Fixture / workload | Before cycles/fixture | After cycles/fixture | CPU throughput | Before → after ns/fixture |
| --- | ---: | ---: | ---: | ---: |
| Defaults / parse | 255,038.43 | 251,718.50 | 1.013x | 117,706.25 → 114,784.38 |
| Defaults / direct query | 109,762.01 | 44,423.01 | 2.471x | 50,041.40 → 20,481.25 |
| Defaults / parse plus query | 370,481.69 | 285,562.66 | 1.297x | 170,025.00 → 131,059.38 |
| Defaults / public pack query | 1,176,725.75 | 1,120,739.44 | 1.050x | 536,643.75 → 511,568.75 |
| Sparse / parse | 351,467.50 | 278,874.75 | 1.260x | 160,225.00 → 127,162.50 |
| Sparse / direct query | 123,883.74 | 61,989.89 | 1.998x | 56,484.38 → 28,400.78 |
| Sparse / parse plus query | 456,813.82 | 363,251.94 | 1.258x | 208,309.38 → 165,603.12 |
| Sparse / public pack query | 1,169,002.00 | 1,139,383.31 | 1.026x | 533,456.25 → 519,400.00 |
| Complete / parse | 892,020.56 | 521,387.93 | 1.711x | 409,062.50 → 238,962.50 |
| Complete / direct query | 159,259.26 | 62,708.42 | 2.540x | 72,634.38 → 28,789.85 |
| Complete / parse plus query | 1,060,905.25 | 587,457.47 | 1.806x | 484,703.12 → 267,900.00 |
| Complete / public pack query | 1,249,462.62 | 1,185,889.94 | 1.054x | 570,681.25 → 541,050.00 |

| Parse fixture, 256 songs | Before allocations/frees | After allocations/frees | Requested bytes before → after |
| --- | ---: | ---: | ---: |
| Defaults only | 264 / 264 | 264 / 264 | 97,052 → 97,052 |
| One override per song | 520 / 520 | 264 / 264 | 103,196 → 97,052 |
| Six overrides per song | 1,800 / 1,800 | 264 / 264 | 133,916 → 97,052 |

Complete-override parsing removes **1,536 temporary allocations/frees and 36,864 requested bytes per fixture**. Reallocations remain zero. Measured peak bytes remain 73,784: the retained metadata has the same song keys and value slots, and table growth dominates the peak. Parse-plus-query probes have the same allocation counts as parsing; both direct and public queries remain allocation-free.

Public pack-query gains are much smaller than internal-map gains because path handling, the outer map, and locking remain. Their individual run ranges include regressions. These results do not establish whole-frame music-wheel or gameplay FPS gains, nor gains for every Golf.ini layout. Fixtures contain a default entry followed by zero, one, or six overrides per song; override-only performance is not separately measured. Standard-map hash seeds, executable layout, and shared-machine activity introduce variation.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Measured builds use `--release --locked`, static MSVC runtime settings from `.cargo/config.toml`, default LTO, and the baseline CPU target. The existing benchmark helper pins the current thread to logical processor 2 and reads `QueryThreadCycleTime`. It discards one warmup sample and reports the median of eleven samples. Samples repeat parse and parse-plus-query fixtures 16 times, direct queries 64 times, and public queries eight times; reported cycles divide out those repetitions. The driver alternates before/after order and takes medians of process medians. Compilation and fixture construction stay outside CPU measurements; no compilation started by this task overlaps reported comparisons. Other work on the shared machine is uncontrolled.

Each text fixture contains 256 distinct, 24-byte song-folder names, including Unicode and colons. Queries visit all six difficulty names for every song (1,536 calls per fixture). The sparse fixture overrides Beginner; complete fixtures override every difficulty including Edit. Public queries call the production `golf::par` through `Path` handling, the pack map, and its read lock. Pack installation and path construction occur outside timing. A native Rust path probe verifies that all 768 generated paths resolve to the inserted pack and folder keys. Inputs/results pass through `black_box`. The four TSVs retain all **816 process/workload records**, executable SHA-256 values, elapsed times, and allocation counters.

Frozen references come from `83024d64d`. Three new checks compare exact f64 bits and missing-value results across default/override combinations, duplicate values, invalid pars, negative zero, unknown suffixes, comments, BOM/CRLF input, colon/bracket/Unicode names, missing defaults, every ASCII case combination and single-byte ASCII mutation of difficulty names, Unicode rejections, and public pack replacement/removal. The same classification is checked for section suffix parsing and queries.

Validation passes: **1,140 release tests** across chart, gameplay, and simfile (28 ignored), including the actual file reload/removal test; **45 chart debug tests** (six ignored); and all three new checks in the measured default-LTO after archive. Clippy passes for all chart targets with existing dependency warnings and no diagnostics in changed code. All changed/new Rust files pass `rustfmt --check`. Only formatting changed after the measured production build. Logs and executables are under `target/golf-metadata/`.

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-chart -p deadsync-gameplay -p deadsync-simfile
cargo test --locked -p deadsync-chart
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-chart --all-targets
cargo test --release --locked -p deadsync-chart --test golf_metadata --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_golf_metadata --log-dir target/golf-metadata/runs --before target/golf-metadata/before/chart.exe --after target/golf-metadata/after/chart.exe --output docs/performance/2026-10-26-golf-final.tsv
```

To reconstruct archives, keep the committed harness and frozen reference. Restore production `golf.rs` from `83024d64d` for `before`; add the borrowed-key parse branch for `borrowed`; add indexed difficulty dispatch/lookup and remove its old name table for `slots`; switch the private song map to FxHashMap for `after`. Build each with the default-release command and save Cargo's reported `golf_metadata` executable before changing the next stage. Compare consecutive archives with the same driver using six, eight, and eight runs respectively.
