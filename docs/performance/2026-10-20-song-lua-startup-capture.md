# Song Lua startup: function classification, flat snapshots and final buffer reservation

This pass starts from `31a7355f5` and follows M-HOTPATH, M-MEM-REUSE and M-INITIAL-CAPACITY in [rust-performance.md](../../rust-performance.md). Three changes remove work from compiler preparation and capture:

1. Classify functions using their environments. In the configured Lua 5.1 runtime, every Lua closure/main chunk has a table environment; mlua returns no environment for Rust/C functions. This replaces full FunctionInfo construction used only to inspect `what`, avoiding owned source/short-source strings and broader debug queries in method binding/assignment and queued/recursive discovery. Recursive capture reuses the returned environment instead of looking it up again. Upvalue-count queries remain unchanged.
2. Store captured table entries as one Lua-owned sequence of alternating keys and values. This removes the inner two-element table allocated for every entry. Expansion and restoration retain the same traversal order, key/value identity, cells and aliases. Lua-owned storage preserves the existing 8,000-reference auxiliary-stack constraint; no Rust vector of all table handles is introduced.
3. Reserve the final live actor vector from the extraction count before filling it. This removes vector growth and spare capacity retained by the direct one-layer return introduced in the preceding pass. Returned actors and indices stay identical.

The public API, dependencies, VM model, callback order and existing upvalue FFI remain unchanged. No production unsafe code or mutable cache is added. Function classification relies explicitly on the configured Lua 5.1 semantics; the new test includes Lua closures with no global accesses, main chunks, Rust/C functions, bound functions and custom environments.

## Measurements

The [raw measurements and validation receipt](2026-10-20-song-lua-startup-capture.json) include 54 samples per variant/workload: 648 normal-library samples and 324 scoped samples. Complete-compile throughput changes range from +5.71% to +10.13%. All medians and per-round ranges are retained below.

Normal-library measurements use two preserved executables with the identical unchanged compile_pipeline harness. Six serial pairs alternate variant order. Context preparation and returned DTO formatting/final destruction are excluded; disk reads, host/VM creation, Lua execution, startup, extraction and finalization are included. Each fresh process warms one operation, then records nine samples. The new offline fixture has eight queued actors, eight timed function actions and 64 shared cyclic nodes; it reports no unsupported function actions.

| Case | CPU cycles/operation | Allocations | Reallocations | Requested bytes | Operations/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Basic offline | 667,909,310 → 619,008,109.50 | 430,630 → 314,493 | 13,035 → 13,044 | 46,881,458 → 46,446,285 | 3.26 → 3.53 | +8.29% |
| Offline queues, cycles and function actions | 1,194,539,525.50 → 1,129,244,965 | 583,326 → 447,865 | 18,801 → 20,004 | 55,628,203 → 54,773,159 | 1.83 → 1.93 | +5.71% |
| Live 128 actors + 256 wrappers | 1,787,282,917 → 1,653,358,071 | 1,248,210 → 893,619 | 28,160 → 28,151 | 142,835,608 → 140,771,005 | 1.22 → 1.32 | +8.11% |
| Live 256 animated actors | 1,444,507,770.50 → 1,334,248,312.50 | 1,018,085.50 → 741,750 | 24,144 → 24,135 | 115,422,921 → 113,482,664 | 1.51 → 1.64 | +8.25% |
| Live two wrapper-heavy layers | 2,789,308,323 → 2,532,305,884.50 | 2,075,592 → 1,479,991 | 41,700 → 41,690 | 240,947,809 → 237,222,949 | 0.78 → 0.86 | +10.13% |
| Live wrappers + 4,096 timing segments | 1,773,903,954.50 → 1,627,343,579 | 1,256,193 → 901,618 | 36,360 → 36,351 | 143,941,371 → 141,877,494 | 1.23 → 1.34 | +9.07% |

Scoped before/after helpers execute in one release test binary. Six rounds alternate variant order, with nine retained samples after one discarded sample. Each sample prepares a fresh Lua state outside timing. Binding/assignment process 64 actors with 128 shared native method bindings each. Snapshot storage takes 64 snapshots of one prepared 256-field table, with GC stopped before timing; temporary Rust handle destruction is included, while Lua state destruction is excluded. These isolate classification and table representation rather than predicting complete-compile gains.

| Case | CPU cycles/operation | Allocations | Reallocations | Requested bytes | Operations/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Bind 128 native methods | 339,605.93 → 253,414.45 | 396.02 → 140.02 | 5.12 → 5.12 | 33,714.75 → 32,818.75 | 6,431.55 → 8,634.88 | +34.26% |
| Assign 128 native methods | 309,016.57 → 219,918.41 | 267.02 → 11.02 | 0 → 0 | 11,144.25 → 10,248.25 | 7,066.98 → 9,944.91 | +40.72% |
| Snapshot 256 fields | 386,961.35 → 201,519.87 | 518.02 → 6.02 | 8 → 9 | 32,992.25 → 16,608.25 | 5,645.26 → 10,828.28 | +91.81% |

Per-round throughput changes:

- Basic offline: aggregate +8.29%; rounds +6.09% to +8.75%; aggregate cycles -7.32%.
- Offline queues, cycles and function actions: aggregate +5.71%; rounds +1.33% to +7.93%; aggregate cycles -5.47%.
- Live 128 actors + 256 wrappers: aggregate +8.11%; rounds +2.56% to +14.17%; aggregate cycles -7.49%.
- Live 256 animated actors: aggregate +8.25%; rounds +2.67% to +13.61%; aggregate cycles -7.63%.
- Live two wrapper-heavy layers: aggregate +10.13%; rounds +8.38% to +11.89%; aggregate cycles -9.21%.
- Live wrappers + 4,096 timing segments: aggregate +9.07%; rounds +4.46% to +10.25%; aggregate cycles -8.26%.
- Bind 128 native methods: aggregate +34.26%; rounds +29.95% to +38.63%; aggregate cycles -25.38%.
- Assign 128 native methods: aggregate +40.72%; rounds +37.75% to +43.63%; aggregate cycles -28.83%.
- Snapshot 256 fields: aggregate +91.81%; rounds +87.08% to +95.05%; aggregate cycles -47.92%.

Measurements pin logical CPU 2, use 32-MiB worker stacks and rustc 1.98.1, release optimization, static Windows CRT and disabled LTO. No build, test or other benchmark overlaps the authoritative runs. Windows QueryThreadCycleTime counts scheduled worker cycles including kernel execution, not user-only PMU events. Normal-library allocation counters cover Rust GlobalAlloc across the process; scoped counters cover the current test thread. Both include mlua’s normal Rust-backed allocator and exclude direct native malloc. Accounting overhead is included. Requested bytes describe traffic rather than RSS.

Complete compiles measure the combined changes. The scoped helper gains are not additive estimates for the whole compiler. These fixtures are not a representative chart corpus, and snapshot storage with GC stopped is deliberately a narrow allocation/CPU experiment. The root application enables song-lua test-support; these benchmarks use the default-feature library and default-feature test build. They do not establish renderer FPS or full application/fixture-suite throughput.

## Behavior and validation

All 72 complete outputs compare exactly across before/after/trials. Raw reports retain complete Debug DTO snapshots, hashes and sample counters; Debug represents a live VM by actor/tracked counts. The existing differential compiler check additionally compiles the new capture fixture, and live fixtures retain eight compared updates with states and emitted outputs.

Four new checks validate C/Lua classification, binding and assignment routing/identity (including invalid UTF-8 errors), snapshot traversal order with binary/table/function keys, and exact restoration of cycles, shared tables, replaced upvalues, nil values and custom environments. Snapshot checks compare against four frozen helpers; binding checks compare against two frozen helpers. All six match git commit 31a7355f5 apart from private visibility. The existing 10,000-node restoration check also passes without changing Lua’s stack/reference limits.

The verified baseline has 776 passes, 15 failures and 16 ignored benchmarks; the candidate has 780 passes, 15 failures and 18 ignored benchmarks. All fifteen normalized failure diagnostics match exactly. Baseline production, executable, log and build-receipt hashes were verified before editing; baseline example/test executables were retained. Source, test helpers, fixture inputs, measured executables and receipts are hashed in the JSON report. Normalization removes only source positions, workspace prefixes, thread/process IDs and the backtrace hint. Existing expectations and tolerances are unchanged.

## Reproduction

```powershell
cargo +1.98.1 build -p deadsync-song-lua --example compile_pipeline --release --locked --offline -j 2 --config profile.release.lto=false --message-format=json
# Preserve the baseline example before applying the production diff, then rebuild.
python scripts/benchmark_song_lua_pipeline.py --suite startup --before <before.exe> --after <after.exe> --out target/startup-capture-pairs
cargo +1.98.1 test -p deadsync-song-lua --lib --release --locked --offline -j 2 --config profile.release.lto=false --no-run --message-format=json
# Run the reported test executable serially with RUST_MIN_STACK=33554432.
python scripts/benchmark_song_lua_compile.py --suite startup --binary <test-executable> --out target/startup-capture-scopes.json
```
