# Option text preparation - 0.5.1207

Baseline: `fc570795b92dfa3b2978ec73c93e042fa114d378` / 0.5.1206.
This pass applies the repeated-string-copy and allocation-reuse guidance in
`rust-performance.md` (`M-HOTPATH`, `M-MEM-REUSE`, `M-INITIAL-CAPACITY`).

1. **Search current-value detail:** borrow the selected row's existing actor
   text, then retain one formatted detail string per open overlay. Repeated
   frames compare the actual choice text and translation Arc before sharing
   the result. Player changes, replacement choices, locale switches and
   translation reloads cannot reuse stale detail text. The translation
   formatter itself is unchanged.
2. **Search query and caret:** short text fits in the actor's inline payload.
   Longer queries create each caret-on/off shared string only when needed and
   keep it until the query changes. Typing, backspace and ghost acceptance
   invalidate both variants. Long UTF-8 queries reserve String scratch once
   without a repeated validity scan; the caret-off variant copies the existing
   str directly. Closing the overlay releases its cache. The player-options
   search hint also keeps the translation's existing Arc.
3. **Option layout labels:** preserve translated Arcs and copy dynamic labels
   directly into final shared text, removing temporary String copies and the
   second text vector. A private conversion trait keeps one choice-generation
   implementation while retaining the original owned-string representation
   for input handling. Numeric input choices therefore do not acquire an
   extra temporary Arc. Layout geometry and layout-cache invalidation stay
   unchanged. This improves layout construction/rebuilds, not cached layout
   reads on every frame.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0
(`88d9e12ae`), repository release profile (opt-level 3, LTO). The standalone
integration test compiles the production helper modules. Frozen parent
search and layout callers supply the behavior oracle; their five function
bodies were checked against the parent commit, ignoring whitespace and
visibility changes. The benchmark baseline extracts their text-producing
expressions, substituting prebuilt inputs for state/row access. Both sides
use the unchanged translation formatter.

Three warmups precede seven timing samples per measurement. Five complete
runs alternate old-first/new-first order, after builds and checks finish.
Tables report medians of the five per-run medians. CPU cycles are Windows
`QueryThreadCycleTime` measurements for the calling thread. Allocator counting
is disabled during timing; a separate operation records allocations,
reallocations, frees, requested bytes and freed bytes. Outputs are consumed
through black boxes and dropped inside the measured scope.

Warm cases prepopulate the retained entry outside timing. Cold cases include
cache construction and destruction. The 60-frame workloads include cache
construction, both blink states or choice changes, and cache destruction.
An additional current-value case changes on every frame; the typing case
performs 30 query edits with ten frames per edit. The translated-current case
includes the actual translation lookup on both sides; other current cases
receive a pre-resolved template. Input strings and fixture lists are prepared
outside timing.

Menu benchmarks include choice resolution, final shared-array construction
and destruction, excluding font measurement, geometry and screen state.
Fixtures contain 2, 8 or 128 localized, literal, mixed, or dynamic string
labels. Throughput counts text preparations, frames, or menu labels, as
appropriate. These are focused text-path measurements, not full-frame FPS
or whole-application CPU measurements. Requested bytes describe allocation
churn, not RSS or allocator metadata; thread cycles do not measure retired
instructions or cache misses.

## Results

All 34 paired workloads have lower median wall time and thread cycles in
this run. Warm current detail including translation lookup uses **81.5% fewer
cycles** and drops from two allocations to none. The six-byte query/caret
case uses **87.5% fewer cycles** with no allocations or reallocations.
Preparing eight translated menu labels uses **73.5% fewer cycles**, reduces
allocation/free calls from **19 to 2**, and requested bytes from **720 to 272**.

The [CSV](option-frame-0.5.1207.csv) contains all 340 measurements, including
seven-sample timing ranges and all churn counters. The host is shared;
background activity is not controlled. These relative results are evidence
for these fixtures, not a guarantee of identical gains on other machines.

| Workload | Old ns/op | New ns/op | Thread cycles old/new | Fewer cycles | Throughput old/new (M units/s) |
|---|---:|---:|---:|---:|---:|
| Current / short, warm | 309.4 | 30.8 | 677.9 / 67.7 | 90.0% | 3.232 / 32.468 |
| Current / short, cold | 302.6 | 233.3 | 663.3 / 512.0 | 22.8% | 3.305 / 4.286 |
| Current / Unicode, warm | 336.5 | 43.5 | 737.1 / 95.3 | 87.1% | 2.972 / 22.967 |
| Current / Unicode, cold | 332.8 | 282.6 | 729.7 / 620.0 | 15.0% | 3.005 / 3.539 |
| Current / 520 bytes, warm | 664.0 | 36.1 | 1,455.6 / 79.3 | 94.6% | 1.506 / 27.685 |
| Current / 520 bytes, cold | 670.0 | 618.9 | 1,469.5 / 1,356.0 | 7.7% | 1.492 / 1.616 |
| Current / 60 frames, two values | 14,580.1 | 1,722.4 | 31,943.6 / 3,778.3 | 88.2% | 4.115 / 34.835 |
| Current / warm, including translation lookup | 286.0 | 52.8 | 623.4 / 115.5 | 81.5% | 3.496 / 18.954 |
| Current / value changes every frame, 60 frames | 14,509.5 | 12,111.3 | 31,817.2 / 26,538.9 | 16.6% | 4.135 / 4.954 |
| Query / 30 edits, 10 frames each | 46,216.8 | 8,935.8 | 101,222.4 / 19,568.9 | 80.7% | 6.491 / 33.573 |
| Query / 6 bytes, warm | 163.8 | 21.3 | 358.6 / 44.7 | 87.5% | 6.106 / 46.948 |
| Query / 6 bytes, cold | 162.5 | 21.6 | 356.9 / 46.7 | 86.9% | 6.155 / 46.189 |
| Query / 6 bytes, 60 frames | 7,253.9 | 1,421.8 | 15,862.2 / 3,109.0 | 80.4% | 8.271 / 42.200 |
| Query / 12 bytes, warm | 157.8 | 26.6 | 346.3 / 58.4 | 83.1% | 6.336 / 37.658 |
| Query / 12 bytes, cold | 165.5 | 126.1 | 362.6 / 276.9 | 23.6% | 6.043 / 7.933 |
| Query / 12 bytes, 60 frames | 7,469.7 | 1,830.0 | 16,388.1 / 4,015.5 | 75.5% | 8.032 / 32.787 |
| Query / 36 bytes, warm | 180.0 | 22.3 | 390.6 / 49.0 | 87.5% | 5.554 / 44.883 |
| Query / 36 bytes, cold | 175.4 | 124.8 | 384.6 / 273.7 | 28.8% | 5.703 / 8.013 |
| Query / 36 bytes, 60 frames | 7,382.2 | 1,601.1 | 16,149.5 / 3,511.1 | 78.3% | 8.128 / 37.474 |
| Query / 540 UTF-8 bytes, warm | 180.6 | 21.3 | 392.3 / 46.8 | 88.1% | 5.538 / 46.959 |
| Query / 540 UTF-8 bytes, cold | 199.5 | 170.2 | 435.9 / 373.3 | 14.4% | 5.012 / 5.876 |
| Query / 540 UTF-8 bytes, 60 frames | 8,084.2 | 1,709.3 | 17,734.3 / 3,748.0 | 78.9% | 7.422 / 35.102 |
| Menu / 2 translated labels | 534.0 | 201.4 | 1,172.8 / 442.8 | 62.2% | 3.745 / 9.928 |
| Menu / 2 literal labels | 571.0 | 266.9 | 1,251.4 / 586.4 | 53.1% | 3.503 / 7.495 |
| Menu / 2 mixed labels | 621.1 | 241.2 | 1,362.4 / 530.1 | 61.1% | 3.220 / 8.294 |
| Menu / 2 dynamic string labels | 716.4 | 289.4 | 1,571.0 / 635.9 | 59.5% | 2.792 / 6.911 |
| Menu / 8 translated labels | 1,635.9 | 432.9 | 3,590.0 / 951.3 | 73.5% | 4.890 / 18.478 |
| Menu / 8 literal labels | 1,696.2 | 647.5 | 3,679.4 / 1,407.4 | 61.7% | 4.717 / 12.355 |
| Menu / 8 mixed labels | 1,674.0 | 644.8 | 3,651.4 / 1,413.9 | 61.3% | 4.779 / 12.408 |
| Menu / 8 dynamic string labels | 1,215.2 | 706.1 | 2,664.4 / 1,524.0 | 42.8% | 6.584 / 11.329 |
| Menu / 128 translated labels | 24,408.9 | 5,102.6 | 53,427.0 / 11,172.3 | 79.1% | 5.244 / 25.085 |
| Menu / 128 literal labels | 26,054.5 | 9,120.2 | 57,060.3 / 19,998.6 | 65.0% | 4.913 / 14.035 |
| Menu / 128 mixed labels | 26,586.4 | 7,075.4 | 58,191.1 / 15,487.8 | 73.4% | 4.814 / 18.091 |
| Menu / 128 dynamic string labels | 20,233.4 | 10,587.4 | 44,328.6 / 23,170.1 | 47.7% | 6.326 / 12.090 |

Warm current/query cases have zero heap allocation, reallocation and free
churn after preparation. This is bounded retention, not zero total memory:
one current detail entry keeps its choice/template references and formatted
text alive, and up to two long-query variants live until the next query edit
or overlay close. Existing actors can retain their own Arc references after
invalidation. Short queries need no retained heap text.

The cold 540-byte Unicode case uses two allocation/free calls instead of
one, but removes the old reallocation, requests fewer bytes (1,620 -> 1,103)
and uses 14.4% fewer cycles. Its 60-frame lifecycle reduces requested bytes
from 64,800 to 1,663 and uses 78.9% fewer cycles. A changing current value on
every frame still saves 16.6% of cycles and halves allocation calls; repeated
values save much more. First use of a 520-byte current value is the smallest
measured cycle improvement (7.7%). Numeric input still makes its original
single String allocation; font layout and cached menu reads are outside the
label benchmark scope.

The 128-label fixtures are scaling cases. Localized fixtures repeat `On`,
literal fixtures repeat `1920x1080`, mixed fixtures alternate `Off`/`16:9`,
and dynamic fixtures contain distinct 103+ byte strings. The current short
fixture is `100%`; query sizes are byte counts before adding the three-byte
caret. The long Unicode query deliberately crosses the scratch boundary.

| Workload | Allocations old/new | Reallocations old/new | Frees old/new | Requested/freed bytes old/new |
|---|---:|---:|---:|---:|
| Current / short, warm | 2 / 0 | 0 / 0 | 2 / 0 | 40 / 0 |
| Current / short, cold | 2 / 1 | 0 / 0 | 2 / 1 | 40 / 32 |
| Current / Unicode, warm | 2 / 0 | 0 / 0 | 2 / 0 | 54 / 0 |
| Current / Unicode, cold | 2 / 1 | 0 / 0 | 2 / 1 | 54 / 40 |
| Current / 520 bytes, warm | 3 / 0 | 0 / 0 | 3 / 0 | 1,601 / 0 |
| Current / 520 bytes, cold | 3 / 2 | 0 / 0 | 3 / 2 | 1,601 / 1,081 |
| Current / 60 frames, two values | 120 / 2 | 0 / 0 | 120 / 2 | 2,400 / 64 |
| Current / warm, including translation lookup | 2 / 0 | 0 / 0 | 2 / 0 | 40 / 0 |
| Current / value changes every frame, 60 frames | 120 / 60 | 0 / 0 | 120 / 60 | 2,400 / 1,920 |
| Query / 30 edits, 10 frames each | 300 / 19 | 250 / 0 | 300 / 19 | 13,990 / 824 |
| Query / 6 bytes, warm | 1 / 0 | 1 / 0 | 1 / 0 | 24 / 0 |
| Query / 6 bytes, cold | 1 / 0 | 1 / 0 | 1 / 0 | 24 / 0 |
| Query / 6 bytes, 60 frames | 60 / 0 | 30 / 0 | 60 / 0 | 960 / 0 |
| Query / 12 bytes, warm | 1 / 0 | 1 / 0 | 1 / 0 | 36 / 0 |
| Query / 12 bytes, cold | 1 / 1 | 1 / 0 | 1 / 1 | 36 / 32 |
| Query / 12 bytes, 60 frames | 60 / 1 | 30 / 0 | 60 / 1 | 1,440 / 32 |
| Query / 36 bytes, warm | 1 / 0 | 1 / 0 | 1 / 0 | 108 / 0 |
| Query / 36 bytes, cold | 1 / 1 | 1 / 0 | 1 / 1 | 108 / 56 |
| Query / 36 bytes, 60 frames | 60 / 2 | 30 / 0 | 60 / 2 | 4,320 / 112 |
| Query / 540 UTF-8 bytes, warm | 1 / 0 | 1 / 0 | 1 / 0 | 1,620 / 0 |
| Query / 540 UTF-8 bytes, cold | 1 / 2 | 1 / 0 | 1 / 2 | 1,620 / 1,103 |
| Query / 540 UTF-8 bytes, 60 frames | 60 / 3 | 30 / 0 | 60 / 3 | 64,800 / 1,663 |
| Menu / 2 translated labels | 7 / 2 | 0 / 0 | 7 / 2 | 192 / 80 |
| Menu / 2 literal labels | 9 / 4 | 0 / 0 | 9 / 4 | 274 / 144 |
| Menu / 2 mixed labels | 8 / 3 | 0 / 0 | 8 / 3 | 216 / 104 |
| Menu / 2 dynamic string labels | 7 / 4 | 0 / 0 | 7 / 4 | 574 / 320 |
| Menu / 8 translated labels | 19 / 2 | 0 / 0 | 19 / 2 | 720 / 272 |
| Menu / 8 literal labels | 27 / 10 | 0 / 0 | 27 / 10 | 1,048 / 528 |
| Menu / 8 mixed labels | 23 / 6 | 0 / 0 | 23 / 6 | 816 / 368 |
| Menu / 8 dynamic string labels | 19 / 10 | 0 / 0 | 19 / 10 | 2,248 / 1,232 |
| Menu / 128 translated labels | 259 / 2 | 0 / 0 | 259 / 2 | 11,280 / 4,112 |
| Menu / 128 literal labels | 387 / 130 | 0 / 0 | 387 / 130 | 16,528 / 8,208 |
| Menu / 128 mixed labels | 323 / 66 | 0 / 0 | 323 / 66 | 12,816 / 5,648 |
| Menu / 128 dynamic string labels | 259 / 130 | 0 / 0 | 259 / 130 | 36,098 / 19,696 |

## Behavior and validation

The new full-overlay regression compares every actor property with the
parent, normalizing only text ownership variants. It covers blinking,
typing, backspace, ghost completion, focus changes, long/Unicode queries,
closing/reopening, P1/P2 selection, out-of-range selection indices, live
choice replacement and empty choice lists. Every row in all 24 option
submenus is compared for both input/display text and all layout fields,
including invalid row indices and changed/empty dynamic lists. Geometry
comparisons use the missing-font fallback; the font-measurement code is
unchanged.

Standalone tests cover UTF-8 inline boundaries, cached actor lifetimes,
clone/invalidation behavior, locale changes and same-locale resource reloads,
placeholder edge cases, shared label identity, zero warmed allocation/free
churn, short-query zero allocations, and the original single-allocation
numeric input path.

- Full theme suite: **1,279 passed**, 5 pre-existing ignored tests, serial.
- Focused integration suite: **10 passed**, one manual benchmark ignored,
  in both debug and release modes.
- `cargo check -p deadsync`: passed.
- Performance Clippy passed with the existing
  `-A clippy::large_enum_variant` exception. A run without that exception
  reported only the unchanged `SimplyLoveRuntimeRequest` in `effects.rs:928`.
- An initial parallel theme run failed
  `song_lua_kenpo_nested_rotation_matches_native_motion`, which sets shared
  viewport metrics. The complete serial suite passed; no gameplay code was
  changed by this pass.
- Changed Rust files pass scoped rustfmt checking; `git diff --check` passes.

The performance lint command is:

```powershell
cargo clippy -p deadsync-theme-simply-love --lib --test option_frame_perf --no-deps -- -A clippy::all -D clippy::perf -A clippy::large_enum_variant
```

## Reproduce

```powershell
cargo test -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo test -p deadsync-theme-simply-love --test option_frame_perf
cargo test -p deadsync-theme-simply-love --release --test option_frame_perf
cargo test -p deadsync-theme-simply-love --release --test option_frame_perf benchmark_option_frame -- --ignored --test-threads=1 --nocapture
```

Repeat the last command five times, setting `DEADSYNC_PERF_REVERSE=1` for
runs 2 and 4, and removing it for runs 1, 3 and 5. The integration harness
uses the shared `tests/support/perf.rs` allocator and timing implementation.
