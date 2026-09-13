# Multitap compilation - 0.5.1192

Baseline: `5ba1955cd` / 0.5.1191. This pass applies the `M-HOTPATH`,
`M-THROUGHPUT`, and `M-MEM-REUSE` guidance in `rust-performance.md` to three
remaining sources of work in multitap compilation.

## Changes

1. **Visibility without geometry.** Explosion compilation only needs a boolean.
   Its predicate now checks the strict previsibility and final-tap boundaries
   directly, without calculating bounce position, squash, interpolation, or
   quantization. Unordered floating-point comparisons fall back to the existing
   phase implementation. The beat list, sorting, deduplication, lane filtering,
   and output sampling remain unchanged. Both predicates allocate nothing.
2. **Color lookup without a lowercase copy for short noteskin names.** Names of
   up to 16 bytes use ASCII-insensitive byte comparisons against the existing
   ordered keys. Longer names retain lowercase conversion and `str::contains`:
   probes showed that repeated byte scans lose to substring search on long
   inputs. Case handling, Unicode bytes, substring precedence, and live metric
   callbacks retain their existing behavior. No persistent cache is introduced.
3. **Append bounce curves directly to the output.** Y curves are constructed
   from the needed fields instead of cloning an ease and replacing its deltas
   and easing name. They append after the original frame-ease range in the same
   order, eliminating the temporary parabola vector and cloned source strings.
   The caller's existing output capacity is reused. Final easing names still
   require owned strings, and output growth can still allocate.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0
(`88d9e12ae`), system allocator. The repository's release profile uses opt-level
3 and LTO. Frozen parent functions and current functions run in the same binary;
the six copied baseline functions were checked against `5ba1955cd`. Unchanged
phase/state helpers are shared. The curve split is also extracted from the
parent actor builder for an isolated comparison.

Each result uses seven timing samples after three warmups. Allocation counters
run separately for one operation. CPU cycles use Windows `QueryThreadCycleTime`
for the calling thread; all workloads are single-threaded. Five complete runs
alternate old/new measurement order. Builds and other test runs finish before
measurement. Reported summaries are medians of the five per-run medians; the
CSV retains every sample range, control, and allocation count.

An operation is 128 visibility checks, 128 color lookups, one batch of curve
splits, one four-lane explosion compilation, or one actor compilation. Throughput
counts checks, lookups, curves, descriptors, or taps respectively. Curve-only
benchmarks reuse an output vector with enough capacity, restore the input Y
values each iteration, and include destruction of the previous iteration's
curves. Actor and explosion benchmarks build and drop fresh outputs, including
allocation and cleanup. Actor results combine the color and curve improvements.

Allocated/freed bytes are allocator-requested bytes, including reallocation
sizes. They measure churn, not process RSS or peak resident memory. No hardware
cache-miss or gameplay frame-rate measurements are claimed. These are targeted
compilation benchmarks; they do not establish an end-to-end song-load speedup.

## Results

Results are recorded in [multitap-compile-0.5.1192.csv](multitap-compile-0.5.1192.csv).

| Workload | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| Visibility, 8 taps, late (128 checks) | 3.693 | 0.250 | 8,099.6 | 551.3 | 93.19% |
| Default color (128 lookups) | 43.971 | 9.719 | 96,384.5 | 21,293.2 | 77.91% |
| Mixed-case color (128 lookups) | 10.287 | 1.125 | 22,559.6 | 2,457.4 | 89.11% |
| 8 curves, reserved output | 3.264 | 1.092 | 7,185.2 | 2,345.9 | 67.35% |
| 128 curves, reserved output | 126.070 | 17.873 | 276,161.9 | 39,263.1 | 85.78% |
| 512 curves, reserved output | 496.477 | 92.384 | 1,087,351.6 | 202,049.8 | 81.42% |
| 1 explosion descriptor, control | 1.125 | 1.212 | 2,565.4 | 2,743.8 | -6.95% |
| 16 disjoint explosion descriptors | 16.062 | 15.762 | 34,735.9 | 34,722.1 | 0.04% |
| 512 disjoint explosion descriptors | 2,201.637 | 1,672.194 | 4,817,792.0 | 3,664,195.8 | 23.94% |
| 512 overlapping explosion descriptors | 344.137 | 247.812 | 754,256.8 | 543,289.9 | 27.97% |
| 32-tap actor, 4 children | 1,237.138 | 860.288 | 2,704,240.1 | 1,884,489.8 | 30.31% |
| 128-tap actor, 4 children | 4,393.231 | 3,227.694 | 9,621,905.9 | 7,067,200.4 | 26.55% |

| Workload | Allocations old/new | Reallocations old/new | Requested bytes old/new |
|---|---:|---:|---:|
| Default color, 128 lookups | 128 / 0 | 0 / 0 | 896 / 0 |
| 128 curves | 257 / 128 | 7 / 0 | 348,400 / 832 |
| 512 curves | 1,025 / 512 | 9 / 0 | 1,397,680 / 3,328 |
| 512 overlapping explosion descriptors | 5 / 5 | 27 / 27 | 28,528 / 28,528 |
| 128-tap actor, 4 children | 3,961 / 1,665 | 77 / 68 | 10,617,680 / 9,210,715 |

Free counts and freed bytes equal their allocation counterparts in these
workloads, so the same reductions apply to cleanup churn. Short-name color
lookups remove one allocation and free per call. Splitting 512 curves reduces
requested bytes by 99.76%; compiling a 128-tap actor with four children reduces
allocation calls by 57.97% and requested bytes by 13.25%. Explosion output
allocation is unchanged: this pass improves its CPU work.

Throughput for the selected workloads increases as follows:

- Default color lookup: 2,910,978 to 13,169,889 units/s (352.4% higher).
- Curve splitting: 1,031,267 to 5,542,063 units/s (437.4% higher).
- Dense explosion descriptors: 1,487,777 to 2,066,078 units/s (38.9% higher).
- 128-tap actor compilation: 29,136 to 39,657 units/s (36.1% higher).

The 512-descriptor explosion, short-name color, and nonempty curve workloads
improved in every paired run. The 128-tap/four-child actor saved 23.5-31.3% of
cycles across paired runs. Smaller actor cases were noisier: the eight-tap,
four-child workload had one run 9.7% slower despite a 14.7% reduction in the
median-of-medians comparison.

The one-descriptor explosion control takes 0.088 us more per operation in the
summary (7.0% more cycles); the duplicate one-descriptor overlap control has
equal median cycle counts. Sixteen-descriptor cases vary across runs. No gain
is claimed for those tiny workloads. The 17-, 64-, and 1,541-byte color controls
retain their original allocation counts; their cycle medians are within 4.1%
of baseline and individual pairs include small regressions. These controls
are kept in the CSV rather than treated as improvements.

The largest visibility-only microbenchmark removes 99.1% of cycles at a late
128-tap beat, but that is a predicate result, not a whole-compilation speedup.
End-to-end explosion compilation still scans descriptors and constructs eases.

## Behavior and validation

Seven new regression/allocation tests cover:

- Visibility at exact/adjacent tap and previsibility boundaries, duplicate and
  unsorted taps, signed zero, infinities, NaNs, and randomized floating-point
  patterns. The full finite range is tested in release; debug inputs avoid the
  parent quantizer's existing i32 overflow on extreme beats.
- Complete explosion output equality, including lane filtering, overlapping
  descriptors, unusual taps, and an existing output prefix.
- All color keys in pairs, mixed ASCII case, Unicode, precedence, and names on
  either side of the 16-byte cutoff.
- Complete curve output equality, skipped spans, missing Y values, prefix and
  curve ordering, timing units, metadata, and independent expected Y-only deltas
  with `inQuad`/`outQuad` easing.
- Complete actor output equality with varied taps, lanes, child states, skins,
  styles, and stateful metric resolvers. Callback counts and order also match.
- Zero allocation/free churn for visibility and short-name color lookup, and
  only final easing-name allocations for curve splitting into reserved output.

Validation:

- `cargo test -p deadsync-song-lua --lib -- --test-threads=1`: 597 passed,
  5 failed, 54 ignored. The failures are the same pre-existing failures recorded
  in [the previous pass](multitap-speed-0.5.1191.md):
  `compile_song_lua_extracts_actorproxy_targets`,
  `compile_song_lua_layers_share_init_globals_and_actor_refs`,
  `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`,
  `compile_song_lua_runs_cmd_queuecommand_builders`, and
  `compile_song_lua_supports_notefield_column_api`.
- New regression tests in debug and release: 7 passed, 1 manual benchmark ignored
  in each profile. Release includes the unrestricted floating-point cases.
- `cargo test -p deadsync --test song_lua_wrapper`: 7 passed.
- `cargo clippy -p deadsync-song-lua --lib --no-deps -- -A clippy::all -D clippy::perf`: passed.
- `rustfmt --check --edition 2024 crates/deadsync-song-lua/src/multitap.rs`
  (including its test modules) and `git diff --check`: passed.

The external `C:\GitHub\lua-songs` parity corpus is unavailable in this checkout;
whole-song ITGmania parity was not run. No unsafe code or dependencies were added.
The workspace patch version advances exactly once, from 0.5.1191 to 0.5.1192.

## Reproduce

```powershell
cargo test -p deadsync-song-lua --lib multitap_compile -- --test-threads=1
cargo test --release -p deadsync-song-lua --lib multitap_compile -- --test-threads=1
cargo test --release -p deadsync-song-lua --lib multitap_compile_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test --release -p deadsync-song-lua --lib multitap_compile_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

Repeat five times, alternating the environment variable. Keep compilation and
other CPU-heavy work out of the measurement interval.
