# GIF decoder results: 2026-11-17

The native decoder leads the reference implementations overall in CPU cycles
and allocation calls for every measured output mode and corpus category. These
measurements cover 173 inputs and complete animations on Windows x64. Production
continues to forbid unsafe code and depends only on the local shared-image crate.

The changes replace the eagerly initialized 40 KiB LZW dictionary with a compact,
output-bounded table reused across frames and CLEAR codes, specialize short
phrase copies without exposing extra output, and deinterlace through row
permutation cycles with one saved row instead of a second full pixel frame.

## Measured results

CPU entries are geometric means of per-file native/reference thread-cycle
ratios, shown for two independent processes. Values below 1 favor native.
Allocation columns are total calls and requested bytes for one complete decode
of each of the 173 inputs, with native before reference. Requested bytes include
replacement sizes on reallocation and measure neither peak memory nor RSS.

| Comparison | CPU ratio: run 1 / run 2 | Allocation calls | Requested bytes |
| --- | --- | --- | --- |
| Retained indexed / `gif` | 0.5199 / 0.5134 | 2,943 / 3,805 | 85,093,846 / 89,773,621 |
| Streaming indexed / `gif` | 0.4889 / 0.4878 | 1,105 / 1,972 | 36,222,918 / 40,907,961 |
| Reusable RGBA / `gif` + `gif-dispose` | 0.4992 / 0.4952 | 1,482 / 3,009 | 170,417,386 / 185,973,897 |
| RGBA animation / `image` | 0.3628 / 0.3612 | 1,482 / 4,025 | 170,417,386 / 524,258,477 |

Streaming indexed decoding uses about 51% fewer cycles than `gif`; native RGBA
playback uses about 50% fewer cycles than `gif` + `gif-dispose` and 64% fewer
than `image`, using these per-file geometric means. Total allocation calls are
44% lower for indexed streaming, 51% lower against `gif` compositing, and 63%
lower against `image`. Native requests fewer bytes on all 173 inputs and fewer
allocation calls on 172 of 173 inputs in each comparison. The allocation-count
exception is the XMP file described below. Per-category ratios, totals, every
individual timing loss, and all raw rows are retained in the result files.

## Workload and output checks

The candidates were all 35 repository GIF assets, all 13 upstream sample GIFs,
and 128 deterministic generated GIFs. The generator uses both native and Rust
`gif` encoders. It covers palettes of 2 through 256 colors, tiny files, flat
colors, bands, random data, interlacing, short sub-blocks, local palettes,
transparent animations, subrectangles, delays, and disposal. Each generated file
is checked against its intended indexed frames before measurement.

Before timing every accepted input, the harness checks every frame's indexed
pixels, palette, rectangle, and controls against `gif`; the caller-owned
`read_into_buffer` path is checked as well. Complete transparent-background
canvases must match both `gif-dispose` paths and `image`, including frame delays.
No input is excluded for being slow. The three excluded upstream files are:

- `gifplayer-muybridge.gif`: missing mandatory LZW EOI.
- `moon_impact.gif`: malformed graphic control rejected by strict decoding.
- `set_hsts.gif`: background index outside the global palette.

Both complete timing runs and the allocation run account for exactly the same
173 accepted inputs. Every candidate's bytes and SHA-256 are recorded.

The retained `gif` comparison moves frames from its owning iterator instead of
cloning pixels. Streaming `gif` comparisons use caller-owned pixel storage
through `read_into_buffer`. Native streaming recycles returned frame buffers.
RGBA compositor modes reuse canvases; `image` supplies owned animation frames.
Native APIs also retain extension payloads and original sub-block boundaries,
which differ from reference metadata and ownership policies.

## Known exceptions and limits

`beacon_xmp.gif` remains slower in every output mode. Native retains every
original sub-block; `gif` exposes selected metadata in a flat representation.
Native allocation calls are 95 retained, 92 streaming, or 94 with compositing,
versus 18, 15, or 16 for the corresponding `gif` modes and 19 for `image`.
Native still requests fewer bytes. Lossless metadata retention is preserved.

Two generated 32-color random files have small indexed timing losses in one run
each: retained native-encoded input is 0.73% slower in run 2, and streaming
reference-encoded input is 0.05% slower in run 1. Each wins in the other run.
The summaries retain these cases without replacing or discarding measurements.

Results use Rust's release profile with LTO and one codegen unit, pinned to
logical CPU 0 on an Intel Xeon E5-2696 v4. Each mode has five batches lasting at
least 40 ms; wall-time and cycle medians are selected independently. Allocation
measurements use a separate calibrated System-forwarding binary, with timing
intentionally omitted. Every timed decode includes destruction of its outputs.

The shared host had unrelated background compilation. An earlier incomplete
attempt failed allocation while an unrelated ITGmania trace used about 69 GiB
private memory; that attempt is discarded in full. After the trace exited, the
affected input passed an 80 ms rerun and both full runs completed. Run-start free
physical memory was about 48 GiB. CPU-cycle evidence and repetitions bound the
conclusions on this host; other CPU and OS combinations remain unmeasured.

## Validation and retained evidence

- Windows x64: 23 tests passed; one optional external-oracle test ignored.
- Windows x86: the same 23 tests passed.
- ARM64 Windows: production library check passed.
- Strict Clippy passed for the library and all benchmark targets/features;
  formatting checks passed. The production dependency tree has no external crates.
- Independent giflib 5.2.2 oracle: 73 cases, 64 independent encoder rewrites,
  and 256 generated frames passed. An isolated Cargo test wrapper avoided
  unrelated root-workspace Git dependency resolution on WSL.
- ASan: 100,000 decode and 100,000 raw-LZW executions passed with native-crate
  feedback. Compiler invocations and compiled object sections verify that
  references and harnesses retained ASan without contributing coverage feedback.
  The source hashes captured by this campaign still match the decoder.

[Reproduction commands](README.md) describe corpus generation and measurement.
[Measurement provenance](results/2026-11-17/measurements.json) records compiler,
source, binary, input and artifact hashes and exact accepted/excluded input sets.
[Run 1](results/2026-11-17/summary-1.json) and
[run 2](results/2026-11-17/summary-2.json) contain all per-file data and subgroup
summaries. [Verification](results/2026-11-17/verification.json) links test logs.
The [fuzz manifest](results/2026-11-17/fuzz/manifest.json), logs, initial seeds,
final corpora and exact campaign source snapshot are retained alongside them.
PowerShell 5 decorates stderr as `NativeCommandError` even for successful native
commands; the original logs are retained and successful exit codes are recorded.
