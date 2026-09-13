# Multitap sampling and speed-mod reads ? 0.5.1191

Baseline: `34aca2540` / 0.5.1190. This pass follows `M-HOTPATH`,
`M-THROUGHPUT`, and the allocation/copy guidance in `rust-performance.md`.

## Changes

1. **Multitap phase calculation:** scan earlier intervals only for their
   contribution to elasticity, then calculate position, squash, interpolation,
   and note quantization for the final applicable bounce. Repeated floating-point
   multiplications retain their original order. The phase operation remains
   allocation-free; the gain is fewer calculations per useful sample.
2. **Explosion sample streaming:** consume adjacent overlay samples directly
   instead of first materializing a vector of complete overlay states. Sorting
   and deduplication of beat boundaries are unchanged. The public slice-based
   sampling API delegates to the same streaming implementation. Each eliminated
   sample occupies 508 bytes on this target; a 1,024-sample temporary therefore
   requested 520,192 bytes. Output eases and the sorted beat list still allocate.
3. **Speed-mod reads:** retain the finite set of ordinary Lua speed-mod strings
   when installing each method, then compare their identities on reads. Retained
   handles keep the immutable strings alive. Unknown identities take the original
   text conversion path, preserving coercion and invalid-value errors. Reads
   always consult the live raw owner field. This avoids both the owned string
   copy and the shared-reference allocation caused by borrowing a fresh Lua
   string. The retained handle/identity array costs 256 bytes per installed method
   on this target, plus setup work; the setup benchmark reports this tradeoff.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0
(`88d9e12ae`), system allocator. `cargo test --release` uses the repository's
optimized release profile with LTO. Original implementations are frozen under
`tests/perf/` and execute in the same binary as the new implementations.

Each measurement reports the median and range of seven timing samples after
three warmups. Allocation tracking runs separately for one operation, avoiding
its counter-update cost in timing. Windows `QueryThreadCycleTime` measures the
calling thread's CPU cycles. All workloads are single-threaded. Five complete
repetitions alternate old/new order for sampling and getter workloads; method
installation is always measured old then new. No builds run during measurement.
Summary figures are medians of the five per-run medians. The complete results,
including slower controls and sample ranges, are in
[multitap-speed-0.5.1191.csv](multitap-speed-0.5.1191.csv).

An operation is 128 phase/getter calls, one generated sample sequence, one
four-lane explosion compilation, or one method installation. Throughput counts
phase/getter calls, samples, input descriptors, or installations respectively.
The isolated sample-stream benchmark uses identical generated states for both
versions; the explosion benchmark includes both multitap improvements. Lua GC
is disabled for getter/installation timing; Lua creation is excluded. Sample
and explosion timing includes output construction and destruction. Allocation
bytes are requested bytes (including reallocation sizes), not process RSS or
hardware cache-miss measurements.

## Results

| Workload | Old ?s/op | New ?s/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| Phase, 8 taps, late | 12.846 | 4.146 | 28,149.6 | 9,095.1 | 67.69% |
| Phase, 32 taps, late | 50.681 | 8.955 | 111,087.6 | 19,621.7 | 82.34% |
| Phase, 128 taps, late | 201.424 | 30.508 | 441,312.0 | 66,870.8 | 84.85% |
| Isolated stream, 256 samples | 224.131 | 212.688 | 490,150.4 | 465,929.9 | 4.94% |
| Isolated stream, 1,024 samples | 1,550.553 | 1,285.297 | 3,396,179.4 | 2,815,547.1 | 17.10% |
| Explosions, 128 overlapping descriptors | 105.237 | 68.744 | 231,188.4 | 150,714.2 | 34.81% |
| Explosions, 512 overlapping descriptors | 554.337 | 346.650 | 1,214,616.9 | 760,457.8 | 37.39% |
| Active speed getter, no receiver | 56.559 | 50.314 | 123,990.1 | 110,321.9 | 11.02% |
| Active speed getter, receiver | 77.190 | 69.681 | 169,026.1 | 152,776.3 | 9.61% |
| Inactive speed getter, receiver | 62.429 | 54.423 | 136,827.4 | 119,100.2 | 12.96% |
| Cleared speed getter, receiver | 60.915 | 54.910 | 133,323.1 | 120,328.0 | 9.75% |
| Default speed getter, receiver | 50.136 | 51.126 | 109,911.2 | 112,083.1 | -1.98% |
| Method installation, warm Lua VM | 1.509 | 2.550 | 3,368.0 | 5,652.1 | -67.82% |

A/R/F = allocation/reallocation/free calls. Byte totals include reallocations.

| Workload | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| Phase, 8 taps, late | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 9,964,270 | 30,875,342 |
| Phase, 32 taps, late | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 2,525,599 | 14,294,190 |
| Phase, 128 taps, late | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 635,475 | 4,195,593 |
| Isolated stream, 256 samples | 147/8/147 | 146/8/146 | 825,878/825,878 | 695,830/695,830 | 1,142,188 | 1,203,644 |
| Isolated stream, 1,024 samples | 586/10/586 | 585/10/585 | 3,307,616/3,307,616 | 2,787,424/2,787,424 | 660,410 | 796,703 |
| Explosions, 128 overlapping descriptors | 9/19/9 | 5/19/5 | 136,176/136,176 | 22,384/22,384 | 1,216,296 | 1,861,988 |
| Explosions, 512 overlapping descriptors | 9/27/9 | 5/27/5 | 337,392/337,392 | 28,528/28,528 | 923,625 | 1,476,994 |
| Active speed getter, no receiver | 128/0/128 | 0/0/0 | 512/512 | 0/0 | 2,263,108 | 2,544,040 |
| Active speed getter, receiver | 256/0/256 | 128/0/128 | 5,632/5,632 | 5,120/5,120 | 1,658,249 | 1,836,946 |
| Inactive speed getter, receiver | 256/0/256 | 128/0/128 | 5,632/5,632 | 5,120/5,120 | 2,050,332 | 2,351,962 |
| Cleared speed getter, receiver | 256/0/256 | 128/0/128 | 5,632/5,632 | 5,120/5,120 | 2,101,294 | 2,331,081 |
| Default speed getter, receiver | 128/0/128 | 128/0/128 | 5,120/5,120 | 5,120/5,120 | 2,553,079 | 2,503,629 |
| Method installation, warm Lua VM | 8/0/0 | 9/1/0 | 378/0 | 778/128 | 662,526 | 392,157 |

### Repeatability and tradeoffs

- Late 8-tap phase samples improve by 65.55?69.64% in paired CPU cycles across
  all five repetitions; 32-tap samples improve by 81.46?83.15%, and 128-tap
  samples by 84.64?85.04%. All retain zero allocator churn.
- The 512-descriptor overlapping explosion workload improves in every run
  (34.69?37.95% fewer paired cycles). Requested/freed bytes drop from 337,392
  to 28,528 (91.54%), and allocation/free calls from nine to five. Its median
  throughput increases from 0.924 to 1.477 million descriptors/s.
- Isolated streaming removes exactly one sample allocation. For 1,024 samples
  this removes 520,192 requested/freed bytes per operation. Aggregate cycles
  improve by 17.10%, but individual paired runs range from a 6.42% regression
  to a 26.11% improvement. The 256-sample result is also variable: aggregate
  cycles improve by 4.94%, but one run is 45.51% slower. These are memory wins;
  they do not establish a consistent CPU speedup for every isolated sequence.
- Small allocating workloads are especially noisy: 16-sample streaming has
  paired changes ranging from 17.22% more cycles to 78.51% fewer cycles;
  the 16-descriptor non-overlap explosion workload ranges from 82.12% more
  to 67.00% fewer cycles. The CSV retains these outliers. The one-sample
  stream and one-descriptor overlap case each also have one slower run.
- Active speed-mod method reads with a receiver improve in all five runs
  (9.61?12.07% fewer paired cycles). Each call goes from two allocations/frees
  and 44 requested/freed bytes to one argument-buffer allocation/free and
  40 bytes. Receiver-free ordinary reads go from one allocation/free to zero.
  Inactive and cleared reads also improve in every paired run.
- Default getters have no active-string allocation to remove. The receiver
  case uses 1.98% more aggregate cycles (about 7.7 ns/call more wall time);
  receiver-free defaults use 0.44% more cycles (about 0.9 ns/call). No speedup
  is claimed for these controls. One single-tap phase control has one slower
  repetition despite an improved aggregate median.
- Installation is slower: 1.509 ? 2.550 ?s/method, eight ? nine allocations,
  zero ? one reallocation, and 378 ? 778 requested bytes in a warm Lua VM.
  The retained key/identity array is 256 bytes/method. This moves work to
  installation; at the measured active-method cycle rates, roughly 18 reads
  recover the extra setup CPU cost. Installation always uses old/new order,
  so its timing has a weaker execution-order control than the read benchmarks.

All 52 scenarios and five repetitions (520 old/new measurements) are preserved
in the CSV. Allocation counts and bytes are deterministic for each scenario
across these repetitions. Tiny timing controls are near timer/counter granularity.

## Behavior and validation

New tests compare every phase output bit at strict visibility/tap boundaries,
previsibility edges, duplicate and near-duplicate taps, authored peaks, random
beats, nonfinite query beats, and unsorted public inputs. Explosion and sample
stream tests compare full easing lists across empty inputs, overlapping events,
all lanes, single/double/versus layouts, visibility transitions, repeated calls,
and non-increasing sample times. Getter tests compare defaults, raw lookup,
argument forms, numeric coercion, invalid UTF-8/type errors, switches, clearing,
and direct live field mutation. Allocation tests enforce zero churn for phase
calculation, unchanged streamed samples, and ordinary receiver-free getters;
receiver calls allow their existing one-element argument allocation.

- Full current crate suite: **590 passed, 5 failed, 53 ignored**.
- Original revision in a detached checkout: **582 passed, 5 failed, 51 ignored**.
- All eight new behavior/allocation tests pass in debug and release builds.
- Application Lua wrapper integration tests: **7 passed** on the final implementation.
- Clippy performance lints, rustfmt checks on changed Rust files, and
  `git diff --check`: passed.
- Cargo regenerated the lockfile for the single patch bump to **0.5.1191**;
  only the three packages inheriting the workspace version changed there.

The original revision reproduces these five existing crate-test failures with
identical assertions:

- `compile_song_lua_extracts_actorproxy_targets`
- `compile_song_lua_layers_share_init_globals_and_actor_refs`
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`
- `compile_song_lua_runs_cmd_queuecommand_builders`
- `compile_song_lua_supports_notefield_column_api`

These are targeted workload measurements, not measured whole-song loading or
frame-rate gains. The external full-song parity corpus is absent on this machine.

## Reproduce

```powershell
cargo test -p deadsync-song-lua --lib multitap_work
cargo test -p deadsync-song-lua --lib speed_read
cargo test -p deadsync --test song_lua_wrapper
cargo test --release -p deadsync-song-lua --lib multitap_work_bench -- --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-song-lua --lib speed_read_bench -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
# Repeat the two benchmark commands with reversed old/new order.
Remove-Item Env:DEADSYNC_PERF_REVERSE
```
