# Native input: direct formatting, inline identities, and shorter comparison blocks

This pass retains three safe Rust optimizations in `deadlib-input-native/src/backend/w32_raw_input.rs`:

1. UUID diagnostic hex uses nibble lookups and direct ASCII pushes, removing sixteen generic formatting calls and the `fmt::Write` import. The returned 32-byte lowercase `String` is unchanged.
2. Serial-derived device identities build the exact `rawinput:serial:vvvv:pppp:serial` bytes in an existing `SmallVec` type. The 26-byte prefix leaves 38 inline UTF-8 serial bytes. Longer inputs reserve their complete capacity once. This removes the temporary formatted `String`, its formatting dispatch and common heap churn. Forced inlining keeps the serial-less fallback cheap.
3. HID button diffs compare equal prefixes in four-usage/eight-byte blocks instead of eight-usage/sixteen-byte blocks. Four held buttons now fit a complete block. The original ordered merge, duplicate handling, state updates and emitted fields are unchanged.

No dependency or unsafe code was added. Production source adds 26 lines and removes 14 (net +12); the removed runtime work is more significant than the source-line reduction. These are helper and callback measurements. Hex formatting runs only when its diagnostic log is evaluated; serial construction is device discovery work. None of these results claims an application-wide frame-rate or input-latency improvement.

## Measurements of the final source

The final archive is `direct-final`, including the committed test harness. CPU values below are median thread cycles per individual call. Raw TSV values time a complete fixture: divide hex/serial values by 64 and HID/dispatch values by 256. Throughput is before cycles / after cycles. The paired range spans corresponding process ratios and is not a confidence interval. Secondary ns values are elapsed time.

Earlier isolated eight-pair comparisons measured hex at **4.760x**, short serial identities at **2.813x**, and direct-closure four-button equality at **1.691x**. The first four-pair boxed-callback comparison measured four-button equality at **1.769x**. Final confirmation after the harness lint cleanup follows.

### Hex

[Raw records](2026-10-26-native-input-direct-final-hex.tsv), [source/executable identities](2026-10-26-native-input-direct-final-hex.manifest.json).

| Fixture | Before cycles/call | After cycles/call | CPU throughput | Paired range | Before -> after ns/call |
| --- | ---: | ---: | ---: | --- | ---: |
| hex/uuid | 1115.84 | 222.98 | 5.004x | 4.696-5.166 | 510.14 -> 101.98 |

### Serial identity

[Raw records](2026-10-26-native-input-direct-final-serial.tsv), [source/executable identities](2026-10-26-native-input-direct-final-serial.manifest.json).

| Fixture | Before cycles/call | After cycles/call | CPU throughput | Paired range | Before -> after ns/call |
| --- | ---: | ---: | ---: | --- | ---: |
| serial/empty | 508.82 | 164.38 | 3.095x | 2.955-3.502 | 232.55 -> 75.27 |
| serial/long | 1667.47 | 1236.98 | 1.348x | 1.321-1.403 | 762.43 -> 565.55 |
| serial/medium | 874.77 | 275.54 | 3.175x | 3.112-3.466 | 399.96 -> 125.99 |
| serial/none | 29.20 | 30.61 | 0.954x | 0.529-0.997 | 13.41 -> 14.09 |
| serial/short | 542.74 | 189.12 | 2.870x | 2.696-3.120 | 247.80 -> 86.47 |
| serial/unicode | 841.22 | 215.49 | 3.904x | 3.549-3.964 | 384.55 -> 98.44 |

### Boxed callback HID diff

[Raw records](2026-10-26-native-input-direct-final-dispatch.tsv), [source/executable identities](2026-10-26-native-input-direct-final-dispatch.manifest.json).

| Fixture | Before cycles/call | After cycles/call | CPU throughput | Paired range | Before -> after ns/call |
| --- | ---: | ---: | ---: | --- | ---: |
| dispatch/all/len=0 | 8.27 | 8.23 | 1.006x | 0.940-1.194 | 3.82 -> 3.78 |
| dispatch/all/len=1 | 54.55 | 53.05 | 1.028x | 1.002-1.053 | 24.91 -> 24.24 |
| dispatch/all/len=16 | 774.55 | 768.32 | 1.008x | 0.988-1.020 | 353.96 -> 351.32 |
| dispatch/all/len=4 | 197.23 | 195.59 | 1.008x | 0.989-1.053 | 91.39 -> 89.33 |
| dispatch/all/len=64 | 3045.78 | 3008.08 | 1.013x | 0.995-1.039 | 1391.83 -> 1375.05 |
| dispatch/last/len=0 | 8.32 | 8.02 | 1.037x | 0.988-1.112 | 3.79 -> 3.66 |
| dispatch/last/len=1 | 53.89 | 52.37 | 1.029x | 1.008-1.061 | 25.00 -> 23.96 |
| dispatch/last/len=16 | 71.71 | 62.94 | 1.139x | 1.062-1.173 | 32.78 -> 28.76 |
| dispatch/last/len=4 | 59.97 | 60.22 | 0.996x | 0.965-1.032 | 27.42 -> 27.55 |
| dispatch/last/len=64 | 84.18 | 75.85 | 1.110x | 1.091-1.281 | 38.47 -> 34.66 |
| dispatch/release/len=0 | 8.57 | 8.14 | 1.052x | 0.939-1.086 | 3.91 -> 3.72 |
| dispatch/release/len=1 | 30.18 | 30.65 | 0.985x | 0.962-1.091 | 13.81 -> 14.03 |
| dispatch/release/len=16 | 354.68 | 350.52 | 1.012x | 0.989-1.027 | 162.13 -> 160.19 |
| dispatch/release/len=4 | 95.54 | 93.68 | 1.020x | 0.969-1.049 | 43.63 -> 42.79 |
| dispatch/release/len=64 | 1386.99 | 1375.59 | 1.008x | 0.926-1.075 | 633.92 -> 628.84 |
| dispatch/same/len=0 | 8.43 | 8.10 | 1.040x | 0.935-1.152 | 3.86 -> 3.69 |
| dispatch/same/len=1 | 11.12 | 10.17 | 1.093x | 0.953-1.197 | 5.07 -> 4.63 |
| dispatch/same/len=16 | 17.04 | 14.13 | 1.206x | 1.047-1.362 | 7.77 -> 6.44 |
| dispatch/same/len=4 | 20.21 | 11.53 | 1.753x | 1.690-1.813 | 9.29 -> 5.27 |
| dispatch/same/len=64 | 30.81 | 28.38 | 1.086x | 1.042-1.125 | 14.08 -> 12.98 |

### Allocation changes

Counts below are per identity; requested bytes include reallocations. Peak bytes cover the temporary allocation only. Fixture strings are prepared outside the probe. The new 64-byte inline buffer occupies stack storage, which these allocator counters do not measure.

| Serial fixture | Before allocs/reallocs | After allocs/reallocs | Requested bytes before -> after | Peak temporary bytes before -> after |
| --- | ---: | ---: | ---: | ---: |
| empty | 1/0 | 0/0 | 36 -> 0 | 36 -> 0 |
| long | 1/1 | 1/0 | 318 -> 282 | 282 -> 282 |
| medium | 1/1 | 0/0 | 108 -> 0 | 72 -> 0 |
| none | 0/0 | 0/0 | 0 -> 0 | 0 -> 0 |
| short | 1/0 | 0/0 | 36 -> 0 | 36 -> 0 |
| unicode | 1/1 | 0/0 | 108 -> 0 | 72 -> 0 |

Empty, 8-byte, 32-byte and 18-byte Unicode/NUL serial fixtures eliminate heap allocation. The 256-byte serial drops from one allocation plus one reallocation to one allocation, requesting 282 rather than 318 bytes. Serial-less identity construction and HID diffing remain allocation-free. Hex strings still allocate exactly once for 32 bytes.

### Tradeoffs and calling context

The direct-closure comparison reports several changed-list regressions, including about 15.6% more cycles for four-usage dense replacement. Production uses `Box<dyn FnMut(PadEvent) + Send>`, so a separate benchmark passes an opaque mutable boxed callback with exactly that type. The sink consumes the complete event with `black_box`; downstream application dispatch and hardware I/O are not included.

Four-button equality consistently improves across the direct, boxed and final comparisons. Dense boxed event cases are effectively flat. Large-list equality is context-sensitive: the first boxed rebuild measured **0.894x** for 64 equal usages, while final confirmation measured **1.086x**. One-equal-button results also varied across rebuilds. No general large-list improvement is claimed. The [earlier boxed records](2026-10-26-native-input-direct-dispatch.tsv) and [direct-closure records](2026-10-26-native-input-direct-prefix4.tsv) retain those costs; they are not discarded as noise. The final serial-less control costs about 4.8% more cycles, despite being flat in earlier eight-pair confirmations.

## Method and checks

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Archives use `--release --locked`, repository-default LTO, the baseline CPU target, and static MSVC CRT settings. Sources are snapshotted and checked for modification during compilation; each executable is hashed and runs its normal correctness tests before benchmarking. Local snapshots and logs remain under `target/native-input-kernels/`; committed manifests identify their exact inputs and executables.

The shared helper pins the benchmark thread to logical processor 2 and uses `QueryThreadCycleTime`. Each sample repeats a fixture 1,000 times, discards one warm sample and takes the median of eleven measured samples. Axis screening uses 2,000 repetitions. Final tables use four fresh processes per version, alternating order; earlier isolated confirmations use eight. Fixture setup and allocation probes are outside CPU timing. No compilation launched by this pass overlaps retained timing. Other shared-machine jobs are uncontrolled and sometimes stretch elapsed time substantially.

Hex and serial fixtures contain 64 inputs. Serial fixtures cover absent, empty, 8-byte, 32-byte, 256-byte and Unicode/embedded-NUL strings, with changing vendor IDs. HID fixtures make 256 diffs with 0/1/4/16/64 prior usages and unchanged, last-replaced, all-replaced or all-released lists. Inputs, result identities and emitted events pass through `black_box`. The boxed callback remains opaque to prevent devirtualization of the dispatch under test.

[Final checks](../testing/2026-10-26-native-input-direct-verified.json) record 42 unit plus 24 integration tests passing in both debug and release, formatting, and Clippy. The seven ignored performance entry points are invoked explicitly by the driver. The existing `field_reassign_with_default` lint in `get_device_info` is the sole Clippy exemption; other warnings are denied. Two new unnecessary probe borrows caught on the initial Clippy run were removed, with the initial records preserved separately.

Identity equivalence checks cover all 65,536 vendor values with rotated product IDs and empty, ordinary, Unicode/NUL and long serials. Hex checks cover all repeated byte values and mixed identifiers, including leading zeroes. The existing 10,000 ordered multiset event comparisons verify every event field and duplicate order, including long common prefixes and truncated lists. Additional axis checks exercise every quantization boundary and neighbors, one million float bit patterns, infinities, NaNs and signed zero; WGI production code remains unchanged after the rejected experiments.

## Rejected work

- [Integer post-cast clamps](2026-10-26-native-input-direct-integer-clamp.tsv) slowed ordinary inputs and clipped sticks; clipped stick throughput was 0.290x.
- [Early zero/nonfinite range filtering](2026-10-26-native-input-direct-zero-range.tsv) accelerated subnormal sticks but slowed ordinary sticks and subnormal triggers.
- [Input bit masks](2026-10-26-native-input-direct-zero-mask.tsv) still ran expensive subnormal multiplication and slowed normal sticks. A [standalone extraction](2026-10-26-native-input-direct-mask-probe.rs) and its [generated assembly](2026-10-26-native-input-direct-mask-probe.s) show LLVM placing filtering after multiplication. These are an isolated compiler probe, not disassembly of the complete application.
- [One sentinel-based merge loop](2026-10-26-native-input-direct-one-merge.tsv) removed repeated source/event construction but made most changed-button cases substantially slower. The original merge is retained.
- [Non-inlined serial buffering](2026-10-26-native-input-direct-serial.tsv) improved serial IDs but slowed the serial-less fallback to 0.377x. The explicit inline hint recovered that control before retention.

## Reproduce

Use fresh archive and output names. `direct-host-before` contains the original formatting and eight-usage blocks; `direct-hex` adds nibble hex output; `direct-serial-inline` adds inline serial identity construction; `direct-dispatch-before` has those same production changes plus the boxed benchmark; `direct-final` has all three retained changes and the final harness. In a disposable checkout, undo the corresponding production hunks while retaining the final harness to rebuild earlier stages. The initial axis-clamp screening used ten fixtures before zero fixtures were added; later axis groups contain twelve.

```powershell
python scripts/benchmark_native_input.py --build new-stage
python scripts/benchmark_native_input.py --compare before-stage new-stage --group hex --runs 8 --output docs/performance/new-hex.tsv
python scripts/benchmark_native_input.py --compare before-stage new-stage --group serial --runs 8 --output docs/performance/new-serial.tsv
python scripts/benchmark_native_input.py --compare before-stage new-stage --group dispatch --runs 4 --output docs/performance/new-dispatch.tsv
cargo test -p deadlib-input-native --locked
cargo test -p deadlib-input-native --locked --release
cargo clippy -p deadlib-input-native --locked --all-targets --no-deps -- -D warnings -A clippy::field_reassign_with_default
```
