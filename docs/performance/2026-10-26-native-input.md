# Native input kernels, 2026-10-26

Three safe Rust changes in `deadlib-input-native` preserve persisted device identities and ordered input events:

- `uuid_from_bytes` updates both independent FNV-1a accumulators in one traversal. This removes the nested hash helper, the second traversal and its repeated loads while exposing independent multiplication chains.
- `gamepad_button_edges` tests only the mapped button mask before scanning `BTN_MAP`. D-pad and unknown-bit changes cannot emit raw button events, so they skip ten mask checks. The map and event order remain unchanged.
- `emit_button_diff` skips matching prefixes in blocks of eight `u16` usages using safe fixed-array equality, then resumes the original ordered merge. Reports with fewer than eight previous usages bypass the block scan. A skipped block contains no edges, including when usages are duplicated.

No public signatures, clocks, report parsing, device I/O, allocation policy or unsafe code changed. These are kernel measurements, not end-to-end latency or game throughput measurements. Long held-button lists are a supported stress workload; their gains should not be interpreted as a common four-button dance-pad speedup.

## Retained measurements

Values below are median thread CPU cycles per individual hash, WGI snapshot, or HID diff. The TSV `cycles_per_op` field measures a complete fixture: divide UUID values by 64 and WGI/HID values by 256 to obtain these tables. CPU throughput is before/after median cycles. Paired ranges contain all matching-process ratios, not confidence intervals. Elapsed ns are secondary.

The final source and harness are archived as `verified`. UUID compares `baseline` against it; WGI compares `uuid-fused` against it; HID compares `wgi-test` against it. These tables show four alternating process pairs using that exact harness. Earlier retained comparisons used eight pairs for UUID/HID and twelve for WGI: [UUID](2026-10-26-native-input-retained-uuid.tsv), [WGI](2026-10-26-native-input-retained-wgi.tsv), [HID](2026-10-26-native-input-hid-guarded.tsv). Those showed representative gains of 1.711x, 2.690x and 5.261x respectively. Isolated consecutive stages and rejected candidates also remain in the accompanying TSVs.

Representative gains are **1.780x** for 96-byte identities, **2.697x** for d-pad-only WGI changes, and **5.606x** for unchanged 64-usage HID reports. All allocation probes report zero allocations, reallocations, frees, requested bytes and peak bytes before and after. No allocation reduction is claimed.

### UUID

[Raw measurements](2026-10-26-native-input-audited-uuid.tsv), [source and binary identities](2026-10-26-native-input-audited-uuid.manifest.json).

| Fixture | Before cycles/call | After cycles/call | CPU throughput | Paired range | Before -> after ns/call |
| --- | ---: | ---: | ---: | --- | ---: |
| uuid/len=0 | 2.32 | 2.34 | 0.988x | 0.828-1.143 | 1.04 -> 1.06 |
| uuid/len=1024 | 5507.51 | 2612.51 | 2.108x | 1.814-2.314 | 2512.30 -> 1192.84 |
| uuid/len=256 | 1401.95 | 671.95 | 2.086x | 1.544-2.172 | 639.52 -> 306.68 |
| uuid/len=32 | 104.12 | 71.56 | 1.455x | 1.061-1.734 | 47.47 -> 32.61 |
| uuid/len=8 | 19.82 | 18.03 | 1.099x | 1.065-1.234 | 9.02 -> 8.22 |
| uuid/len=96 | 435.89 | 244.84 | 1.780x | 1.418-1.976 | 198.86 -> 111.68 |

### WGI

[Raw measurements](2026-10-26-native-input-audited-wgi.tsv), [source and binary identities](2026-10-26-native-input-audited-wgi.manifest.json).

| Fixture | Before cycles/call | After cycles/call | CPU throughput | Paired range | Before -> after ns/call |
| --- | ---: | ---: | ---: | --- | ---: |
| wgi/dense | 17.91 | 18.04 | 0.993x | 0.898-1.022 | 8.17 -> 8.22 |
| wgi/dpad | 6.53 | 2.42 | 2.697x | 2.409-2.922 | 3.00 -> 1.10 |
| wgi/pair | 11.46 | 12.42 | 0.923x | 0.911-1.178 | 5.24 -> 5.69 |
| wgi/single | 8.18 | 8.41 | 0.972x | 0.886-1.009 | 3.73 -> 3.84 |
| wgi/unchanged | 2.47 | 2.45 | 1.011x | 0.951-1.098 | 1.12 -> 1.12 |

### HID

[Raw measurements](2026-10-26-native-input-audited-hid.tsv), [source and binary identities](2026-10-26-native-input-audited-hid.manifest.json).

| Fixture | Before cycles/call | After cycles/call | CPU throughput | Paired range | Before -> after ns/call |
| --- | ---: | ---: | ---: | --- | ---: |
| hid/all/len=0 | 6.60 | 6.43 | 1.026x | 0.957-1.072 | 3.02 -> 2.93 |
| hid/all/len=1 | 17.90 | 17.25 | 1.038x | 0.915-1.165 | 8.16 -> 7.86 |
| hid/all/len=16 | 232.44 | 247.17 | 0.940x | 0.859-1.147 | 106.21 -> 112.92 |
| hid/all/len=4 | 57.88 | 60.43 | 0.958x | 0.834-1.221 | 26.48 -> 27.61 |
| hid/all/len=64 | 848.21 | 961.59 | 0.882x | 0.720-1.241 | 387.56 -> 439.43 |
| hid/last/len=0 | 6.54 | 6.74 | 0.971x | 0.885-1.034 | 2.98 -> 3.07 |
| hid/last/len=1 | 17.34 | 17.20 | 1.008x | 0.929-1.101 | 7.92 -> 7.88 |
| hid/last/len=16 | 48.77 | 34.02 | 1.434x | 1.313-1.450 | 22.27 -> 15.52 |
| hid/last/len=4 | 22.14 | 21.64 | 1.023x | 0.994-1.026 | 10.10 -> 9.88 |
| hid/last/len=64 | 154.26 | 47.92 | 3.219x | 3.077-3.301 | 70.44 -> 21.90 |
| hid/release/len=0 | 6.71 | 6.32 | 1.062x | 0.995-1.123 | 3.08 -> 2.91 |
| hid/release/len=1 | 9.96 | 9.65 | 1.032x | 0.951-1.132 | 4.53 -> 4.40 |
| hid/release/len=16 | 110.21 | 118.65 | 0.929x | 0.798-1.281 | 50.37 -> 54.22 |
| hid/release/len=4 | 30.41 | 31.58 | 0.963x | 0.839-1.204 | 13.87 -> 14.47 |
| hid/release/len=64 | 430.69 | 485.49 | 0.887x | 0.758-1.228 | 196.87 -> 221.92 |
| hid/same/len=0 | 6.33 | 6.55 | 0.967x | 0.930-1.139 | 2.90 -> 2.98 |
| hid/same/len=1 | 8.94 | 8.61 | 1.038x | 0.969-1.165 | 4.07 -> 3.92 |
| hid/same/len=16 | 41.19 | 13.39 | 3.077x | 2.809-3.393 | 18.81 -> 6.10 |
| hid/same/len=4 | 16.33 | 17.63 | 0.927x | 0.894-0.969 | 7.45 -> 8.05 |
| hid/same/len=64 | 155.52 | 27.74 | 5.606x | 5.322-5.837 | 71.13 -> 12.70 |

The optimizations are not universally faster. In the final audit, paired WGI changes cost **8.4% more cycles**, and single edges cost **2.8% more**. Dense 64-usage HID replacement and release cost **13.4% / 12.7% more**; unchanged four-usage reports cost **7.9% more**. The earlier eight-pair HID comparison showed smaller dense/release changes, but both datasets are retained rather than dismissing these regressions as noise. Empty UUIDs are effectively flat in the final audit, with a paired range spanning 1x. These costs are retained for the demonstrated d-pad rejection and long-prefix benefits, and they limit where this pass improves throughput. All controls and raw records are included above.

## Method and fixtures

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Builds use `cargo test --release --locked`, repository-default LTO, the baseline CPU target, and the static MSVC runtime in `.cargo/config.toml`. Each archive snapshots the native crate, root Cargo files/configuration, and shared benchmark/allocation helpers, verifies those sources did not change during compilation, saves executable/source SHA-256 identities, and runs its correctness tests.

The existing `tests/support/kernel_bench.rs` pins the benchmark thread to logical processor 2 and reads `QueryThreadCycleTime`. Each sample repeats the complete fixture 1,000 times; one warmup is discarded and the median of eleven measured samples is selected. Final tables use four fresh processes per version; earlier retained UUID/HID comparisons used eight, WGI used twelve. Process order alternates. Fixture setup, allocation counting and compilation are outside timed samples. No compilation launched by this task overlaps retained sampling; other shared-machine jobs are uncontrolled and sometimes cause substantial elapsed-time variation.

UUID fixtures hash 64 prebuilt byte sequences of each stated length. WGI fixtures compare 256 prebuilt snapshots with varying previous bits and unchanged, d-pad-only, one-, two-, or ten-mapped-button changes. HID fixtures make 256 diffs of sorted usages: unchanged, final usage replaced, every usage replaced, or all usages released. Previous/next lists are independent allocations prepared outside timing. Inputs and callback outputs pass through `black_box`. The event sink does no application dispatch, and no physical input devices are opened.

## Rejected candidates

- [Sparse WGI bit iteration](2026-10-26-native-input-wgi-sparse.tsv) preserved ordering but delivered only 0.480x dense and 0.451x unchanged throughput; even single edges slowed. It was removed.
- [Whole-slice HID equality](2026-10-26-native-input-hid-equal.tsv) accelerated unchanged long lists but added up to roughly 43% cycles on short/changed cases. It was replaced with prefix consumption, so matching usages are never traversed a second time.
- `wgi-mask`, `hid-prefix`, and `final` archives/TSVs retain intermediate mask/layout and short-report measurements. The guarded prefix replaced the unguarded version after the rebuilt harness exposed short-report costs.

## Correctness and checks

`tests/native_kernels.rs` checks UUID output byte-for-byte against the original two-pass definition for every prefix length 0..4095 of a deterministic byte stream and every repeated-byte value. WGI checks all 1,024 changed mapped-button subsets in four previous states, with d-pad/unknown bits mixed in, and compares complete ordered code/pressed sequences. HID checks 10,000 sorted multiset cases through lists of up to 128 usages, including duplicates, equal lists, long common prefixes, final changes and truncations. Every emitted identity, timestamp, host time, code, float value bit pattern and pressed flag is checked. Existing calibration, raw-report, cache, direction and stable-slot tests also pass.

[Final check records](../testing/2026-10-26-native-input-verified.json) record **42 unit + 21 integration tests passing in both debug and release**, formatting, and Clippy. Three ignored benchmark entry points are run explicitly by the benchmark driver. Strict Clippy without an exception found the existing `field_reassign_with_default` diagnostic at `get_device_info` (`RID_DEVICE_INFO::default()` followed by `cbSize` assignment). The final command exempts only that pre-existing style lint; other warnings are denied. The initial failure is preserved in `2026-10-26-native-input-checks.json` and its logs. Cross-platform runtime/device behavior was not exercised; all production changes use safe, portable Rust, with the two Windows helpers remaining Windows-only.

Production changes add 24 lines and remove 16 (net +8); most removal is the UUID helper and second pass. No new production abstraction or dependency is introduced.

## Reproduce

Run from the repository root, using fresh stage/output names. Archives are local under `target/native-input-kernels/`; committed manifests preserve their source/executable identities. To rebuild before stages, use a disposable checkout and undo the corresponding production hunks while retaining the harness: `baseline` has none of the three changes; `uuid-fused` has only UUID fusion; `wgi-test` adds the WGI gate; `verified` has all three.

```powershell
python scripts/benchmark_native_input.py --build new-stage
python scripts/benchmark_native_input.py --compare before-stage new-stage --group uuid --runs 8 --output docs/performance/new-uuid.tsv
python scripts/benchmark_native_input.py --compare before-stage new-stage --group wgi --runs 12 --output docs/performance/new-wgi.tsv
python scripts/benchmark_native_input.py --compare before-stage new-stage --group hid --runs 8 --output docs/performance/new-hid.tsv
cargo test -p deadlib-input-native --locked
cargo test -p deadlib-input-native --locked --release
cargo clippy -p deadlib-input-native --locked --all-targets --no-deps -- -D warnings -A clippy::field_reassign_with_default
```
