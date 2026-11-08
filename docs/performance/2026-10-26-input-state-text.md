# Debounce state updates and pad direction text - 2026-10-26

Three measured optimizations in `deadsync-input`, following `rust-performance.md`. The runtime diff adds 45 lines and removes ten: one debounce body, two formatter bodies, an import, and two private formatting helpers. No public API, type, dependency, or unsafe code is added. A test module and two test-only visibility changes reuse the existing scoped allocator. Lua and unrelated worktree changes are excluded.

## Execution simplified

1. **Update debounce state in place.** Use the existing `Option` entry directly instead of copying its state into a local record and assigning it back. Initialization, pruning, raw/report timestamps, action masks, callback timing, and diagnostic snapshots retain their existing behavior.
2. **Write wildcard direction tokens from literals.** Map the four `PadDir` variants directly to their existing `PadDir::...` text. Both owned labels and INI writing bypass general formatting. Owned labels reserve exactly their text length instead of the formatter's estimate.
3. **Encode device direction tokens on the stack.** Write the unsigned decimal device ID, `Pad` prefix, and fixed direction suffix into one bounded ASCII span, then append it once. A 35-byte stack buffer suffices on this 64-bit target; its digit capacity is computed from `usize::MAX`. Safe UTF-8 validation precedes the append. This removes numeric/debug formatting and avoids the extra growth of large-ID owned tokens.

The decimal loop writes digits backward, leaving the final digit outside the loop so zero and single-digit IDs require no division. At most `MAX_DIGITS` positions are consumed; the prefix has three bytes and the longest suffix has twelve. Every byte in the appended span is initialized ASCII. Existing token spelling, parse behavior, ordering, and complete INI bytes are unchanged. Smaller output capacities are intentional.

## Isolated CPU measurements

Consecutive stages compare original -> in-place state -> wildcard literals -> device encoding. Each comparison uses eight fresh processes per version with fixed benchmark fixtures. All archives use the default release profile, including LTO. CPU throughput is before cycles divided by after cycles. Paired ranges are corresponding-process minimum/maximum ratios, not confidence intervals.

| Change / fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| State / eight-slot reverse cycle | 3,009.12 | 2,709.45 | 1.111x | 0.918-1.231x |
| State / 64-slot reverse cycle | 28,289.78 | 26,828.73 | 1.054x | 0.874-1.172x |
| State / 512-slot reverse cycle | 360,554.47 | 344,630.44 | 1.046x | 0.879-1.092x |
| State / 64-slot chatter cycle | 34,196.13 | 29,992.61 | 1.140x | 0.947-1.273x |
| State / 64 slots, zero debounce window | 11,053.45 | 9,126.83 | 1.211x | 0.956-1.378x |
| Wildcards / 512 tokens | 103,395.22 | 79,927.58 | 1.294x | 0.572-1.406x |
| Wildcards / reused INI buffer | 8,837.45 | 7,028.29 | 1.257x | 1.178-2.946x |
| Devices / 512 tokens | 158,883.27 | 110,026.08 | 1.444x | 1.293-1.719x |
| Devices / reused INI buffer | 6,484.69 | 4,677.66 | 1.386x | 1.321-1.709x |

Raw data include every workload and control: [state updates](2026-10-26-debounce-state.tsv), [wildcards](2026-10-26-debounce-wildcards.tsv), and [device tokens](2026-10-26-debounce-devices.tsv). The state-only series improves all seven cycle medians, with slower individual pairs. Controls vary too: repeated-held and not-due controls improve 1.150x/1.143x in that isolated series despite receiving no algorithmic change. These observations limit causal and whole-application claims.

## Combined results

The two original/final comparisons use twelve fresh processes per version. [Debounce data](2026-10-26-debounce-final.tsv) and [formatting data](2026-10-26-debounce-text-final.tsv) retain the complete set of fifteen workloads.

| Fixture | Before cycles | After cycles | CPU throughput | Before -> after ns | Paired range |
| --- | ---: | ---: | ---: | ---: | ---: |
| Reverse cycle, eight slots | 2,793.48 | 2,545.68 | 1.097x | 1,270.31 -> 1,164.26 | 0.926-1.323x |
| Reverse cycle, 64 slots | 26,064.34 | 24,813.79 | 1.050x | 11,950.58 -> 11,307.23 | 0.840-1.184x |
| Reverse cycle, 512 slots | 334,642.75 | 317,204.07 | 1.055x | 152,664.84 -> 144,829.69 | 0.900-1.183x |
| Ordered cycle, 64 slots | 24,626.44 | 22,833.14 | 1.079x | 11,233.98 -> 10,418.17 | 0.822-1.304x |
| Tied-deadline cycle, 64 slots | 24,696.32 | 22,647.08 | 1.090x | 11,258.60 -> 10,323.44 | 0.843-1.333x |
| Chatter cycle, 64 slots | 30,919.07 | 27,912.08 | 1.108x | 14,094.33 -> 12,726.76 | 0.954-1.429x |
| Zero-window cycle, 64 slots | 9,285.02 | 8,446.46 | 1.099x | 4,227.54 -> 3,851.76 | 0.815-1.345x |
| Repeated-held control, 64 edges | 827.84 | 876.71 | 0.944x | 375.00 -> 396.88 | 0.708-1.063x |
| Not-due control, 128 polls | 4,126.34 | 4,155.50 | 0.993x | 1,877.53 -> 1,890.43 | 0.730-1.305x |
| 512 wildcard direction tokens | 94,320.70 | 68,874.12 | 1.369x | 43,013.86 -> 31,401.96 | 1.233-1.545x |
| 512 keyboard tokens, control | 96,174.43 | 97,306.24 | 0.988x | 43,866.79 -> 44,377.74 | 0.927-1.045x |
| 512 device direction tokens | 153,813.77 | 107,713.62 | 1.428x | 70,152.54 -> 49,114.65 | 1.306-1.710x |
| 512 gamepad-code tokens, control | 112,583.35 | 110,259.74 | 1.021x | 51,346.48 -> 50,289.85 | 0.940-1.056x |
| Complete owned INI map | 31,482.39 | 28,246.48 | 1.115x | 14,351.57 -> 12,884.77 | 0.994-1.218x |
| Complete reused INI buffer | 8,077.35 | 4,673.81 | 1.728x | 3,680.27 -> 2,128.32 | 1.317-1.850x |

Debounce cycle medians improve 5-11%; their paired ranges all include slower runs. Repeated-held polling has a material slower median (0.944x), and keyboard formatting measures 0.988x. Their fast paths remain unchanged; the data do not establish the cause. The 512-slot case and maximum device ID are stress inputs. The measurements do not establish gameplay FPS or whole-application latency gains.

## Allocations

All warmed debounce cycles and controls record **zero allocations, reallocations, frees, and requested/freed/peak bytes**. Formatting counters are identical across each workload's process runs.

| Fixture | Allocations/frees before -> after | Reallocations before -> after | Requested/freed bytes before -> after | Scoped peak bytes before -> after |
| --- | ---: | ---: | ---: | ---: |
| 512 wildcard tokens | 512 / 512 -> 512 / 512 | 0 -> 0 | 8,192 -> 6,016 | 16 -> 13 |
| 512 device tokens | 512 / 512 -> 512 / 512 | 128 -> 0 | 15,360 -> 10,112 | 40 -> 35 |
| Complete owned INI map | 33 / 33 -> 33 / 33 | 96 -> 96 | 5,600 -> 5,600 | 3,584 -> 3,584 |
| Complete reused INI buffer | 0 / 0 -> 0 / 0 | 0 -> 0 | 0 -> 0 | 0 -> 0 |

Wildcard tokens reduce requested bytes by 26.6%. Device tokens reduce them by 34.2% and remove all 128 measured reallocations. The device probe cycles through IDs 0, 1, 37, and `usize::MAX`; the last is a bound stress, while the mixed INI map uses small IDs. Keyboard and gamepad-code controls keep 512 allocations/frees, respectively 9,216/9,728 requested/freed bytes and 18/19 peak bytes, with no reallocations. The scoped counters track allocation requests and live requested bytes.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Measured binaries use `--release --locked`, default LTO, the baseline CPU target, and static MSVC runtime settings from `.cargo/config.toml`. The existing helper pins the thread to logical processor 2 and reads `QueryThreadCycleTime`. Every fixture repeats **256 times per sample**; each process discards one warmup sample and reports the median of eleven samples. The driver alternates before/after process order and reports medians of process medians. No compilation started by this task overlaps retained comparisons; other shared-machine activity is uncontrolled. Allocation probes share the existing scoped System-allocator wrapper.

Each debounce cycle presses and releases all slots, drains delayed releases at 50 ms, then prunes at 80 ms. Reverse, ordered, and tied press times vary heap insertion order. Chatter re-presses and releases every third slot inside the 20 ms window; the zero-window case emits immediately. Store preparation stays outside timing and storage is reused. Token probes use 512 prepared bindings. INI probes serialize all 32 actions with one key, two wildcard directions, and one device direction each; reusable output is preallocated. Fixtures and results use `black_box`. Five TSVs retain all **696 process/workload records**, elapsed time, allocation counters, and executable SHA-256 values.

Two new checks cover a 2,048-operation replay against the complete production debounce module frozen from `0f7ded20c`, plus token and INI comparisons against four frozen formatter bodies. Replay checks exact store/heap debug state after every operation, emitted edge fields, clock-call counts, state after clock panics, and debug-message text with logging enabled and disabled. It includes clears, resizes, changing windows, both input sources, backward receipt times, and arbitrary masks/host timestamps. Formatter checks cover all four directions, decimal power boundaries, zero, maximum IDs, 4,096 generated IDs in every direction, parser round trips, complete maps, and repeated appends after an existing Unicode prefix.

Validation passes: **607 release tests** across input/core/config/profile (19 ignored), **368 debug tests** across input/core/config (15 ignored), and Clippy for all input targets with existing warnings and no diagnostics in the new test/reference files. All four retained default-LTO archives pass all 78 input unit tests (five ignored). Changed runtime bodies and new/reference Rust files pass formatting checks. `keymap.rs` retains pre-existing formatting differences; its only edit exposes a test module within the crate. Source audits verify the frozen references, the two test-only visibility edits, final production bytes against measured snapshots, and every raw record's executable digest.

Heap movement, cached-deadline layouts, in-place due resolution, and deferred diagnostic-snapshot trials did not retain consistent benefits. All heap methods, due resolution, and queue-drain code exactly match the starting source. Trial artifacts, logs, snapshots, and executable archives remain under `target/debounce-moves/`.

```powershell
cargo test --release --locked -p deadsync-input --lib --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_debounce_moves --log-dir target/debounce-moves/runs-accepted --before target/debounce-moves/base/config.exe --after target/debounce-moves/devices/config.exe --output docs/performance/2026-10-26-debounce-final.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_input_direction_text --log-dir target/debounce-moves/runs-accepted --before target/debounce-moves/base/config.exe --after target/debounce-moves/devices/config.exe --output docs/performance/2026-10-26-debounce-text-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-input -p deadsync-core -p deadsync-config -p deadsync-profile -- --test-threads=1
cargo test --locked -p deadsync-input -p deadsync-core -p deadsync-config -- --test-threads=1
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-input --all-targets
```

Keep the committed test harness fixed. For `base`, restore the changed runtime bodies/import from `0f7ded20c` and remove the two new private formatters, retaining the test-module declarations and visibility. Apply only the state-body change for `state`, then wildcard token formatting for `wildcards`, then device encoding for `devices`. Build/archive the reported unit-test executable at each stage. Compare consecutive stages with eight runs each, using `benchmark_debounce_moves` for state and `benchmark_input_direction_text` for both formatting steps. Both original/final comparisons use twelve runs.
