# Lua command preprocessing - 0.5.1631

Parent: `7508e0b1b` (0.5.1630). Date: 2026-09-30.

Command preprocessing runs before Lua syntax loading and when compiling noteskin
command strings. This pass follows the local guide's M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT recommendations with three changes:

1. Command bodies borrow their original text until an actual comment requires
   removal. Bodies without `--` skip the second character scan; quoted comment
   markers still borrow the original body. The preceding matching-parenthesis
   scan validates string boundaries before this helper is called. Actual
   comments produce one owned String, copying untouched spans in bulk while
   preserving the parent's spaces and newline counts.
2. Command slices use an eight-element inline SmallVec. Up to eight slices need
   no heap storage; larger bodies spill normally. Collection still precedes
   command-name validation, preserving error priority and expansion order.
   Empty slices, including trailing semicolons, count toward that capacity.
3. Long-string and long-comment delimiter searches no longer format a temporary
   terminator String. A byte scan finds closing brackets and checks the equals
   run once per candidate. After eight misses, delimiters up to 16 bytes use
   a stack-buffer memmem search, avoiding repeated candidate overhead on dense
   near matches. Wider delimiters retain the linear scan. Wrong-length runs
   preserve overlapping closing-bracket candidates.

The changes add no dependency, public API change, unsafe code or retained heap
cache. The public preprocessor still returns an owned output String; actual
comments and command batches exceeding inline capacity still need storage.
Successful delimiter searches, comment-free bodies, and splits of at most eight
slices have zero allocation, reallocation, free, requested-byte and freed-byte
churn in the prepared helper checks.

## Measurements

The [raw CSV](lua-cmd-preprocessing-0.5.1631.csv) contains 224 rows: 28 workloads,
two implementations and four independent runs. Percentage ranges cover all four
paired runs; absolute figures use run 1. Allocator counts agree across all four
runs for each workload and implementation.

| Workload | Thread cycles/op, old -> new | CPU reduction | Throughput gain | Allocations/op, old -> new | Requested bytes/op, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| Strip comment-free body | 245.0 -> 57.6 | 64.5% to 76.5% | 184.3% to 330.0% | 1 -> 0 | 7 -> 0 |
| Strip body with quoted comment marker | 374.5 -> 124.2 | 60.0% to 66.8% | 151.0% to 201.9% | 1 -> 0 | 23 -> 0 |
| Strip body with actual line comment | 340.5 -> 289.4 | 15.0% to 20.6% | 17.7% to 25.9% | 1 -> 1 | 21 -> 21 |
| Strip 1,280-byte comment-free body | 17,621.7 -> 134.7 | 99.1% to 99.3% | 10775.2% to 13454.2% | 1 -> 0 | 1,280 -> 0 |
| Split 7 commands | 555.9 -> 311.3 | 44.0% to 46.5% | 78.9% to 86.5% | 1 -> 0 | 192 -> 0 |
| Split 8 commands | 593.2 -> 347.0 | 41.5% to 44.2% | 71.0% to 78.7% | 1 -> 0 | 192 -> 0 |
| Find short long-string terminator | 299.6 -> 27.4 | 90.1% to 91.8% | 915.5% to 1136.4% | 1 -> 0 | 4 -> 0 |
| Find 66-byte terminator | 1,729.9 -> 98.6 | 94.3% to 94.7% | 1634.4% to 1808.7% | 1 -> 0 | 199 -> 0 |
| Find terminator after 40,960-byte payload | 14,673.9 -> 1,238.1 | 90.1% to 94.0% | 922.4% to 1594.5% | 1 -> 0 | 4 -> 0 |
| Find terminator in 24,576-byte dense payload | 83,135.6 -> 28,716.8 | 64.9% to 67.5% | 185.1% to 208.0% | 1 -> 0 | 4 -> 0 |
| Complete source: 1 call / 8 commands | 3,151.1 -> 1,592.0 | 47.6% to 49.5% | 90.2% to 98.2% | 3 -> 1 | 571 -> 300 |
| Complete source: 64 calls / 4 commands each | 96,363.9 -> 46,535.7 | 51.6% to 54.5% | 106.6% to 119.8% | 129 -> 1 | 18,274 -> 11,682 |
| Complete source: 64 calls / commented arguments | 261,217.0 -> 106,670.1 | 59.2% to 63.1% | 145.7% to 171.4% | 641 -> 65 | 20,726 -> 14,582 |
| Complete source: 64 calls / long-string arguments | 386,597.8 -> 104,890.1 | 72.1% to 73.7% | 258.7% to 281.1% | 897 -> 1 | 51,042 -> 33,954 |
| All 152 noteskin files | 1,239,506.2 -> 947,660.4 | 21.8% to 23.5% | 28.0% to 30.9% | 1,156 -> 153 | 363,359 -> 311,670 |

Complete corpus preprocessing uses 21.8% to 23.5% fewer calling-thread CPU
cycles and has 28.0% to 30.9% greater byte throughput. Allocations and frees fall
from 1,156/1,156 to 153/153
per corpus iteration (86.8% fewer allocations); reallocations
fall from 102 to 57. Requested and freed bytes
both fall from 363,359/363,359 to
311,670/311,670 (14.2% fewer requested bytes).

The corpus contains all 152 repository noteskin Lua files, totaling 148,744
UTF-8 bytes. Files are loaded once before measurement. Each iteration preprocesses
every file and drops the output or error inside the measured window, including
existing corpus errors. This measures preprocessing, without filesystem reads,
Lua evaluation, asset decoding or rendering.

The nonempty comment-free strip helpers remove their single body allocation
and free; the empty strip helper already has zero churn in both versions.
Splits through eight slices remove the Vec's allocation, frees and any growth
reallocation. Successful delimiter searches remove all terminator allocations,
frees and growth reallocations, including the dense fallback. Actual-comment
stripping retains its original one-String allocation while reducing character
copy work. The CSV records the complete operations' remaining output costs.

## Controls and limits

Negative percentages mean increased cost or lower throughput. Empty inputs and
large spill cases are included alongside the optimized common paths.

| Workload | Thread cycles/op, old -> new | CPU reduction | Throughput gain |
| --- | ---: | ---: | ---: |
| Strip empty body | 33.7 -> 39.1 | -149.2% to -8.9% | -59.9% to -8.4% |
| Split empty body | 150.4 -> 46.2 | 63.6% to 69.9% | 176.0% to 233.9% |
| Split 1 command | 173.9 -> 68.7 | 53.6% to 61.4% | 111.2% to 159.5% |
| Split 9 commands: first spill | 848.7 -> 529.6 | 33.2% to 45.6% | 49.6% to 83.8% |
| Split 16 commands | 1,224.7 -> 807.6 | 23.9% to 34.1% | 31.8% to 52.0% |
| Split 64 commands | 3,610.4 -> 3,115.9 | 0.2% to 19.4% | 0.2% to 23.8% |
| 66-byte delimiter / 256 longer near matches | 27,278.0 -> 27,929.7 | -10.0% to -2.4% | -9.1% to -2.3% |
| Complete empty source | 27.7 -> 33.7 | -49.3% to -21.7% | -33.1% to -18.1% |
| Complete source: 1 call / 1 command | 1,317.9 -> 1,174.3 | 10.9% to 26.8% | 11.8% to 36.9% |
| Complete source: 1 call / 9 commands | 3,731.0 -> 1,939.2 | 42.9% to 48.3% | 75.1% to 93.5% |
| Complete source: 1 call / 64 commands | 18,744.2 -> 9,403.8 | 47.5% to 50.2% | 90.4% to 101.0% |

Controls with higher CPU cost in every paired run: Strip empty body, 66-byte delimiter / 256 longer near matches, Complete empty source.
Controls with mixed CPU changes across runs: none.
The raw CSV includes all controls and timing ranges. Inline storage has its own
entry/return and spill costs, and removing a heap allocation does not guarantee
lower CPU cost for every input. The wide near-match control retains a linear
byte scan rather than constructing a wide terminator.

These are measurements of the named operations, not whole-game frame rates or
complete loading times. Requested/freed bytes count allocator churn, not peak
RSS or retained memory. The command slice list trades small transient stack
storage for heap churn; command bodies borrow only within preprocessing.
Error messages still allocate their owned Strings. No general zero-allocation
claim is made for the public API.

## Method and behavior checks

The entire baseline command preprocessor is frozen from `7508e0b1b`, modulo
helper visibility and formatting, so baseline helpers never call new helpers.
The audit checks it against the parent and verifies the exact Cargo patch bump.
Both implementations use the existing scoped System allocator harness.
Allocation, reallocation, free, requested-byte and freed-byte counts are
collected separately from timing, including output destruction.

Seven warmed samples per workload produce median wall time and Windows
`QueryThreadCycleTime` calling-thread CPU cycles. Throughput is bytes/second
for stripping, delimiters, full preprocessing and the corpus; split throughput
is command slices/second. Empty helper/source controls use one nominal unit.
Runs alternate old-first and new-first. All four benchmark runs are serial,
with builds, tests and Clippy finished before timing. Fixture construction and
corpus loading are outside measurement. The harness's allocator checks are
included equally for both implementations.

Fixtures cover bodies with literal comment markers and real comments, splits
of 0/1/7/8/9/16/64 commands, delimiter widths up to 66 bytes, payloads up to
40,960 bytes, dense/near matches, sources containing up to 64 command calls,
and the complete noteskin corpus. The 64-call source batches have four commands
per call. Tiny helpers use 4,096 iterations per sample; large delimiter/source
cases use 128; the corpus uses 64.

Eight new behavior tests pass in debug and release. They cover:

- Exact borrowed/owned comment-stripping text, line endings, quoted markers,
  long comments, Unicode and embedded NULs.
- Nested command splitting, empty entries and inline spill boundaries through
  257 commands.
- Long delimiters through 1,024 equals signs, overlapping candidates,
  unterminated strings, 2,048 random payloads and dense fallback boundaries.
- Identifier rules, multiple expansions, exact error strings and priority,
  plus 1,024 generated sources and malformed suffixes.
- Lua argument effects, method order and partial state/error behavior.
- Exact output or error equality on all 152 noteskin files and allocation
  assertions for both prepared helpers and complete preprocessing.

Both full library suites report **676 passed, 4 failed, 65 ignored**. The four
failures match the parent's assertions and source locations:

- `compile_song_lua_extracts_actorproxy_targets`
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`
- `compile_song_lua_runs_cmd_queuecommand_builders`
- `compile_song_lua_supports_notefield_column_api`

Clippy passes with existing repository warnings and no new warnings in the
changed production code or fixtures. Formatting and `git diff --check` pass.
Environment: Windows x86_64, Intel Xeon E5-2696 v4 at 2.20 GHz, rustc 1.98.1
(LLVM 22.1.8), normal workspace release profile with optimization level 3 and
fat LTO. Commands:

```text
cargo test --locked -p deadsync-song-lua --lib
cargo test --locked -p deadsync-song-lua --release --lib
cargo clippy --locked -p deadsync-song-lua --lib --tests
cargo metadata --locked --no-deps --format-version 1
cargo test --locked -p deadsync-song-lua --release --lib cmd_preprocess_bench -- --ignored --nocapture --test-threads=1
```

The benchmark command runs four times; `DEADSYNC_PERF_REVERSE=1` reverses order
for runs 2 and 4. Preliminary delimiter probes informed the dense fallback;
the CSV contains only the final implementation's release runs.

Workspace version: **0.5.1630 -> 0.5.1631**, exactly one patch increment.
Cargo.toml and Cargo.lock are included in the pass. The four excluded files
`deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh` and `optimize.ps1`
are outside the commit.
