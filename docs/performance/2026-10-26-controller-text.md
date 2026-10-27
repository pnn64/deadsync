# Controller UUID and screenshot-flag text - 2026-10-26

Three measured optimizations in `deadsync-config`, following the direct-execution guidance in `rust-performance.md`. Runtime edits affect three existing function bodies: 53 lines added and 28 removed. No public API, dependency, or unsafe code is added. Lua and unrelated worktree changes are excluded.

## Execution simplified

1. **Decode UUIDs in one pass.** Replace the initial hexadecimal validation scan and sixteen string-slicing/radix parses with sixteen byte-pair conversions that validate while decoding. Unicode edge trimming, exact 32-byte length, ASCII hex spelling, and rejection results remain unchanged. Partial local output is never published on failure.
2. **Append encoded UUIDs in one operation.** Fill a 32-byte stack array with lowercase hexadecimal digits, validate its ASCII text with safe `from_utf8`, then append the whole span. This replaces 32 individual `String::push` operations per UUID. The list's existing capacity reservation, delimiters, and empty-list handling remain unchanged.
3. **Look up screenshot-flag text.** Replace two walks over five flags with a table containing all 32 combinations. The table trades a small amount of constant data for direct indexing and one owned-string copy. Mask zero still produces `Off`; unsupported high bits are ignored, and a nonzero mask containing only unsupported bits still produces an empty string. Flag names, order, separators, allocation counts, and string capacities are preserved.

## Isolated measurements

Consecutive stages compare original -> UUID decode -> UUID append -> flag lookup, using eight fresh processes per version and the same final harness. All measured executables use the default release profile, including LTO. A controller fixture loads or saves **all six backend orders**, with the indicated UUID count per backend. CPU throughput is before cycles divided by after cycles. Paired ranges are corresponding-process minimum/maximum ratios, not confidence intervals.

| Change / fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| Decode / load one UUID per backend | 2,369.92 | 1,529.64 | 1.549x | 1.331-1.615x |
| Decode / load 16 UUIDs per backend | 26,367.44 | 11,379.70 | 2.317x | 1.959-2.525x |
| Decode / load 64 UUIDs per backend | 123,780.85 | 60,403.65 | 2.049x | 1.879-2.312x |
| Decode / mixed valid, duplicate, and invalid entries | 37,290.99 | 17,851.51 | 2.089x | 1.984-2.990x |
| Append / save one UUID per backend | 1,948.06 | 1,889.75 | 1.031x | 0.897-1.089x |
| Append / save 16 UUIDs per backend | 8,934.34 | 7,233.22 | 1.235x | 1.105-1.511x |
| Append / save 64 UUIDs per backend | 24,916.69 | 19,453.17 | 1.281x | 1.050-1.407x |
| Flag lookup / format all 256 masks | 36,896.57 | 32,016.14 | 1.152x | 1.088-1.174x |
| Flag lookup / 256 single-flag masks | 37,877.47 | 35,178.29 | 1.077x | 0.883-1.105x |
| Flag lookup / 256 multi-flag masks | 38,169.00 | 30,771.14 | 1.240x | 1.194-1.269x |

Raw data retain all controls: [decode](2026-10-26-controller-decode.tsv), [append](2026-10-26-controller-encode.tsv), and [flag lookup](2026-10-26-controller-flags.tsv). Material slower observations are retained. Empty loading slows from 987.75 to 1,179.82 cycles in the decode step (0.837x, paired 0.746-0.921x). In the append step, untouched mixed loading measures 0.942x. The flag-lookup archive slows untouched loading at 16 UUIDs (0.944x, paired 0.847-0.962x), 64 UUIDs (0.863x, paired 0.809-0.911x), and mixed entries (0.916x, paired 0.891-0.993x). These comparisons establish archive-level tradeoffs; they do not establish why untouched paths change. The final combination still improves every populated loading fixture in every paired run.

## Combined results and allocations

The [original/final comparison](2026-10-26-controller-final.tsv) uses twelve fresh processes per version. This is the complete set of thirteen workloads.

| Fixture | Before cycles | After cycles | CPU throughput | Before -> after ns | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: |
| Load empty orders | 1,022.03 | 977.47 | 1.046x | 445.31 -> 426.56 | 0.766-1.104x |
| Save empty orders | 1,046.06 | 994.59 | 1.052x | 457.81 -> 435.94 | 0.867-1.112x |
| Load one UUID per backend | 2,332.19 | 1,334.16 | 1.748x | 1,045.31 -> 589.06 | 1.241-1.926x |
| Save one UUID per backend | 2,074.97 | 1,972.07 | 1.052x | 926.57 -> 879.69 | 0.917-1.117x |
| Load 16 UUIDs per backend | 26,861.29 | 11,729.55 | 2.290x | 12,346.88 -> 5,325.00 | 1.969-2.421x |
| Save 16 UUIDs per backend | 9,164.14 | 7,075.45 | 1.295x | 4,157.82 -> 3,215.62 | 1.007-1.440x |
| Load 64 UUIDs per backend | 125,156.14 | 63,744.18 | 1.963x | 57,046.88 -> 29,043.75 | 1.645-2.188x |
| Save 64 UUIDs per backend | 26,113.64 | 19,374.30 | 1.348x | 11,875.00 -> 8,807.81 | 1.134-1.817x |
| Load mixed entries | 36,526.17 | 17,686.90 | 2.065x | 16,620.32 -> 8,037.50 | 1.814-2.307x |
| Format all 256 masks | 37,671.69 | 32,153.33 | 1.172x | 17,175.00 -> 14,628.12 | 1.040-1.375x |
| Format 256 single-flag masks | 38,885.82 | 35,435.53 | 1.097x | 17,696.87 -> 16,120.31 | 0.905-1.221x |
| Format 256 multi-flag masks | 39,403.67 | 32,108.74 | 1.227x | 17,995.32 -> 14,642.19 | 1.004-1.487x |
| Format 256 `Off` masks, unchanged return branch | 37,359.58 | 29,210.64 | 1.279x | 16,998.44 -> 13,323.44 | 1.192-1.421x |

Allocation counters are **identical before and after, in every stage and process run**. All loading fixtures remain allocation-free. Saving empty orders uses one allocation/free and 240 requested/freed/peak bytes. Saving one, 16, or 64 UUIDs per backend uses seven allocations/frees and respectively 432, 3,402, or 12,906 requested/freed/peak bytes. Reallocations remain zero.

Flag fixtures also have unchanged counters: all masks use 249 allocations/frees, 3,595 requested/freed bytes, and 29 peak bytes; single flags use 256 allocations/frees, 1,278 bytes, and six peak bytes; combinations use 256 allocations/frees, 4,864 bytes, and 29 peak bytes; `Off` uses 256 allocations/frees, 768 bytes, and three peak bytes. The seven unsupported-only masks explain the allocation-free results within the all-mask fixture. No allocation reduction is claimed.

These are configuration loading/saving paths. Capacity 64 is the supported bound, not an assumed normal controller count. Empty-order and single-UUID save results include slower paired runs. The `Off` return body is unchanged; its speedup is a control observation, not a distinct algorithmic improvement. The measurements do not establish gameplay FPS or whole-application latency gains.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Archives use `--release --locked`, default LTO, the baseline CPU target, and static MSVC runtime settings from `.cargo/config.toml`. The existing helper pins the benchmark thread to logical processor 2 and reads `QueryThreadCycleTime`. Each process discards one warmup sample and reports the median of eleven samples. The driver alternates before/after order and reports medians of process medians. No compilation started by this task overlaps retained comparisons; other shared-machine activity is uncontrolled.

Fixtures use the public controller load/save functions, including native order sanitization, bounded snapshots, and mutex operations. Prepared INI maps stay outside measurement. The mixed fixture supplies 32 entries per backend: Unicode-trimmed uppercase UUIDs, malformed strings, duplicate UUIDs, and 32-byte strings containing non-ASCII text. Save probes use the order installed by the matching load probe. Flag fixtures contain 256 prepared masks each. Every fixture repeats 32 times per sample; inputs and outputs use `black_box`. Four TSVs retain all **936 process/workload records**, elapsed time, allocation counters, and archived executable SHA-256 values.

Two new checks compare against six production function bodies frozen from `17d5ad286`. Controller checks cover every byte value in every UUID byte position, uppercase spellings, Unicode whitespace within lists, every ASCII replacement at every hex position, Unicode replacements, all six backend keys, missing/wrong-case keys, duplicates, overflow beyond 64 slots, and exact serialized bytes, vector capacity, and string capacities. Screenshot checks exhaust all 256 possible input masks, comparing text and capacity. Source audits restrict runtime changes to the three bodies, verify the frozen references, match final production bytes to the retained measured archives, and verify every raw record's executable digest.

Validation passes: **488 release tests** across config/profile (ten ignored), **249 debug tests** across config (six ignored), and Clippy for all config targets with existing warnings and no diagnostics in the changed methods or new tests. Each retained default-LTO archive passes both new checks. All four changed/new Rust files pass formatting checks. Logs, source snapshots, executables, and rejected-trial artifacts remain under `target/pad-ini-codecs/`.

Span-based and byte-tail INI escape-decoder screening trials slowed dense inputs and were discarded; `ini.rs` exactly matches the starting source. A smaller two-table flag formatter also failed to improve common cases. Adding an inlining hint to the full lookup shifted slowdowns to other controller controls and was discarded. Its source and eight-process comparison remain in `flags-inline-trial.tsv`; the retained formatter has no new inlining attribute.

```powershell
cargo test --release --locked -p deadsync-config --test controller_text --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_controller_text --log-dir target/pad-ini-codecs/runs --before target/pad-ini-codecs/before/config.exe --after target/pad-ini-codecs/after/config.exe --output docs/performance/2026-10-26-controller-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-config -p deadsync-profile -- --test-threads=1
cargo test --locked -p deadsync-config -- --test-threads=1
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-config --all-targets
```

Keep the committed integration harness fixed to reconstruct archives. Restore the three modified function bodies from `17d5ad286` for `before`; apply UUID decoding for `decode`, then UUID appending for `encode`, then flag lookup for `after`. Build/archive the reported integration executable at each stage with the first command. Compare consecutive stages with eight runs each and the original/final pair with twelve runs.
