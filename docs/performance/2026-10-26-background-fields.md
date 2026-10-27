# Background filename and color fields

Three safe Rust changes streamline background metadata parsing: search long filename fields at dots, encode normalized hex keys directly, and borrow ASCII color fields without constructing separator-replacement Strings. The runtime source baseline is `4d346b0af`; concurrent unrelated commits and work remain intact. The excluded `deadlib-lua` and `deadlib-lua-legacy` crates are outside this change.

Isolated release CPU throughput improves **5.748×** for paths around 128 bytes, **1.817×** for direct RGB key encoding, and **1.483×** for borrowed RGB inputs. Combined RGB normalization reduces 512 allocations plus 256 reallocations to 256 allocations and zero reallocations per 256-field batch, with requested bytes falling from 4,352 to 2,048 (**52.9%**). These measurements cover CPU kernels and in-memory compositions.

The production diff is **39 inserted / 8 removed lines** across `bgchanges.rs` and `changes.rs`, plus a four-line test-module registration. Extra source preserves the original short-input and unusual-input behavior while common paths perform fewer repeated checks, formatting calls, and allocations. No dependencies, manifests, release settings, shared benchmark helpers, or production unsafe code change.

## Changes and exact behavior

1. `bgchange_field_rejects_non_media` keeps the existing scalar loop below 32 bytes. Longer fields use the existing memchr dependency to find dots, then compare only the following three bytes. Matching remains case-insensitive ASCII substring matching: `.INI`, `.xmlExtra`, and `file.ini.png` are still rejected. Byte slices keep Unicode paths and dots near EOF safe.
2. `normalized_background_lua_color` converts the already-quantized RGBA bytes into uppercase hex digits directly in an eight-byte stack buffer, then appends those bytes to the result. This removes three or four generic hex-formatting calls. The leading `#`, opaque-alpha omission, output bytes, and String capacities stay identical. The output grows to eight bytes for RGB or sixteen for RGBA, removing one intermediate growth reallocation per valid key while preserving final capacities. RGB/RGBA decoding is unchanged, including legacy plus signs and malformed Unicode behavior.
3. Both background activation color parsing and cache-key color normalization use a small shared input helper. ASCII fields are borrowed directly because the parser already accepts comma and caret separators. Non-ASCII fields keep the original replacement String, preserving malformed Unicode hex diagnostics, including the normalized diagnostic text. Rounding, clamping, uppercase output, omission of opaque alpha, and output String capacities are unchanged. This removes the temporary input copy on ordinary metadata, including invalid ASCII fields.

## Isolated CPU measurements

Eight alternating process pairs for each cumulative change. A batch processes **256 fields**, except composed workloads, which parse a **128-record tag**. Values are medians across processes. CPU throughput is the ratio of before/after thread CPU cycles. The paired range is the minimum and maximum ratio for matching run numbers, not a confidence interval. All workloads and unchanged controls remain in the TSVs.

### Filename scan only

| Workload | Cycles/batch before → after | ns/batch before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `media/short` | 6,741.05 → 6,825.08 | 3,059.38 → 3,148.43 | 0.988× | 0.929–1.061× |
| `media/path128` | 50,992.61 → 8,870.88 | 23,260.93 → 4,150.78 | 5.748× | 4.691–6.662× |
| `media/path1024` | 354,177.00 → 34,324.32 | 162,437.50 → 15,200.00 | 10.319× | 9.433–11.331× |
| `media/early` | 2,723.17 → 2,961.53 | 1,231.25 → 1,339.07 | 0.920× | 0.741–1.342× |
| `media/late` | 48,653.55 → 7,826.55 | 22,164.06 → 3,554.69 | 6.216× | 5.154–6.991× |
| `media/dotted` | 71,731.92 → 70,773.32 | 32,782.81 → 32,364.06 | 1.014× | 0.916–1.131× |
| `media/unicode` | 9,855.21 → 9,105.83 | 4,478.12 → 4,234.38 | 1.082× | 0.874–1.293× |
| `media/tiny` | 1,430.19 → 1,447.33 | 640.62 → 650.00 | 0.988× | 0.741–1.095× |

Raw data: [scan comparison](2026-10-26-background-fields-scan.tsv), 416 records. Short filenames and early matches exercise the original loop; longer paths and dotted directory names exercise the dot search.

The short-name and tiny-input medians are both 0.988× in the isolated scan comparison; early rejection is 0.920×. Their paired ranges cross 1. The length gate can add fixed work on short inputs, while the long-path improvements hold in every process pair. No gain is claimed for the dotted-directory control.

### Direct key encoding added after the scan

| Workload | Cycles/batch before → after | ns/batch before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `normalize/rgb` | 232,536.22 → 127,992.51 | 106,453.12 → 58,723.43 | 1.817× | 1.670–2.107× |
| `normalize/rgba` | 317,396.98 → 194,549.04 | 145,154.68 → 89,014.07 | 1.631× | 1.394–1.746× |
| `normalize/upper` | 236,778.77 → 127,450.62 | 108,117.18 → 58,123.44 | 1.858× | 1.657–2.018× |
| `normalize/invalid` | 61,634.92 → 61,453.14 | 28,462.50 → 28,014.06 | 1.003× | 0.778–1.141× |
| `normalize/caret` | 415,853.06 → 277,811.53 | 189,921.88 → 126,910.94 | 1.497× | 1.359–1.615× |
| `normalize/comma` | 401,918.22 → 279,396.07 | 183,660.93 → 127,687.50 | 1.439× | 1.298–1.543× |
| `normalize/mixed` | 404,672.27 → 286,214.27 | 185,296.88 → 130,554.68 | 1.414× | 1.356–1.556× |
| `normalize/unicode` | 315,442.09 → 198,695.52 | 144,845.31 → 90,665.62 | 1.588× | 1.527–1.636× |
| `compose/keys` | 823,824.75 → 668,706.81 | 376,668.75 → 306,887.50 | 1.232× | 1.003–1.477× |

Raw data: [key encoding comparison](2026-10-26-background-fields-format.tsv), 416 records. The normalized RGB, RGBA, and uppercase batches vary one component through every byte value. Caret/comma/mixed/Unicode inputs include decimal parsing before encoding. Invalid colors return before encoding and serve as a control. All direct parser measurements remain unchanged controls in the raw data.

### Borrowed ASCII color inputs added after direct key encoding

| Workload | Cycles/batch before → after | ns/batch before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `normalize/rgb` | 130,729.40 → 88,125.83 | 59,603.12 → 40,082.81 | 1.483× | 1.273–2.024× |
| `normalize/rgba` | 193,557.85 → 157,652.47 | 88,528.12 → 71,889.07 | 1.228× | 1.162–1.355× |
| `normalize/upper` | 133,363.38 → 83,578.04 | 60,998.43 → 38,085.93 | 1.596× | 1.331–1.798× |
| `normalize/invalid` | 60,962.68 → 15,584.52 | 27,750.00 → 7,076.56 | 3.912× | 3.292–4.860× |
| `normalize/caret` | 288,416.14 → 243,473.53 | 131,509.38 → 111,207.82 | 1.185× | 1.070–1.280× |
| `normalize/comma` | 273,164.33 → 241,600.90 | 124,809.37 → 110,448.43 | 1.131× | 1.040–1.342× |
| `normalize/mixed` | 281,652.82 → 243,435.80 | 128,907.82 → 111,242.18 | 1.157× | 1.040–1.358× |
| `normalize/unicode` | 200,815.08 → 204,268.77 | 91,868.75 → 93,470.31 | 0.983× | 0.857–1.104× |
| `compose/keys` | 661,902.37 → 668,377.50 | 302,793.75 → 305,306.25 | 0.990× | 0.861–1.117× |

Raw data: [input-copy comparison](2026-10-26-background-fields-copies.tsv), 416 records. `normalize/unicode` keeps the original allocation path. Normalization includes parsing, byte quantization, uppercase formatting, allocation of the returned key, and destruction of owned results.

Every isolated process pair improves the valid ASCII normalization kernels. The Unicode fallback is effectively unchanged at 0.983×. This change alone does not establish a composed-key improvement: `compose/keys` is 0.990× with a paired range crossing 1. The combined original/final composed-key measurement improves in every process pair.

## Combined original versus final

Twelve alternating process pairs; [all 624 records](2026-10-26-background-fields-final.tsv):

| Workload | Cycles/batch before → after | ns/batch before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `media/short` | 7,576.18 → 6,926.26 | 3,435.94 → 3,111.72 | 1.094× | 0.823–1.193× |
| `media/path128` | 52,650.84 → 9,224.15 | 24,014.06 → 4,185.94 | 5.708× | 4.363–6.945× |
| `media/path1024` | 374,645.31 → 35,421.81 | 171,500.00 → 15,968.75 | 10.577× | 8.478–11.659× |
| `media/early` | 3,023.26 → 3,213.61 | 1,369.53 → 1,468.75 | 0.941× | 0.722–1.103× |
| `media/late` | 52,657.71 → 8,490.19 | 24,023.43 → 3,854.68 | 6.202× | 5.543–7.442× |
| `media/dotted` | 74,307.61 → 72,294.39 | 34,163.28 → 33,115.62 | 1.028× | 0.924–1.285× |
| `media/unicode` | 10,319.93 → 9,877.50 | 4,800.00 → 4,535.94 | 1.045× | 0.836–1.337× |
| `media/tiny` | 1,383.88 → 1,526.21 | 619.53 → 684.38 | 0.907× | 0.610–1.447× |
| `color/rgb` | 12,307.44 → 13,171.71 | 5,594.53 → 5,997.66 | 0.934× | 0.784–1.150× |
| `normalize/rgb` | 236,826.77 → 88,396.75 | 107,971.88 → 40,346.87 | 2.679× | 2.489–3.202× |
| `color/rgba` | 15,171.23 → 15,059.76 | 6,912.50 → 6,900.78 | 1.007× | 0.847–1.231× |
| `normalize/rgba` | 320,411.72 → 150,995.40 | 146,554.68 → 68,943.75 | 2.122× | 1.882–2.720× |
| `color/upper` | 13,084.26 → 13,476.97 | 5,975.78 → 6,231.25 | 0.971× | 0.875–1.367× |
| `normalize/upper` | 237,365.25 → 87,549.61 | 108,412.50 → 40,076.56 | 2.711× | 2.389–3.188× |
| `color/invalid` | 11,105.33 → 10,656.03 | 5,075.78 → 4,841.41 | 1.042× | 0.855–1.304× |
| `normalize/invalid` | 65,606.49 → 15,584.50 | 29,946.88 → 7,076.57 | 4.210× | 2.924–5.227× |
| `color/caret` | 79,140.03 → 78,447.24 | 36,078.12 → 35,789.85 | 1.009× | 0.870–1.145× |
| `normalize/caret` | 420,277.33 → 246,265.30 | 192,503.12 → 112,885.93 | 1.707× | 1.557–1.985× |
| `color/comma` | 76,612.36 → 82,375.95 | 34,994.54 → 37,589.85 | 0.930× | 0.786–1.171× |
| `normalize/comma` | 424,461.56 → 243,219.71 | 194,887.50 → 111,192.18 | 1.745× | 1.516–2.013× |
| `color/mixed` | 77,418.33 → 78,529.55 | 35,459.38 → 35,809.38 | 0.986× | 0.856–1.342× |
| `normalize/mixed` | 424,488.98 → 241,782.67 | 193,887.50 → 110,921.88 | 1.756× | 1.549–1.914× |
| `color/unicode` | 64,217.47 → 65,985.48 | 29,403.12 → 30,182.82 | 0.973× | 0.815–1.056× |
| `normalize/unicode` | 324,908.00 → 206,995.35 | 148,579.68 → 94,867.19 | 1.570× | 1.287–1.829× |
| `compose/media` | 516,113.07 → 528,418.75 | 235,512.50 → 241,656.25 | 0.977× | 0.851–1.334× |
| `compose/keys` | 857,312.06 → 664,399.06 | 392,581.25 → 303,493.75 | 1.290× | 1.109–1.455× |

`compose/media` splits the 128-record tag into owned fields, checks the first filename, and parses both color columns. `compose/keys` splits the same tag and normalizes both color columns as cache keys. These are in-memory compositions of the actual production functions. They do not measure directory enumeration, file resolution, Lua execution, chart loading end to end, or application startup. Unchanged controls can shift with shared-machine conditions and executable layout; smaller changes whose paired range crosses 1 are not established improvements.

## Allocations

One separate untimed batch is probed with the existing thread-local System allocator wrapper. Tracking is disabled during timing. Inputs and reusable fixture storage are prepared outside the probe. Owned output construction and destruction are included. Requested bytes count cumulative allocation/reallocation requests; peak live bytes measure requested sizes, excluding allocator bookkeeping. Allocation counters are identical in every process for each workload/version.

| Workload | Allocations before → after | Reallocations before → after | Requested bytes before → after | Peak live bytes before → after |
| --- | ---: | ---: | ---: | ---: |
| `normalize/rgb` | 512 → 256 | 256 → 0 | 4,352 → 2,048 | 8 → 8 |
| `normalize/rgba` | 512 → 256 | 511 → 255 | 8,944 → 6,128 | 16 → 16 |
| `normalize/upper` | 512 → 256 | 256 → 0 | 4,352 → 2,048 | 8 → 8 |
| `normalize/invalid` | 256 → 0 | 0 → 0 | 1,792 → 0 | 7 → 0 |
| `normalize/caret` | 512 → 256 | 512 → 256 | 10,496 → 6,144 | 16 → 16 |
| `normalize/comma` | 512 → 256 | 512 → 256 | 10,496 → 6,144 | 16 → 16 |
| `normalize/mixed` | 512 → 256 | 512 → 256 | 10,496 → 6,144 | 16 → 16 |
| `normalize/unicode` | 512 → 512 | 256 → 0 | 6,656 → 6,144 | 16 → 16 |
| `compose/media` | 1,409 → 1,409 | 0 → 0 | 47,289 → 47,289 | 47,289 → 47,289 |
| `compose/keys` | 1,921 → 1,665 | 384 → 128 | 54,713 → 51,385 | 47,305 → 47,305 |

All `media/*` and `color/*` batches allocate zero bytes before and after. The scan leaves every allocation counter unchanged. Direct key encoding removes one growth reallocation per valid key; its output capacities are unchanged. The input-copy change removes one allocation per nonempty ASCII color field; the non-ASCII fallback keeps its input copy. Returned keys retain their original capacities. Full counters, frees, and freed-byte totals are recorded in the TSVs. RGBA output still grows once from eight to sixteen bytes; RGB output has no growth reallocations.

## Method and reproducibility

- Windows x86-64 MSVC; Intel Xeon E5-2696 v4 at 2.20 GHz, 44 logical processors. Rust 1.99.1 (`58cf8f9c3`), LLVM 23.1.1. Measured builds use the unchanged release profile: optimization level 3, LTO enabled, and static CRT.
- Four fresh unit-test executables contain the same fixed harness: `before`, `media`, `format`, and `after`. Each adjacent pair isolates one change. The combined comparison reuses the first and last archives. The baseline source differs from the baseline commit only by registration of the new test module.
- The shared helper pins the benchmark thread to logical processor 2, discards one warm sample, and selects the median of 11 measured samples by `QueryThreadCycleTime`. The corresponding `Instant` sample supplies ns/batch. The outer driver alternates process order and records full executable SHA-256 digests. No compilation from this pass overlaps retained sampling. External machine activity is uncontrolled.
- Fixture setup stays outside timing and allocation probes. `black_box` protects inputs and outputs. A measured sample repeats media batches 64 times (1,024-byte paths: 8), color batches 64, normalization batches 32, and composed tags 8.
- Filename batches cover ordinary short paths, paths around 128 and 1,024 bytes, early/late rejected substrings, dotted directory names, Unicode names, and tiny inputs. Hex fixtures cover every byte in the varying channel; decimal fixtures use caret, comma, and mixed separators. The Unicode decimal fixture has Unicode whitespace. Each composed record contains an RGB hex color and a caret-delimited RGBA color.

Local builds, source snapshots, raw process logs, source audits, and validation logs are under `target/background-fields-20261026-4d346b0af/`. Four archive digests:

| Stage | SHA-256 |
| --- | --- |
| `before` | `897bacf3b5a954e23bd3978643fd04147b5521b3d8620369ed13ede93e9c0995` |
| `media` | `7dae284316d118b73ac1f9dda72bee98138ad34391b078849f2594b96b6106f7` |
| `format` | `a50b5cad1fe2491bb52e15cc512d1b82af87c26a6ce6b1a85457d3b4a93a97bc` |
| `after` | `f64aac9684720261c059b4c6e346f996c003812c1a152a9c00d9f8ca7dd1d7be` |

## Correctness and checks

The new [harness](../../crates/deadsync-simfile/src/changes/background_fields.rs) compares production functions against [frozen baseline functions](../../crates/deadsync-simfile/src/changes/background_fields_original.rs). All four ordinary differential tests pass in every measured archive. The [public-API integration test](../../crates/deadsync-simfile/tests/background_color_keys.rs) checks every byte in each RGBA channel in both color columns: distinct colors keep separate cache owners, equivalent hex/caret inputs merge into two ordered activations, and stored color bits stay exact. The fixture resolves a temporary Lua filename without executing Lua. They check ASCII case combinations and embedded non-media matches at every offset through 129, deterministic generated paths with NUL/Unicode/dots, every byte at each color component, upper/lowercase hex and Unicode whitespace, every ASCII mutation in RGB/RGBA hex text, malformed Unicode slice outcomes and panic payloads, exact float bit patterns, mixed/empty separators, infinities/NaN/signed zero, quantization half-step boundaries, normalized String capacities, and both columns of a composed tag.

The full baseline archive unit suite passes **273 tests, 0 failed, 20 ignored**. The source audit reconstructs both complete baseline files from every archived source stage, verifies the frozen reference against `4d346b0af` after rustfmt normalization, checks fixed harness/helper hashes, matches final runtime bytes to the measured source snapshot, and verifies all **1,872 retained records** against archive digests and invariant allocation counts.

Final validation:

- Release tests for `deadsync-simfile` and `deadsync-chart`: **326 passed, 0 failed, 28 ignored**.
- Debug tests for `deadsync-simfile`: **276 passed, 0 failed, 20 ignored**.
- Scoped rustfmt for the three edited/new functions and all three new test files.
- Scoped Clippy with `--no-deps --all-targets` completes successfully. It reports style/dead-code warnings in existing code and repeated loading of the shared kernel benchmark helper by multiple test modules, including the new harness. It reports no diagnostics in the runtime edits or new test function bodies.

Broad release tests and Clippy disable LTO with `--config profile.release.lto=false` to reduce link time. All measured archives use default LTO-enabled release builds. Ignored tests are opt-in benchmarks; the new ignored benchmark is explicitly executed in every retained sampling process.

To reproduce, keep the new test harness and frozen reference fixed. Restore the two runtime files from `4d346b0af` and retain only the test-module registration for the original stage. Build the library unit-test executable from Cargo's JSON `compiler-artifact` output; archive it and run ordinary differential checks. Apply the dot search, direct key encoding, then borrowed input edits cumulatively, archiving after each stage.

```powershell
cargo test --release --locked -p deadsync-simfile --lib --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_background_fields --log-dir target/background-fields-20261026-4d346b0af/runs --before target/background-fields-20261026-4d346b0af/before/simfile.exe --after target/background-fields-20261026-4d346b0af/media/simfile.exe --output docs/performance/2026-10-26-background-fields-scan.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_background_fields --log-dir target/background-fields-20261026-4d346b0af/runs --before target/background-fields-20261026-4d346b0af/media/simfile.exe --after target/background-fields-20261026-4d346b0af/format/simfile.exe --output docs/performance/2026-10-26-background-fields-format.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_background_fields --log-dir target/background-fields-20261026-4d346b0af/runs --before target/background-fields-20261026-4d346b0af/format/simfile.exe --after target/background-fields-20261026-4d346b0af/after/simfile.exe --output docs/performance/2026-10-26-background-fields-copies.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_background_fields --log-dir target/background-fields-20261026-4d346b0af/runs --before target/background-fields-20261026-4d346b0af/before/simfile.exe --after target/background-fields-20261026-4d346b0af/after/simfile.exe --output docs/performance/2026-10-26-background-fields-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile -p deadsync-chart -- --test-threads=1
cargo test --locked -p deadsync-simfile -- --test-threads=1
cargo clippy --no-deps --release --locked --config profile.release.lto=false -p deadsync-simfile --all-targets
```

Use fresh output paths when reproducing. Compute medians per version/workload from the TSVs, divide median cycles for throughput, and use matching run numbers for the paired minimum/maximum.
