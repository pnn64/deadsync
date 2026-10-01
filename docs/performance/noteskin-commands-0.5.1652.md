# Noteskin command parsing, expansion and plan application - 0.5.1652

Parent: `78094fd5c` (0.5.1651). Date: 2026-10-01.

This pass applies `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT guidance to command preparation during
noteskin loading. These paths are used by sprite animation application in
`deadsync-assets/src/noteskin/texture.rs` and tap explosion assembly in
`deadsync-noteskin/src/runtime.rs`. The measurements concern command preparation;
they do not establish an improvement in rendering frame rate or full load time.

1. Parse command arguments directly into one borrowed `SmallVec`, keeping the
   command separate. The old parser first collected the command and arguments,
   then copied arguments into another buffer. Six arguments now fit inline;
   larger commands use one growable argument buffer. Quoting, nested delimiters,
   trimming, empty-field handling and arbitrary argument counts retain their
   existing behavior. One-argument tokens use 34.7–39.3% fewer CPU cycles;
   six-argument tokens use 53.9–56.5% fewer and eliminate a 128-byte allocation
   per token. Seven-argument tokens halve allocations and requested bytes.
2. Assemble `PlayCommand` lookup keys in a 128-byte `ArrayString` for names up to
   121 UTF-8 bytes. Larger names use one exact-capacity `String`. Both paths
   lowercase ASCII in place and append the same `command` suffix. Short-name
   expansion uses 11.2–31.6% fewer CPU cycles, and a complete 16-call request
   drops from 33 allocations, 23 reallocations and 33 frees to 1, 7 and 1.
   Remaining churn is the required expanded output string. Large names and
   non-ASCII names still work, with no new truncation or length restriction.
3. Apply sprite animation plans through a private visitor instead of collecting
   a temporary vector. Owning collection APIs remain available. Map application
   still sorts command names and resolves the final effect clock before the
   first callback, so every callback receives the same clock in the same order.
   Eight delay-only commands now have zero heap churn instead of one allocation,
   two reallocations and one free (896 requested bytes), using 6.9–10.0% fewer
   CPU cycles. Property plans retain their required frame-delay vectors; for
   eight commands, requested bytes fall from 1,024 to 128, with 5.1–10.5% fewer
   CPU cycles. Commands above the existing eight-reference inline capacity
   retain the sorted-reference spill.

No dependencies or unsafe production code were added. Zero churn applies to
the scoped inline paths, not to all noteskin loading. Necessary owning outputs,
oversized arguments/names and frame-delay vectors can still allocate.

## Measurement method

The [raw CSV](noteskin-commands-0.5.1652.csv) contains 240 rows: 20 workloads,
two implementations and six serial runs, alternating old-first/new-first.
Each value is the median of seven timing batches; the CSV also retains each
batch range. CPU cycles use Windows `QueryThreadCycleTime` for the calling
thread. Throughput is useful items per second, as specified below. Allocation
counts are measured separately for one complete operation, including result
destruction, and agree across all six runs for every workload/implementation.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz (22 cores / 44
logical processors), Rust 1.98.1, LLVM 22.1.8. The repository release profile
uses opt-level 3 and full LTO, with test unwinding. The benchmark processes were
pinned to logical processor 6 (affinity mask 64), using `--test-threads=1`, after
compiler processes finished. Fixtures and maps are built outside measurement;
the parser, expansion/application, callback work and temporary destruction are
inside it. Allocation bytes count requested allocator sizes, including each
reallocation size. They describe churn, not process RSS or peak resident memory.

Frozen implementations are in `tests/command_preparation/baseline_token.rs`,
`baseline_plans.rs` and `baseline_explosion.rs`. A source audit checks all ten
function bodies and compiler attributes against the parent. The plan and
expansion baselines share the new token parser and unchanged parsing helpers
with production to isolate the plan-vector and key changes, respectively.
Token parsing is separately compared against the actual parent parser. These
are isolated comparisons of the three changes, rather than a benchmark of the
entire parent executable against the new executable.

All six runs are retained. Eighteen workloads improve CPU and throughput in
every run. The 32-command property-heavy case and the script with no animation
plans vary in sign; no consistent timing win is claimed for those two cases.
The 32-command property case reduces requested bytes from 4,992 to 1,024 and
reallocations from four to zero in every run. Its CPU reduction ranges from
−6.3% to +7.9%, with the slower first run's batch ranges overlapping. The
no-plan script already allocated nothing and remains allocation-free. Additional
runs were made to examine these timing variations; the initial results are
included, rather than discarded.

## Token parsing

Each operation parses and destroys 1,024 identical tokens, with 256 operations
per batch. Timing, cycles, counts and bytes below are per 1,024-token operation;
divide by 1,024 for per-token values. Throughput in the CSV is tokens/s.
`rgba` has four numeric arguments, `nested` one nested expression, and `many`
24 arguments. Zero churn is independently asserted for zero through six
arguments; larger tokens are checked against the parent's allocation cost.

| Workload | ns/op, old → new (run 1) | Thread cycles/op, old → new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old → new | Requested bytes, old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `command_token_one` | 56,491.4 → 36,228.5 | 123,553.6 → 78,624.7 | 34.7% to 39.3% | 53.1% to 64.3% | 0/0/0 → 0/0/0 | 0 → 0 |
| `command_token_rgba` | 99,954.7 → 78,972.3 | 217,884.6 → 172,537.3 | 8.3% to 23.3% | 9.1% to 30.3% | 0/0/0 → 0/0/0 | 0 → 0 |
| `command_token_nested` | 130,435.2 → 111,747.3 | 282,477.6 → 243,470.1 | 11.5% to 24.2% | 12.9% to 32.2% | 0/0/0 → 0/0/0 | 0 → 0 |
| `command_token_six` | 184,617.2 → 81,097.3 | 401,093.4 → 176,454.8 | 53.9% to 56.5% | 116.9% to 130.2% | 1024/0/1024 → 0/0/0 | 131072 → 0 |
| `command_token_seven` | 254,875.4 → 159,262.9 | 553,369.8 → 346,175.5 | 36.4% to 39.0% | 56.8% to 64.0% | 2048/0/2048 → 1024/0/1024 | 262144 → 131072 |
| `command_token_many` | 682,123.8 → 546,207.0 | 1,487,099.6 → 1,189,402.8 | 19.5% to 21.5% | 24.3% to 27.3% | 2048/2048/2048 → 1024/2048/1024 | 1441792 → 917504 |

## Plan application

Maps have 1, 8 or 32 entries in lexical command order, with alternating clock
directives. Each entry has two animation plans: either two delay assignments,
or four `Sprite.LinearFrames` frame delays followed by a uniform delay.
Each operation dispatches the whole map to the same consuming callback; there
are 4,096 operations per batch. Counts and timing are per map request, and
throughput is plans/s. The callback consumes values and drops required delay
storage inside both measured variants.

| Workload | ns/op, old → new (run 1) | Thread cycles/op, old → new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old → new | Requested bytes, old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `command_plans_1_delays` | 900.2 → 796.4 | 1,955.0 → 1,747.4 | 6.3% to 19.5% | 6.8% to 24.5% | 1/0/1 → 0/0/0 | 128 → 0 |
| `command_plans_1_properties` | 1,419.6 → 1,144.5 | 3,077.2 → 2,492.4 | 6.6% to 19.0% | 8.2% to 24.0% | 2/0/2 → 1/0/1 | 144 → 16 |
| `command_plans_8_delays` | 6,668.4 → 6,031.2 | 14,544.2 → 13,091.9 | 6.9% to 10.0% | 7.6% to 10.6% | 1/2/1 → 0/0/0 | 896 → 0 |
| `command_plans_8_properties` | 9,117.2 → 8,115.4 | 19,772.1 → 17,697.2 | 5.1% to 10.5% | 5.7% to 12.3% | 9/2/9 → 8/0/8 | 1024 → 128 |
| `command_plans_32_delays` | 26,311.0 → 25,014.5 | 57,238.9 → 54,486.1 | 1.9% to 8.9% | 1.2% to 10.5% | 2/4/2 → 1/0/1 | 4480 → 512 |
| `command_plans_32_properties` | 34,515.5 → 36,660.4 | 75,158.8 → 79,878.1 | -6.3% to 7.9% | -5.9% to 8.4% | 34/4/34 → 33/0/33 | 4992 → 1024 |

The single-script API uses 8,192 operations per batch and reports scripts/s.
`none` has three unrelated commands; `one` and `many` have one and three delay
plans. All three new script operations have zero heap churn.

| Workload | ns/op, old → new (run 1) | Thread cycles/op, old → new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old → new | Requested bytes, old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `command_script_none` | 304.6 → 243.3 | 667.2 → 533.1 | -8.6% to 24.2% | -7.7% to 31.9% | 0/0/0 → 0/0/0 | 0 → 0 |
| `command_script_one` | 257.3 → 161.6 | 555.9 → 354.4 | 32.0% to 36.5% | 47.9% to 59.2% | 1/0/1 → 0/0/0 | 128 → 0 |
| `command_script_many` | 540.6 → 448.2 | 1,176.3 → 971.9 | 14.9% to 17.4% | 19.0% to 20.7% | 1/0/1 → 0/0/0 | 128 → 0 |

## Command expansion

Each operation expands 16 `PlayCommand` calls into a new output string, including
output growth and destruction, with 2,048 operations per batch. Throughput is
expanded calls/s. Names are `Glow` (`short`), 121 ASCII bytes (`boundary`), 122
ASCII bytes (`spill`), mixed accented/Japanese text (`unicode`), or 512 ASCII
bytes (`long`). Both implementations use the same prebuilt command map,
recursion stack and operation budget. A separate warmed-output test confirms
zero short-key churn, and oversized-name tests require at most one exact-size
key allocation with no reallocation.

| Workload | ns/op, old → new (run 1) | Thread cycles/op, old → new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old → new | Requested bytes, old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `command_expand_short` | 12,307.4 → 10,998.1 | 26,768.5 → 23,779.0 | 11.2% to 31.6% | 11.9% to 46.2% | 33/23/33 → 1/7/1 | 2742 → 2294 |
| `command_expand_boundary` | 16,777.9 → 11,366.9 | 35,175.6 → 24,709.1 | 20.1% to 29.8% | 25.4% to 47.6% | 33/23/33 → 1/7/1 | 10038 → 2294 |
| `command_expand_spill` | 17,413.6 → 12,360.4 | 37,827.7 → 26,830.4 | 20.5% to 29.1% | 25.8% to 40.9% | 33/23/33 → 17/7/17 | 10102 → 4358 |
| `command_expand_unicode` | 13,080.9 → 8,957.3 | 28,337.8 → 19,432.3 | 15.1% to 31.4% | 17.8% to 46.0% | 33/23/33 → 1/7/1 | 2870 → 2294 |
| `command_expand_long` | 23,084.5 → 19,345.3 | 50,190.2 → 41,872.5 | 16.1% to 21.7% | 19.3% to 28.0% | 33/23/33 → 17/7/17 | 35062 → 10598 |

## Behavior and validation

Six new normal tests compare the observable behavior and allocation costs:

- Token commands and argument slices match for quoted/nested fields, empty
  fields, malformed delimiters, Unicode whitespace and names, argument counts
  across the inline boundary, 4,096 deterministic generated inputs, and a
  1,024-argument token. No argument limit was introduced.
- Streaming callback sequences, values and final clocks match the parent for
  empty maps, both default clocks, Lua method chains, invalid plans, multiple
  clock directives, and command counts around the inline boundary. The owning
  collection APIs also return equal values.
- Expansion text, completion flags and remaining budgets match for ASCII and
  Unicode names, both sides of the 121-byte limit, 1,024-byte names, missing
  names, self/mutual recursion, depth limits, operation budgets, Lua syntax and
  output above the 256 KiB limit. Parsed dim/bright animations and sampled states
  match, including non-finite times.
- Allocation assertions require zero churn on inline token/delay/key paths,
  reduced churn for growing arguments and map plans, and one exact-capacity
  fallback allocation for oversized keys.

Validation completed:

```text
cargo test -p deadsync-noteskin --lib command_preparation_perf --offline --locked
  6 passed, 1 manual benchmark ignored
cargo test -p deadsync-noteskin -p deadsync-assets --lib --release --offline --locked
  noteskin: 299 passed, 11 ignored
  assets:   153 passed, 3 ignored
cargo check --workspace --offline --locked
  passed
cargo clippy -p deadsync-noteskin -p deadsync-assets --lib --offline --locked --no-deps -- -D clippy::perf -A clippy::large_enum_variant
  passed; existing general warnings and the existing large-enum exception remain
rustfmt --check --edition 2024 <the two changed source and four new test files>
git diff --check
  passed
```

The affected production consumer's full library suite passed alongside noteskin.
All six manual benchmark runs passed. The workspace was compiled, while release
behavior tests were scoped to the changed crate and its asset consumer.

## Reproduction

Build the benchmark binary and run the ignored test in release mode. Alternate
`DEADSYNC_PERF_ORDER` for repeated runs; run serially on the same processor after
builds finish. `--nocapture` prints cycles, throughput, sample ranges and counts.

```powershell
cargo test -p deadsync-noteskin --lib --release --offline --locked --no-run
$taskBenchProcess = Get-Process -Id $PID
$taskBenchProcess.ProcessorAffinity = [IntPtr]64
$taskBenchBinary = Get-ChildItem target/release/deps -Filter 'deadsync_noteskin-*.exe' |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
foreach ($taskRun in 1..6) {
    $env:DEADSYNC_PERF_ORDER = if ($taskRun % 2 -eq 0) { 'new-first' } else { 'old-first' }
    & $taskBenchBinary.FullName benchmark_command_preparation --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw "Benchmark run $taskRun failed" }
}
Remove-Item Env:DEADSYNC_PERF_ORDER
```

DeadSync's workspace version changes exactly once from 0.5.1651 to 0.5.1652;
`Cargo.lock` updates the three workspace-versioned packages accordingly. The
fixture archive, performance guideline and optimization scripts are excluded
from the commit.
