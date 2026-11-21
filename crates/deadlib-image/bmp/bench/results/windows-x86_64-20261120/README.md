# BMP optimization measurements, 2026-11-20

These measurements cover `deadlib-image-bmp` against `image =0.25.10` on the
recorded Windows host. They measure warmed, in-memory codec calls, including
allocation and destruction. They do not measure file I/O or application startup.
The production crate remains dependency-free and retains `#![forbid(unsafe_code)]`.

The original native source is the repository at `f755f2acd`; the standalone
benchmark and baseline regressions were committed as `8bf1d2d7a` before any
production optimization. Full native outputs were frozen before the first change.

## Final matrix results

The final historical `all` matrix has exactly the same **662 case/mode keys** as
the baseline. The consuming matrix contains 120 native cases and 98 admitted
image comparisons. All 22 other native cases remain independently checked and
are excluded from reference speed claims because their output differs.

| Pipeline | Recorded comparisons | Faster than image | Minimum image/native cycles | Geometric mean |
| --- | --- | --- | --- | --- |
| Existing borrowed RGBA API, final full matrix | 98 | 98 | 1.025106x | 2.193693x |
| Consuming RGBA API, final run 1 | 98 | 98 | 1.247209x | 2.451872x |
| Consuming RGBA API, confirmation run 2 | 98 | 98 | 1.298004x | 2.460663x |
| Native encoder, final full matrix | 84 | 84 | 1.34834x | 2.57097x |
| Repository BMP fixtures, consuming API | 12 | 12 | 1.664257x | 2.583789x |

Both complete consuming runs beat image in every admitted equivalent case.
Per-case data and summaries: [run 1](final-owned-r1.md),
[run 2](final-owned-r2.md), [borrowed matrix](final-before-after.md), and
[repository corpus](final-external-owned.md). The corpus includes the deliberately
misnamed `misnamed.png` BMP. The intentionally invalid `win8-unused-entry.bmp`
remains rejected with `pixel index exceeds the palette`; it is not timed.
External expectations come from current native decoding, so corpus comparisons
establish agreement with image while independent synthetic cases and frozen
replay establish preservation of the original native behavior.

These are geometric means of per-case cycle ratios, with each recorded case
equally weighted. They are not a throughput prediction for an application's
particular mix of images. The encoder comparison includes duplicate normalized
inputs from top/bottom variants; it does not represent 84 independent formats.

The closest borrowed result is medium bottom-up RGBA32, only 2.5% ahead of image.
The first final consuming run takes 177,171 cycles on that case versus image's
1,245,065 (7.03x). The consuming API avoids dependence on a separate RGBA copy.

### Representative complete pipelines

All rows below are bottom-up sources. Baseline uses the original borrowed
`decode().to_rgba8()` pipeline; final uses `decode().into_rgba8()` from
`final-owned-r1.tsv`. Thus the RGBA rows include the benefit of the new API as
well as decoder optimization. The historical borrowed-only comparison is
retained in [final-before-after.md](final-before-after.md), including every
observed regression.

| Input | Size | Baseline cycles, millions | Final cycles, millions | Image cycles, millions | Gain over baseline | Gain over image |
| --- | --- | --- | --- | --- | --- | --- |
| Indexed 1-bit | 1920x1080 | 26.529 | 12.588 | 22.865 | 2.107x | 1.816x |
| Indexed 8-bit | 1920x1080 | 23.852 | 10.580 | 27.965 | 2.254x | 2.643x |
| RGB24 | 1920x1080 | 26.920 | 21.407 | 33.637 | 1.258x | 1.571x |
| RGBA32 | 1920x1080 | 51.544 | 9.766 | 31.232 | 5.278x | 3.198x |
| BGRX32 | 1920x1080 | 42.328 | 22.030 | 44.870 | 1.921x | 2.037x |
| BGRA32, all-zero alpha | 1920x1080 | 54.681 | 17.269 | 43.860 | 3.166x | 2.540x |
| Indexed 1-bit | 511x257 | 1.898 | 0.679 | 0.846 | 2.797x | 1.247x |
| RGBA32 | 511x257 | 2.137 | 0.177 | 1.245 | 12.063x | 7.027x |

| Input | Size | Baseline MPixels/s | Final MPixels/s | Image MPixels/s |
| --- | --- | --- | --- | --- |
| Indexed 1-bit | 1920x1080 | 170.790 | 360.790 | 198.549 |
| Indexed 8-bit | 1920x1080 | 190.206 | 429.152 | 162.396 |
| RGB24 | 1920x1080 | 168.404 | 212.252 | 135.140 |
| RGBA32 | 1920x1080 | 87.903 | 463.984 | 144.774 |
| BGRX32 | 1920x1080 | 107.127 | 205.786 | 101.083 |
| BGRA32, all-zero alpha | 1920x1080 | 82.843 | 262.648 | 103.463 |
| Indexed 1-bit | 511x257 | 151.291 | 423.934 | 339.817 |
| RGBA32 | 511x257 | 134.482 | 1620.732 | 230.516 |

### Allocations

The existing APIs retain identical allocation calls and requested bytes across
all 662 baseline/final allocation rows. The new consuming API reduces two calls
to one and halves requested bytes in all 40 RGBA-storage cases; the other 80
cases keep their prior counts. The stack palette adds no heap allocation.

The following complete 1920x1080 pipeline cells show calls / requested bytes:

| Input | Baseline and final borrowed | Final consuming | Image consuming |
| --- | --- | --- | --- |
| Indexed 1-bit | 3 / 10,368,006 | 3 / 10,368,006 | 6 / 14,517,352 |
| Indexed 8-bit | 3 / 10,368,768 | 3 / 10,368,768 | 6 / 14,519,032 |
| RGB24 or BGRX32 | 2 / 14,515,200 | 2 / 14,515,200 | 3 / 14,515,320 |
| RGBA32 | 2 / 16,588,800 | 1 / 8,294,400 | 2 / 8,294,520 |
| BGRA32, all-zero alpha | 2 / 16,588,800 | 1 / 8,294,400 | 3 / 14,515,320 |

Source data: [final-all-allocations.tsv](final-all-allocations.tsv),
[final-owned-allocations.tsv](final-owned-allocations.tsv), and
[baseline-allocations.tsv](baseline-allocations.tsv).

## Environment and measurement rules

- Intel Xeon E5-2696 v4, 22 cores / 44 logical processors, Windows x86-64 MSVC.
- Rust 1.100.0 (`0ef1bc386`), LLVM 23.1.5; release optimization, LTO, one codegen
  unit, `panic=abort`, static CRT. Full metadata and hashes: [environment.txt](environment.txt).
- Benchmark thread pinned to logical CPU 2. CPU work uses
  `QueryThreadCycleTime`; throughput uses `Instant`. Modes rotate within each
  sample. Both implementations receive identical verified input bytes.
- Baseline, the first three stage matrices, and the final historical matrix use
  seven samples of at least 20 ms per mode. Focused refinements and final owned
  comparisons use nine samples of at least 40 ms per mode.
- Allocation counters run in a separate executable, so their atomic operations
  do not affect the timing executable. Requested bytes count allocation and
  reallocation requests; they are not peak live memory or RSS.
- No compilation or test job was intentionally run alongside timed samples.
  Results still describe this host and workload, not every CPU or BMP file.

The 120-case synthetic matrix covers 1x1, 17x9, 511x257 and 1920x1080, both row
orientations where supported, packed and byte palettes, short palettes, OS/2
headers, RGB24, 555/565/333 masks, RGBA4444, and common 32-bit layouts. Expected
pixels and metadata are generated independently of both decoders. Raw TSVs keep
median, minimum and maximum cycle counts and throughput for every measured case.

## Changes retained after measurement

| Change | Evidence from the isolated step | Removed work |
| --- | --- | --- |
| `b3833dc1a`: direct RGBA pixel stores | Medium RGB/indexed conversion about 1.50-1.61x faster; 1080p RGB about 1.22-1.25x and indexed about 1.32-1.36x | Per-pixel three-byte extension followed by a separate alpha push |
| `3c4baa4e7`: specialized masked rows | 1080p decode: RGB555 1.90x, RGB565 1.98x, BGRX32 3.11x, RGBA32 4.48x; ratios are geometric means of row orientations | Generic mask sampling for common layouts, dynamic pixel widths in the inner loop, unnecessary alpha reduction for explicit alpha |
| `a6fc9c4b5`: bulk palette unpacking and validation | Initial 1080p decode: indexed1 2.08x, indexed4 4.20x, indexed8 2.82x; measured word spreading then improved indexed1 decode another 8.85-15.1% across medium/large cases | Per-pixel depth dispatch, per-pixel validation branches for full encoded palettes, per-bit extraction in full bytes |
| `104f4644b`: consuming RGBA conversion | Focused standard/swapped RGBA32 pipeline 1.32-7.34x faster than the previous borrowed pipeline; all 16 pilot cases beat image | An entire allocation and RGBA buffer copy when the caller consumes an RGBA bitmap |
| `d446b651c`: RGBA palette lookup | At 1024 pixels, indexed conversion 1.37-1.77x faster depending on palette size; medium indexed1 conversion 1.74x, 1080p 1.32x | Three separate palette components and a variable palette bound in each large-image pixel conversion |

The one-bit unpacker spreads a byte into eight byte-sized indices with safe
`u64` shifts and masks. The palette conversion uses a fixed 1 KiB stack table
only for images with at least 1024 pixels. Measurements at 1023, 1024, 1025 and
4096 pixels checked that the table setup is worthwhile at that boundary. No
runtime SIMD dispatch, codec dependency, or unsafe production code was added.

The production diff replaces 49 lines and adds 215 across `decode.rs` and
`model.rs` (net +166). The added code is specialization, a consuming API and
documentation; the pixel loops perform fewer operations. This is not a claim
that total source length decreased.

## Before/after evidence

All datasets are retained, including regressions and intermediate results.
`analyze.py` checks the complete case/mode key sets and metadata before applying
filters. Ratios are before/after or image/native, so values above one are faster.

| Step | Before | After |
| --- | --- | --- |
| Direct RGBA stores | [baseline.tsv](baseline.tsv) | [stage1-rgba.tsv](stage1-rgba.tsv) |
| Masked row specialization | [stage1-rgba.tsv](stage1-rgba.tsv) | [stage2-masked.tsv](stage2-masked.tsv) |
| Bulk indexed rows | [stage2-masked.tsv](stage2-masked.tsv) | [stage3-indexed.tsv](stage3-indexed.tsv) |
| One-bit word expansion | [indexed1-before-refinement.tsv](indexed1-before-refinement.tsv) | [indexed1-after-refinement.tsv](indexed1-after-refinement.tsv) |
| Owned RGBA pipeline | [ownership-before.tsv](ownership-before.tsv) | [ownership-after.tsv](ownership-after.tsv) |
| Indexed palette conversion | [palette-before.tsv](palette-before.tsv) | [palette-after.tsv](palette-after.tsv) |
| Palette cutoff | [palette-threshold-before.tsv](palette-threshold-before.tsv) | [palette-threshold-after.tsv](palette-threshold-after.tsv) |

The first bulk-indexed matrix still lost to image on medium bottom-up indexed1
and RGBA32. Those results motivated the one-bit refinement, consuming API and
palette lookup. The tiny indexed conversion cases in the palette step cost
about 3-4% more (roughly 8-26 thread cycles), while their full decode/conversion
remained faster than image. At 1023 pixels, the unchanged direct path varied
within about 2%. These costs are retained in the raw data.

The unchanged borrowed RGBA copy also showed substantial allocation/host noise
in an early full matrix. The focused
[before](control-before-rgba4444-convert.tsv) /
[after](control-after-rgba4444-convert.tsv) control did not reproduce the large
reported regression. Encoder medians also vary; no encoder algorithm improvement
or universal before/after absence of regressions is claimed.

In the final historical matrix, 107 of 120 borrowed full-pipeline medians improve
over the baseline, with a geometric mean speedup of 1.92224x. Thirteen tiny cases
have slower medians; for example, 1x1 BGRA32 changes from 561.12 to 596.68 cycles.
Across all four native operations, 68 of 480 medians are slower. The raw analysis
lists them all. The final results establish the image performance gate on these
measured equivalent cases while retaining the small native regressions in the
report.

## Exact behavior and validation

- Frozen replay passed for all **22,570 records / 42,819,895 bytes**. It compares
  complete inputs and observable probe/decode/conversion/encoding results,
  including representation, palettes, sample bytes, errors, limits and encoded
  bytes in both orientations. The baseline snapshot hash is in `environment.txt`.
- **34 integration tests and one doctest passed in release mode.** Coverage
  includes every 16-bit input word for common masks, every packed index byte,
  partial rows, all palette sizes, invalid public models, alpha rules, limits,
  allocation reuse, and the palette table boundary.
- `cargo fmt -p deadlib-image-bmp --check` and the separate benchmark formatting
  check passed.
- Codec Clippy with all targets and benchmark Clippy with all targets/features
  passed with `-D warnings`.
- Review checked that owned conversion retains the borrowed API's validation
  and limit precedence, including that decoded pixels ignore the input-byte
  limit. Pointer/capacity tests verify actual RGBA storage reuse.

The owned helper adds assertions without changing the snapshot serialization.
The full snapshot and frozen baseline binaries are retained at
`D:/deadsync-bmp-perf-20261120/baseline/`; the generator is committed in the
benchmark workspace, so preservation checks can be recreated from the baseline.

## API and integration scope

Use `decode(bytes)?.into_rgba8()?` when ownership of the decoded bitmap is no
longer needed. This comparison matches image's consuming `into_rgba8` contract.
The existing borrowed `to_rgba8` still produces independent storage and is
measured separately. The two datasets must not be silently substituted.

Reference decode comparisons exclude inputs whose exact output differs:
image's 555/565 channel scaling and implicit mixed BGRA alpha can differ from
the native contract. Those native cases still pass independent expectations and
the frozen baseline replay; exclusion is based on output semantics, not speed.
Encoder comparisons independently require equal source samples and decoded
output pixels, while permitting different legal BMP headers. The TSV's
`image_equivalent` field describes original input decoding, so it must not be
used to discard otherwise validated `image-encode` rows.

The application already has a separate indexed BMP loader in
`crates/deadlib-assets/src/decode.rs`. Beating image does not establish that this
crate beats that specialized runtime path. No application integration was made
as part of this optimization task.

See [the benchmark README](../../README.md) for build/run commands, mode
definitions, allocation accounting and exact snapshot reproduction.
