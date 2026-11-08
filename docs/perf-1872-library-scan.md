# Library scan performance pass (0.5.1872)

Branch: `perf/1872-library-dataflows-20261009`. Starting commit: `a17584eb34cfe5975679d1f2a2a29e635e7565ce`.

The worktree started from the latest committed local main. Main and its dirty compatibility work continued independently during this pass. No source changes were made in that checkout. The patch version advances exactly once, **0.5.1871 -> 0.5.1872**, in Cargo.toml and all three workspace package entries in Cargo.lock.

The pending-branch audit covered 50 local/remote refs representing 29 distinct unmerged heads. A final audit found 51 refs for the same 29 heads, again with no scan.rs overlap. None changed these three scan functions; the pending simfile cache pass changes a different module. No Song Lua production code or dependencies changed.

## Changes

1. **Pack sorting:** walk the sorted index permutation directly, marking visited indices in the existing vector. Remove the inverse-permutation vector and its initialization pass. The stable comparator and all pack data remain unchanged.
2. **Pack group matching:** sort `(hash, index)` pairs instead of sorting indices through a separate hash vector. This removes one allocation and the extra hash lookup during comparisons. Hashing, trimmed ASCII matching, collision comparisons, and earliest-input representative selection are unchanged.
3. **Directory scanning:** reject known non-directory, non-symlink entries before constructing filenames or paths. Reject resource-fork directories before constructing full paths. Preserve directory ordering and the existing metadata-error/symlink fallback.

## Measurements

Windows/MSVC, Intel Xeon E5-2696 v4 @ 2.20 GHz, rustc 1.98.1 (48a229cea 2026-09-01). Release profile, LTO disabled for both variants. Originals are frozen from the starting commit in `scan_original.rs`, with visibility changes only; the source audit also checks all shared helper functions remained unchanged.

Four fixed runs followed one exploratory run. Every run alternates original/current order for ten samples, discards the first, and takes the median of nine. The table uses the median of the four original medians divided by the median of the four current medians. Its range shows all four individual run ratios. Allocation tracking uses the same thread-local instrumented System allocator on both sides and is disabled during timing. Sorting input clones and their destruction are outside timing on both sides; each operation receives fresh unsorted input. Group outputs and directory results are destroyed inside timing on both sides. Own builds and compatibility executions were idle during timing; the machine was not reserved exclusively.

| Operation | Allocations before -> after | Requested bytes before -> after | Observation |
|---|---:|---:|---|
| Sort 128 packs | 2 -> 1 | 2,048 -> 1,024 | Peak requested bytes also halves |
| Sort 4,096 packs | 3 -> 2 | 98,304 -> 65,536 | Peak stays 65,536 due to stable-sort scratch |
| Group 4,096 packs | 3 -> 2 | 98,304 -> 98,304 | Fewer allocations and indirection; same payload bytes |
| Scan 32 directories, 256 files, 32 resource-fork directories | 645 -> 101 | 35,518 -> 5,822 | 1.308x throughput |
| Scan 512 ordinary files | 1,028 -> 4 | 54,270 -> 510 | 1.474x throughput |

Allocation counts above exclude reallocations, which are unchanged (sorting/grouping: zero; mixed scan: four; file-only scan: one). Directory byte totals depend on fixture path lengths. These are allocated/requested bytes, not process RSS or retained heap measurements.

**Timing limits:** this is primarily an allocation reduction for pack sorting. The 128-pack reverse and 1,024-pack tie controls measured 2.5% and 3.9% longer respectively; the directory-only control measured 2.0% longer. Several other large sort cases were effectively neutral, and individual run ratios show host variability. This report does not claim that every input became faster or that whole-application startup improved by the microbenchmark ratios.

| Case | Original ns/op | Current ns/op | Throughput ratio | Four-run ratio range |
|---|---:|---:|---:|---:|
| `sort/8/sorted` | 558.59 | 503.76 | 1.109x | 1.078-1.216x |
| `sort/8/reverse` | 1,372.34 | 1,258.96 | 1.090x | 1.034-1.121x |
| `sort/8/shuffled` | 955.00 | 859.64 | 1.111x | 1.008-1.146x |
| `sort/8/ties` | 871.43 | 823.56 | 1.058x | 0.987-1.100x |
| `sort/128/sorted` | 6,154.68 | 5,593.75 | 1.100x | 1.086-1.117x |
| `sort/128/reverse` | 7,325.00 | 7,506.25 | 0.976x | 0.919-1.038x |
| `sort/128/shuffled` | 38,219.53 | 37,244.54 | 1.026x | 0.974-1.062x |
| `sort/128/ties` | 46,414.06 | 45,940.62 | 1.010x | 0.998-1.122x |
| `sort/1024/sorted` | 47,571.88 | 45,218.75 | 1.052x | 0.972-1.088x |
| `sort/1024/reverse` | 67,128.12 | 67,778.12 | 0.990x | 0.909-1.014x |
| `sort/1024/shuffled` | 443,940.62 | 443,715.62 | 1.001x | 0.988-1.008x |
| `sort/1024/ties` | 377,434.38 | 391,993.75 | 0.963x | 0.936-1.012x |
| `sort/4096/sorted` | 348,481.25 | 327,678.12 | 1.063x | 1.036-1.102x |
| `sort/4096/reverse` | 406,762.50 | 396,315.62 | 1.026x | 0.836-1.215x |
| `sort/4096/shuffled` | 2,861,487.50 | 2,888,150.00 | 0.991x | 0.988-1.005x |
| `sort/4096/ties` | 2,268,962.50 | 2,287,212.50 | 0.992x | 0.992-0.999x |
| `groups/8/false` | 489.40 | 394.00 | 1.242x | 1.188-1.273x |
| `groups/8/true` | 470.30 | 383.38 | 1.227x | 1.153-1.244x |
| `groups/128/false` | 7,138.72 | 5,530.81 | 1.291x | 1.257-1.299x |
| `groups/128/true` | 23,973.54 | 20,773.96 | 1.154x | 1.134-1.276x |
| `groups/1024/false` | 63,373.44 | 53,035.42 | 1.195x | 1.190-1.248x |
| `groups/1024/true` | 236,593.75 | 230,171.88 | 1.028x | 0.971-1.084x |
| `groups/4096/false` | 296,784.38 | 270,690.62 | 1.096x | 1.016-1.128x |
| `groups/4096/true` | 1,145,671.88 | 1,137,903.12 | 1.007x | 0.986-1.019x |
| `dirs/empty` | 145,254.16 | 144,493.75 | 1.005x | 0.996-1.044x |
| `dirs/directories` | 193,100.00 | 197,066.66 | 0.980x | 0.950-1.062x |
| `dirs/mixed` | 501,225.00 | 383,318.75 | 1.308x | 1.295-1.350x |
| `dirs/files` | 733,175.00 | 497,443.75 | 1.474x | 1.455-1.542x |

`groups/.../true` uses 13 repeated group names with mixed casing/whitespace; `false` uses distinct groups. Sort cases cover 8, 128, 1,024, and 4,096 packs, including stable ties. Directory controls cover empty and directory-only inputs.

## Regression and compatibility validation

- Original simfile suite: **203 passed, 0 failed**. Changed suite: **208 passed, 0 failed, 2 ignored**.
- New tests cover every permutation of up to seven packs, stable ties, non-ASCII names, representative selection against both the frozen original and the first matching input, filesystem filtering/errors, and allocation reductions.
- One ignored test is the manual benchmark. The other requires Windows symbolic-link privilege, which this environment lacks (WinError 1314 verified); its symlink/broken-link cases remain runnable on a suitable host. The production symlink and metadata-error fallback is unchanged.
- `rustfmt --check` for changed Rust files, `git diff --check`, frozen-source equivalence, scope audit, and the exact version-change audit passed.
- Fresh baseline/current Song Lua compatibility harness: **142 semantic tests passed, 38 failed, 77 ignored; 30 actor tests passed, 1 failed**, on both versions. Every original test outcome and all **39 failure diagnostics** matched after the documented nondeterministic-field normalization.
- Six full-song ITGmania archives: **1,650,933 comparisons passed, 0 failed**, on both versions. Per-archive totals: 363,873; 363,873; 304,425; 212,220; 205,071; 201,471.
- Artifact provenance confirms all three compatibility executables were rebuilt from this isolated worktree for both baseline and current runs. No new test failures were observed.

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile --lib benchmark_library_scan -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --no-run
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive-filename>
```

Run the generated semantic and actor executables serially with `--test-threads=1` and compare baseline/current per-test outcomes plus failure diagnostics. Normalize only thread IDs and the known unordered Sprite.Load diagnostic entries; do not suppress differences in failures. Whole-song selectors: Epidermis (both fixtures), Sharkmode, Warp Zone, Let Me Hear That, and Riddle.

Raw baseline/current logs, four fixed benchmark logs, the exploratory log, branch audit, and machine-readable summaries are retained in this worktree's ignored `target/` directory. Test/benchmark source is committed so results can be reproduced. The complete fixture store was restored before baseline execution after the initial disk-space constraint cleared. Compilation reused the previous isolated pass's build cache; source and logs belong to this worktree.

Excluded from the commit: `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, `optimize.ps1`. No merge or push.
