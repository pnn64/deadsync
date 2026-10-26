# Borrowed preference parsing - 2026-10-26

Three measured optimizations in `deadsync-config`, following the allocation and direct-execution guidance in `rust-performance.md`. Runtime changes affect only three existing `FromStr` bodies: 49 lines added and 17 removed. No public API, type, dependency, helper function, or unsafe operation is added. Lua and unrelated worktree changes are excluded.

## Execution simplified

1. **Note-scroll clock:** compare borrowed, trimmed input directly against its 20 supported aliases. Remove the temporary lowercase string, its full-input copy, and its allocation/free. The `0`/`1` aliases use direct equality.
2. **Log level:** compare borrowed text with the seven accepted names, including `err` and `warning`, instead of allocating lowercase text.
3. **Default fail mode:** compare borrowed text with `immediate`, `immediatecontinue`, and `immediate_continue`, removing another temporary string from system-option loading.

All three keep Unicode edge trimming, ASCII-only case folding, exact alias spelling, and the same `Result` values. Internal whitespace and punctuation remain significant. Unknown or non-ASCII names receive the same rejection result. The longer source replaces allocation, normalization, and deallocation with direct comparisons; no normalization buffer or intermediate collection is introduced.

## Isolated CPU measurements

Consecutive stages compare original -> clock -> log level -> fail mode, with eight fresh processes per version and a fixed final harness. Every executable uses the default release profile, including LTO. CPU throughput is before cycles divided by after cycles; paired ranges are minimum/maximum corresponding-process ratios, not confidence intervals.

| Change / fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| Clock / parse 512 values | 74,678.04 | 14,610.47 | 5.111x | 4.904-5.553x |
| Clock / load 32 audio sections | 27,128.83 | 23,294.43 | 1.165x | 1.117-1.506x |
| Clock / reject 64 long values | 46,942.14 | 912.28 | 51.456x | 40.813-56.565x |
| Log level / parse 512 values | 93,393.82 | 12,398.33 | 7.533x | 6.683-8.363x |
| Log level / load 32 system sections | 117,929.79 | 105,778.44 | 1.115x | 0.922-1.235x |
| Fail mode / parse 512 values | 75,003.85 | 10,858.39 | 6.907x | 6.155-7.057x |
| Fail mode / load 32 system sections | 94,481.03 | 88,461.92 | 1.068x | 0.860-1.164x |

Raw data retain every workload and control: [clock](2026-10-26-preference-clock.tsv), [log level](2026-10-26-preference-log.tsv), and [fail mode](2026-10-26-preference-fail.tsv). Untouched paths vary between archives. In the clock-only step, fail parsing measures 0.985x (paired 0.955-0.996x) and system loading 0.987x. In the log-only step, the font control measures 0.975x and already-optimized long clock rejection 0.960x. These are retained observations, not improvements attributed to the changes.

## Combined results and allocation removal

The [original/final comparison](2026-10-26-preference-final.tsv) uses twelve fresh processes per version with all three changes. It includes the complete set of seven workloads below.

| Fixture | Before cycles | After cycles | CPU throughput | Before -> after ns | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: |
| Parse 512 clock values | 76,859.29 | 13,886.80 | 5.535x | 35,051.56 -> 6,306.25 | 4.482-6.713x |
| Parse 512 log levels | 79,760.80 | 11,462.00 | 6.959x | 36,354.68 -> 5,200.00 | 5.487-7.842x |
| Parse 512 fail modes | 71,601.57 | 10,957.85 | 6.534x | 32,701.57 -> 4,973.44 | 4.661-7.131x |
| Reject 64 long clock values | 48,269.42 | 974.03 | 49.556x | 21,962.50 -> 421.88 | 41.512-54.464x |
| Parse 512 fonts, unchanged control | 69,499.19 | 67,365.92 | 1.032x | 31,687.50 -> 30,723.43 | 0.812-1.071x |
| Load 32 audio sections | 29,008.30 | 24,796.64 | 1.170x | 13,196.88 -> 11,295.31 | 1.003-1.386x |
| Load 32 system sections | 97,619.19 | 87,964.61 | 1.110x | 44,496.88 -> 40,101.56 | 0.885-1.135x |

| Fixture | Allocations/frees before -> after | Requested/freed bytes before -> after | Peak bytes before -> after |
| --- | ---: | ---: | ---: |
| Clock parsing | 480 / 480 -> 0 / 0 | 3,505 -> 0 | 14 -> 0 |
| Log parsing | 480 / 480 -> 0 / 0 | 2,446 -> 0 | 10 -> 0 |
| Fail parsing | 480 / 480 -> 0 / 0 | 6,262 -> 0 | 18 -> 0 |
| Long clock rejection | 64 / 64 -> 0 / 0 | 262,609 -> 0 | 4,110 -> 0 |
| Audio loading | 30 / 30 -> 0 / 0 | 205 -> 0 | 12 -> 0 |
| System loading | 60 / 60 -> 0 / 0 | 536 -> 0 | 18 -> 0 |

Reallocations remain zero. Empty inputs explain the difference between fixture counts and allocations. Across the two 32-map loader fixtures, the changes remove **90 temporary allocations/frees and 741 requested/freed bytes**. The isolated log step removes 30 allocations and 154 bytes from system loading; the fail step removes its remaining 30 allocations and 382 bytes. Font-control allocation counts remain 480 allocations/frees, 2,557 requested/freed bytes, and ten peak bytes. All allocation observations are identical across their process runs.

These parsers run during settings loading. The data establish lower parser CPU/allocation cost and median gains in the measured loaders; they do not establish gameplay FPS or whole-application latency gains. System-loader paired results include slower runs despite consistent allocation removal. Long-input rejection is a separate stress case: its input includes 4,096 trailing characters, while normal preferences are short.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Archives use `--release --locked`, default LTO, the baseline CPU target, and static MSVC runtime settings from `.cargo/config.toml`. The existing helper pins the benchmark thread to logical processor 2 and reads `QueryThreadCycleTime`. Each process discards one warmup sample and reports the median of eleven samples. The driver alternates before/after process order and reports medians of process medians. No compilation started by this task overlaps the retained comparisons; other activity on the shared machine is uncontrolled.

Normal parser fixtures contain 512 prepared strings spanning aliases, uppercase spellings, Unicode edge whitespace, rejected text, and empty values. The long-clock fixture contains 64 strings. Loader fixtures contain 32 prepared INI maps and execute the complete existing audio/system option loaders. Every fixture repeats 32 times per sample; fixture construction and INI parsing stay outside measurement. Inputs and results use `black_box`. Four TSVs retain all **504 process/workload records**, elapsed time, allocation counters, and archived executable SHA-256 values.

Two new differential checks compare against production parser bodies frozen from `2e4e32cc4`. They cover every ASCII case combination of all accepted aliases, insertion of every ASCII byte at every position, Unicode insertions and edge whitespace, long rejections, and complete loader results with alternate defaults, missing keys, and invalid values. Production audits restrict edits to the three parser bodies, verify the frozen reference, and match final source bytes and all 504 records to the measured archives.

Validation passes: **486 release tests** across config/profile (nine ignored), **247 debug tests** across config (five ignored), and Clippy for all config targets with existing warnings and no diagnostics in the changed methods or new tests. All four changed/new Rust files pass formatting checks. Each measured default-LTO archive passes both new differential checks. Logs, source snapshots, and executables remain under `target/preference-codecs/`.

```powershell
cargo test --release --locked -p deadsync-config --test preference_codecs --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_preference_codecs --log-dir target/preference-codecs/runs --before target/preference-codecs/before/config.exe --after target/preference-codecs/after/config.exe --output docs/performance/2026-10-26-preference-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-config -p deadsync-profile -- --test-threads=1
cargo test --locked -p deadsync-config -- --test-threads=1
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-config --all-targets
```

Keep the committed integration harness fixed to reconstruct archives. Restore the three modified parser bodies from `2e4e32cc4` for `before`; apply the clock change for `clock`, then the log-level change for `log`, then fail mode for `after`. Build/archive the reported integration executable at each stage using the first command. Compare consecutive stages with eight runs each and the original/final pair with twelve runs.
