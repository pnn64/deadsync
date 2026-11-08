# Lua compiler argument storage, command extraction and upvalue scans

Baseline: `e54a02471` (`0.5.2349`). Release patch: `0.5.2350`.

This pass removes temporary storage from compilation and introduces a safe library boundary for inspecting captured Lua state. It follows the hot-path, allocation-reuse and batching guidance in [rust-performance.md](../../rust-performance.md). The gains are scoped: complete compilation timings are mixed, and several controls regress. No broad compiler-speed or gameplay-FPS improvement is claimed.

1. `deadlib-lua-legacy::IgnoredArgs` owns the existing checked zero/one/many argument mechanism previously private to Song Lua. Seven noteskin host callbacks now use it. One input stays inline, and larger lists retain the original `MultiValue` conversion. Every input is still checked; caught Rust panic payloads, reference-pressure errors, arity, nils, identity and ownership survive forwarding. Conversion from an existing `MultiValue` retains its buffer, including the actor tracing path. The performance gain comes from the new noteskin callers; moving the game helper alone is not counted as a gain.
2. Noteskin command extraction uses `SmallVec<[String; 8]>` and returns a single command's existing string. Longer lists keep exact-capacity heap storage. Reads retain the same sequence iterator, conversion order, raw nil termination and error path. The first checked read happens before constructing the collection, avoiding short-list setup for an empty sequence. Actor schemas, names, resources and command semantics stay in `deadsync-noteskin`.
3. `Function::for_each_upvalue` holds one VM lock and stack guard, roots the function, and lends its immutable name bytes while returning owning values. Native C closures are excluded so private callback storage remains inaccessible. Song Lua snapshots use it instead of per-cell protected calls and temporary name strings. UTF-8 errors and `_ENV` filtering remain in the game snapshot policy. The old count/read helpers remain available to the frozen benchmark/debug path; the count helper is test-only.

The public visitor uses the library's existing stack and conversion machinery. Three reserved slots hold the function, current value and conversion scratch space. A rooted Lua prototype owns each borrowed name through the visit. Safe nested Lua calls may run; errors and unwinding restore the original stack. Native tests exercise these boundaries on Lua 5.1 and Lua 5.4. Miri was not run against the foreign Lua runtime.

## Measurements

[Full evidence and raw samples](2026-10-23-lua-compiler-boundaries.json) include the pre-production prototype, intermediate measurements, rejected candidate, production samples, compiler/binary/source hashes and suite receipts. Production scopes retain 27 samples per variant (three fresh processes, alternating order, nine retained samples after warmup). CPU affinity is logical CPU 2. All comparisons use rustc 1.99.1, release optimization, static Windows CRT, `-j 2`, and `profile.release.lto=false`.

Cycles come from `QueryThreadCycleTime`; throughput uses wall time. Allocation calls, reallocations and requested bytes are counted separately and include reallocation traffic. Scoped counters cover the test thread and mlua's Rust-backed allocator; direct native malloc is excluded. These are traffic measurements, not RSS or retained memory. Counting overhead is included on both sides.

| Operation | Thread cycles before → after | Operations/s before → after | Allocations | Reallocations | Requested bytes | Throughput |
|---|---:|---:|---:|---:|---:|---:|
| Extra arguments / 768 callbacks | 582,219 → 610,476 | 3,755 → 3,580 | 1,024 → 1,024 | 0 → 0 | 119,168 → 119,168 | -4.65% |
| No arguments / 768 callbacks | 226,752 → 210,843 | 9,657 → 10,384 | 256 → 256 | 0 → 0 | 1,408 → 1,408 | +7.53% |
| One receiver / 768 callbacks | 407,433 → 289,591 | 5,367 → 7,551 | 1,024 → 256 | 0 → 0 | 32,128 → 1,408 | +40.69% |
| Extract 0 commands | 561.40 → 513.17 | 3,862,693 → 4,250,726 | 0 → 0 | 0 → 0 | 0 → 0 | +10.05% |
| Extract 1 command | 1,274 → 913.58 | 1,719,563 → 2,377,525 | 3 → 1 | 0 → 0 | 30 → 3 | +38.26% |
| Extract 8 commands | 3,178 → 2,931 | 690,073 → 747,064 | 10 → 9 | 0 → 0 | 247 → 55 | +8.26% |
| Extract 64 commands | 18,976 → 19,370 | 114,688 → 112,888 | 66 → 66 | 0 → 0 | 2,091 → 2,091 | -1.57% |
| Complete 172-entry noteskin compile | 11,264,630 → 11,153,892 | 193.87 → 195.28 | 19,388 → 18,872 | 1,789 → 1,789 | 1,068,483 → 1,029,955 | +0.73% |
| Scan upvalues / 0 | 94.10 → 156.91 | 23,703,704 → 14,124,138 | 0 → 0 | 0 → 0 | 0 → 0 | -40.41% |
| Scan upvalues / 1 | 1,435 → 365.05 | 1,531,788 → 6,037,736 | 2 → 0 | 0 → 0 | 80 → 0 | +294.16% |
| Scan upvalues / 8 | 10,887 → 1,885 | 200,729 → 1,165,491 | 16 → 0 | 0 → 0 | 640 → 0 | +480.63% |
| Scan upvalues / 32 | 42,312 → 6,725 | 51,722 → 326,239 | 64 → 0 | 0 → 0 | 2,560 → 0 | +530.75% |
| Scan upvalues / 60 | 78,504 → 12,543 | 27,856 → 174,661 | 120 → 0 | 0 → 0 | 4,800 → 0 | +527.00% |

Receiver-only calls remove 768 allocations and 30,720 requested bytes per 768-call batch. The complete noteskin fixture removes 516 allocations and 38,528 requested bytes: 344 allocations from checked argument storage and 172 from command storage. Its throughput change is +0.73%, so the stronger whole-workload result is allocation reduction. The four-input callback control is 4.65% slower; the long command list is 1.57% slower. Empty-upvalue scans add about 63 cycles, while nonempty scans improve roughly 4–6.3×. These costs are retained in the table.

The scoped callback VMs, actors and runners are prepared outside timing, with GC stopped. Each receiver/extra/empty unit executes 768 calls. Command units extract one complete list. Upvalue units scan one prepared function, including temporary handle destruction. Complete noteskin compilation includes disk access, VM construction/execution/extraction and DTO/VM destruction, with normal GC.

## Complete Song Lua compilation

Three alternating before/after pairs retain five samples per process: 15 samples per variant/fixture. The six fixtures cover offline loading, shared/cyclic function-action probes, actor/wrapper-heavy live preparation, two layers and 4,096 timing segments. Prepared context, final DTO formatting and final destruction are excluded; source reads, host/VM creation, execution, extraction and finalization are included. Reallocations are unchanged in every complete Song Lua fixture; all counts are in the evidence.

| Complete Song Lua fixture | Thread cycles before → after | Compiles/s before → after | Allocations | Requested bytes | Throughput |
|---|---:|---:|---:|---:|---:|
| offline_basic | 446,056,492 → 446,659,011 | 4.90 → 4.89 | 254,584 → 254,584 | 39,568,244 → 39,568,244 | -0.24% |
| offline_capture | 1,040,191,413 → 1,084,153,737 | 2.10 → 2.01 | 387,817 → 387,649 | 48,601,164 → 48,595,788 | -4.02% |
| live_wrappers | 1,342,661,744 → 1,353,448,634 | 1.63 → 1.61 | 720,108 → 720,108 | 120,725,564 → 120,725,564 | -0.80% |
| live_actors | 1,154,474,953 → 1,157,313,748 | 1.89 → 1.89 | 603,622 → 603,622 | 98,026,649 → 98,026,649 | -0.27% |
| live_two_layers | 2,017,235,072 → 1,956,755,580 | 1.08 → 1.12 | 1,180,831 → 1,180,831 | 202,871,596 → 202,871,596 | +3.06% |
| live_timing4096 | 1,419,755,418 → 1,388,226,652 | 1.54 → 1.57 | 728,091 → 728,091 | 121,831,266 → 121,831,266 | +2.20% |

The capture-heavy fixture's initial −4.02% throughput result was investigated with three reversed-order confirmation pairs using the identical binaries and workload. Confirmation was −0.20%; pooling all 30 samples per variant gives -0.85%. Both runs consistently remove 168 allocations and 5,376 requested bytes. Both runs are retained, and neither establishes a complete-compiler CPU gain.

All 252 complete outputs (210 timed operations plus 42 warmups across initial and confirmation runs) matched through exact Debug DTO snapshots. The normal-library snapshot reports retained scenes through their Debug representation; it does not serialize every internal VM cell. Separate snapshot restoration and live-state regression tests remained green.

## Validation and limits

- Before edits: Song Lua **831 passed, 0 failed, 41 ignored**; noteskins **292 passed, 0 failed, 9 ignored**. The six historical Song Lua failures mentioned in the task were already absent at this baseline; none were fixed or suppressed here.
- Candidate: Song Lua **832 passed, 0 failed, 41 ignored**; noteskins **292 passed, 0 failed, 9 ignored**. The added Song check compares invalid upvalue-name errors with the original reader. Existing restoration tests cover captured cells, aliases, cycles and deep graphs. Existing compiler/live-state regressions cover complete DTOs and eight initial updates.
- Library tests: **7 passed on Lua 5.1 and 7 on Lua 5.4**. They cover GC rooting, names/order, aliases, nested visitors, coroutine calls, signed-zero/NaN bits, caller stack restoration after success/error/panic, C-closure exclusion, argument arity/identity and existing-buffer retention.
- Noteskin differential checks compare complete bincode bundle bytes and preserve holes, whitespace, numeric coercion, binary-string errors, metamethod policy, callback side effects and caught Rust panic inputs. Targeted rustfmt, Python syntax and `git diff --check` checks pass. No further full-suite run is required for the final version-only bump.

Direct string accumulation was rejected because growth introduced three reallocations for eight commands and seven for 64 commands; the eight-command case slowed about 11%. Inline collection storage keeps the exact join allocation and avoids that growth. The rejected body and raw results remain in the evidence.

No theme crate or rendering policy changes. Rendered pixels, whole-app FPS and a representative chart corpus were not measured. No input interaction was migrated in this batch, so no new VM-free gameplay coverage or retained-memory result is claimed. Lua remains in loading/probes and in existing live-scene fallback. The reusable checked transport and capture-inspection mechanisms support subsequent compiler work toward explicit runtime state.

## Reproduction

```powershell
cargo test -p deadsync-noteskin --lib --release --locked -j 2 --config profile.release.lto=false --no-run
cargo test -p deadsync-song-lua --lib --release --locked -j 2 --config profile.release.lto=false --no-run
# Preserve baseline/candidate binaries before rebuilding; full suites need not be run.
python scripts/benchmark_lua_boundaries.py --noteskin <noteskin-tests.exe> --song <song-tests.exe> --stage production --rounds 3 --out <scopes.json>
cargo build -p deadsync-song-lua --example compile_pipeline --release --locked -j 2 --config profile.release.lto=false
python scripts/benchmark_song_lua_pipeline.py --suite startup --before <before.exe> --after <after.exe> --out <pairs> --pairs 3 --samples 5
python scripts/benchmark_song_lua_pipeline.py --suite startup --only offline_capture --before <before.exe> --after <after.exe> --out <confirmation> --pairs 3 --samples 5 --reverse-first
```

Measurements precede the release metadata bump. The production Rust files match their measured hashes; the workspace and lockfile patch version are the subsequent metadata-only change.
