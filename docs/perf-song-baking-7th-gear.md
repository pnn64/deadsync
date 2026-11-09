# 7th Gear baking: ordinary Rust host table reads

Measured on 2026-11-09, starting at `b282637b68873e4508b6ecddf9427bc4b1b28591`.
This pass applies recommendation 9, **fast Rust host calls**, from the supplied
optimization notes. The production change is confined to ordinary field reads
in `deadlib-lua`'s host action boundary.

Across five alternating pairs, the median full bake fell from **49.315 to
47.389 seconds (3.90% shorter)**. Main-thread cycle counts fell 3.89%. Timing
variation is material: one pair was slower and another was nearly unchanged.
This is a modest measured improvement on this host, with all samples retained in
[the results JSON](perf-song-baking-7th-gear-results.json).

## Repeated measurements

Each row is two serial, fresh processes pinned to logical CPU 2. Order alternates
between before/after and after/before; no build or profiler ran concurrently.
The initial baseline and both profiled runs are excluded from these medians.

| Pair | Order | Before seconds | After seconds | Before / after |
| --- | --- | ---: | ---: | ---: |
| 1 | before, after | 50.307 | 51.688 | 0.973x |
| 2 | after, before | 49.315 | 45.014 | 1.096x |
| 3 | before, after | 48.487 | 44.228 | 1.096x |
| 4 | after, before | 47.530 | 47.389 | 1.003x |
| 5 | before, after | 51.038 | 49.078 | 1.040x |

The paired speedup range is **0.973-1.096x**, not a confidence interval. The first
three pairs suggested an 8.72% lower median; two additional pairs were collected
because of the variation, and all five are used for the final 3.90% figure.
Allocation counts (28,931,641), reallocations (757,456) and cumulative requested
bytes (about 2.669 GB) are effectively unchanged.

Every repeated bake and both profiles produced the exact same complete compiled
DTO fingerprint, with no normalization:

```text
c85bc7f33501e734a1e29a83d1712ff0054cd8810810f90a865fba59fd0c47ac
```

The second profile reduced the inclusive `HostCall::table_get` sample share from
13.57% to 7.43%. Sampling overhead and stack depth affect those shares; the
unprofiled repeated measurements above establish the whole-song result.

## Workload and profile

The input is `C:/GitHub/rework/lua-songs/7th Gear/7th Gear.ssc`, raw chart index 4
(single Challenge), OpenGL, one active player, default profiles, 640 x 480,
rate 1, autoplay 1, seed 0, and explicit Baked mode. The OpenGL branch matters:
the complete compiled scene has **389 actors and three ActorFrameTextures**.
The benchmark asserts successful preparation and that no live Lua VM remains.

The measured scope is the actual gameplay loader's `prepare_song_lua`, including
context/timing preparation. Initial simfile/chart parsing and the complete DTO's
Debug/SHA-256 fingerprint are outside that scope. Allocation counts cover Rust
`GlobalAlloc` on all process threads; native malloc is excluded. CPU cycles come
from Windows `QueryThreadCycleTime` on the baking thread.

The first unprofiled baseline took **51.022 seconds**, with 28,931,641 allocations,
757,456 reallocations and 2,668,674,351 requested allocation bytes. The allocation
bytes are cumulative requests, including reallocations, rather than peak memory.

A process-local Windows sampler captured the owned child process's main thread
using `SuspendThread`, `GetThreadContext`, `StackWalk64`, and PDB/DbgHelp symbols.
It requested a 5 ms interval and retained 5,967 samples inside the benchmark's
recorded preparation timestamps. Sampling and stack unwinding add substantial
pause overhead: the profiled wall time was 93.394 seconds, including 38.648
seconds suspended across the whole process capture. Profiled times are used only
to locate work, never to calculate the improvement below.

The chronological update pass accounts for **98.36%** of baking samples and
replays 8,041 frames. Relevant inclusive sample shares (overlapping, not additive):

| Function | Baseline samples |
| --- | ---: |
| `HostCall::table_get` | 13.57% |
| `Table::get` | 9.77% |
| `Table::slot` | 6.77% |
| `Vm::maybe_gc` | 5.85% |

`Vm::run` also has 15.25% self samples. Rather than redesigning dispatch, value
layout or GC, this pass targets the unnecessary continuation setup and cleanup
around ordinary host table reads. The loader's environment fallback is one
frequent caller of that boundary.

## Change and correctness

After the existing pending-error and handle checks, a host `Get` action reads a
table directly when its raw value is non-nil or it has no metatable. Such a read
cannot call Lua. It stores the value in the existing scratch-result register and
returns without constructing a continuation frame or running nested-call cleanup.

The result retains its existing temporary-root lifetime. Metatable fallback,
non-table indexing, error protection, and calls that can enter Lua use the
existing driver. No table values or metamethod resolutions are cached, so later
field and metatable writes remain visible. Sampling frequency, actor ordering,
float calculations and the song's input files are unchanged.

Regression coverage checks collection while a result is held only by the scratch
slot; appended results and arguments across reentry; a false field followed by
metamethod fallback after deletion; later ordinary writes; stale table and key
handles; and ordinary reads attempted after an instruction-limit abort.

**1,048 tests passed:** the complete `deadlib-lua` library/integration suite
(956), plus the song bridge's `loader`, `unit_environment`, `unit_class_lookup`,
`song_multitap`, `song_compiler`, `song_update_output`, `song_column_output`, and
`preparation` targets (92). The standalone stdio allocation-failure harness also
completed successfully. These test builds used opt-level 0 for the Lua crates
and debug information disabled; the production timing binaries use the release
settings below. Frozen fixture expectations were not regenerated.

```powershell
cargo test -p deadlib-lua --lib --tests --config profile.dev.package.deadlib-lua.opt-level=0 --config profile.dev.debug=0 -j 2
cargo test -p deadsync-song-lua --features test-support --test loader --test unit_environment --test unit_class_lookup --test song_multitap --test song_compiler --test song_update_output --test song_column_output --test preparation --config profile.dev.package.deadlib-lua.opt-level=0 --config profile.dev.package.deadsync-song-lua.opt-level=0 --config profile.dev.debug=0 -j 2
```

## Reproduction

Both variants use the existing normal-library production benchmark target:

```powershell
cargo build --release --config profile.release.lto=false --config profile.release.debug=1 -p deadsync-shell --bin song_lua_compile_performance --features song-lua-performance --message-format=json -j 2
```

Cargo's artifact records confirm `deadsync-song-lua` has only the `performance`
feature, which exposes the compiled layers for fingerprinting. `test-support`
and its diagnostic observations are disabled. Building the root example would
unify development features and is unsuitable for this timing comparison.

Copy each variant's executable before rebuilding the other, then run each in a
fresh process with affinity mask 4 (logical CPU 2):

```powershell
song_lua_compile_performance.exe --compile 'C:/GitHub/rework/lua-songs/7th Gear/7th Gear.ssc' --report result.json --iterations 1 --renderer OpenGL --chart-index 4
```

Host: Windows x86_64, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1, opt-level
3, static MSVC runtime, LTO disabled and line-table debug information equally for
both variants. These results describe this case and build, not a portable loading
time guarantee. Full input/binary hashes and raw repeated measurements accompany
the results JSON. The same corrected benchmark was used for both variants; its
earlier assertion expecting a retained VM in explicit Baked mode was incorrect.
The only changed production source is `crates/deadlib-lua/src/vm/host.rs`.

Local raw profiles, PDBs, executable copies, test logs, the process-local sampler
source/build recipe, and the alternating-run driver remain under
`target/seventh-bake-optimization/`.
