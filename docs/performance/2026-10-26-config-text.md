# Configuration aliases and direct writing — 2026-10-26

Three safe Rust optimizations in `deadsync-config`, following the allocation and execution-overhead guidance in `rust-performance.md`. They simplify configuration loading and serialization. These are cold settings paths; the measurements do not establish gameplay FPS or input latency gains. The production diff adds 19 lines and removes 22 across four existing functions. No dependency, public API, production function/type, or unsafe block is added. Lua and unrelated worktree changes are excluded.

## Execution simplified

1. **Recognize filtered kernel aliases directly.** The target/type parsers iterate ASCII alphanumeric bytes, fold case, dispatch on the first byte, and compare the remaining stream with the supported suffix. This removes Unicode character decoding, trimming, owned normalized strings, and their growth/copy/destruction. Every non-ASCII UTF-8 byte fails the ASCII filter, preserving the original character filter's treatment of Unicode. Whitespace is already discarded by that filter. Rejected candidates can stop at their first mismatch.
2. **Write boolean lines directly.** Append the key and the selected `=0\n` or `=1\n` suffix. This removes general `Display`/integer formatting from the shared helper used throughout configuration serialization. Arbitrary key text and existing destination contents retain their exact bytes.
3. **Format fixed-decimal values into the destination.** Pass borrowed `format_args!` to the existing line writer for fingerprint, window, and step milliseconds, replacing three temporary `format!` strings. The same clamping, quantization, and one-decimal formatting still execute.

## Isolated before/after measurements

Consecutive stages are original → direct alias recognition → direct booleans → borrowed numeric formatting. Every archive uses the repository's **default release profile, including LTO**. Each isolated comparison alternates eight fresh processes per version; figures are median thread CPU cycles per fixture.

| Change / workload | Before cycles | After cycles | CPU throughput | Paired ratio range |
| --- | ---: | ---: | ---: | ---: |
| Aliases / 512 ordinary target/type pairs | 303,393.59 | 46,057.28 | 6.587x | 6.037–7.030x |
| Aliases / 512 decorated pairs | 449,981.84 | 195,979.19 | 2.296x | 2.085–2.430x |
| Aliases / 512 rejected pairs | 929,441.86 | 32,355.65 | 28.726x | 26.028–29.596x |
| Aliases / load 32 option sections | 71,412.96 | 57,131.74 | 1.250x | 1.196–1.453x |
| Booleans / write 512 lines | 36,215.79 | 5,796.18 | 6.248x | 5.998–7.796x |
| Booleans / serialize default config | 28,246.90 | 18,650.62 | 1.515x | 1.314–1.614x |
| Numeric formatting / write 32 option sections | 87,964.65 | 65,308.09 | 1.347x | 1.196–1.532x |
| Numeric formatting / serialize default config | 18,829.00 | 18,335.10 | 1.027x | 0.907–1.216x |

Raw results retain all controls: [aliases](2026-10-26-config-aliases.tsv), [booleans](2026-10-26-config-bools.tsv), and [numeric formatting](2026-10-26-config-numbers.tsv). Ranges are minimum/maximum ratios of corresponding process runs, not confidence intervals. Numeric formatting's full-config CPU gain is small and inconsistent across pairs, while its allocation reduction and option-section gain are consistent. Untouched controls vary: for example, boolean writing measures 0.967x in the numeric comparison, and option loading measures 0.980x in the boolean comparison. The measurements do not identify the causes of those small changes.

## Combined results and allocations

The [combined comparison](2026-10-26-config-final.tsv) alternates twelve fresh processes per version, comparing the original production code with all three changes.

| Fixture / workload | Before cycles/fixture | After cycles/fixture | CPU throughput | Before → after ns/fixture |
| --- | ---: | ---: | ---: | ---: |
| 512 ordinary alias pairs | 288,584.18 | 45,004.36 | 6.412x | 131,618.75 → 20,578.12 |
| 512 decorated alias pairs | 434,750.61 | 191,383.43 | 2.272x | 198,373.43 → 87,315.63 |
| 512 rejected alias pairs | 894,654.56 | 32,228.76 | 27.760x | 408,085.94 → 14,748.44 |
| Write 512 boolean lines | 36,092.31 | 6,027.67 | 5.988x | 16,458.60 → 2,767.19 |
| Write 32 option sections | 94,721.12 | 65,362.97 | 1.449x | 43,162.50 → 29,821.88 |
| Load 32 option sections | 68,360.51 | 56,301.74 | 1.214x | 31,200.00 → 25,676.57 |
| Serialize default config | 30,167.53 | 18,588.90 | 1.623x | 13,703.12 → 8,421.88 |

| Workload | Allocations/frees before → after | Reallocations before → after | Requested bytes before → after | Peak bytes before → after |
| --- | ---: | ---: | ---: | ---: |
| Ordinary or decorated aliases | 1,024 / 1,024 → 0 / 0 | 256 → 0 | 12,288 → 0 | 16 → 0 |
| Rejected aliases | 1,024 / 1,024 → 0 / 0 | 2,304 → 0 | 135,168 → 0 | 128 → 0 |
| Write 32 option sections | 128 / 128 → 32 / 32 | 0 → 0 | 1,024 → 256 | 8 → 8 |
| Load 32 option sections | 128 / 128 → 64 / 64 | 0 → 0 | 960 → 448 | 8 → 8 |
| Serialize default config | 15 / 15 → 12 / 12 | 1 → 1 | 12,426 → 12,402 | 8,287 → 8,287 |

Boolean writing remains allocation-free with a warmed destination. Direct numeric formatting removes **96 allocations/frees and 768 requested bytes per 32-section fixture**, or three allocations per section. The remaining section-write allocations come from magic-offset formatting. Full default serialization removes three allocations/frees and 24 requested bytes; output-buffer growth still determines its peak. Allocation counters are identical across all runs of each stage.

All combined paired runs improve every measured workload. Default-config serialization ranges from 1.160–1.710x; ordinary alias parsing ranges from 5.308–6.932x. The very large rejected-alias gain reflects a fixture split between an immediately unsupported prefix with 64 trailing letters and a valid name followed by an extra digit. It does not imply that every malformed preference has the same gain. Decorated aliases contain punctuation and Unicode between letters and still require scanning those bytes. Executable layout and shared-machine activity introduce variation.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Measured archives use `--release --locked`, default LTO, static MSVC runtime settings from `.cargo/config.toml`, and the baseline CPU target. The existing helper pins the current thread to logical processor 2 and reads `QueryThreadCycleTime`. Each process discards one warmup sample and reports the median of eleven samples. Samples repeat each 512-pair alias fixture 32 times, 512 boolean lines 64 times, 32-section writing 16 times, 32-section loading 32 times, and default-config serialization 16 times. Reported cycles divide out those repetitions. The driver alternates process order and reports medians of process medians. Fixture construction and compilation stay outside measurement; no compilation started by this task overlaps reported comparisons. Other work on the shared machine is uncontrolled.

Alias fixtures alternate Digest/Accumulator and Rising/Loudest. Boolean keys are dynamically constructed strings. Option fixtures vary all four millisecond fields and both booleans; parser inputs always come from the frozen original writer. Section writers reuse a 32 KiB output buffer, while full serialization constructs and drops the real default configuration and its output. Inputs/results pass through `black_box`. All **504 process/workload records** retain cycles, elapsed time, scoped allocation counters, and executable SHA-256 values.

Seven frozen functions match production at `6d1c4e5a1` byte for byte. Three new checks compare parser results and exact output across every ASCII case combination of all aliases, ASCII insertion/replacement at each position, sampled Unicode scalars and Unicode decorations, long accepted/rejected inputs, arbitrary boolean keys/prefixes, non-finite numbers, signed zero, rounding boundaries, and 4,096 varied option sets. They also compare current/original loading of each serialized option set. Clamping and other unchanged primitives are shared with the reference. The harness shares the existing config-test allocation counter.

Validation passes: **480 release tests** across config and profile with serial test execution (six ignored); **241 config debug tests** (two ignored); and **241 config unit tests** in the measured default-LTO after archive (two ignored). The three new checks also pass in every intermediate archive. Clippy completes for all config targets with existing warnings and no diagnostics in changed code. All changed/new Rust files pass `rustfmt --check`. Final production sources match the measured sources byte for byte; restoring the four designated functions reproduces the original files exactly. Every raw record matches its archived executable's SHA-256. Logs and archives remain under `target/config-text/`.

The initial parallel release run failed two profile tests: `perf_tests::sidecar_snapshots_preserve_bytes_callbacks_errors_and_load_reports` and `performance_profile_regressions::stats_snapshots_and_known_pack_sync_preserve_profile_and_file_behavior`. Both manipulate shared process-wide session/cache state. The entire suite then passed with `--test-threads=1`; config tests passed in both runs. Validation uses that serial result, and the initial failures remain recorded in `target/config-text/release.log` alongside the passing `release-serial.log`.

```powershell
cargo test --release --locked -p deadsync-config --lib --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_config_text --log-dir target/config-text/runs --before target/config-text/before/config.exe --after target/config-text/after/config.exe --output docs/performance/2026-10-26-config-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-config -p deadsync-profile -- --test-threads=1
cargo test --locked -p deadsync-config
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-config --all-targets
```

To reconstruct archives, keep the committed harness, reference, and test-module registration. Restore `null_or_die.rs` and `writer.rs` from `6d1c4e5a1` for `before`; apply direct kernel alias recognition for `aliases`; add direct boolean writing for `bools`; replace the three fixed-decimal `format!` calls with `format_args!` for `after`. Build each with the default-release command and save Cargo's reported `deadsync_config` test executable before changing the next stage. Compare consecutive archives with the same driver and eight runs each.
