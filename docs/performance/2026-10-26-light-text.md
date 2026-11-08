# Light preference and serial name parsing

Baseline runtime source: `c528f2695`; measured on 2026-10-26. Three changes affect only parsing in `crates/deadsync-lights/src/lib.rs`. The combined runtime I/O loader fixture improves **1.492x**, removing all **55 temporary allocations and 452 requested bytes per 32 loads**. These are configuration parsing measurements; they do not establish gameplay FPS, hardware output throughput, or whole-application startup gains.

## Retained changes

1. `DriverKind::from_str` normalizes ASCII alphanumeric bytes into a 23-byte stack buffer, sized to the longest accepted alias. It removes the temporary `String`, Unicode scalar decoding, and redundant trim. A 24th normalized byte rejects the name immediately; arbitrarily many ignored bytes remain accepted around valid aliases.
2. `GameplayPadLightMode::from_str` uses the same direct scan with an eight-byte buffer, sized to `notedata`. It removes its temporary allocation and stops after a ninth normalized byte.
3. `SerialPortName::parse` validates printable ASCII and copies the full slice for trimmed names of 16-64 bytes. Shorter names retain the original scalar loop. An initial bulk-only screening slowed short COM names by about 13%, so that variant was discarded. The retained long-name path avoids per-byte destination writes and length updates.

UTF-8 non-ASCII bytes never satisfy ASCII alphanumeric classification, so the byte scans preserve the original character filtering. Empty or fully ignored driver/mode values still select Off/Input. Only normalized length is bounded: raw input length is unrestricted. The port range `0x20..=0x7e` is exactly the old ASCII/non-control predicate. Trimming, the 64-byte capacity, zero-filled tails, and the complete supplied fallback record remain identical. Warning wrappers and aliases are unchanged.

The production diff changes only these three method bodies: **30 inserted / 10 removed lines**. It adds no abstraction, runtime dependency, or unsafe Rust. The two fixed normalization buffers and the longer-port branch are the additional execution machinery.

## Consecutive before/after measurements

Each step uses eight fresh processes per version, alternating execution order. Columns report cycles per complete fixture and median-before / median-after throughput; ranges contain all paired process ratios. Most fixtures contain 512 values; overlong rejection contains 64 and runtime loading contains 32.

### Driver step

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `driver/plain` | 93,128.88 | 31,393.65 | 2.966x | 2.607-3.797x |
| `driver/decorated` | 110,104.12 | 41,448.64 | 2.656x | 2.431-3.121x |
| `driver/long_rejected` | 72,252.37 | 5,616.11 | 12.865x | 12.387-14.178x |

### Mode step

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `mode/plain` | 85,958.26 | 25,639.49 | 3.353x | 3.141-3.742x |
| `mode/decorated` | 137,005.73 | 31,292.47 | 4.378x | 4.154-5.621x |
| `mode/long_rejected` | 71,337.50 | 2,046.66 | 34.855x | 32.617-37.780x |

### Port step

| Fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `port/short` | 14,877.98 | 14,720.22 | 1.011x | 1.000-1.330x |
| `port/long` | 110,525.10 | 54,375.98 | 2.033x | 1.974-2.445x |
| `port/invalid` | 74,649.72 | 37,215.54 | 2.006x | 1.986-2.414x |
| `port/mixed` | 61,442.86 | 42,850.51 | 1.434x | 1.402-1.883x |
| `load/runtime_io` | 13,530.12 | 14,055.71 | 0.963x | 0.949-1.247x |

Raw measurements include every workload, including unchanged parsers and the PacDrive ordering control: [driver](2026-10-26-light-driver.tsv), [mode](2026-10-26-light-mode.tsv), and [serial names](2026-10-26-light-port.tsv).

The port step also has measured regressions outside its long-name targets: runtime loading is 0.963x, the unchanged driver/plain path 0.909x, and driver/long_rejected 0.855x. These methods' source bodies do not change at that step; the measurements do not establish the cause. The combined comparison below still improves every changed parser target and runtime loading. Short-name and ordering controls remain approximately flat by their combined medians.

## Combined original/final measurements

Twelve fresh processes per version compare all three changes against the original source. [Complete raw data](2026-10-26-light-final.tsv).

| Fixture | Before cycles | After cycles | CPU throughput | Before -> after ns | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: |
| `driver/plain` | 95,455.05 | 32,254.49 | 2.959x | 43,525.00 -> 14,712.11 | 2.333-3.691x |
| `driver/decorated` | 109,028.05 | 43,702.79 | 2.495x | 49,946.88 -> 19,922.26 | 1.978-2.739x |
| `driver/long_rejected` | 71,332.36 | 6,544.69 | 10.899x | 32,804.69 -> 2,976.95 | 8.129-12.195x |
| `mode/plain` | 81,852.92 | 24,393.65 | 3.356x | 37,370.71 -> 11,122.66 | 2.669-3.614x |
| `mode/decorated` | 130,818.58 | 29,518.46 | 4.432x | 59,785.93 -> 13,449.22 | 3.606-4.743x |
| `mode/long_rejected` | 70,474.08 | 2,050.09 | 34.376x | 32,170.32 -> 928.91 | 28.818-34.694x |
| `port/short` | 14,727.08 | 14,775.95 | 0.997x | 6,715.62 -> 6,729.30 | 0.760-1.058x |
| `port/long` | 111,162.18 | 54,936.74 | 2.023x | 50,795.32 -> 25,047.65 | 1.751-2.125x |
| `port/invalid` | 74,157.56 | 37,330.43 | 1.987x | 33,879.68 -> 17,118.75 | 1.660-2.021x |
| `port/mixed` | 61,469.43 | 43,261.22 | 1.421x | 28,121.88 -> 19,730.86 | 1.145-1.555x |
| `load/runtime_io` | 21,350.66 | 14,310.37 | 1.492x | 9,837.11 -> 6,526.95 | 1.224-1.838x |
| `control/pac_order` | 3,716.92 | 3,730.64 | 0.996x | 1,688.67 -> 1,695.31 | 0.809-1.113x |

The unusually large rejection gains apply to stress inputs containing a valid alias plus 256 additional ASCII letters. Normal mixed driver/mode fixtures improve 2.959x/3.356x. Long serial fixtures are 60-byte paths; their 2.023x gain does not apply to the short COM-name control (0.997x). Paired ranges show machine/process variability; every changed target's combined range remains above 1x. The untouched ordering control is 0.996x, with a wider paired range.

## Allocations

The existing scoped System-allocator wrapper measures one complete fixture independently of timing. Counts are identical across all process runs for each version/workload. Every after fixture records zero allocations, reallocations, frees, and requested/freed/peak bytes.

| Fixture | Allocations/frees before -> after | Requested/freed bytes before -> after | Scoped peak bytes before -> after |
| --- | ---: | ---: | ---: |
| `driver/plain` | 472 / 472 -> 0 / 0 | 4,789 -> 0 | 27 -> 0 |
| `driver/decorated` | 468 / 468 -> 0 / 0 | 10,072 -> 0 | 138 -> 0 |
| `driver/long_rejected` | 64 / 64 -> 0 / 0 | 16,963 -> 0 | 279 -> 0 |
| `mode/plain` | 450 / 450 -> 0 / 0 | 3,343 -> 0 | 13 -> 0 |
| `mode/decorated` | 435 / 435 -> 0 / 0 | 6,216 -> 0 | 48 -> 0 |
| `mode/long_rejected` | 64 / 64 -> 0 / 0 | 16,720 -> 0 | 264 -> 0 |
| `load/runtime_io` | 55 / 55 -> 0 / 0 | 452 -> 0 | 23 -> 0 |

All before fixtures have zero reallocations. Port parsing and the ordering control already allocate nothing; they retain zero counts. The counts describe requested heap bytes, including the largest scoped live request, rather than process RSS. The alias fixtures include empty values, which explain fewer allocations than values before the changes.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Archived binaries use `--release --locked`, the repository's default LTO, the baseline CPU target, and its static MSVC runtime settings. The existing helper pins the test thread to logical processor 2 and reads `QueryThreadCycleTime`. Each sample repeats the complete fixture **128 times**, discards one warmup sample, and reports the median of eleven measured samples. Tables report medians of process medians. Four TSVs retain all **864 process/workload records**, elapsed time, allocation counters, and executable SHA-256 values. No compilation started by this task overlaps retained comparisons; other shared-machine activity is uncontrolled.

Fixtures are prepared outside measurement and passed through `black_box`, as are parser inputs and results. Plain aliases cycle original/uppercase/Unicode-edge-whitespace/rejected-or-empty inputs. Decorated variants add punctuation and an ignored Unicode character after every letter. Long port names have the form `/dev/` plus 55 decimal digits. Invalid ports place ESC after 16-63 printable bytes. Mixed ports cycle short, long, invalid, non-ASCII, and all-whitespace input. Runtime loading parses four options from 32 prebuilt INI maps through the actual generic loader, warning/default wrappers, debounce parser, and serial parser. Logger output is disabled, and no driver or hardware is opened by the new harness.

Three correctness tests compare production with frozen pre-change implementations. Alias checks cover all 40 driver names and nine mode names, case variations, insertion of every ASCII byte at every boundary, Unicode and punctuation, long valid input containing thousands of ignored characters, and overlong normalized rejections. Port checks replace every position with every ASCII byte at lengths around the short/long branch and capacity boundaries, and cover Unicode byte widths, trimming, controls, and three distinct fallback records. Both visible strings and complete 65-byte records are compared via derived Debug output. Runtime loader comparisons cover complete option records, missing keys, present empty values, ignored-only values, invalid values, and custom defaults. Parser Result equality plus unchanged warning wrappers preserves warning decisions.

Validation passes: **534 release tests** across lights/config/profile (12 ignored), **295 debug tests** across lights/config (eight ignored), and Clippy for all lights/config targets with existing warnings and no diagnostics in changed runtime bodies or new test/reference files. All four default-LTO archives pass the three new compatibility tests. The lights source and both new Rust files pass scoped rustfmt checks.

Source audits verify the frozen implementations against `c528f2695`, allowing only type qualification and rustfmt's equivalent single-expression match blocks. Restoring the three old bodies reconstructs the entire starting lights source exactly. Final production bytes match the final measured snapshot, and every raw record's executable digest matches its archived binary. Logs, source snapshots, screening trials, and executable archives remain under `target/light-text/`.

```powershell
cargo test --release --locked -p deadsync-config --test light_text --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_light_text --log-dir target/light-text/runs --before target/light-text/before/config.exe --after target/light-text/driver/config.exe --output docs/performance/2026-10-26-light-driver.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_light_text --log-dir target/light-text/runs --before target/light-text/driver/config.exe --after target/light-text/mode/config.exe --output docs/performance/2026-10-26-light-mode.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_light_text --log-dir target/light-text/runs --before target/light-text/mode/config.exe --after target/light-text/after/config.exe --output docs/performance/2026-10-26-light-port.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_light_text --log-dir target/light-text/runs --before target/light-text/before/config.exe --after target/light-text/after/config.exe --output docs/performance/2026-10-26-light-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-lights -p deadsync-config -p deadsync-profile -- --test-threads=1
cargo test --locked -p deadsync-lights -p deadsync-config -- --test-threads=1
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-lights -p deadsync-config --all-targets
```

Keep the committed integration harness fixed. Restore only the three production method bodies from `c528f2695` for `before`; apply driver parsing for `driver`, gameplay mode parsing for `mode`, then the longer-port branch for `after`. Build and archive the single reported `light_text` executable after each stage and run its three correctness checks. Consecutive comparisons use eight runs each; original/final uses twelve. Broader verification disables release LTO to reduce linking time; performance archives retain the default LTO.
