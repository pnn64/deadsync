# Song Lua VM reads: method names, registry identities and upvalue counts

This pass starts from `4277c1b63`. It follows M-HOTPATH, M-MEM-REUSE and the benchmark-before-unsafe requirement in [rust-performance.md](../../rust-performance.md). Three changes remove temporary representations from compiler preparation and actor dispatch:

1. Validate method names while their Lua strings remain rooted on the VM stack, returning only binding/frame-callback flags. This avoids the shared reference counter allocated by a temporary BorrowedStr guard. Binding and class lookup also borrow the existing key when pushing it, rather than cloning its handle. UTF-8 validation still precedes class/property reads and binding mutation; invalid-byte diagnostics and prefix/suffix/NUL semantics remain identical.
2. Scan registry entries by raw table identity under one existing read lock. This removes the Value/Table handle conversion and repeated locks for each table in the scan. It still stops at the first nil and applies checked mlua conversion to unusual non-table entries. Appending remains a protected raw write outside the read scope. No membership cache or persistent index is added, so authored replacements, holes and mutations remain observable. Duplicate scanning remains linear in registry size; registering many new actors remains quadratic overall.
3. Request only the upvalue count through Lua’s `>u` debug query. Recursive snapshot capture no longer constructs complete function metadata and owned source strings just to read that count. Snapshot representation, upvalue reads, restoration and function execution are unchanged.

The new helpers use the existing locked, stack-restoring raw-read boundary. Their reserved slots contain only rooted values, raw reads and checked conversions. The callback raw operations do not invoke authored code or allocate/throw in Lua; stack reservation and checked conversions use the existing guard. Only flags, a boolean or a primitive count escape. The existing boundary itself is unchanged. Public APIs, dependencies and mutable caches are unchanged.

## Measurements

The [raw samples and validation receipt](2026-10-21-song-lua-vm-reads.json) contain 54 samples per variant/workload: 648 normal-library samples and 648 production-helper samples. Complete-compile throughput changes range from +3.68% to +12.79%. Every median and per-round range is retained below.

Before production changes, six alternating prototype rounds measured the same raw-read bodies: method-name checks improved throughput by about 56% and removed one allocation per fresh name, 1,024-entry identity scans by about 6x, and count-only upvalue queries by about 8x while removing two allocations. These exploratory results justified the raw boundary work; prototype sources, binaries, raw rounds and build receipts are hashed. A source verifier checks that production helpers match these prototypes after formatting, helper names and module qualifications. The following tables measure actual production helpers and complete compiler calls.

Complete compiler measurements use two preserved normal-library executables with the identical unchanged compile_pipeline harness. Six pairs alternate variant order. Each fresh process warms one operation and retains nine samples. Prepared context and returned DTO formatting/final destruction are excluded; disk reads, host/VM creation, Lua execution, startup, extraction and finalization are included. The existing six inputs include offline function captures, actor-heavy live execution, runtime-created wrappers, two layers and 4,096 timing segments.

| Case | CPU cycles/operation | Allocations | Reallocations | Requested bytes | Operations/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Basic offline | 589,983,178 → 556,178,315.50 | 314,489 → 252,730 | 13,044 → 13,044 | 46,446,040 → 45,457,113 | 3.70 → 3.93 | +6.08% |
| Offline queues, cycles and function actions | 1,090,085,508.50 → 1,051,435,513.50 | 448,956 → 376,476 | 20,162 → 20,162 | 54,993,379 → 53,829,602 | 2.00 → 2.08 | +3.68% |
| Live 128 actors + 256 wrappers | 1,611,709,803.50 → 1,478,646,385 | 894,715 → 705,776 | 28,309 → 28,309 | 140,991,473 → 137,968,208 | 1.35 → 1.48 | +9.16% |
| Live 256 animated actors | 1,297,675,544.50 → 1,175,680,307 | 742,846 → 591,627 | 24,293 → 24,293 | 113,703,132 → 111,283,707 | 1.68 → 1.86 | +10.31% |
| Live two wrapper-heavy layers | 2,495,464,315.50 → 2,210,648,922 | 1,481,087 → 1,163,437 | 41,848 → 41,848 | 237,443,417 → 232,361,109 | 0.88 → 0.99 | +12.79% |
| Live wrappers + 4,096 timing segments | 1,614,560,780.50 → 1,443,092,770 | 902,714 → 713,781 | 36,509 → 36,509 | 142,097,966 → 139,075,038 | 1.35 → 1.51 | +11.86% |

Scoped before/after helpers run in one release test executable, with six alternating-order rounds and nine retained samples after one discarded sample. Each sample prepares a fresh Lua state outside timing. Name cases process 64 actors with 128 binding, assignment or lookup operations each. Assignment and lookup materialize fresh keys from a prepared Lua array inside timing, matching fresh callback/iterator handles. Lookup returns prepared class methods without invoking their bodies. Registration cases add 128 new actors to an initially empty registry, or repeatedly register the last actor in a 1,024-entry registry. Those rows normalize per registration. The count case queries one prepared 32-upvalue function 8,192 times, normalized per query. Temporary/result handle destruction is included; VM/input destruction is excluded. GC configuration is unchanged.

| Case | CPU cycles/operation | Allocations | Reallocations | Requested bytes | Operations/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Bind 128 method names | 260,609.96 → 185,115.66 | 140.02 → 12.02 | 5.16 → 5.16 | 32,968.75 → 30,920.75 | 8,391.57 → 11,830.27 | +40.98% |
| Assign 128 method names | 289,902.91 → 240,786.36 | 137.02 → 9.02 | 0.02 → 0.02 | 12,313.75 → 10,265.75 | 7,556.35 → 9,087.17 | +20.26% |
| Lookup 128 method names | 358,927.07 → 297,532.24 | 129 → 1 | 0.02 → 0.02 | 2,113.50 → 65.50 | 6,105.33 → 7,364.36 | +20.62% |
| Registration in a growing 0-127 entry registry | 25,732.09 → 5,094.80 | 1.01 → 0.01 | 0.05 → 0.05 | 47.88 → 31.88 | 84,787.86 → 428,165.25 | +404.98% |
| Register existing last actor in 1,024 entries | 353,232.95 → 57,515 | 0 → 0 | 0 → 0 | 0 → 0 | 6,194.29 → 38,094.10 | +514.99% |
| Query count for a 32-upvalue function | 1,026.79 → 113.76 | 2 → 0 | 0 → 0 | 123 → 0 | 2,131,446.12 → 19,068,901.30 | +794.65% |

Per-round throughput changes:

- Basic offline: aggregate +6.08%; rounds -0.07% to +9.31%; aggregate cycles -5.73%.
- Offline queues, cycles and function actions: aggregate +3.68%; rounds +0.25% to +8.18%; aggregate cycles -3.55%.
- Live 128 actors + 256 wrappers: aggregate +9.16%; rounds +5.58% to +15.45%; aggregate cycles -8.26%.
- Live 256 animated actors: aggregate +10.31%; rounds +5.25% to +14.29%; aggregate cycles -9.40%.
- Live two wrapper-heavy layers: aggregate +12.79%; rounds +8.52% to +23.85%; aggregate cycles -11.41%.
- Live wrappers + 4,096 timing segments: aggregate +11.86%; rounds +8.67% to +18.00%; aggregate cycles -10.62%.
- Bind 128 method names: aggregate +40.98%; rounds +33.44% to +49.23%; aggregate cycles -28.97%.
- Assign 128 method names: aggregate +20.26%; rounds +13.52% to +30.29%; aggregate cycles -16.94%.
- Lookup 128 method names: aggregate +20.62%; rounds +13.00% to +26.91%; aggregate cycles -17.11%.
- Registration in a growing 0-127 entry registry: aggregate +404.98%; rounds +302.96% to +448.80%; aggregate cycles -80.20%.
- Register existing last actor in 1,024 entries: aggregate +514.99%; rounds +473.86% to +567.37%; aggregate cycles -83.72%.
- Query count for a 32-upvalue function: aggregate +794.65%; rounds +628.16% to +988.16%; aggregate cycles -88.92%.

All measurements pin logical CPU 2, use 32-MiB worker stacks, rustc 1.98.1, release optimization, static Windows CRT and disabled LTO. No build, test or other benchmark overlaps authoritative measurements. Windows QueryThreadCycleTime counts scheduled worker cycles including kernel execution, not user-only PMU events. Normal-library counters cover Rust GlobalAlloc across the process; scoped counters cover the test thread. Both include mlua’s normal Rust-backed Lua allocator and exclude direct native malloc. Counter overhead is included. Requested bytes describe traffic rather than RSS.

Whole-compilation rows measure the combined changes; scoped gains are not additive whole-compiler estimates. The fixtures are not a representative chart corpus. The application enables song-lua test-support; these timings cover the default-feature library and test build. They do not establish application renderer FPS or full fixture-suite throughput.

## Behavior and validation

All complete DTO snapshots match exactly across 72 normal-library runs, including every measured sample. Raw reports retain snapshots, hashes and counters; Debug represents the retained VM by actor/tracked counts. The existing compiler differential test additionally compares all states and emitted outputs through eight live updates and compiles the queue/action fixture without unsupported function captures.

Four new checks compare method-name results/errors for empty, long, Unicode, invalid UTF-8 and embedded-NUL names; class lookup values, hooks, errors and frame-callback policy; registration with holes, duplicates, in-place changes, replacement registries, unusual values and live replay; and count-only debug queries for C, main, bound and 0/1/8/32/60-upvalue functions, including coroutine calls. Registry tests verify that table equality and registry metamethods do not run. Four frozen helpers match commit 4277c1b63 apart from visibility. Existing binding/error tests and the 10,000-node restoration test remain intact.

The verified baseline suite has 780 passes, 15 failures and 18 ignored benchmarks; the candidate has 784 passes, 15 failures and 20 ignored benchmarks. All fifteen normalized failure diagnostics match exactly. Before editing, the previous committed source, executable, suite log and build receipts were verified and the example/test executables retained. Current source, frozen helpers, fixtures, executables, raw reports and receipts are hashed. Normalization removes only source positions, workspace prefixes, thread/process IDs and the backtrace hint. Existing expectations and tolerances are unchanged.

## Reproduction

```powershell
cargo +1.98.1 build -p deadsync-song-lua --example vm_reads --release --locked --offline -j 2 --config profile.release.lto=false
# Prototype: vm_reads <report.json> <reverse:0|1>; run six alternating rounds.
cargo +1.98.1 build -p deadsync-song-lua --example compile_pipeline --release --locked --offline -j 2 --config profile.release.lto=false --message-format=json
# Preserve the baseline example before applying production changes, then rebuild.
python scripts/benchmark_song_lua_pipeline.py --suite startup --before <before.exe> --after <after.exe> --out target/vm-reads-pairs
cargo +1.98.1 test -p deadsync-song-lua --lib --release --locked --offline -j 2 --config profile.release.lto=false --no-run --message-format=json
# Run the reported test executable serially with RUST_MIN_STACK=33554432.
python scripts/benchmark_song_lua_compile.py --suite vm_reads --binary <test-executable> --out target/vm-reads-scopes.json
```
