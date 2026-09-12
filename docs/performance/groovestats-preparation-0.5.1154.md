# GrooveStats request and unlock preparation — 0.5.1154

Baseline: `c157d3295` (0.5.1153). This pass applies the supplied
`rust-performance.md` guidance on measured hot paths (M-HOTPATH), initial
capacity (M-INITIAL-CAPACITY), and allocation reuse (M-MEM-REUSE).
The guide itself is excluded from the commit.

Two-player submission preparation uses 78.58% fewer thread cycles and 70.62%
less requested byte churn. Consuming a draft into a job uses 42.58% fewer cycles
for the ordinary fixture, including common input cloning. A large enabled unlock
plan uses 62.77% fewer cycles and 64.57% less byte churn. Thirty-three of 38 cases
use fewer cycles; five control regressions are listed below.

## Changes

1. **Encode request JSON directly.** Serialize borrowed typed payloads into
   the final byte buffer, removing the intermediate JSON value tree and its
   owned field names, strings and map nodes. A small map of borrowed payloads
   preserves last-payload-wins behavior for duplicate slots. Reserve a body
   capacity hint and send the encoded bytes directly through the HTTP client.
2. **Move draft data into requests.** Consume the draft's job strings and API
   key instead of cloning them through temporary request objects. Retry
   preparation borrows its payload directly while retaining the stored retry
   entry. Existing borrowing helpers remain available.
3. **Build final unlock outputs directly.** Iterate eligible events and quests
   without temporary event, folder-slice or per-event download vectors.
   Count valid downloads for an exact vector reservation, reserve complete
   name capacities, and trim names in place. Preserve empty and duplicate
   folder groups. Empty download outputs allocate nothing.

## Method

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8. Cargo release uses optimization level 3
and full LTO. Seven old functions are frozen in the test-only baseline; shared
types and the borrowing job/request helpers remain unchanged.

The 38 comparisons ran in the same release binary. Three invocations alternate
old/new, new/old, old/new, with black-boxed inputs and opaque function pointers
where appropriate. Each invocation uses three warmups and seven timing batches,
then separately counts one operation. Tables report the median of the three
per-invocation medians. Batches contain 512 operations for parts, drafts, retries
and downloads; 4,000 for jobs; and 256 for plans. No benchmark ran alongside a
Cargo build.

Request comparisons include preparation **and encoding** on both sides.
The locked ureq 3.4.1 `send_json` calls `SendBody::from_json`, which uses
`serde_json::to_vec_pretty`. The old benchmark performs that same encoding and
retains the old JSON tree until the operation ends. New requests already own
compact JSON bytes. HTTP setup, networking and response handling are outside
the timed scope. The localhost HTTP behavior test exercises the new sending path.

Both sides clone input fixtures inside the timed draft and job comparisons
because those operations consume their inputs. That common setup cost is
included in timing and churn. The new job conversion itself allocates nothing;
its benchmark still includes seven input-string clones and their destruction.
Other cases borrow prebuilt fixtures. All returned values are destroyed inside
the measured operation.

CPU counts use Windows QueryThreadCycleTime for the calling thread. Allocation
instrumentation delegates to System and counts allocations, reallocations,
frees and requested bytes. Bytes describe churn, including replacement buffers,
not peak RSS, committed heap pages, cache misses or the production allocator.
These results measure local preparation, not network latency or game frame rate.

Request fixtures cover empty, one player, two players, duplicate slots, long
strings and escaping-heavy strings. Long and escaped cases repeat comment and
player-option fragments 256 times, including Unicode, quotes and control bytes.
Unlock fixtures have 0, 1 or 128 quests per event, with two thirds valid URLs
in populated cases, all-invalid URLs, or whitespace-only titles. Plan fixtures
combine SRPG and ITL events with ITL enabled/disabled and downloads enabled/disabled.

## Timing, CPU and throughput

Throughput below is complete operations per second. Positive CPU savings mean
fewer cycles. Raw harness output additionally reports input players or quests
per second, with a minimum unit count of one for empty controls.

| Case | Old ns/op | New ns/op | Old cycles/op | New cycles/op | CPU saved | Old → new ops/s |
|---|---:|---:|---:|---:|---:|---:|
| parts_empty | 282.4 | 288.1 | 622.9 | 635.3 | -1.99% | 3,541,076 → 3,471,017 |
| drafts_empty | 304.7 | 313.7 | 670.9 | 700.9 | -4.47% | 3,281,917 → 3,187,759 |
| parts_single | 7,733.0 | 1,904.1 | 16,959.4 | 4,165.4 | 75.44% | 129,316 → 525,183 |
| drafts_single | 9,421.7 | 2,271.3 | 20,653.6 | 4,976.0 | 75.91% | 106,138 → 440,276 |
| parts_double | 14,337.3 | 3,077.0 | 31,505.5 | 6,747.1 | 78.58% | 69,748 → 324,992 |
| drafts_double | 18,083.6 | 3,869.1 | 39,608.2 | 8,484.6 | 78.58% | 55,299 → 258,458 |
| parts_duplicate | 19,334.0 | 3,142.4 | 42,393.9 | 6,816.1 | 83.92% | 51,722 → 318,228 |
| drafts_duplicate | 24,667.0 | 4,874.4 | 54,063.4 | 10,694.6 | 80.22% | 40,540 → 205,153 |
| parts_long | 45,143.9 | 29,646.7 | 98,925.1 | 64,932.1 | 34.36% | 22,151 → 33,731 |
| drafts_long | 52,854.5 | 33,900.4 | 115,796.1 | 74,189.7 | 35.93% | 18,920 → 29,498 |
| parts_escaped | 53,148.8 | 41,628.5 | 116,492.8 | 91,092.5 | 21.80% | 18,815 → 24,022 |
| drafts_escaped | 61,767.4 | 41,006.1 | 135,349.2 | 89,780.6 | 33.67% | 16,190 → 24,387 |
| job_single | 846.9 | 486.3 | 1,858.5 | 1,067.1 | 42.58% | 1,180,777 → 2,056,344 |
| retry_single | 8,013.5 | 2,040.4 | 17,540.7 | 4,483.5 | 74.44% | 124,789 → 490,100 |
| job_long | 1,325.4 | 861.6 | 2,906.3 | 1,888.7 | 35.01% | 754,489 → 1,160,631 |
| retry_long | 25,771.7 | 16,803.9 | 56,456.5 | 36,818.6 | 34.78% | 38,802 → 59,510 |
| job_escaped | 1,311.5 | 883.9 | 2,876.3 | 1,935.1 | 32.72% | 762,486 → 1,131,350 |
| retry_escaped | 29,167.8 | 21,830.3 | 63,912.7 | 47,845.0 | 25.14% | 34,284 → 45,808 |
| downloads_empty | 48.4 | 2.9 | 110.6 | 9.4 | 91.50% | 20,661,157 → 344,827,586 |
| plan_empty_enabled | 105.1 | 21.5 | 236.6 | 53.2 | 77.51% | 9,514,748 → 46,511,628 |
| plan_empty_no_itl | 103.9 | 21.5 | 234.1 | 53.2 | 77.27% | 9,624,639 → 46,511,628 |
| plan_empty_no_downloads | 27.0 | 19.5 | 65.2 | 48.9 | 25.00% | 37,037,037 → 51,282,051 |
| downloads_one | 857.4 | 364.1 | 1,876.0 | 794.0 | 57.68% | 1,166,317 → 2,746,498 |
| plan_one_enabled | 2,677.7 | 1,125.0 | 5,884.5 | 2,475.4 | 57.93% | 373,455 → 888,889 |
| plan_one_no_itl | 1,257.8 | 464.1 | 2,766.9 | 1,004.9 | 63.68% | 795,039 → 2,154,708 |
| plan_one_no_downloads | 401.6 | 354.3 | 886.6 | 783.7 | 11.61% | 2,490,040 → 2,822,467 |
| downloads_many | 70,775.6 | 24,544.7 | 155,114.9 | 53,782.6 | 65.33% | 14,129 → 40,742 |
| plan_many_enabled | 197,055.5 | 73,336.7 | 431,645.9 | 160,714.3 | 62.77% | 5,075 → 13,636 |
| plan_many_no_itl | 83,911.3 | 26,721.5 | 183,954.7 | 58,589.3 | 68.15% | 11,917 → 37,423 |
| plan_many_no_downloads | 19,185.9 | 19,619.9 | 42,042.0 | 43,041.7 | -2.38% | 52,122 → 50,969 |
| downloads_invalid | 2,106.4 | 1,528.5 | 4,609.1 | 3,343.5 | 27.46% | 474,744 → 654,236 |
| plan_invalid_enabled | 23,404.3 | 22,746.1 | 51,279.8 | 49,836.8 | 2.81% | 42,727 → 43,964 |
| plan_invalid_no_itl | 2,274.6 | 1,550.4 | 4,972.2 | 3,410.0 | 31.42% | 439,638 → 644,995 |
| plan_invalid_no_downloads | 19,063.7 | 19,449.6 | 41,771.9 | 42,648.2 | -2.10% | 52,456 → 51,415 |
| downloads_blank_titles | 67,764.8 | 24,042.0 | 148,373.8 | 52,610.1 | 64.54% | 14,757 → 41,594 |
| plan_blank_titles_enabled | 199,670.7 | 71,222.7 | 437,303.2 | 155,947.9 | 64.34% | 5,008 → 14,040 |
| plan_blank_titles_no_itl | 80,423.0 | 24,584.8 | 176,249.1 | 53,850.4 | 69.45% | 12,434 → 40,676 |
| plan_blank_titles_no_downloads | 19,577.3 | 20,114.8 | 42,944.8 | 44,128.1 | -2.76% | 51,080 → 49,715 |

## Allocation churn

Each allocation count equals its free count, and each allocated-byte count
equals its freed-byte count in every row. All figures are per operation.

| Case | Allocs/frees old → new | Reallocs old → new | Allocated/freed bytes old → new |
|---|---:|---:|---:|
| parts_empty | 4 → 4 | 0 → 0 | 199 → 73 |
| drafts_empty | 4 → 4 | 0 → 0 | 199 → 73 |
| parts_single | 47 → 10 | 3 → 0 | 5,746 → 1,422 |
| drafts_single | 64 → 17 | 4 → 1 | 6,488 → 1,946 |
| parts_double | 85 → 14 | 4 → 0 | 11,277 → 2,659 |
| drafts_double | 119 → 27 | 5 → 1 | 13,133 → 3,859 |
| parts_duplicate | 124 → 18 | 4 → 0 | 14,870 → 2,829 |
| drafts_duplicate | 174 → 37 | 5 → 1 | 17,730 → 4,705 |
| parts_long | 85 → 14 | 7 → 1 | 78,589 → 72,721 |
| drafts_long | 119 → 27 | 9 → 2 | 179,222 → 95,851 |
| parts_escaped | 85 → 14 | 8 → 1 | 94,135 → 71,185 |
| drafts_escaped | 119 → 27 | 9 → 2 | 143,415 → 93,803 |
| job_single | 12 → 7 | 0 → 0 | 133 → 90 |
| retry_single | 56 → 16 | 3 → 0 | 5,894 → 1,617 |
| job_long | 12 → 7 | 0 → 0 | 13,648 → 11,055 |
| retry_long | 56 → 16 | 7 → 1 | 77,429 → 39,200 |
| job_escaped | 12 → 7 | 0 → 0 | 13,136 → 10,799 |
| retry_escaped | 56 → 16 | 7 → 1 | 60,326 → 38,176 |
| downloads_empty | 0 → 0 | 0 → 0 | 0 → 0 |
| plan_empty_enabled | 1 → 0 | 0 → 0 | 16 → 0 |
| plan_empty_no_itl | 1 → 0 | 0 → 0 | 16 → 0 |
| plan_empty_no_downloads | 0 → 0 | 0 → 0 | 0 → 0 |
| downloads_one | 5 → 4 | 3 → 0 | 438 → 151 |
| plan_one_enabled | 18 → 12 | 8 → 0 | 1,509 → 461 |
| plan_one_no_itl | 7 → 4 | 4 → 0 | 843 → 175 |
| plan_one_no_downloads | 6 → 5 | 0 → 0 | 127 → 111 |
| downloads_many | 341 → 256 | 260 → 0 | 31,086 → 13,027 |
| plan_many_enabled | 942 → 768 | 691 → 0 | 107,839 → 38,207 |
| plan_many_no_itl | 343 → 256 | 345 → 0 | 45,807 → 15,067 |
| plan_many_no_downloads | 258 → 257 | 0 → 0 | 10,121 → 8,073 |
| downloads_invalid | 0 → 0 | 0 → 0 | 0 → 0 |
| plan_invalid_enabled | 259 → 257 | 0 → 0 | 10,137 → 8,073 |
| plan_invalid_no_itl | 1 → 0 | 0 → 0 | 16 → 0 |
| plan_invalid_no_downloads | 258 → 257 | 0 → 0 | 10,121 → 8,073 |
| downloads_blank_titles | 341 → 256 | 260 → 0 | 29,375 → 11,826 |
| plan_blank_titles_enabled | 942 → 768 | 691 → 0 | 104,417 → 35,805 |
| plan_blank_titles_no_itl | 343 → 256 | 345 → 0 | 44,096 → 13,866 |
| plan_blank_titles_no_downloads | 258 → 257 | 0 → 0 | 10,121 → 8,073 |

## Tradeoffs and compatibility

- `parts_empty` used 1.99% more cycles (282.4 → 288.1 ns/op).
- `drafts_empty` used 4.47% more cycles (304.7 → 313.7 ns/op).
- `plan_many_no_downloads` used 2.38% more cycles (19,185.9 → 19,619.9 ns/op).
- `plan_invalid_no_downloads` used 2.10% more cycles (19,063.7 → 19,449.6 ns/op).
- `plan_blank_titles_no_downloads` used 2.76% more cycles (19,577.3 → 20,114.8 ns/op).

JSON values, field names, omitted fields, header/query order, duplicate headers
and query parameters, and last-payload-wins semantics are preserved. JSON object
property ordering and whitespace change: typed fields follow their serialization
order, slots use numeric order, and the body is compact instead of pretty-printed.
The contract tests compare decoded JSON values, not identical old/new byte strings.
The HTTP test verifies that the encoded body arrives unchanged, without double
encoding, with application/json and the expected authentication/query values.

The request body's Rust type changes from JsonValue to Vec<u8>, and
the sender accepts a byte slice. Workspace callers pass the prepared body through
and compile with this change. Retry data is still owned independently of the
request, and job tokens, profile IDs, scores and comments retain their values.

The JSON buffer reserves 1 KiB per unique player plus unescaped string lengths.
This is a capacity hint: sufficiently large escaped strings can still grow the
buffer, as the churn table shows. It is not a zero-allocation request API. Output
strings and collections retain necessary ownership across background submission
and download work. Unlock counting scans accepted-event URLs before construction;
all-invalid lists stop after counting, and all measured unlock outputs avoid
reallocations. Names retain at most their trimmed suffix's unused capacity.

Draft-to-job collection can shrink and reuse the original vector allocation.
Consequently, ordinary draft cases report one reallocation even when their JSON
buffer does not grow. This still removes the temporary request-player vector
and the extra string copies.

## Validation and reproduction

- 264 online behavior tests pass in debug and release, including six new tests.
- Old/new request comparisons cover every u8 slot, duplicates, empty input,
  optional counts, Unicode/control escaping and long strings.
- Draft/retry tests compare complete job values and request JSON, verify stored
  retry data is unchanged, and check moved string buffers and allocation budgets.
- Unlock tests compare ordering, ITL gating, missing progress, empty/duplicate
  folder groups, Unicode trimming, blank names and auto-download/player separation.
- A bounded localhost TCP server checks the actual HTTP request and JSON response.
- Allocation assertions require zero churn for empty outputs and exact final
  allocation budgets with no growth for populated download outputs.

```powershell
cargo test -p deadsync-online -p deadsync-net -p deadsync-score --lib --locked -- --test-threads=1
cargo test -p deadsync-online --release --lib --locked -- --test-threads=1
cargo test -p deadsync-online --release --lib --locked groovestats_preparation_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-online --release --lib --locked groovestats_preparation_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-online --release --lib --locked groovestats_preparation_bench -- --ignored --test-threads=1 --nocapture
cargo clippy -p deadsync-online --all-targets --locked -- -D clippy::perf
cargo check --all-targets --locked
```

Debug: 508 tests passed (264 online, 233 score, 11 net). Release: 264 online
tests passed. The performance lint check and root all-targets check passed;
Clippy reports existing non-performance warnings. Targeted rustfmt and Git
whitespace checks passed. Live network tests remain ignored. Benchmark logs,
per-round measurements and source hashes are local ignored artifacts under
`target/groovestats-preparation-perf/`; the frozen baseline and harness are
committed so comparisons can be reproduced.
