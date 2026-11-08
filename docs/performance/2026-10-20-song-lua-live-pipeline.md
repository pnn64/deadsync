# Song Lua live compile: direct ownership and conditional lookup tables

This pass starts from `8892bfd18`. It follows M-HOTPATH, M-MEM-REUSE and M-THROUGHPUT in [rust-performance.md](../../rust-performance.md). Three production changes remove preparation work:

1. A single live layer with layer zero ownership and already-ordered parents returns its compiled output directly. Its overlay/live-scene indices already match. This avoids splitting an owned DTO into one new DTO, identity remapping, temporary actor storage and copied output buffers. Other layer/order inputs retain the existing splitter and validation.
2. Live extraction borrows its existing context. The caller is inside `if context.live`, so cloning it and assigning `live = true` repeats information already available. This removes owned strings/vectors and timing-data copies.
3. Extraction builds the table-address map only for actor-target proxies. Late resolution builds it only when any ActorProxy exists, because a player-target proxy may rebind to an actor through Lua getters. Proxy resolution, duplicate-address last-wins behavior and Actor-to-Sprite fallback remain unchanged. The guards inspect Rust enum fields without invoking Lua.

The one-layer guard checks every layer and parent before taking the shortcut. It does not skip actor decoding or Lua property reads. An authored `__newindex` can override layer markers; that input still falls through to the original invalid-layer error. No production unsafe code, dependency, public API or mutable cache is added. The benchmark-only affinity call pins its disposable worker thread.

## Measurements

Proxy-free extraction improves throughput by 8.75% and cycles by 8.06%, with every pair improving throughput (+4.17% to +10.74%). It removes one allocation and 4,368 requested bytes per call. Borrowing the prepared 4,096-segment context removes nine allocations and 32,874 requested bytes; the within-executable throughput gains are +7.49% and +17.79%. The single-layer wrapper compile removes 23 allocations and 990,632 requested bytes. Four complete-compile medians are slower by 0.80-1.54%, and the timing-heavy case improves by 0.10%. These measurements do not demonstrate an overall compile speedup.

The [raw measurements and validation receipt](2026-10-20-song-lua-live-pipeline.json) record six serial pairs with alternating variant order, nine retained samples per run, and 54 samples per variant/case (972 total). Each executable uses the normal library with no song-lua test-support/performance features. Separate before/after executables use the identical harness, counted System allocator, rustc 1.98.1, release optimization, static Windows CRT and disabled LTO. Logical CPU 2 and a 32-MiB worker stack are used. No build or test overlaps the authoritative measurements.

Complete compilation starts with an already-prepared context and includes disk reading, host/VM creation, Lua execution, startup commands, actor decoding and finalization. Returned DTO formatting and final destruction are outside timing. Extraction uses prepared minimal Lua tables (128 Quads, optionally one ActorProxy); context cases use an empty root and 4,096 BPM segments, isolating clone cost before the same extraction call. Scoped samples batch 64 calls; earlier batch outputs are destroyed inside timing and the last one afterward. Each process warms one operation first.

| Case | CPU cycles/operation | Allocations | Reallocations | Requested bytes | Operations/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Basic offline control | 560,060,397.50 → 566,547,939 | 430,633 → 430,626 | 13,035 → 13,035 | 46,881,670 → 46,881,254 | 3.90 → 3.86 | -1.11% |
| Live 128 actors + 256 wrappers | 1,515,474,968.50 → 1,531,850,332 | 1,247,456 → 1,247,433 | 28,000 → 28,000 | 143,749,650 → 142,759,018 | 1.44 → 1.43 | -1.04% |
| Live 256 animated actors | 1,200,535,497.50 → 1,211,499,193 | 1,017,331 → 1,017,314 | 23,984 → 23,984 | 116,021,914 → 115,346,552 | 1.82 → 1.80 | -0.80% |
| Live two wrapper-heavy layers | 2,379,672,386 → 2,416,716,948 | 2,074,832 → 2,074,815 | 41,540 → 41,540 | 240,906,456 → 240,871,219 | 0.92 → 0.90 | -1.54% |
| Live wrappers + 4,096 timing segments | 1,524,062,698.50 → 1,522,606,085 | 1,255,455 → 1,255,412 | 36,200 → 36,200 | 144,888,937 → 143,864,547 | 1.43 → 1.44 | +0.10% |
| Scoped owned-context control | 74,806.62 → 72,080.02 | 25.03 → 25.03 | 0 → 0 | 33,515.38 → 33,515.38 | 29,325.51 → 30,292.27 | +3.30% |
| Scoped borrowed-context control | 63,387.48 → 67,228.74 | 16.03 → 16.03 | 0 → 0 | 641.38 → 641.38 | 34,543.25 → 32,560.86 | -5.74% |
| Extract 128 Quads, no proxy | 4,534,827.21 → 4,169,112.64 | 791.73 → 790.73 | 7 → 7 | 349,887.62 → 345,519.62 | 482.59 → 524.84 | +8.75% |
| Extract 128 Quads + actor proxy | 4,401,029.84 → 4,416,487.49 | 797.25 → 797.25 | 8 → 8 | 681,759.25 → 681,759.25 | 497.37 → 495.15 | -0.45% |

Whole-compile results measure the combined changes; they are not independent estimates of each optimization. Host setup and Lua initialization dominate the stage profile used to select this work. The offline case exercises map elision but bypasses the live changes; the two-layer case still uses the splitter while borrowing the context and skipping unused maps. Scoped extraction includes all Lua reads and DTO construction, so it measures more than map creation. The proxy case retains map construction and is a control for elision. Context owned/borrowed rows above are each before/after controls; the following within-executable comparison isolates borrowing instead of cloning:

| Executable | Owned → borrowed CPU cycles | Allocations | Requested bytes | Throughput change |
| --- | ---: | ---: | ---: | ---: |
| before | 74,806.62 → 63,387.48 | 25.03 → 16.03 | 33,515.38 → 641.38 | +17.79% |
| after | 72,080.02 → 67,228.74 | 25.03 → 16.03 | 33,515.38 → 641.38 | +7.49% |

Per-pair throughput changes preserve observed variation:

- Basic offline control: aggregate -1.11%; pairs -1.33% to +0.02%; aggregate cycles +1.16%.
- Live 128 actors + 256 wrappers: aggregate -1.04%; pairs -2.08% to +1.99%; aggregate cycles +1.08%.
- Live 256 animated actors: aggregate -0.80%; pairs -6.00% to +1.87%; aggregate cycles +0.91%.
- Live two wrapper-heavy layers: aggregate -1.54%; pairs -4.57% to +9.80%; aggregate cycles +1.56%.
- Live wrappers + 4,096 timing segments: aggregate +0.10%; pairs -1.92% to +0.34%; aggregate cycles -0.10%.
- Scoped owned-context control: aggregate +3.30%; pairs -6.01% to +7.58%; aggregate cycles -3.64%.
- Scoped borrowed-context control: aggregate -5.74%; pairs -6.88% to +7.65%; aggregate cycles +6.06%.
- Extract 128 Quads, no proxy: aggregate +8.75%; pairs +4.17% to +10.74%; aggregate cycles -8.06%.
- Extract 128 Quads + actor proxy: aggregate -0.45%; pairs -7.29% to +0.75%; aggregate cycles +0.35%.

These fixtures exercise real compiler paths but are not a representative chart corpus. The timing-segment and extraction cases are synthetic. The root application enables song-lua test-support; these measurements cover the default-feature library instead of that application feature configuration. No gameplay FPS, whole-application throughput or full-fixture-suite duration improvement is established. The combined benchmark does not isolate the cause of the small complete-compile slowdowns. Allocation reductions describe requested traffic, not RSS. The direct one-layer return retains the original actor vector and its spare capacity; it does not promise the exact capacity formerly allocated by the splitter.

CPU cycles use Windows QueryThreadCycleTime for the pinned worker, including scheduled kernel execution; they are not user-only PMU counts. Allocation accounting covers Rust GlobalAlloc across the process, including mlua’s normal Rust-backed allocator, and excludes direct native malloc. Counter overhead is included. Independent fresh processes and alternating variant order reduce persistent allocator and ordering effects.

## Behavior and validation

Each measured output is formatted only after timing and compared exactly against the same workload’s complete Debug DTO snapshot across every before/after run. Raw run reports retain those snapshots. Debug represents the retained Lua scene by actor/tracked counts. The existing complete-compiler differential test additionally compares states and emitted outputs through eight live advances for offline, one-layer and two-layer fixtures.

Two new checks cover empty live sessions and authored invalid layer markers, plus late Actor-to-AftSprite fallback without proxies, player-to-actor proxy rebinding, duplicate table references (last index wins), hidden player targets, dynamic getter order and getter errors. They compare exact results/errors and Lua read logs with the frozen compiler helper. The frozen compiler matches bacdc3e37 with three private visibility adapters; it is a behavior oracle, not this pass’s timing baseline.

The verified baseline suite has 774 passes, 15 failures and 16 ignored benchmarks; the candidate has 776 passes, 15 failures and 16 ignored benchmarks. All fifteen normalized failure diagnostics match exactly. Baseline production source, executable, full-suite log and build receipt hashes were verified against the previous committed pass before editing production code. Its test executable was retained separately. Normalization removes only Rust source positions, workspace prefixes, test thread/process IDs and the backtrace hint. Existing expectations and tolerances are unchanged. Candidate source, executables, build receipts, raw reports and test evidence are hashed in the JSON receipt.

## Reproduction

```powershell
cargo +1.98.1 build -p deadsync-song-lua --example compile_pipeline --release --locked --offline -j 2 --config profile.release.lto=false --message-format=json
# Preserve the baseline executable before applying the production diff, then rebuild.
python scripts/benchmark_song_lua_pipeline.py --before <before.exe> --after <after.exe> --out target/live-pipeline-pairs
cargo +1.98.1 test -p deadsync-song-lua --lib --release --locked --offline -j 2 --config profile.release.lto=false --no-run --message-format=json
# Execute the reported test binary with --test-threads=1 and RUST_MIN_STACK=33554432.
```
