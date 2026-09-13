# Help text and lobby updates — 0.5.1202

Baseline: `fcb00c347` / 0.5.1201. This pass applies `M-HOTPATH`,
`M-MEM-REUSE`, `M-INITIAL-CAPACITY`, `M-BOX-DST`, and `M-THROUGHPUT`
from `rust-performance.md` to three sources of UI allocation churn.

1. **Help text wrapping:** reuse one line buffer instead of cloning candidate
   strings for every word and character. Long words are split using borrowed
   UTF-8 slices, emitted directly into the output. The output reserves space
   based on the input length. Font measurements, zoom, hard-break decisions,
   whitespace normalization, and the measurement-call sequence are preserved.
2. **Lobby text construction:** format directly into one output string instead
   of allocating a vector of lines, temporary percentages, truncated names,
   screen names, and song-path components before joining them. Player ordering
   is unchanged. The owned output and player-order vector still allocate.
3. **Lobby snapshot refresh:** update scores and ready flags in the existing
   snapshot, retaining unchanged text and equal-length boxed player arrays.
   Score-only updates have zero allocation/free churn. Text and player arrays
   remain boxed and exactly sized, preserving the previous retained-memory
   layout. Changed names and membership can still allocate. Cache matching,
   including its treatment of nonfinite and nonpositive percentages, is unchanged.

Help wrapping runs when description layouts are rebuilt. Lobby text and
snapshots are rebuilt on cache misses, including changing gameplay scores;
cache hits already reuse the panel. These are targeted UI benchmarks, not
measurements of game FPS, audio latency, or total application CPU usage.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0
(`88d9e12ae`), system allocator. The repository release profile uses opt-level
3 and LTO. Production source modules and frozen parent implementations compile
into the same integration-test binary. The isolated binary uses the existing
thread-local allocation tracker without conflicting with the theme's gameplay
allocator diagnostics. Its two actor DSL arms invoke the same production
builders as the theme. No production dependencies or unsafe code were added.

Each measurement uses three warmups and seven timing samples, followed by a
separate allocation-counted operation. Five complete runs alternate old/new
order. Builds and other checks for this pass finish before benchmarking.
CPU cycles use Windows `QueryThreadCycleTime` for the calling thread; these
workloads are single-threaded. Results below are medians of the five per-run
medians. The [CSV](ui-text-0.5.1202.csv) records all 170 measurements, including
timing ranges, throughput, allocation/reallocation/free counts, and bytes.

Wrapping uses synthetic menu prose and long paths plus multilingual text,
with actual `measure_line_width_logical` calls and prepared font glyph tables.
One operation wraps a complete input. Throughput counts Unicode scalar values:
the paragraph has 564, the path 1,056, and the Unicode input 512. Body-text
operations format an entire lobby. Snapshot operations alternate prebuilt
score/ready states and include replacing/dropping the old snapshot in the
baseline. Lobby throughput counts players, except empty and membership/rename
controls, which count operations. Input construction is outside measurement;
output allocation and destruction are included. Snapshot storage persists
between operations, just as it does in the screen-owned cache.

Allocated/freed bytes are allocator-requested bytes, including reallocation
sizes. They measure memory churn, not RSS, allocator overhead, cache misses, or
peak resident memory. Snapshot types retain their previous storage layout;
wrapping buffers and body output may retain spare capacity until consumed.

## Results

| Workload | Old µs/op | New µs/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| Short help text | 0.994 | 0.623 | 2,187.3 | 1,372.7 | 37.2% |
| Help paragraph | 28.832 | 10.669 | 63,184.3 | 23,410.2 | 62.9% |
| Long path wrapping | 239.741 | 51.684 | 525,402.4 | 113,298.9 | 78.4% |
| Unicode wrapping | 140.080 | 39.815 | 306,987.9 | 87,307.8 | 71.6% |
| Lobby text, 2 players | 4.762 | 1.747 | 10,444.7 | 3,837.4 | 63.3% |
| Lobby text, 8 players | 14.942 | 6.239 | 32,750.9 | 13,681.5 | 58.2% |
| Lobby text, 32 players | 54.261 | 23.202 | 118,946.7 | 50,860.1 | 57.2% |
| Snapshot scores, 2 players | 0.590 | 0.070 | 1,292.5 | 154.1 | 88.1% |
| Snapshot scores, 8 players | 1.377 | 0.164 | 3,015.4 | 360.9 | 88.0% |
| Snapshot scores, 32 players | 4.929 | 0.515 | 10,808.7 | 1,132.2 | 89.5% |
| Snapshot, all 8 names changed | 1.433 | 0.701 | 3,145.1 | 1,535.1 | 51.2% |
| Snapshot, 8/9-player membership changes | 1.478 | 1.240 | 3,241.2 | 2,712.0 | 16.3% |

| Workload | Allocations old/new | Reallocations old/new | Requested bytes old/new |
|---|---:|---:|---:|
| Help paragraph | 91 / 2 | 99 / 3 | 8,520 / 684 |
| Long path wrapping | 1,081 / 2 | 1,063 / 1 | 81,058 / 4,224 |
| Unicode wrapping | 561 / 2 | 521 / 1 | 39,279 / 6,656 |
| Lobby text, 2 players | 21 / 2 | 8 / 0 | 1,215 / 297 |
| Lobby text, 8 players | 51 / 2 | 28 / 1 | 3,203 / 1,001 |
| Lobby text, 32 players | 171 / 2 | 104 / 3 | 13,014 / 4,073 |
| Snapshot scores, 8 players | 21 / 0 | 0 / 0 | 670 / 0 |
| Snapshot scores, 32 players | 69 / 0 | 0 / 0 | 2,564 / 0 |

Free counts and freed bytes match allocation counts and requested bytes for
the rows above. Rename and membership controls have different old/new payload
sizes at destruction; their separate free counts/bytes are retained in the CSV.

Paragraph throughput rises from 19.56 to 52.86 million characters/s; path
wrapping from 4.40 to 20.43 million characters/s. Eight-player lobby text rises
from 0.535 to 1.282 million players/s, and score-only snapshot refreshes from
5.81 to 48.79 million players/s. These throughput figures describe the isolated
operations and must not be interpreted as complete panel-render throughput.

Every nonempty workload, including renamed players and changed membership,
used fewer cycles in every paired run. Paragraph cycle reductions ranged from
60.9–66.2%; eight-player text from 57.4–61.6%; eight-player score snapshots from
87.7–89.2%. The empty-wrap control remains allocation-free in both versions:
27.7 vs 28.5 ns/op, with equal median cycle counts (68.6); individual pairs vary
from 6.7% more to 1.3% fewer cycles. No empty-wrap speedup is claimed. The empty
snapshot benchmark is an update-method control, not a real score-change cache
miss. The CSV also includes 128-player stress cases.

## Behavior and validation

Nine new tests cover independently expected wrapping/layout, differential text
and measurement-call sequences, and allocation budgets. Cases include blank
lines, CRLF, Unicode whitespace and multibyte characters, long words, unusual
widths/zooms, missing/nonfinite/negative scores, player ordering, readiness,
screen names, song paths, status text, cache hits, renamed/reordered/removed/
added players, and snapshot scalar updates. The integration binary also runs
five existing HUD tests. Frozen lobby text functions were checked against the
parent commit; player ordering and cache percentage matching are unchanged.

- `cargo test -p deadsync-theme-simply-love --lib -- --test-threads=1`:
  **1,274 passed, 0 failed, 5 ignored**.
- `cargo test -p deadsync-theme-simply-love --test ui_text_perf -- --test-threads=1`:
  **14 passed, 0 failed, 2 manual benchmarks ignored**.
- The same integration command with `--release`: **14 passed, 0 failed**.
- Clippy performance lints pass with the existing `large_enum_variant` warning
  in unchanged `src/effects.rs:928` allowed. Without that exception the existing
  `SimplyLoveRuntimeRequest` enum warning causes the strict check to fail.
- Formatting checks and `git diff --check` pass.

The workspace version advances exactly once: **0.5.1201 → 0.5.1202**.
`Cargo.lock` updates the three packages inheriting the workspace version.

## Reproduce

```powershell
cargo test -p deadsync-theme-simply-love --test ui_text_perf -- --test-threads=1
cargo test --release -p deadsync-theme-simply-love --test ui_text_perf -- --test-threads=1
cargo test --release -p deadsync-theme-simply-love --test ui_text_perf -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test --release -p deadsync-theme-simply-love --test ui_text_perf -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo clippy -p deadsync-theme-simply-love --lib --test ui_text_perf --no-deps -- -A clippy::all -D clippy::perf -A clippy::large_enum_variant
```

Repeat measurement five times, alternating order and keeping builds outside
the measurement interval. Cycle reporting is available on Windows; other
platforms report zero for unavailable cycle measurements, not zero CPU work.
