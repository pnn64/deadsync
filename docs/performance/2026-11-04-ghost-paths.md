# Ghost-history validation and persistence — 2026-11-04

Baseline: `03f965da8fc2b76b6d68350da5391490d0877a8a`. Three safe changes in `deadsync-score/src/ghost.rs`:

1. Build each validated native chart path in one capacity-reserved `PathBuf`, removing the formatted filename and cloned joins. Hash validation, path contents and extension behavior are preserved.
2. Open the path already validated by `save_best` and share the existing opened-file decoder with `load`. This removes the second hash scan and path construction. The number and order of file operations remain unchanged; the temporary path in public `load` is released before reading.
3. Check finite history points in eight-lane blocks using boolean AND, with scalar handling of the first point and remainder. This allows independent classification work to run together while preserving the fast first-point rejection. `update_best` and loaded JSON use the same predicate. No floating-point arithmetic changes.

Public types, serde attributes, signatures, stored JSON, vector contents and public vector capacities are preserved. Private path capacity and allocator activity intentionally change. No production unsafe code, dependency, runtime table or global state is added. The source diff has 32 added production lines and eight removed lines, plus four test-registration lines; the runtime simplifications remove repeated path building, hash validation and per-element short-circuit branching. The extra decoder helper owns an already-open file; persistence still uses the existing temporary write, rename and cleanup sequence.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz; rustc 1.99.1. Cargo release opt-level 3, fat LTO, static CRT. Five fresh processes per stage execute twelve alternating original/current rounds on CPU 2, discard the first and retain the median of eleven. Our CPU benchmark processes run sequentially. Other user builds can run concurrently. All fixture setup and allocation instrumentation are outside CPU timing; each allocation sample includes result destruction. Raw TSVs retain cycles, nanoseconds and all six allocator counters.

The baseline control uses the actual baseline production module and a test-only scalar predicate adapter; the independently frozen module still compiles at the other call site. Its ratios expose compiler-context and timing bias. Cycle columns below are independent process medians; paired throughput is the median of the five within-process ratios. Ranges in the machine-readable checks preserve all five ratios. These measurements establish kernel/allocation effects, not an application-wide or disk-throughput speedup.

Path fixtures call the real private helper 100,000 times with a 64-character hex hash. `finite/n/valid` scans all n values; numeric suffixes place infinity at that index. Iterations scale with the number of points actually examined, capped at one million, so first-point rejection has enough repetitions. Winning-update fixtures reset the last value of each existing vector before calling the complete public method; cloning reuses its warmed capacities. Save/load fixtures use real files in the host NTFS temporary directory. `save/unchanged/0` has no file and empty histories; 64/4096 have unchanged JSON histories. `save/path-reuse` holds the new path builder and finite predicate fixed while restoring only the repeated-path save body. Its allocation difference isolates the second optimization.

Normal and short path construction measure **1.685x** and **1.910x** paired throughput (controls 0.991x and 1.000x), reducing allocations **3 to 1** and reallocations **4/5 to 0**. Long and verbatim path CPU ratios (1.024x/1.026x) are too small to claim broadly; their allocation reductions remain concrete. Windows verbatim normalization still allocates internally: **7 to 5 allocations**, **10 to 9 reallocations**, rather than the single allocation of the ordinary paths.

Valid finite scans measure **1.303x to 1.726x**, including **1.400x to 1.726x** for 64-16,384 points. Complete winning updates measure **1.150x to 1.510x**, with controls 1.005x-1.049x. The isolated path-reuse save removes **5 allocations and 12 reallocations** per call across all three fixtures; combined changes remove **9 allocations and 14 reallocations**. The missing-file empty save measures 1.083x in the isolated ablation and 1.099x combined, against controls 0.997x/0.999x. Existing-file CPU ratios of roughly 0.995x-1.023x remain inconclusive, especially at 4,096 points.

The retained finite helper has a small early-exit cost: first-invalid cases measure **0.913x-0.931x** (about **7.4%-9.5% more cycles**, roughly **0.8-1.0 extra cycle**); their controls are 1.000x-1.018x. An invalid middle point in an eight-point block measures **0.912x**, or about **9.7% more cycles** (1.6 cycles; control 1.003x), because the block completes its independent lanes. The scalar prefix avoids the initial prototype's much larger first-point delay; it does not eliminate every cycle of overhead. These tradeoffs accompany the measured gains on valid histories and later-invalid points.

| Workload | Frozen reference cycles | Current cycles | Paired throughput | Identical-source control |
| --- | ---: | ---: | ---: | ---: |
| `finite/1024/0` | 9.01 | 10.00 | 0.913× | 1.000× |
| `finite/1024/1023` | 1485.81 | 983.20 | 1.494× | 0.997× |
| `finite/1024/512` | 759.13 | 522.51 | 1.440× | 0.994× |
| `finite/1024/valid` | 1461.20 | 951.84 | 1.549× | 1.000× |
| `finite/16384/0` | 9.14 | 10.01 | 0.924× | 1.011× |
| `finite/16384/16383` | 22318.84 | 16682.00 | 1.338× | 1.000× |
| `finite/16384/8192` | 11454.48 | 8488.53 | 1.356× | 1.014× |
| `finite/16384/valid` | 23903.91 | 17286.52 | 1.400× | 1.000× |
| `finite/64/0` | 8.59 | 9.52 | 0.915× | 1.004× |
| `finite/64/32` | 82.58 | 47.23 | 1.745× | 1.002× |
| `finite/64/63` | 128.31 | 67.58 | 1.861× | 0.992× |
| `finite/64/valid` | 122.88 | 68.29 | 1.726× | 1.001× |
| `finite/8/0` | 8.98 | 9.78 | 0.931× | 1.018× |
| `finite/8/4` | 16.50 | 18.11 | 0.912× | 1.003× |
| `finite/8/7` | 20.46 | 17.96 | 1.161× | 1.001× |
| `finite/8/valid` | 24.22 | 18.54 | 1.303× | 0.987× |
| `load/4096` | 1243818.70 | 1237387.40 | 1.004× | 0.995× |
| `load/64` | 278817.68 | 277726.77 | 0.996× | 0.997× |
| `path/long` | 2085.75 | 2003.27 | 1.024× | 1.003× |
| `path/normal` | 2009.85 | 1192.47 | 1.685× | 0.991× |
| `path/short` | 1870.39 | 1009.81 | 1.910× | 1.000× |
| `path/verbatim` | 5934.87 | 5537.03 | 1.026× | 1.009× |
| `save/path-reuse/0` | 100123.83 | 92261.33 | 1.083× | 0.997× |
| `save/path-reuse/4096` | 1646721.90 | 1632223.95 | 0.995× | 1.009× |
| `save/path-reuse/64` | 286967.72 | 277682.85 | 1.023× | 1.001× |
| `save/unchanged/0` | 103607.30 | 93917.46 | 1.099× | 0.999× |
| `save/unchanged/4096` | 1673808.15 | 1655666.55 | 1.004× | 1.001× |
| `save/unchanged/64` | 298780.10 | 294679.84 | 1.023× | 0.993× |
| `update/winning/1024` | 5725.89 | 3925.36 | 1.390× | 1.038× |
| `update/winning/16384` | 122990.26 | 102848.92 | 1.198× | 1.049× |
| `update/winning/64` | 401.52 | 265.89 | 1.510× | 1.019× |
| `update/winning/8` | 84.79 | 68.77 | 1.150× | 1.005× |

Allocation counters are per complete operation (fixture and report printing excluded). `allocated_bytes` includes reallocations; `peak_bytes` is the maximum additional live allocation. Differences in temporary-directory PID length can change byte totals between processes; the full observed range is shown.

| Workload | Allocations, old → current | Reallocations, old → current | Allocated bytes, old → current | Peak bytes, old → current |
| --- | ---: | ---: | ---: | ---: |
| `load/4096` | 12 → 10 | 55 → 54 | 462664–462668 → 462567–462570 | 229376 → 229376 |
| `load/64` | 12 → 10 | 31 → 30 | 11080–11084 → 10983–10986 | 3584 → 3584 |
| `path/long` | 3 → 1 | 3 → 0 | 4059 → 720 | 2706 → 720 |
| `path/normal` | 3 → 1 | 4 → 0 | 572 → 111 | 354 → 111 |
| `path/short` | 3 → 1 | 5 → 0 | 331 → 81 | 225 → 81 |
| `path/verbatim` | 7 → 5 | 10 → 9 | 1984 → 1827 | 813 → 640 |
| `save/path-reuse/0` | 11 → 6 | 24 → 12 | 7678–7682 → 3943–3946 | 1280 → 1152 |
| `save/path-reuse/4096` | 15 → 10 | 66 → 54 | 466302–466306 → 462567–462570 | 229504 → 229504 |
| `save/path-reuse/64` | 15 → 10 | 42 → 30 | 14718–14722 → 10983–10986 | 3712 → 3712 |
| `save/unchanged/0` | 15 → 6 | 26 → 12 | 7872–7878 → 3943–3946 | 1393–1394 → 1152 |
| `save/unchanged/4096` | 19 → 10 | 68 → 54 | 466496–466502 → 462567–462570 | 229504 → 229504 |
| `save/unchanged/64` | 19 → 10 | 44 → 30 | 14912–14918 → 10983–10986 | 3712 → 3712 |

Every finite-predicate and winning-update workload records zero allocations, reallocations, frees, allocated bytes, freed bytes and peak bytes. Raw data includes all six counters for every path and I/O fixture.

## Correctness and qualification

The baseline, final release, and package opt-level 0 with debug assertions and overflow checks each pass **262 unit tests and 2 score-kernel integration tests**. Six unit benchmarks/dumps and one integration benchmark remain ignored during the normal gates; the ghost benchmark and trace are explicitly executed. The opt-level 0 run keeps cached release dependency builds, so it is a target-package debug check rather than a full all-dependency debug build. These gates cover the score library plus its existing `score_kernels` integration target; other package integration targets are not claimed.

Differential checks compile frozen baseline code beside production and compare: native path `OsStr` bytes and invalid-hash errors; relative, drive-relative, UNC and verbatim Windows paths; Unicode and unpaired UTF-16 surrogates; long hashes; every invalid position in lengths 0–79; every binary64 exponent, both signs, five mantissas and eight lanes (163,840 predicate classifications); random update sequences, exact floating-point bits and actual public vector capacities; changed flags; exact JSON bytes and errors; malformed numbers; unknown JSON fields; signed zero; 32 MiB, 32 MiB+1 and 32 MiB+2 files; a forced temporary-write error; and directory import results. UNC/verbatim lexical fixtures do not perform network I/O.

The deterministic trace includes native paths/errors, the binary64 classification corpus and 4,096 random update sequences with flags, lengths, capacities and every stored f64 bit pattern. Its byte identity across the three profiles is checked without NaN normalization. JSON bytes and filesystem errors are compared directly in each profile’s tests.

- `baseline`: 2,324,769 trace bytes; SHA-256 `0fe394507f9ce4ce5aef715ee2ab02af3e1bfee1fb5d1bdc5be03d95170420ca`.
- `debug`: 2,324,769 trace bytes; SHA-256 `0fe394507f9ce4ce5aef715ee2ab02af3e1bfee1fb5d1bdc5be03d95170420ca`.
- `final`: 2,324,769 trace bytes; SHA-256 `0fe394507f9ce4ce5aef715ee2ab02af3e1bfee1fb5d1bdc5be03d95170420ca`.

Scoped Clippy for the library and tests passes with no ghost-source diagnostics. A production-only release `cargo check --lib` also passes. Both checks fingerprint source and build inputs. The audit confirms the frozen original module and isolated repeated-path save body against Git after canonical Rust formatting, preserves all other production functions and public data definitions, and checks every archived overlay, fixture, executable, profile, log, trace and raw TSV row.

The initial exploratory batch used a borrowed-path decoder and no scalar prefix. It passed the then-current tests, but delayed rejection of an invalid first point. An archived three-process primitive ablation measured the prefix and selected it before the final baseline/debug/release run. The exploratory manifest and source/binaries/log hashes are retained for provenance; their timings are not substituted for the final measurements.

## Reproduce and artifacts

Run from the isolated worktree, with `DEADSYNC_GHOST_PATHS_ARCHIVE` set to a scratch location with sufficient free space. The driver reconstructs the cached Cargo workspace from the archived source/dependency closure, verifies the source archive, overlays the current target crate and prior committed changes, restores required fixtures, and archives each stage’s sources and executables before another build can overwrite them. Compilation temporary files use the scratch volume; tests retain the host NTFS temporary directory.

```powershell
$env:DEADSYNC_GHOST_PATHS_ARCHIVE='D:\deadsync-ghost-paths-20261104'
python scripts/benchmark_ghost_paths.py baseline --baseline 03f965da8fc2b76b6d68350da5391490d0877a8a --runs 5 --reference baseline
python scripts/benchmark_ghost_paths.py debug --debug --reference baseline
python scripts/benchmark_ghost_paths.py final --runs 5 --reference baseline
```

The checked-in driver, private frozen references, differential tests, stage manifests, raw baseline/final TSVs and `2026-11-04-ghost-paths-checks.json` retain the reproducible inputs and results. Large binary snapshots and logs are local under `D:\deadsync-ghost-paths-20261104`; the manifests record absolute paths and SHA-256 values. The source archive and external fixture roots are local prerequisites recorded in the manifests.
