# INI and default keymap serialization

Three safe Rust changes remove repeated formatter work and UUID nibble lookups. In isolated comparisons, ordinary string INI lines improve **1.686×**, saving 16 UUIDs for each input backend improves **1.343×**, and writing the fixed default keymap section into a reused buffer improves **48.807×**. Creating that section in a new String improves **20.946×** and removes seven growth reallocations. These are CPU kernel measurements; they do not measure application startup or disk latency.

The runtime source baseline is `c54347f4c` for the three edited files. The pass leaves concurrent unrelated work intact, including Lua work. No changes are made to `deadlib-lua`, `deadlib-lua-legacy`, dependencies, release settings, or the shared benchmark helpers.

## Changes and behavior

1. **Append the INI key directly.** `writer::push_line` appends the string key and `=` before formatting the value once. This removes formatting dispatch for the key and delimiter. The value still receives the original default formatter options. Keys, values, newlines, partial output, the panic payload on a failing `Display`, and the single invocation of `Display` are preserved.
2. **Look up both UUID hex digits together.** `append_uuid_hex` uses a compile-time 256-entry table of ASCII byte pairs. Each UUID byte now selects one pair instead of performing two nibble lookups and two separate assignments. All 16 pairs fill the existing 32-byte stack buffer, followed by the existing checked UTF-8 conversion and bulk append. An input byte always indexes 0–255; every table entry contains lowercase ASCII hex. The table declares 512 bytes of read-only data and requires no runtime initialization or heap allocation. UUID decoding, list sizing, backend ordering, and native pad-order handling remain unchanged.
3. **Append the fixed default keymaps as one block.** A private macro generates both the existing public 32-entry defaults array and a `concat!` string from the same tuple literals. `write_default_keymap_ini_section` appends the resulting 766-byte block once, removing its loop and 32 runtime formatting calls. The header, row order, empty values, per-row newlines, and final blank line are identical. Custom keymap serialization is unchanged. The constant adds precomputed serialized bytes; the macro keeps the source values in one declaration.

The production diff is **27 inserted / 11 removed lines** across three files. The extra source builds compile-time data; execution loses formatter calls and repeated digit extraction. Public signatures, default tuple contents, and ordering are unchanged. No production unsafe code is added or removed.

## Isolated measurements

Each row is a median across eight alternating before/after process pairs. Throughput is before/after thread CPU cycles; greater than 1 is faster. The paired range is the minimum and maximum ratio among process pairs, not a confidence interval. Raw TSVs retain every workload, including unchanged controls.

### INI line writer only

| Workload | Cycles/op before → after | ns/op before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `lines/strings` | 16,609.11 → 9,852.64 | 7,600.39 → 4,478.91 | 1.686× | 1.485–1.977× |
| `lines/unicode` | 15,739.69 → 10,198.17 | 7,164.06 → 4,650.39 | 1.543× | 1.369–1.812× |
| `lines/owned` | 19,243.97 → 13,515.54 | 8,775.00 → 6,164.84 | 1.424× | 1.254–1.721× |
| `lines/numbers` | 20,102.26 → 14,444.13 | 9,161.33 → 6,585.55 | 1.392× | 1.268–1.575× |
| `lines/floats` | 80,753.71 → 71,035.68 | 36,893.75 → 32,396.09 | 1.137× | 0.996–1.404× |
| `compose/lines` | 24,645.74 → 21,073.72 | 11,236.72 → 9,631.25 | 1.170× | 0.983–1.214× |
| `compose/defaults` | 26,067.34 → 25,993.60 | 11,892.19 → 11,857.03 | 1.003× | 0.837–1.158× |
| `roundtrip/ini` | 43,248.35 → 43,440.42 | 19,673.43 → 19,759.38 | 0.996× | 0.707–1.191× |

Raw data: [writer comparison](2026-10-26-ini-serialization-writer.tsv), 368 records. `lines/*` writes 256 rows per operation. The main string and integer workloads improve in every process pair. The float range reaches 0.996×, so its smaller median improvement has less consistent evidence. Full composition with defaults and the INI round trip are effectively unchanged at this step.

### UUID encoder added after the writer

| Workload | Cycles/op before → after | ns/op before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `save/empty` | 1,131.80 → 1,201.25 | 510.55 → 521.49 | 0.942× | 0.790–1.294× |
| `save/one` | 1,987.51 → 1,984.92 | 912.11 → 913.67 | 1.001× | 0.810–1.396× |
| `save/sixteen` | 6,606.43 → 4,919.89 | 3,029.30 → 2,235.55 | 1.343× | 1.017–1.557× |
| `save/cap` | 21,572.74 → 15,992.63 | 9,947.65 → 7,293.75 | 1.349× | 1.259–1.492× |
| `compose/lines` | 23,491.65 → 22,855.43 | 10,689.84 → 10,425.00 | 1.028× | 0.933–1.116× |
| `compose/defaults` | 25,914.72 → 24,026.68 | 11,796.09 → 10,955.47 | 1.079× | 0.782–1.276× |
| `roundtrip/ini` | 44,582.51 → 43,553.61 | 20,275.00 → 19,828.13 | 1.024× | 0.844–1.154× |

Raw data: [UUID encoder comparison](2026-10-26-ini-serialization-uuid.tsv), 368 records. A save processes all six backends, each holding the stated number of UUIDs; `cap` means 64 per backend. The 16- and 64-UUID cases improve in every process pair. One UUID per backend is effectively unchanged. The empty save never enters the encoder; its 0.942× median is an unchanged control and is retained in the data.

### Default keymap block added after the UUID encoder

| Workload | Cycles/op before → after | ns/op before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `defaults/reused` | 2,717.57 → 55.68 | 1,240.97 → 25.21 | 48.807× | 38.006–53.236× |
| `defaults/owned` | 4,559.56 → 217.68 | 2,083.98 → 98.48 | 20.946× | 15.674–27.720× |
| `compose/defaults` | 25,173.90 → 20,211.15 | 11,485.94 → 9,257.03 | 1.246× | 0.967–1.346× |

Raw data: [default keymap comparison](2026-10-26-ini-serialization-defaults.tsv), 368 records. One operation writes the entire 766-byte section. Both default-only workloads improve in every process pair. Unchanged controls also move in this build comparison: for example, `save/one` is 0.913× and `load/randomcap` is 0.898×. They do not call the edited default writer. Shared-machine conditions and executable layout can affect these short kernels; the raw controls remain available, and their shifts are not attributed to the changed code.

## Combined baseline versus all three changes

Twelve alternating process pairs; [all 552 records](2026-10-26-ini-serialization-final.tsv):

| Workload | Cycles/op before → after | ns/op before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `lines/strings` | 17,111.58 → 10,058.42 | 7,800.39 → 4,600.00 | 1.701× | 1.478–1.934× |
| `lines/unicode` | 16,834.62 → 9,426.49 | 7,667.58 → 4,298.44 | 1.786× | 1.441–2.100× |
| `lines/owned` | 20,243.73 → 13,273.75 | 9,217.19 → 6,066.80 | 1.525× | 1.258–1.971× |
| `lines/numbers` | 21,402.10 → 14,427.84 | 9,741.01 → 6,635.94 | 1.483× | 1.251–1.764× |
| `lines/floats` | 78,536.42 → 73,062.62 | 35,887.50 → 33,471.88 | 1.075× | 0.931–1.358× |
| `load/empty` | 1,301.57 → 1,166.95 | 588.28 → 526.17 | 1.115× | 0.707–1.383× |
| `save/empty` | 1,247.55 → 1,174.67 | 576.56 → 529.68 | 1.062× | 0.724–1.426× |
| `load/one` | 1,784.30 → 1,693.41 | 808.20 → 769.14 | 1.054× | 0.741–1.339× |
| `save/one` | 1,882.04 → 1,739.70 | 859.38 → 789.07 | 1.082× | 0.952–1.442× |
| `load/sixteen` | 13,765.91 → 13,657.87 | 6,273.83 → 6,216.80 | 1.008× | 0.951–1.238× |
| `save/sixteen` | 6,535.27 → 4,901.02 | 2,971.10 → 2,226.18 | 1.333× | 0.928–1.741× |
| `load/cap` | 76,330.26 → 74,821.21 | 34,907.81 → 34,129.69 | 1.020× | 0.927–1.092× |
| `save/cap` | 22,067.47 → 16,183.84 | 10,069.53 → 7,423.05 | 1.364× | 1.175–1.680× |
| `load/upper` | 14,553.02 → 13,694.74 | 6,622.26 → 6,239.84 | 1.063× | 0.880–1.186× |
| `load/mixed` | 10,607.17 → 10,345.65 | 4,825.00 → 4,720.70 | 1.025× | 0.859–1.238× |
| `load/invalid` | 12,018.49 → 11,611.20 | 5,480.07 → 5,291.41 | 1.035× | 0.856–1.262× |
| `load/random` | 14,377.25 → 13,457.24 | 6,507.81 → 6,147.26 | 1.068× | 0.933–1.269× |
| `load/randomcap` | 76,734.11 → 75,463.42 | 35,014.85 → 34,557.03 | 1.017× | 0.946–1.189× |
| `defaults/reused` | 2,760.39 → 55.47 | 1,265.22 → 25.09 | 49.768× | 42.696–55.145× |
| `defaults/owned` | 4,664.27 → 201.17 | 2,126.95 → 90.97 | 23.186× | 19.455–25.588× |
| `compose/lines` | 23,812.31 → 22,843.43 | 10,872.65 → 10,396.88 | 1.042× | 0.872–1.348× |
| `compose/defaults` | 26,899.03 → 22,121.48 | 12,266.41 → 10,108.60 | 1.216× | 1.059–1.343× |
| `roundtrip/ini` | 43,594.76 → 43,337.54 | 19,870.31 → 19,778.12 | 1.006× | 0.817–1.201× |

`compose/defaults` improves 1.216×, with every combined process pair above 1. Its operation loads the 16-UUID fixture, saves all six backend lists, renders an Options section, and appends the default keymaps. `compose/lines` omits the keymaps. `roundtrip/ini` renders the pad-order options, reparses the INI, and reloads the backend lists; it is effectively unchanged at 1.006×, and its paired range crosses 1. The `load/*` functions are unchanged controls. Their median shifts do not establish a decoder improvement.

## Allocations

Counts cover one separate untimed operation, including destruction of owned results. Every process for a version/workload reported identical counters. The benchmark wraps the System allocator only while probing allocations; timed repetitions run with tracking disabled. Requested bytes include the new size of every reallocation, so they are cumulative requests rather than net retained memory.

| Workload | Allocations before → after | Reallocations before → after | Requested bytes before → after | Peak live bytes before → after |
| --- | ---: | ---: | ---: | ---: |
| `lines/owned` | 1 → 1 | 10 → 10 | 18,423 → 18,423 | 9,216 → 9,216 |
| `save/empty` | 1 → 1 | 0 → 0 | 240 → 240 | 240 → 240 |
| `save/one` | 7 → 7 | 0 → 0 | 432 → 432 | 432 → 432 |
| `save/sixteen` | 7 → 7 | 0 → 0 | 3,402 → 3,402 | 3,402 → 3,402 |
| `save/cap` | 7 → 7 | 0 → 0 | 12,906 → 12,906 | 12,906 → 12,906 |
| `defaults/reused` | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| `defaults/owned` | 1 → 1 | 7 → 0 | 2,550 → 766 | 1,280 → 766 |
| `compose/lines` | 8 → 8 | 5 → 5 | 11,610 → 11,610 | 5,646 → 5,646 |
| `compose/defaults` | 8 → 8 | 6 → 6 | 11,800 → 11,800 | 5,726 → 5,726 |
| `roundtrip/ini` | 24 → 24 | 7 → 7 | 16,097 → 16,097 | 8,601 → 8,601 |

All reused line workloads and all load workloads allocate zero bytes. Allocation counters are identical before and after for all 22 workloads other than `defaults/owned`. That workload reduces cumulative requested bytes by **70.0%** (2,550 → 766) and peak live requested bytes by **40.2%** (1,280 → 766). Every allocation is freed; freed-byte totals match the cumulative requested-byte totals in the raw data. The composed defaults case already has sufficient growth capacity at the final append, so its allocation totals do not change.

## Method and fixtures

- Windows x86-64 MSVC; Intel Xeon E5-2696 v4 at 2.20 GHz, 44 logical processors. Rust 1.99.1 (`58cf8f9c3`), LLVM 23.1.1. Measurements use the repository's normal release build: optimization level 3, LTO enabled, and static CRT. No target-CPU, allocator, manifest, lockfile, or dependency changes are used.
- Four fresh archived integration executables contain the same fixed public-API harness: `before`, `writer`, `uuid`, and `after`. The three isolated comparisons are cumulative adjacent stages. The combined comparison reuses the original `before` and final `after` archives.
- The shared helper pins the benchmark thread to logical processor 2. Each process discards one warm sample and selects the median of 11 measured samples by thread CPU cycles from `QueryThreadCycleTime`; the associated `Instant` duration supplies ns/op. The tables then take medians across processes. The driver alternates process order per pair. No compilation from this pass overlaps retained measurements; external machine activity is uncontrolled.
- Inputs and reusable output buffers are prepared outside timing and allocation probes. `black_box` protects inputs and outputs. New output allocation and destruction remain inside the owned/save/compose kernels. Mutex-protected native pad-order calls remain inside load/save operations; those APIs operate on bounded in-memory lists without device I/O.
- ASCII and Unicode string fixtures contain 256 values; scalar fixtures contain 256 integers or floats. Per measured sample, line workloads run 128 operations (floats: 64), load/save workloads 128, reused defaults 4,096, owned defaults 1,024, compositions 64, and the INI round trip 32.
- Every pad-order fixture covers the same six backends. Lowercase valid lists have 0, 1, 16, or 64 entries. Other load controls use 16 uppercase entries, a mix of Unicode whitespace/duplicates/wrong lengths/invalid final digits, 16 invalid final digits, or deterministic varied UUID bytes at 16 and 64 entries. Pad-order fixture construction and INI parsing for the initial fixture are excluded from load timing; the explicit round-trip workload includes reparsing.

Local raw logs, builds, source snapshots, and audits are under `target/ini-serialization-20261026-c54347f4c/`. TSVs include full executable SHA-256 digests. The archive digests are:

| Stage | SHA-256 |
| --- | --- |
| `before` | `f724d769eb496e37923f92f0291b5a71bce70619617d7b10e3099c98d6ecf986` |
| `writer` | `f11bcfc6506021d16bb4f63318ede4204c0755dd2c357526397cc6349d7f49cf` |
| `uuid` | `2133b9b316e7fad7fdb943cac4339d3170819d7106ad45088ca10d88943f8bd9` |
| `after` | `4604ec850e3462719c6daf3caafb06d3e4a65d21f41582bc240e5bf4dbf8cadf` |

## Correctness and validation

The new [integration harness](../../crates/deadsync-config/tests/ini_uuid_kernels.rs) compares public production APIs against [frozen original code](../../crates/deadsync-config/tests/support/ini_uuid_original.rs). Four ordinary tests pass in **each of the four measured release archives**. They cover:

- INI keys and values containing empty text, `=`, embedded newlines, NUL and Unicode; varied prefixes and capacities; long values through 8,192 bytes; signed/unsigned extremes; signed zero, NaN, infinities and float extremes; booleans; and custom `Display` formatter options. A failing `Display` checks the original partial output, panic payload, and exactly one invocation.
- Every byte value at every UUID byte position; every ASCII mutation at every hex-text position; uppercase, Unicode whitespace, malformed lengths and Unicode substitutions; duplicate/invalid entries; all-zero UUIDs; counts through and beyond the 64-entry cap on all six backends; and 10,000 deterministic varied UUIDs. Tests compare decoded native state, every serialized row, Vec/String capacities for pad-order output, and INI round trips.
- Every byte of the default keymap block, varied existing prefixes/capacities, and repeated appends. The original 32 tuple literals are also audited against the baseline source.

The source audit reconstructs the complete original three runtime files by reversing just the three edited functions and the default declaration. It verifies the original decoder and surrounding load/save helpers are unchanged, validates frozen references against `c54347f4c`, hashes the fixed harness/helpers, matches final runtime bytes to the measured source snapshot, and checks all **1,656 retained records** against their archive digests. The owned default append is the sole allocation-counter change.

Final validation:

- Release tests for `deadsync-config`, `deadsync-input`, `deadlib-input-native`, and `deadsync-profile`: **641 passed, 0 failed, 19 ignored**.
- Debug tests for `deadsync-config`, `deadsync-input`, and `deadlib-input-native`: **402 passed, 0 failed, 15 ignored**.
- Scoped rustfmt passes for the edited functions/declaration and both new test files.
- Scoped Clippy with `--no-deps --all-targets` completes successfully. Its warnings refer to existing code in score/input/config/native modules and existing tests; there are no diagnostics in the edited functions/declaration or either new test file. The existing unused test-support imports are also reported by the test builds.

Broad release tests and Clippy use `--config profile.release.lto=false` to reduce link time. The archived performance builds use the unchanged default LTO-enabled release profile. Ignored tests are opt-in benchmark suites; the new ignored benchmark was explicitly executed by every retained measurement process.

## Reproduction

Keep the new integration harness and frozen reference fixed. For the original stage, restore only `push_line`, `append_uuid_hex`, `write_default_keymap_ini_section`, and the original `DEFAULT_KEYMAP_INI_LINES` declaration from `c54347f4c`. Build and archive the single `ini_uuid_kernels` test executable from Cargo's JSON `compiler-artifact` output. Apply the writer edit, encoder edit, then the macro/default-block edit in separate stages, rebuilding and archiving after each. Run ordinary checks on every archive with `--test-threads=1` before sampling.

```powershell
cargo test --release --locked -p deadsync-config --test ini_uuid_kernels --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_ini_uuid --log-dir target/ini-serialization-20261026-c54347f4c/runs --before target/ini-serialization-20261026-c54347f4c/before/config.exe --after target/ini-serialization-20261026-c54347f4c/writer/config.exe --output docs/performance/2026-10-26-ini-serialization-writer.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_ini_uuid --log-dir target/ini-serialization-20261026-c54347f4c/runs --before target/ini-serialization-20261026-c54347f4c/writer/config.exe --after target/ini-serialization-20261026-c54347f4c/uuid/config.exe --output docs/performance/2026-10-26-ini-serialization-uuid.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_ini_uuid --log-dir target/ini-serialization-20261026-c54347f4c/runs --before target/ini-serialization-20261026-c54347f4c/uuid/config.exe --after target/ini-serialization-20261026-c54347f4c/after/config.exe --output docs/performance/2026-10-26-ini-serialization-defaults.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_ini_uuid --log-dir target/ini-serialization-20261026-c54347f4c/runs --before target/ini-serialization-20261026-c54347f4c/before/config.exe --after target/ini-serialization-20261026-c54347f4c/after/config.exe --output docs/performance/2026-10-26-ini-serialization-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-config -p deadsync-input -p deadlib-input-native -p deadsync-profile -- --test-threads=1
cargo test --locked -p deadsync-config -p deadsync-input -p deadlib-input-native -- --test-threads=1
cargo clippy --no-deps --release --locked --config profile.release.lto=false -p deadsync-config -p deadsync-input -p deadlib-input-native --all-targets
```

Use fresh output destinations when reproducing so earlier retained measurements remain intact. Derive each table from the TSV records: median cycles and ns per version/workload, throughput as the ratio of cycle medians, and paired minimum/maximum as the ratios for matching run numbers.
