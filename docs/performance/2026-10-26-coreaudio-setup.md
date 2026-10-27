# CoreAudio setup and notification allocations

Baseline: `50524a532`. This batch retains three independently measured changes in
`deadlib-audio-backend-coreaudio`: borrow the startup device name, format latency
notifications directly, and reserve sample-rate output from the returned range
count.

The consecutive-stage comparisons use eight alternating before/after process
pairs. CPU cycles below measure one fixture call, including formatting when the
log is enabled.

| Change and workload | Before cycles | After cycles | CPU throughput | Allocation change |
| --- | ---: | ---: | ---: | --- |
| Borrow startup name, 15-byte name | 766.90 | 596.06 | 1.287x | 1 allocation -> 0 |
| Borrow startup name, 1,408-byte name | 922.79 | 688.39 | 1.341x | 1 allocation -> 0 |
| Direct warning, same device | 1,845.96 | 1,464.10 | 1.261x | 1 allocation -> 0 |
| Direct warning, changed device | 2,069.29 | 1,745.01 | 1.186x | 2 allocations -> 1 |
| Direct warning, name query fails | 1,824.42 | 1,467.52 | 1.243x | 1 allocation -> 0 |
| Direct warning, logging disabled | 188.27 | 14.90 | 12.636x | 1 allocation -> 0 |
| Reserve rates, 11 discrete ranges | 1,842.29 | 1,350.00 | 1.365x | 2 reallocations -> 0 |
| Reserve rates, 32 continuous ranges | 3,592.07 | 2,882.52 | 1.246x | 3 reallocations -> 0 |
| Rate-query error, default available | 174.51 | 158.50 | 1.101x | 56 allocated bytes -> 4 |
| Rate-query error, no default | 173.96 | 21.46 | 8.106x | 1 allocation -> 0 |

A separate sixteen-pair baseline/final confirmation gives:

| Workload | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Startup, short name | 736.91 | 571.30 | 1.290x |
| Startup, long name | 885.75 | 614.82 | 1.441x |
| Warning, same device | 1,668.32 | 1,487.09 | 1.122x |
| Warning, changed device | 1,953.02 | 1,776.76 | 1.099x |
| Warning, name query fails | 1,694.09 | 1,477.78 | 1.146x |
| Warning, disabled | 158.22 | 15.86 | 9.976x |
| Rates, one continuous range | 528.28 | 538.79 | 0.981x |
| Rates, two discrete ranges | 496.40 | 513.27 | 0.967x |
| Rates, 11 discrete ranges | 1,891.98 | 1,377.95 | 1.373x |
| Rates, 32 continuous ranges | 3,648.65 | 2,929.90 | 1.245x |
| Rate-query error, default available | 175.25 | 160.37 | 1.093x |
| Rate-query error, no default | 175.14 | 21.46 | 8.161x |

These are Windows x86_64 measurements of the actual Rust setup/notification code
with deterministic property-query and log-sink replacements. They exclude native
CoreAudio calls, startup rendering, listener atomics and the real logging backend.
The disabled-warning result describes only the extracted notification work.
Native Mac performance and playback remain unverified.

## Removed work and preservation of behavior

1. `start` uses `prep.device_name` directly in its existing log call. The owned
   name remains available until the preparation object is consumed, so the
   temporary clone and its allocation/copy/free are unnecessary. The formatted
   name and all numeric fields are unchanged.
2. `OutputChangeListener::refresh` retains the optional queried name and uses
   `std::fmt::from_fn` to write the same prefix into the logger's formatter. It
   removes the temporary event `String`, including the unconditional 30-byte
   allocation when the device is unchanged or the name query fails. A successful
   device-name query still allocates its returned 14-byte name in this fixture;
   the separate 104-byte event allocation disappears. The device-name query
   still occurs under exactly the same condition, even when logging is disabled.
   Message text, rounding, ordering and fallback wording are preserved.
   `from_fn` is stable since Rust 1.93, below the repository's Rust 1.97 minimum.
3. `supported_sample_rates_hz` obtains the ranges before allocating its output.
   At most two endpoints per range, eleven common rates and one default can be
   pushed, so reserving that bound prevents every output reallocation. The
   existing sort, deduplication, float validation, rounding and common-rate
   tolerance checks remain intact. A query error returns its zero-or-one-element
   fallback directly, removing a redundant sort/dedup and the oversized buffer.
   The capacity bound's arithmetic fits: an owned vector of 16-byte ranges
   cannot have a length exceeding `isize::MAX / 16`.

The allocation tradeoff is explicit. The two-discrete-range fixture reserves 96
bytes including the query vector, versus 88 before; it is about 3% slower in both
the staged and final comparisons. One continuous range still allocates 72 bytes,
with a 2% final cycle variation. Eleven discrete ranges reduce total allocated
bytes from 568 to 312 and peak live bytes from 400 to 312. Thirty-two ranges reduce
total allocated bytes from 1,352 to 816 and peak live bytes from 960 to 816.
The bound is retained for the larger allocation and throughput improvements;
small-list gains are not claimed. Returned rate contents and ordering are exact;
vector capacity intentionally changes.

Production changes are 19 added and 14 removed lines. No public interfaces,
dependencies or unsafe code are added. The borrowed formatter replaces owned
temporary storage without introducing a new application type. The callback's
timing calculations are untouched.

## Correctness and measurement

`scripts/benchmark_coreaudio_setup.py` extracts the complete production rate
function and its endpoint helper, plus the actual name preparation/startup log
and latency-notification block. The fixture property call returns the same owned
`Vec<AudioValueRange>` representation as the dependency. The fixed log sink
copies formatted UTF-8 into fixed thread-local storage; a separate check
proves that this sink itself does not allocate.

All four normal checks pass in debug and optimized builds:

- Differential rate results for 1,125 combinations of special float endpoints
  and defaults, empty/error queries, and 2,400 randomized lists of up to 95 ranges.
  Cases cover NaN, infinities, invalid/negative/zero endpoints, reversed ranges,
  duplicates, fractional rounding/tolerance boundaries and saturation.
- Exact startup messages for empty, ordinary, Unicode, quoted and multiline
  names, with the preparation object's original name retained.
- Exact notification messages and query counts for unchanged/changed devices,
  failed name lookup and enabled/disabled logging.
- Allocation-free operation of the fixed log sink.

`cargo check --all-targets --locked --offline` passes for both Apple ARM64 and
Intel. ARM64 Clippy passes with the existing `collapsible_if` warning in untouched
device enumeration. Rust formatting and Python compilation pass.

The host is an Intel Xeon E5-2696 v4 running Windows 11, using rustc 1.99.1 / LLVM
23.1.1, the baseline CPU target, optimization level 3, thin LTO, one codegen unit,
and the static MSVC runtime. The existing helper pins its thread to logical
processor 2, reads `QueryThreadCycleTime`, discards one warmup sample, and takes
the median of eleven samples of 8,192 calls. Process order alternates; the tables
use medians of process medians. All allocations are measured separately from CPU
timing. Compilation started by this task had finished before each comparison;
unrelated shared-machine work is uncontrolled. Unchanged controls vary by several
percent. Only the targeted paths' measured gains are claimed.

The four TSVs retain all 960 process/workload records with wall time, allocation
counts and executable hashes. `2026-10-26-coreaudio-setup.json` retains compiler
commands, hashes, exact stage patches, check output and normalized summaries.
All four stages used identical fixture and benchmark-support files. Raw source
archives, generated harnesses, binaries and logs remain in
`target/coreaudio-setup/`.

## Reproduction

Save the baseline crate source from `50524a532` to a file. Apply the name change
for the second stage, then notification formatting for the third, then capacity
reservation. The JSON contains exact patches from the baseline for every stage.
Keep the fixture and support files identical across all archives.

```powershell
python scripts/benchmark_coreaudio_setup.py --archive target/coreaudio-setup/before --source PATH_TO_BASELINE
python scripts/benchmark_coreaudio_setup.py --archive target/coreaudio-setup/name --source PATH_TO_NAME_STAGE
python scripts/benchmark_coreaudio_setup.py --archive target/coreaudio-setup/event --source PATH_TO_EVENT_STAGE
python scripts/benchmark_coreaudio_setup.py --archive target/coreaudio-setup/capacity
python scripts/benchmark_coreaudio_setup.py --debug --archive target/coreaudio-setup/debug
python scripts/benchmark_sampling.py --before target/coreaudio-setup/before/coreaudio.exe --after target/coreaudio-setup/name/coreaudio.exe --test-filter benchmark_coreaudio_setup --runs 8 --log-dir target/coreaudio-setup/runs --output docs/performance/2026-10-26-coreaudio-startup-name.tsv
python scripts/benchmark_sampling.py --before target/coreaudio-setup/name/coreaudio.exe --after target/coreaudio-setup/event/coreaudio.exe --test-filter benchmark_coreaudio_setup --runs 8 --log-dir target/coreaudio-setup/runs --output docs/performance/2026-10-26-coreaudio-latency-log.tsv
python scripts/benchmark_sampling.py --before target/coreaudio-setup/event/coreaudio.exe --after target/coreaudio-setup/capacity/coreaudio.exe --test-filter benchmark_coreaudio_setup --runs 8 --log-dir target/coreaudio-setup/runs --output docs/performance/2026-10-26-coreaudio-rate-capacity.tsv
python scripts/benchmark_sampling.py --before target/coreaudio-setup/before/coreaudio.exe --after target/coreaudio-setup/capacity/coreaudio.exe --test-filter benchmark_coreaudio_setup --runs 16 --log-dir target/coreaudio-setup/runs --output docs/performance/2026-10-26-coreaudio-setup-final.tsv
cargo check -p deadlib-audio-backend-coreaudio --all-targets --locked --offline --target aarch64-apple-darwin --target-dir target/coreaudio-check
cargo check -p deadlib-audio-backend-coreaudio --all-targets --locked --offline --target x86_64-apple-darwin --target-dir target/coreaudio-intel-check
cargo clippy -p deadlib-audio-backend-coreaudio --all-targets --no-deps --locked --offline --target aarch64-apple-darwin --target-dir target/coreaudio-check
```
