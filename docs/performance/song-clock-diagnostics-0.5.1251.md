# Consolidated song-clock diagnostics - 0.5.1251

Baseline: `263793ae5029de9e9488161808525013894e79f8`. Measured on 2026-09-15.

## Change

Consolidate the four diagnostic log branches in
`music_time_ns_from_song_clock` into one message and one final call to
`song_clock_music_time_ns`. This removes **28 runtime lines**.

Diagnostic age calculations now run inside the log arguments, after both the
snapshot's diagnostic flag and the log macro's level filters. With diagnostics
off or globally filtered, the `Instant` path no longer calculates durations in
both the diagnostic wrapper and the authoritative clock helper. Timestamp
arithmetic, rate normalization, and the formatted diagnostic message stay the
same, including `-0.000` for equal `Instant` values.

## Results

With diagnostics off, the fallback paths use **21.3% and 32.2% fewer thread
cycles**, saving approximately **9.3 and 19.9 ns per call**. Their throughput
increases by about **1.27x and 1.47x**. With diagnostics enabled but globally
filtered, the same paths improve by **21.5% and 32.3%**.

The host-clock path and active formatting show no clear, consistent improvement:
their paired process results include both improvements and regressions. The
small aggregate changes below should not be treated as reliable savings there.

All **126 allocation-counted observations** report zero allocations,
reallocations, frees, allocated bytes, and freed bytes. The formatting benchmark
uses a stack buffer; this does not measure allocations or I/O in the application's
real logger.

These are isolated clock-conversion costs. Gameplay calls this wrapper for input
edges that still need their music timestamp resolved. This is not a per-frame
operation, and these results do not establish an FPS or whole-game CPU gain.

### All workloads

Each value is the median of seven process medians. Cycle change is
`(new / old - 1)`; negative values mean fewer cycles. Throughput is millions of
clock conversions per second. Timing, cycle, and throughput medians are computed
independently from the recorded values.

| Diagnostics | Clock path | ns/call old -> new | Thread cycles/call old -> new | Cycle change | Million calls/s old -> new |
|---|---|---:|---:|---:|---:|
| Off | Host | 32.2 -> 30.9 | 70.5 -> 67.7 | -4.0% | 31.09 -> 32.36 |
| Off | Instant, past | 43.6 -> 34.3 | 95.6 -> 75.2 | -21.3% | 22.93 -> 29.16 |
| Off | Instant, future | 61.7 -> 41.8 | 135.2 -> 91.7 | -32.2% | 16.21 -> 23.90 |
| Filtered | Host | 30.7 -> 30.5 | 67.2 -> 66.9 | -0.4% | 32.60 -> 32.75 |
| Filtered | Instant, past | 42.7 -> 33.6 | 93.7 -> 73.6 | -21.5% | 23.41 -> 29.77 |
| Filtered | Instant, future | 62.0 -> 42.0 | 135.7 -> 91.9 | -32.3% | 16.14 -> 23.84 |
| Formatted | Host | 1041.6 -> 1034.3 | 2283.9 -> 2268.1 | -0.7% | 0.96 -> 0.97 |
| Formatted | Instant, past | 1043.6 -> 1021.1 | 2288.3 -> 2238.9 | -2.2% | 0.96 -> 0.98 |
| Formatted | Instant, future | 1047.6 -> 1046.0 | 2297.0 -> 2293.7 | -0.1% | 0.95 -> 0.96 |

### Variation

Within each process, compare its new cycle median with its old cycle median.
These ranges span the seven paired comparisons; they are not confidence intervals.

| Diagnostics | Clock path | Range of paired cycle changes |
|---|---|---:|
| Off | Host | -7.4% to +2.0% |
| Off | Instant, past | -25.9% to -19.3% |
| Off | Instant, future | -35.3% to -30.0% |
| Filtered | Host | -4.7% to +11.1% |
| Filtered | Instant, past | -24.7% to -20.2% |
| Filtered | Instant, future | -33.1% to -30.6% |
| Formatted | Host | -8.0% to +6.9% |
| Formatted | Instant, past | -4.7% to +3.0% |
| Formatted | Instant, future | -2.7% to +2.3% |

The [CSV](song-clock-diagnostics-0.5.1251.csv) contains every process median,
within-process timing range, cycle count, throughput, and allocation observation.

## Method

- Windows 11 Pro x86-64, build 26100; Intel Xeon E5-2696 v4 at 2.20 GHz,
  22 cores / 44 logical processors.
- Rust 1.98.1 (`48a229cea`), LLVM 22.1.8; repository release profile,
  optimization level 3 and full LTO.
- The frozen baseline wrapper and the production wrapper compile into the same
  optimized test executable. Both use the unchanged production clock helper.
  The baseline wrapper was checked against the commit above, ignoring whitespace.
- Seven fresh processes, alternating old-first/new-first order. The benchmark
  thread pins itself to logical processor 4 (affinity mask 16). No builds or other
  tests ran during the recorded measurements; the machine was not exclusively
  reserved.
- Each variant has 1,024 warmups, followed by the shared performance helper's
  three warmups and seven timed samples. Off/filtered samples contain 1,048,576
  calls; formatted samples contain 16,384 calls. A separate single call measures
  allocation churn after timing.
- Inputs and returned timestamps pass through `black_box`. Each call receives
  a snapshot at 8,192.0004 seconds, rate 1.25, and callback gap 5 ms. The host case
  uses two nonzero host timestamps 5 ms apart. Fallback cases have a zero host
  anchor and captured `Instant` values 5 ms before/after the snapshot.
- Off: snapshot diagnostics false, maximum log level Debug. Filtered: diagnostics
  true, maximum log level Info. Formatted: diagnostics true, maximum level Debug;
  the test logger formats the complete message into a 512-byte stack buffer and
  black-boxes its contents. It performs no file I/O or metadata formatting.
- Windows `QueryThreadCycleTime` measures calling-thread cycles. Wall time
  determines throughput. These are OS thread-cycle measurements, not hardware
  counters for retired instructions. Loop and black-box overhead are included.
- Measured executable SHA-256:
  `fde05f25da66b81cce55ba388aac8f883d1ecacd5ad7451c84fb072614987a58`.

## Validation

- Exact timestamp, log level, and formatted message equivalence in **14,112
  cases**, in both debug and release builds. Covers diagnostics on/off, global
  filtering, host/fallback anchors, past/equal/future captures, long and negative
  song times, integer extremes, zero/negative/subnormal rates, infinities, and NaNs.
  Log source locations naturally change; the production logging target stays
  `deadsync_gameplay`.
- Finite test rates reach 1,000,000. An initial `f32::MAX` stress case encountered
  an existing debug overflow in the unchanged clock helper's `i128` addition;
  that arithmetic is outside this diagnostic cleanup.
- `cargo test -p deadsync-core -p deadsync-gameplay --locked --quiet -j 4`:
  **807 passed**, 9 ignored, including the manual benchmark.
- Release equivalence test and all seven explicit benchmark runs passed.
- `cargo check --locked --quiet -j 4` passed.
- New tests and the changed function pass rustfmt; `git diff --check` passes.
  Whole-file rustfmt differences in `runtime_config.rs` were verified identical
  to the baseline's existing differences.

## Reproduce

Run from the repository root in PowerShell, on a machine where affinity mask 16
is valid. Stop builds and other benchmarks before recording results.

```powershell
cargo test -p deadsync-gameplay --test song_clock_diagnostics --release --locked -- --nocapture

$bench = cargo test -p deadsync-gameplay --test song_clock_diagnostics --release --locked --no-run --message-format=json |
    ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'song_clock_diagnostics' -and $_.executable } |
    Select-Object -Last 1 -ExpandProperty executable

$env:DEADSYNC_PERF_AFFINITY = '16'
foreach ($run in 1..7) {
    if ($run % 2 -eq 0) { $env:DEADSYNC_PERF_REVERSE = '1' }
    else { Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue }
    & $bench --ignored --exact song_clock_diagnostics_bench --nocapture --test-threads=1
}
Remove-Item Env:DEADSYNC_PERF_AFFINITY, Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue
```
