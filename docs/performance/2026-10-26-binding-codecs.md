# Binding buffers and UUID codecs — 2026-10-26

Three safe Rust optimizations in `deadsync-input`, following the allocation and execution-overhead guidance in `rust-performance.md`. These paths load and serialize keymap configuration; this is cold reconfiguration work, and the measurements do not establish input-event latency or gameplay FPS gains. The production diff adds 18 lines and removes 16 across three existing functions. No dependency, public API, production function/type, or unsafe block is added. Lua and unrelated worktree changes are excluded.

## Execution simplified

1. **Transfer full binding buffers directly.** When a parsed action's vector has no spare capacity, pass it to the existing `bind_owned` implementation. This avoids copying its bindings into another allocation and then destroying the original buffer. Vectors with spare capacity retain the copying path: blindly consuming them can introduce shrink reallocations when converted into boxed slices. Defaults and reverse-key bookkeeping retain their existing implementations.
2. **Write UUID hexadecimal digits directly.** Replace sixteen general formatting calls with two uppercase ASCII table lookups per byte. Code and device formatting, string capacity planning, leading zeroes, and token syntax are unchanged.
3. **Decode validated UUID digits directly.** Reuse the existing scan that establishes exactly 32 ASCII hex digits. Decode fixed byte pairs using their low nibbles and letter bits, eliminating sixteen substring/radix parses and their unreachable failure tracking. The scan and suffix state machine remain, preserving malformed and partial-token behavior.

## Isolated before/after measurements

Consecutive stages are original → conditional buffer ownership → direct UUID formatting → direct UUID parsing. Every archive uses the repository's **default release profile, including LTO**. Each isolated comparison alternates eight fresh processes per version; figures are median thread CPU cycles per fixture.

| Change / workload | Before cycles | After cycles | CPU throughput | Paired ratio range |
| --- | ---: | ---: | ---: | ---: |
| Buffer ownership / qualified load | 78,697.61 | 71,421.52 | 1.102x | 1.054–1.258x |
| Buffer ownership / qualified load and write | 113,258.57 | 107,525.86 | 1.053x | 0.983–1.125x |
| UUID formatting / 1,024 tokens | 1,002,874.92 | 319,598.84 | 3.138x | 3.083–3.177x |
| UUID formatting / qualified keymap write | 33,828.72 | 13,619.29 | 2.484x | 2.376–2.592x |
| UUID parsing / 1,024 tokens | 304,779.20 | 206,302.57 | 1.477x | 1.452–1.548x |
| UUID parsing / qualified load | 73,553.08 | 67,528.82 | 1.089x | 1.036–1.148x |

Raw results include all controls: [buffer ownership](2026-10-26-binding-owned.tsv), [UUID formatting](2026-10-26-binding-format.tsv), and [UUID parsing](2026-10-26-binding-parse.tsv). Ranges are minimum/maximum ratios of corresponding process runs, not confidence intervals. The retained capacity guard removes qualified-load allocations without increasing reallocations. An unconditional-ownership trial was discarded because spare-capacity keyboard buffers needed shrink reallocations.

## Combined results and allocations

The [combined comparison](2026-10-26-binding-final.tsv) alternates twelve fresh processes per version, comparing original production code with all three changes.

| Fixture / workload | Before cycles/fixture | After cycles/fixture | CPU throughput | Before → after ns/fixture |
| --- | ---: | ---: | ---: | ---: |
| 1,024 tokens / format | 992,568.70 | 327,823.27 | 3.028x | 453,078.13 → 149,464.07 |
| 1,024 tokens / parse | 302,143.45 | 207,511.52 | 1.456x | 137,775.78 → 94,643.75 |
| Keyboard keymap / load | 20,370.63 | 19,243.98 | 1.059x | 9,271.10 → 8,772.66 |
| Keyboard keymap / write | 1,334.15 | 1,406.17 | 0.949x | 598.44 → 631.25 |
| Keyboard keymap / load and write | 20,674.15 | 19,941.92 | 1.037x | 9,519.53 → 9,112.50 |
| Qualified keymap / load | 77,665.27 | 67,585.43 | 1.149x | 35,521.88 → 30,810.93 |
| Qualified keymap / write | 33,610.94 | 13,372.35 | 2.513x | 15,328.90 → 6,092.18 |
| Qualified keymap / load and write | 114,304.62 | 83,082.46 | 1.376x | 52,114.85 → 38,074.21 |

| Workload | Before allocations/frees | After allocations/frees | Reallocations before → after | Requested bytes before → after | Peak bytes before → after |
| --- | ---: | ---: | ---: | ---: | ---: |
| Format 1,024 tokens | 1,024 / 1,024 | 1,024 / 1,024 | 0 → 0 | 55,680 → 55,680 | 55 → 55 |
| Keyboard keymap load | 66 / 66 | 66 / 66 | 0 → 0 | 9,056 → 9,056 | 6,016 → 6,016 |
| Qualified keymap load | 171 / 171 | 139 / 139 | 4 → 4 | 32,416 → 27,296 | 18,776 → 18,776 |

Qualified loading removes **32 temporary allocations/frees and 5,120 requested bytes per 32-action keymap**. Load-and-write probes have the same allocation counts as loading. Token parsing and warmed keymap writing remain allocation-free; token formatting still allocates one output string per token. Counts are identical across all runs of each stage. Peak bytes are unchanged because other keymap construction work determines the peak.

The keyboard-only writer control uses about **5.4% more cycles** in the combined comparison. It contains no UUIDs, but executable layout and shared-machine effects can influence unchanged work; the measurements do not identify the cause. Its paired throughput range is 0.896–1.400x. Keyboard load-and-write has only a modest median gain and a 0.940–1.141x range. Qualified load-and-write improves in every paired combined run (1.299–1.413x). These findings do not imply an improvement for every keymap layout or unmeasured device workload.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Measured archives use `--release --locked`, default LTO, static MSVC runtime settings from `.cargo/config.toml`, and the baseline CPU target. The existing helper pins the current thread to logical processor 2 and reads `QueryThreadCycleTime`. Each process discards one warmup sample and reports the median of eleven samples. Samples repeat token formatting 32 times and parsing 64 times; all keymap workloads repeat 64 times. Reported cycles divide out those repetitions. The driver alternates process order and reports medians of process medians. Fixture construction and compilation stay outside measurement; no compilation started by this task overlaps reported comparisons. Other work on the shared machine is uncontrolled.

Token fixtures contain 1,024 distinct codes with device indices and varied UUID bytes. Parser fixtures always use frozen original formatting, keeping their inputs identical across stages. Keymaps specify all 32 actions. The keyboard fixture uses the built-in configuration lines; the qualified fixture supplies a keyboard key, an unqualified pad direction, a device-qualified pad direction, and a UUID-qualified pad code for each action. Writers reuse preallocated output buffers. Inputs and results pass through `black_box`. All **576 process/workload records** retain cycles, elapsed time, scoped allocation counters, and executable SHA-256 values; archived executable digests were verified against every record.

Three new checks compare frozen production references from `dd91f5bf9`: exact formatting and round trips over every UUID byte, code/device boundaries, uppercase/lowercase hex, leading zeroes, malformed lengths, every ASCII mutation at each UUID position, Unicode insertions, overflow, repeated/reordered qualifiers, and partial parses. Keymap checks compare bindings and reverse keyboard lookup across defaults, absent/empty sections, duplicate actions/bindings, empty values, whitespace, unknown keys, and malformed tokens. Existing unchanged parsing and keymap primitives are shared with the reference.

Validation passes: **1,168 release tests** across input, gameplay, and config (10 ignored); **102 input debug tests** (five ignored); and all three new checks in every measured stage archive. Clippy completes for all input targets with existing warnings and no diagnostics in changed code. All changed/new Rust files pass `rustfmt --check`. Only formatting changed after the measured production build; canonicalized measured sources match the final files. Logs and executable archives remain under `target/binding-codecs/`.

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-input -p deadsync-gameplay -p deadsync-config
cargo test --locked -p deadsync-input
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-input --all-targets
cargo test --release --locked -p deadsync-input --test binding_codecs --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_binding_codecs --log-dir target/binding-codecs/runs --before target/binding-codecs/before/input.exe --after target/binding-codecs/after/input.exe --output docs/performance/2026-10-26-binding-final.tsv
```

To reconstruct archives, keep the committed harness and frozen reference. Restore `lib.rs` and `bindings.rs` from `dd91f5bf9` for `before`; apply the conditional `bind_owned` branch for `owned-exact`; add direct UUID formatting for `format`; add direct UUID parsing for `after`. Build each with the default-release command and save Cargo's reported `binding_codecs` executable before changing the next stage. Compare consecutive archives with the same driver and eight runs each.
