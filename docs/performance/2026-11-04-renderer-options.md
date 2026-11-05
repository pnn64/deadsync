# Renderer options and dense slots, 2026-11-04

Three retained optimizations in `deadlib-render-core`, starting at `64f4ace17`
on isolated branch `perf/non-lua-kernels-20261104`. Lua and the main worktree's
existing edits are outside this batch.

## Changes and behavior

1. `BackendType::from_str` compares trimmed input against its ASCII aliases
   directly. It avoids allocating and freeing a lowercase copy on every parse.
   Alias order and platform `cfg` gates are retained. Invalid input still
   includes the original, untrimmed text and case in the same error message.
2. `PresentModePolicy::from_str` also compares trimmed aliases directly. On
   failure it builds the error once and lowercases that string in place. The
   fixed prefix/suffix are already lowercase; this produces exactly the old
   error text, including trimmed/lowercased input, without a temporary string.
3. `DenseSlotMap::get_slot` uses wrapping subtraction for the one-based handle.
   Zero becomes `u64::MAX`: narrower `usize` conversion rejects it, while on a
   64-bit target that index cannot be below any vector length. All nonzero
   handles keep the old index. The existing safe bounds check therefore also
   handles zero, removing the separate underflow branch. State, returned
   references, insertion/drain behavior and diagnostic formatting are unchanged.

All production changes use safe Rust, without new dependencies or state. The
source diff adds 59 and removes 14 lines. Explicit alias guards are more verbose
than a match on an allocated lowercase string; their execution removes that
intermediate allocation and normalization pass. Error creation uses its final
buffer directly, and slot lookup shares one bounds check.

## Final measurements

| Workload | Before cycles/256 | After cycles/256 | CPU throughput |
| --- | ---: | ---: | ---: |
| Renderer, valid aliases | 55,225.47 | 12,923.06 | 4.273x |
| Renderer, invalid input | 103,717.77 | 71,086.83 | 1.459x |
| Present policy, valid aliases | 51,397.54 | 11,806.25 | 4.353x |
| Present policy, invalid input | 107,009.69 | 77,509.77 | 1.381x |
| Dense slots, hits | 1,026.53 | 838.71 | 1.224x |
| Dense slots, mixed handles | 1,025.07 | 795.91 | 1.288x |

Per-256 parser allocation totals (each result is dropped in the batch):

| Parser/workload | Allocations before/after | Allocated bytes before/after | Peak bytes before/after |
| --- | ---: | ---: | ---: |
| Renderer, valid aliases | 256 / 0 | 2,386 / 0 | 14 / 0 |
| Renderer, invalid input | 448 / 256 | 18,688 / 16,384 | 79 / 64 |
| Present policy, valid aliases | 256 / 0 | 2,048 / 0 | 9 / 0 |
| Present policy, invalid input | 448 / 256 | 21,248 / 18,944 | 87 / 74 |

There are no parser reallocations. Frees equal allocations and freed bytes
equal allocated bytes. Valid parses and both slot workloads have zero heap
churn after the change. Invalid inputs retain one final error allocation per
call. Empty invalid inputs already had no lowercase allocation, explaining
the original mixed-invalid count of 448 rather than 512.

All five runs improve every measured workload. Paired throughput ranges:

- Renderer, valid aliases: 3.791-4.360x.
- Renderer, invalid input: 1.444-1.488x.
- Present policy, valid aliases: 3.760-5.298x.
- Present policy, invalid input: 1.363-1.450x.
- Dense slots, hits: 1.214-1.224x.
- Dense slots, mixed handles: 1.287-1.325x.

## Evidence and limits

Adjacent TSVs and manifests retain every measured stage:

- `baseline`: unchanged production methods with the first eight-case harness.
- `backend`: retained direct alias comparisons.
- `backend-stack`: discarded 14-byte stack normalization candidate. Valid parses
  measured 3.208x versus 4.825x for direct comparisons in the first workload.
- `policy`: retained direct policy comparisons and in-place error normalization.
- `dense-slot`: retained wrapping index calculation.
- `control`: exact pre-batch production sources with the final six workloads.
- `final`: all three changes, five process runs.
- `debug`: the complete test suite in the unoptimized profile.

The first backend workload used four common portable names. From `policy`
onward it cycles through all 15 aliases accepted on this Windows x86-64 target,
mixing lowercase, uppercase and Unicode/ASCII whitespace padding. Early stages
also included two unchanged YUV-validation controls; the final harness omits
those unrelated paths. Compare paired variants within a process, rather than
absolute cycle counts across stages.

The final unchanged-source control ranges from 0.999x to 1.073x; the largest
bias is for valid policy parsing. Slot controls are 0.999x/1.000x. Earlier
eight-case harnesses showed slot bias around 6-8%. These controls limit timing
precision, and the retained improvements exceed their respective differences.
Allocation counts are exact within the counted allocator and do not depend on
cycle calibration. Results describe these CPU kernels, not GPU throughput or
whole-application startup/frame-rate improvements.

## Method and reproduction

Windows x86-64, Rust 1.99.1 / LLVM 23.1.1; CPU identifier
`Intel64 Family 6 Model 79 Stepping 1, GenuineIntel`. The runner builds exact
crate sources in a small workspace with cached dependencies, optimization
level 3, LTO, no incremental compilation and the static MSVC runtime flag.
The `test-util` feature enables the existing frame-comparison suite as well.

Each process pins its benchmark thread to CPU 2, alternates variants for 12
rounds, discards the first and reports the median of 11 `QueryThreadCycleTime`
samples. Each sample contains 3,000 batches of 256 operations. The table uses
medians of five process runs. Fixture setup and allocation counting stay outside
timing. Both timed variants use the same thread-local counted system allocator;
counts include allocations, reallocations, frees and byte churn. Parser timing
includes result destruction, so temporary and returned
error strings are freed within the measured work. Slots use a warmed 1,024-entry
table and a deterministic access pattern; mixed inputs include zero and
out-of-range handles.

Manifests record source/executable/correctness-log SHA-256 hashes, compiler,
CPU, dependency lockfile, flags and commands. Exact sources, executables and
raw logs are archived at `D:\deadsync-renderer-options-20261104` on the
measurement machine. From the repository root in PowerShell:

```powershell
$env:DEADSYNC_RENDERER_OPTIONS_ARCHIVE = 'D:\deadsync-renderer-options-reproduction'
python scripts/benchmark_renderer_options.py control --baseline 64f4ace17e6b0d7438673dda26376a630fc08a60
python scripts/benchmark_renderer_options.py final --runs 5
python scripts/benchmark_renderer_options.py debug --debug
$env:RUSTFLAGS = '-C target-feature=+crt-static'
$env:TEMP = "$env:DEADSYNC_RENDERER_OPTIONS_ARCHIVE\tmp"
$env:TMP = $env:TEMP
cargo clippy --offline --manifest-path "$env:DEADSYNC_RENDERER_OPTIONS_ARCHIVE\workspace\Cargo.toml" --features test-util --all-targets --no-deps
```

Offline builds require cached dependencies. The manifest's embedded lockfile
can pin the recorded dependency resolution when recreating the workspace.

## Correctness and checks

All 29 tests pass in release and debug with `test-util`, including structural
and semantic frame comparisons, geometry resolution, sphere mapping and the
existing prepared-UV tests. New differential tests compare both parsers against
frozen originals across every ASCII case pattern of all known aliases, ASCII
byte mutations, Unicode substitutions, whitespace, controls, malformed and
long inputs. They compare exact error text and exercise unsupported platform
aliases on this target. Platform gates and alias-to-variant mappings are also
checked against the pre-batch source; non-native targets are not executed.

Dense-slot tests cover zero, ordinary and out-of-range handles, `u32`/`u64`
boundaries, replacement, partial drain, reuse, clear, zero-sized values and
diagnostic output. A `u32` conversion model checks the narrower-pointer index
boundaries. The slot change does not mutate the map or add an unchecked access.

Clippy completes with one existing unit-struct `Default` warning in the
unchanged dense-map constructor. Modified functions and new tests have no
warnings. The frozen constructor keeps its original spelling with that lint
allowed in the test module. Changed Rust passes rustfmt, the Python runner
compiles, and whitespace checks pass. The checks manifest records commands
and log hashes; all archived source, executable and correctness hashes were
verified, and final/debug sources match the measured working-tree files.
