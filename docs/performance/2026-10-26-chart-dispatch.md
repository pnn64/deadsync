# Lane dispatch, Golf pack hashing and video paths

Baseline runtime source: `8139799a1`; measured on 2026-10-26. Isolated CPU throughput improves **1.527x** for Pump single lane decoding, **1.107x** for one-pack Golf hits, and **1.111x** for recognized-video paths. Combined original/final ratios for those fixtures are 1.515x, 1.060x and 1.096x. These warmed kernels and public metadata queries do not measure whole-screen or gameplay latency.

The production diff is **46 inserted / 45 removed lines**, including four lines registering the test module. Runtime code therefore loses 3 net lines. There is no new dependency, public type or unsafe Rust.

## Retained changes and exactness

1. `step_type_lanes` trims exactly as before, dispatches on the two accepted byte lengths, checks the separator once, then compares each shared family prefix once. Only Pump single/double and Dance double require non-default results. ASCII-insensitive comparisons and the four-lane fallback are unchanged. The length arms bound every byte/slice access. This replaces six repeated whole-name comparisons with a length/family/suffix decision.
2. The private Golf pack registry uses the existing `FxHashMap` alias instead of `HashMap`/RandomState. Pack roots are local library-scan keys, and the registry is only inserted into and queried; its iteration order is not exposed. The PathBuf key, Path hash/equality semantics, read/write lock, poison recovery, replacement logic and complete GolfPars implementation remain unchanged. Only the build hasher and its initialization change; one standard-library import is removed.
3. Video classification reads the extension's encoded bytes directly and dispatches on three/four-byte patterns. This removes an unnecessary Unicode-validity step, the variable-length lowercase loop, zero-filled temporary array and subsequent variable slice. Every accepted extension is ASCII. The standard library documents OsStr's encoding as a self-synchronizing superset of UTF-8/ASCII; a non-ASCII or invalid encoding cannot equal a complete accepted ASCII extension after ASCII-only case folding. `Path::extension` still chooses the same extension. Invalid names before a valid ASCII extension retain their old result. Byte `to_ascii_lowercase` is retained rather than blindly OR-ing the case bit: control byte 0x14 must not turn into the digit `4` in mp4/f4v/m4v.

The local Rust 1.99.1 `std/src/ffi/os_str.rs` documentation was checked for the encoded-byte guarantee. No unchecked string conversion, pointer cast or platform-specific production code is added.

## Consecutive before/after measurements

Each isolated step uses eight fresh processes per version, alternating order. Cycles are median process medians per complete 256-input fixture. CPU throughput is median-before / median-after; ranges contain every paired process ratio.

### Lane names

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `lanes/pumpdouble` | 7,619.05 | 7,102.03 | 1.073x | 0.861-1.280x |
| `lanes/pumpsingle` | 10,229.90 | 6,699.90 | 1.527x | 1.328-1.867x |
| `lanes/dancedouble` | 8,318.70 | 7,587.33 | 1.096x | 0.881-1.347x |
| `lanes/default` | 6,927.11 | 6,529.27 | 1.061x | 0.936-1.347x |
| `lanes/mixed` | 8,685.69 | 7,640.49 | 1.137x | 0.950-1.446x |
| `lanes/unknown` | 3,794.95 | 3,736.64 | 1.016x | 0.844-1.243x |
| `lanes/trimmed` | 11,472.31 | 9,939.23 | 1.154x | 0.932-1.379x |
| `lanes/unicode` | 4,954.19 | 4,846.15 | 1.022x | 0.914-1.270x |

[Complete step data](2026-10-26-dispatch-lanes.tsv).

### Golf registry

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `golf/onehit` | 206,048.77 | 186,136.00 | 1.107x | 1.042-1.203x |
| `golf/onemiss` | 167,711.71 | 153,855.78 | 1.090x | 0.945-1.211x |
| `golf/smallhit` | 205,620.04 | 189,589.71 | 1.085x | 0.982-1.141x |
| `golf/smallmiss` | 172,465.27 | 158,876.85 | 1.086x | 1.011-1.130x |
| `golf/largehit` | 212,047.31 | 200,794.47 | 1.056x | 1.013-1.140x |
| `golf/largemiss` | 171,933.67 | 157,741.64 | 1.090x | 0.951-1.145x |
| `golf/longhit` | 325,247.55 | 339,367.56 | 0.958x | 0.800-1.023x |
| `golf/longmiss` | 175,997.86 | 165,077.70 | 1.066x | 0.889-1.232x |

[Complete step data](2026-10-26-dispatch-golf.tsv).

### Video paths

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `video/video` | 39,357.38 | 35,432.10 | 1.111x | 1.053-1.181x |
| `video/image` | 34,972.54 | 31,234.18 | 1.120x | 1.037-1.178x |
| `video/mixed` | 35,473.25 | 31,983.55 | 1.109x | 0.979-1.207x |
| `video/unicode` | 38,932.10 | 35,077.12 | 1.110x | 0.952-1.150x |
| `video/rejected` | 40,190.79 | 34,741.02 | 1.157x | 1.058-1.205x |

[Complete step data](2026-10-26-dispatch-video.tsv).

## Original/final measurements

Twelve fresh processes per version compare all three changes against the original runtime. [Complete data](2026-10-26-dispatch-final.tsv).

| Fixture | Before cycles | After cycles | CPU throughput | Before -> after ns | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: |
| `lanes/pumpdouble` | 8,847.74 | 8,069.19 | 1.096x | 4,048.44 -> 3,714.84 | 1.072-1.281x |
| `lanes/pumpsingle` | 12,003.91 | 7,923.43 | 1.515x | 5,480.08 -> 3,603.52 | 1.423-1.783x |
| `lanes/dancedouble` | 9,468.51 | 8,313.57 | 1.139x | 4,307.03 -> 3,781.64 | 1.069-1.310x |
| `lanes/default` | 8,448.17 | 6,497.55 | 1.300x | 3,894.92 -> 2,954.30 | 1.171-1.490x |
| `lanes/mixed` | 10,214.47 | 8,112.07 | 1.259x | 4,668.75 -> 3,699.61 | 1.143-1.492x |
| `lanes/unknown` | 4,372.85 | 4,256.24 | 1.027x | 1,985.94 -> 1,933.59 | 0.959-1.128x |
| `lanes/trimmed` | 13,230.02 | 10,818.09 | 1.223x | 6,039.85 -> 4,925.78 | 0.960-1.344x |
| `lanes/unicode` | 5,771.31 | 5,767.88 | 1.001x | 2,623.82 -> 2,625.78 | 0.946-1.150x |
| `video/video` | 39,360.81 | 35,924.26 | 1.096x | 17,967.18 -> 16,360.93 | 0.932-1.287x |
| `video/image` | 35,382.38 | 31,628.58 | 1.119x | 16,121.09 -> 14,394.53 | 0.983-2.087x |
| `video/mixed` | 35,632.74 | 32,448.27 | 1.098x | 16,241.40 -> 14,796.88 | 0.923-1.298x |
| `video/unicode` | 39,808.39 | 36,034.01 | 1.105x | 18,126.57 -> 16,399.22 | 1.041-1.297x |
| `video/rejected` | 40,187.37 | 35,805.94 | 1.122x | 18,332.03 -> 16,372.66 | 1.060-1.235x |
| `golf/onehit` | 196,579.39 | 185,491.23 | 1.060x | 89,600.00 -> 84,546.88 | 1.002-1.155x |
| `golf/onemiss` | 161,733.78 | 154,946.42 | 1.044x | 73,892.18 -> 70,750.00 | 1.025-1.127x |
| `golf/smallhit` | 202,958.62 | 190,772.96 | 1.064x | 92,590.62 -> 87,032.82 | 1.021-1.291x |
| `golf/smallmiss` | 170,105.65 | 160,814.61 | 1.058x | 77,768.75 -> 73,306.25 | 0.910-1.332x |
| `golf/largehit` | 216,763.15 | 197,851.79 | 1.096x | 98,964.07 -> 90,409.38 | 1.017-1.200x |
| `golf/largemiss` | 170,633.81 | 161,020.39 | 1.060x | 77,809.38 -> 73,381.25 | 0.953-1.838x |
| `golf/longhit` | 334,689.44 | 335,670.38 | 0.997x | 152,771.88 -> 153,089.07 | 0.854-1.057x |
| `golf/longmiss` | 176,598.03 | 160,265.88 | 1.102x | 80,587.50 -> 73,065.62 | 1.014-1.162x |

Long-path Golf hits do not improve: the isolated median costs 4.3% more cycles (0.958x), and the combined median costs 0.3% more (0.997x). Their paired ranges span 1x. The small/large registry results support retaining the hasher change; it is not a universal win for every path shape. Other paired ranges also span 1x, so improved medians do not establish a gain on every process run. Unchanged controls shift across separately linked binaries and processes; the complete TSVs retain them and do not establish the cause of every shift.

## Allocations and method

All six scoped allocator counters remain **zero before and after for every one of the 21 fixtures and all 1,512 process/workload records**: allocations, reallocations, frees, requested bytes, freed bytes and scoped peak bytes. Inputs and registry construction stay outside timing and the allocation probe. These changes improve CPU cost; no heap-allocation reduction is claimed for queries.

Windows x86_64 MSVC, Intel Xeon E5-2696 v4 (44 logical processors), rustc 1.99.1 / LLVM 23.1.1. The four unit-test archives use `--release --locked`, default release LTO and unchanged CPU/static-runtime settings. The existing helper pins the benchmark thread to logical processor 2 and reads `QueryThreadCycleTime`. Each process discards one warmup sample and retains the median of eleven measured samples. A sample repeats a 256-input fixture 128 times for lane decoding, 64 for video paths and 32 for Golf queries. The driver alternates before/after fresh-process order. Inputs/results use `black_box`. The existing System-allocator wrapper probes one complete fixture separately, then disables tracking during timing; allocator requests still pass through the wrapper. No compilation started by this task overlaps retained measurements. Unrelated shared-machine activity is uncontrolled.

Lane fixtures cover each recognized family, default Dance single, all aliases, unknown names, Unicode and leading/trailing Unicode whitespace. Video fixtures cover recognized extensions, images, mixed types, long Unicode directory prefixes and rejected Unicode/control extensions. There is no filesystem I/O in these path-classification or Golf-query fixtures.

The Golf benchmark registers 1, then 64, then 1024, then 128 additional packs under distinct prefixes in the same fresh process: cumulative registry sizes are 1, 65, 1089 and 1217. Each fixture prepares 256 query paths, cycling through its new packs; the 1024-pack fixture queries its first 256 packs. The long-hit fixture adds 126 ASCII padding bytes to pack names. Miss queries replace the complete pack filename with an absent name, so the long-miss label does not represent long padded paths. Golf data and difficulty queries are fixed; locking is uncontended, and this test does not model multithread contention or library loading.

## Correctness and validation

The lane check compares frozen original bodies on all ASCII case variants of six names, every one-byte ASCII mutation, inserted Unicode/control characters, 100000 generated ASCII strings and Unicode trim boundaries. It also compares complete parsed note-event vectors using the original and new lane counts. Video checks cover all extension case variants, every ASCII byte mutation, Unicode suffixes/prefixes, path-component edge cases, control-byte/digit distinctions and every unpaired UTF-16 surrogate on Windows, both in extensions and before valid ASCII extensions. Unix builds additionally check invalid byte paths. Golf checks compare public replacement/query behavior and returned f64 bits, including negative zero, overrides, empty replacement, absent packs/songs and path-component equivalence, against a frozen complete original runtime. Each archived stage passes all three ordinary checks.

Source audits reconstruct all three complete baseline runtime files, verify the frozen bodies and full Golf runtime, verify fixed harness/helper/driver hashes, compare final production bytes with the measured snapshot and verify every raw record's executable digest. Root Cargo configuration, lockfile and chart manifest match the baseline. Excluded Lua edits in the shared worktree are outside this pass.

Validation passes: **579 release tests** across chart/score/simfile (33 ignored), **308 debug tests** across chart/score (14 ignored), scoped rustfmt for every changed runtime fragment and all three new Rust files, and chart/score/simfile Clippy with `--no-deps`. Existing warnings remain; there are no Clippy diagnostics in changed runtime code or the new test/reference files. Broad validation disables release LTO to reduce linking time; performance archives retain default LTO.

Sources, archived executables, logs and audit scripts remain under `target/chart-dispatch/`. The four TSVs preserve all elapsed times, counters and executable SHA-256 values.

```powershell
cargo test --release --locked -p deadsync-chart --lib --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_metadata_dispatch --log-dir target/chart-dispatch/runs --before target/chart-dispatch/before/chart.exe --after target/chart-dispatch/lanes/chart.exe --output docs/performance/2026-10-26-dispatch-lanes.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_metadata_dispatch --log-dir target/chart-dispatch/runs --before target/chart-dispatch/lanes/chart.exe --after target/chart-dispatch/golf/chart.exe --output docs/performance/2026-10-26-dispatch-golf.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_metadata_dispatch --log-dir target/chart-dispatch/runs --before target/chart-dispatch/golf/chart.exe --after target/chart-dispatch/after/chart.exe --output docs/performance/2026-10-26-dispatch-video.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_metadata_dispatch --log-dir target/chart-dispatch/runs --before target/chart-dispatch/before/chart.exe --after target/chart-dispatch/after/chart.exe --output docs/performance/2026-10-26-dispatch-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-chart -p deadsync-score -p deadsync-simfile -- --test-threads=1
cargo test --locked -p deadsync-chart -p deadsync-score -- --test-threads=1
cargo clippy --no-deps --release --locked --config profile.release.lto=false -p deadsync-chart -p deadsync-score -p deadsync-simfile --all-targets
```

Keep the committed harness/reference files fixed. Restore the lane function, private Golf registry/import and video method/helper from `8139799a1` for `before`, keeping the test registration. Apply lane dispatch for `lanes`, registry hashing for `golf`, and video classification for `after`. Build/archive the single reported unit-test executable at each stage and run its three ordinary correctness tests before collecting measurements.
