# Score kernel optimizations (2026-11-02)

Three independent optimizations in `deadsync-score`, based on `57cdd5ec4`:

1. `GhostData::update_best` checks whether a run can replace the saved run before
   validating every point. Tied and worse histories now take constant work;
   replacement histories still validate all points and use `clone_from`.
2. `parse_itl_points` uses a bounded `u64` accumulator instead of propagating
   `Option<u32>` through two checked arithmetic operations per digit. Values cap
   at 2^32, preserving the distinction between `u32::MAX` and overflow. Each
   subsequent multiplication fits `u64`; overflowing groups remain invalid.
   Exhausted input returns immediately, and the first digit needs no cap.
3. `parse_gs_comment_counts` scans ASCII digits as bytes, removing UTF-8 character
   decoding, character offsets and `len_utf8` from the numeric loop. Unicode
   trimming, saturating counts, suffix matching and duplicate/zero rules remain.

No production unsafe code or dependencies were added. The production diff is
17 added / 12 removed lines, including two explanatory comments. The removed
work is per-point validation for rejected runs, per-digit optional state and
checked arithmetic for ITL, and per-digit character decoding for GrooveStats.

## Measurements

CPU cycles per operation, medians across five rotating runs of the archived
production executables. A ghost operation updates three independent histories;
parser operations process 128 strings. The last column independently compares
frozen originals with final production functions in one executable across seven
runs, alternating variants within each run.

| Workload | Before cycles/op | After cycles/op | CPU throughput | Paired throughput |
| --- | ---: | ---: | ---: | ---: |
| Tied ghost histories, 64 points each | 132.87 | 7.04 | 18.874x | 27.853x |
| Tied ghost histories, 4096 points each | 7364.96 | 7.11 | 1035.859x | 964.142x |
| Worse ghost histories, 4096 points each | 7367.09 | 6.53 | 1128.191x | 1016.837x |
| ITL names, 128 charts | 5147.27 | 4094.86 | 1.257x | 1.255x |
| GrooveStats comments, 128 comments | 33790.05 | 29438.24 | 1.148x | 1.182x |

All 16 workloads have **zero allocations, reallocations and frees** inside the
measured operations, both before and after; each run asserts these counts.
Winning ghost updates reuse existing vector capacity. Fixture construction is
excluded. Winning and invalid-history controls remain in the raw results;
winning histories still require linear validation and copying. These are CPU
kernel measurements, not an application-wide frame-rate claim.

The raw [stage measurements](2026-11-02-score-kernels-final.tsv) include
`before`, `ghost`, `digits`, `bytes` and `final`, with
[source/compiler/binary hashes](2026-11-02-score-kernels-final.manifest.json).
The final [paired measurements](2026-11-02-score-kernels-final-paired.tsv) have
[a separate manifest](2026-11-02-score-kernels-final-paired.manifest.json).
Baseline and immediate before/after measurements were also recorded in the
`-before`, `-ghost`, `-digits` and `-bytes` TSV files and their manifests.

A candidate using `str::parse` after scanning each ITL numeric group was slower
and was discarded; `-itl.tsv` and its manifest record that experiment. Initial
bounded-accumulator measurements exposed extra work on digit-free names. The
final early exit removes that regression: 3633.39 -> 3328.51 cycles per 128 names
in the archived comparison, with the paired check approximately unchanged.
Intermediate binaries and logs remain under `target/score-kernels`.

## Method and reproduction

Windows build 26200; Intel64 Family 6 Model 154 Stepping 4; rustc 1.97.1
(8bab26f4f), LLVM 22.1.6, x86_64-pc-windows-msvc. Cargo release settings are
unchanged: opt-level 3, LTO, static MSVC runtime. The existing benchmark helper
pins the measuring thread to logical CPU 2 and reads `QueryThreadCycleTime`.
Each process measures 12 rounds and discards warmup. Inputs, outputs and function
pointers use `black_box`. Wall-clock nanoseconds are retained in the TSV files
as a secondary metric. Unchanged controls varied in early cross-build runs;
the final rotating runs and alternating same-executable checks provide the
reported comparisons.

Build each stage *before* editing it again:

```text
python scripts/benchmark_score_kernels.py --build STAGE
python scripts/benchmark_score_kernels.py --compare before ghost digits bytes final --runs 5 --output docs/performance/2026-11-02-score-kernels-final.tsv
python scripts/benchmark_score_kernels.py --paired --runs 7 --output docs/performance/2026-11-02-score-kernels-final-paired.tsv
```

`--build` compiles and archives the actual production integration-test binary,
runs correctness tests, and freezes source files and hashes. Comparison rejects
changed binaries, compilers, input fixtures or benchmark instrumentation.
Original function bodies are retained in `tests/support/score_kernels_original.rs`.

## Correctness and checks

The regression tests compare ghost update results and stored floating-point
**bits**, including independent bests, ties, signed zero, length changes, NaNs
and infinities. Numeric tests compare against the original functions over
integer boundaries, long overflows/leading zeros, Unicode, unknown suffixes,
duplicate counts and 16,384 deterministic generated strings.

Both debug and release test suites passed: **271 tests each**, with eight
benchmark tests ignored by the default runner. The new kernel benchmarks were
executed separately for the measurements above.

```text
cargo test -p deadsync-score --release --locked
cargo test -p deadsync-score --locked
cargo clippy -p deadsync-score --all-targets --locked --no-deps
```

The all-target Clippy check completed with existing warnings; none point to the
changed functions or new test files. A stricter run with `-- -D warnings` fails
on nine pre-existing diagnostics in the `deadsync-rules` dependency (including
`field_reassign_with_default` and `unnecessary_mut_passed`). No lint suppressions
or unrelated source fixes were added.

All new Rust test files and changed functions pass rustfmt checks. Whole-crate
`cargo fmt -p deadsync-score -- --check` reports pre-existing formatting in
`leaderboard.rs` and the unrelated `lib.rs` Routine score test. Those lines are
unchanged by this optimization.
