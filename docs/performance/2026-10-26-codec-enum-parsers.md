# Endian call boundaries and borrowed enum parsing — 2026-10-26

Three measured optimizations in `deadlib-bincode` and `deadsync-config`, following the execution-overhead and allocation guidance in `rust-performance.md`. The runtime diff adds 10 lines and removes 11, including removal of one compiler restriction. No public API, runtime function/type, dependency, or unsafe operation is added. Lua and unrelated worktree changes are excluded.

## Execution simplified

1. **Allow endian conversion to inline.** Remove `#[inline(never)]` from the existing generic endian-vector helper. This lets the compiler specialize its type/configuration gates at call sites. The helper body, initialization bookkeeping, error handling, and reader calls are unchanged. The measured benefit is for small, endian-converting vectors; large-vector and native-order controls do not share that benefit.
2. **Parse graph choices through borrowed text.** Orientation and origin parsers compare trimmed input with their supported names using `eq_ignore_ascii_case`, removing two temporary lowercase strings from every complete NullOrDie option load. They retain strict spelling, ASCII-only case folding, Unicode trimming, and the same rejection results.
3. **Parse coin mode through borrowed text.** Apply direct case-insensitive comparisons to Pay, Free, and Home. This removes the normalized-string allocation from the parser and its complete option-loader path. The accepted names and `Result` values are unchanged.

## CPU measurements

All measured executables use the repository's **default release profile, including LTO**. Codec results compare only the call-boundary change using twelve fresh processes per version. Enum isolated results compare original → graph parsers → coin parser using eight processes per version at each stage.

| Change / workload | Before cycles/fixture | After cycles/fixture | CPU throughput | Paired ratio range |
| --- | ---: | ---: | ---: | ---: |
| Endian helper / eight u32 values | 42.88 | 30.87 | 1.389x | 1.250–1.500x |
| Endian helper / eight f64 values | 46.30 | 32.59 | 1.421x | 1.333–1.667x |
| Endian helper / 4,096 u32 values | 4,798.14 | 4,719.25 | 1.017x | 0.917–1.165x |
| Endian helper / 4,096 f64 values | 4,749.26 | 4,792.13 | 0.991x | 0.849–1.032x |
| Graph parsers / 512 orientation-origin pairs | 158,081.15 | 22,611.94 | 6.991x | 6.035–8.050x |
| Graph parsers / load 32 option sections | 56,109.69 | 47,291.95 | 1.186x | 0.991–1.456x |
| Coin parser / 512 values | 76,612.38 | 10,213.61 | 7.501x | 6.638–8.283x |
| Coin parser / load 32 option sections | 18,688.38 | 14,535.01 | 1.286x | 1.223–1.416x |

Raw data include every control: [codec](2026-10-26-codec-boundary.tsv), [graph parsers](2026-10-26-enum-graph.tsv), and [coin parser](2026-10-26-enum-coin.tsv). Ranges are minimum/maximum ratios of corresponding process runs, not confidence intervals. Codec allocations remain zero for all warmed workloads.

The codec comparison has material slower controls: native-order u32 copying changes 2,176.99 → 2,339.05 cycles (0.931x throughput), and unchanged signed-varint decoding changes 29,510.75 → 31,936.39 cycles (0.924x). The signed control regresses in every paired run (0.884–0.951x); native-copy ratios vary widely (0.298–2.981x). These are retained observations, not claimed improvements. The measurements do not establish the cause or demonstrate a universal persistence-codec gain. Large endian conversions are essentially unchanged. The call-boundary benefit is scoped to the small-vector cases above.

## Combined enum results and allocations

The [combined enum comparison](2026-10-26-enum-final.tsv) uses twelve fresh processes per version with both parser changes. It is separate from the codec comparison; no whole-application speedup is inferred by multiplying ratios.

| Workload | Before cycles/fixture | After cycles/fixture | CPU throughput | Before → after ns/fixture |
| --- | ---: | ---: | ---: | ---: |
| Parse 512 graph-choice pairs | 139,862.64 | 22,752.56 | 6.147x | 63,754.68 → 10,410.94 |
| Parse 512 coin modes | 77,222.84 | 9,826.07 | 7.859x | 35,196.88 → 4,457.82 |
| Load 32 graph option sections | 53,372.82 | 45,186.12 | 1.181x | 24,321.88 → 20,739.06 |
| Load 32 coin option sections | 17,697.18 | 14,469.84 | 1.223x | 8,045.31 → 6,571.88 |

| Workload | Allocations/frees before → after | Requested bytes before → after | Peak bytes before → after |
| --- | ---: | ---: | ---: |
| Parse graph choices | 1,024 / 1,024 → 0 / 0 | 7,040 → 0 | 10 → 0 |
| Parse coin modes | 512 / 512 → 0 / 0 | 2,149 → 0 | 7 → 0 |
| Load graph option sections | 64 / 64 → 0 / 0 | 448 → 0 | 8 → 0 |
| Load coin option sections | 32 / 32 → 0 / 0 | 133 → 0 | 7 → 0 |

These probes have zero reallocations throughout. Loading both groups for 32 configurations removes **96 temporary allocations/frees and 581 requested bytes**. INI map construction is outside these load probes. Allocation counts are identical across all process runs. Unchanged enum-suite controls are also retained: decorated kernel aliases measure 0.970x, and NullOrDie serialization measures 0.989x in the combined comparison. Loader paired ranges include slower runs (0.920–1.390x for graph options, 0.880–1.331x for coin options), despite consistent allocation removal and median gains. These settings paths are cold; the data do not establish gameplay FPS or input-latency gains.

## Rejected trials

A byte-wise Boolean reduction slowed 4,096-value decoding to 0.789x and 65,536-value decoding to 0.846x. A 64-bit word reduction initially gained 9% at 4,096 values, but a twelve-process ablation with the endian change measured 0.972x at that size and only 1.036x at 65,536; it was also discarded. A spare-capacity rewrite that published vector length once made large f64 endian decoding 0.406x as fast, so the existing initialization loop was restored. A direct frame-time float cast improved ordinary inputs but regressed mixed/saturating inputs to 0.576x/0.364x; a bit-range variant was worse. Frame conversion and Boolean validation exactly match the starting revision. Trial executables and records remain under `target/codec-bulk/` and `target/frame-kernels/`.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Archives use `--release --locked`, default LTO, static MSVC runtime settings from `.cargo/config.toml`, and the baseline CPU target. The existing helper pins the benchmark thread to logical processor 2 and reads `QueryThreadCycleTime`. Each process discards one warmup sample and reports the median of eleven samples. The driver alternates before/after process order and takes medians of process medians. No compilation started by this task overlaps retained comparisons. Other activity on the shared machine is uncontrolled. Elapsed-time readings for the tiny codec fixtures are especially sensitive to timer granularity; CPU cycles are the primary measure.

Codec samples repeat each fixture 128 times. Vectors contain eight or 4,096 u32/f64 values encoded with legacy fixed integers and big endian; numeric fixtures include arbitrary f64 bit patterns. Native copying and variable i32 decoding use their corresponding configurations. Boolean controls decode 64, 4,096, or 65,536 values, plus a 4,096-value last-byte failure. Output allocations are prewarmed. Inputs/results pass through `black_box`.

Enum parser fixtures contain 512 graph pairs or coin strings, including canonical names, mixed case, Unicode edge whitespace, and rejected names. The graph loader uses the existing 32-section fixture and frozen original serialization; coin loaders use 32 prepared INI maps. All parser/loader samples repeat 32 times. Existing serialization controls repeat boolean writing 64 times and other writing 16 times. Fixture construction stays outside measurement. The four TSVs retain all **800 process/workload records**, elapsed time, scoped allocation counters, and executable SHA-256 values.

The codec differential check freezes the original helper body from `63cdbfd0a` and compares numeric output bits through re-encoding, reader calls, remaining input, appended-vector behavior, short/missing peeks, and state after a consuming reader panics. It covers all supported signed/unsigned integer widths and f32/f64, including signed zero and signaling NaN payloads, in both byte orders. Existing codec tests cover public owned/borrowed/reusable APIs, truncation, limits, and wire compatibility. The codec's Rust 1.86 minimum is preserved.

Two new config checks extend the existing frozen-reference harness with every ASCII case combination of supported graph and coin names, ASCII insertion at every position, Unicode insertions/edge whitespace, invalid names, and long graph rejections. The prior option-round-trip checks now use frozen graph parsers too, so new parsing is compared through actual option loading. Frozen references retain their source bodies; the coin method is exposed as a standalone test helper with `Self` resolved to the same public enum.

Validation passes: **578 release tests** across codec, config, and profile (nine ignored), and **339 debug tests** across codec and config (five ignored). Profile checks run serially because earlier parallel runs exposed shared runtime-state interference. The measured default-LTO archives also pass all **19 codec unit tests** (one ignored) and **243 config unit tests** (two ignored). Clippy completes for all codec/config targets with existing warnings and no diagnostics in changed code. All changed/new Rust files pass formatting checks. Only newline normalization changed after measurement; canonicalized measured production sources match the final files. Source audits restrict runtime edits to the attribute and three parser bodies, verify eleven frozen source bodies/methods, confirm discarded production trials were restored, and match every one of the 800 raw records to its archived executable digest. Logs and archives remain under `target/codec-bulk/` and `target/enum-parsers/`.

```powershell
cargo test --release --locked -p deadlib-bincode --lib --no-run --message-format=json
cargo test --release --locked -p deadsync-config --lib --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_codec_bulk --log-dir target/codec-bulk/runs --before target/codec-bulk/before-final/codec.exe --after target/codec-bulk/after-final/codec.exe --output docs/performance/2026-10-26-codec-boundary.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_config_text --log-dir target/enum-parsers/runs --before target/enum-parsers/before/config.exe --after target/enum-parsers/after/config.exe --output docs/performance/2026-10-26-enum-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadlib-bincode -p deadsync-config -p deadsync-profile -- --test-threads=1
cargo test --locked -p deadlib-bincode -p deadsync-config -- --test-threads=1
cargo clippy --release --locked --config profile.release.lto=false -p deadlib-bincode -p deadsync-config --all-targets
```

Keep the committed harnesses to reconstruct archives. For codec `before-final`, reinsert `#[inline(never)]` on `decode_endian_vec_into`; remove it for `after-final`. For enum `before`, restore only the graph parser functions and CoinMode's `FromStr` method from `63cdbfd0a`; apply the graph changes for `graph`, then coin mode for `after`. Build/archive each reported test executable with the matching default-release command. Compare consecutive enum stages with eight runs each; the combined comparison uses twelve. All retained comparisons keep the rest of their harness and production sources fixed.
