# JPEG decoder performance: RGBA8

The optimized native decoder uses **28.3–29.3% fewer process cycles than
image 0.25.10** across the fixed 105-input workload in two fresh runs.
Throughput is **87.60–89.58 MP/s**, versus **62.79–63.30 MP/s** for image.
Against the starting native implementation, two interleaved comparisons
show **12.3–13.7% fewer cycles**. Four safe-Rust optimizations are retained.

This establishes an overall decoder advantage on the measured Windows AVX2
host. Progressive photographs are **near parity** with image, and individual
losses remain. This is a decoder/RGBA8 result; actual loader integration and
end-to-end loading measurements remain separate work.

[Raw evidence, identities and reproduction](results/windows-x86_64-20261119-rgba8/README.md)
cover every input and trial. The baseline production revision is
`9a6f4ebabe71934202b55df110dae03883ca512c`.

## Complete loading output comparison

Windows 11, Xeon E5-2696 v4, Rust 1.100.0 / LLVM 23.1.5, release/fat LTO,
one codegen unit, static MSVC runtime and logical CPU 0. The native adapter
uses compatible input handling, centered linear interpolation and owned RGBA8
conversion. The image adapter uses its actual `into_rgba8` implementation.

Each value is the median of five batches of at least 80 ms. Cycles include
all process threads; `jpeg-decoder` uses one Rayon worker. No builds, tests or
corpus hashing overlapped timed decoding. Allocation instrumentation is separate.
The 105 inputs contain 109,965,477 pixels; corpus totals weight each
image once. MP/s is total pixels divided by the sum of per-image median times,
not a mean of per-image throughput values.

| Decoder | Billion cycles, run 1 | Billion cycles, run 2 | MP/s, run 1 | MP/s, run 2 |
| --- | ---: | ---: | ---: | ---: |
| Native, optimized | 2.691 | 2.751 | 89.58 | 87.60 |
| image 0.25.10 | 3.808 | 3.839 | 63.30 | 62.79 |
| zune-jpeg 0.5.15 adapter | 2.911 | 2.973 | 82.78 | 81.07 |
| jpeg-decoder 0.3.2 adapter | 3.510 | 3.531 | 68.64 | 68.24 |

Native also uses 7.4–7.6% fewer cycles than the direct zune adapter and
22.1–23.3% fewer than the jpeg-decoder adapter. The direct zune adapter requests
Luma/RGB and uses the common RGBA conversion; it does **not** benchmark zune's
direct RGBA output option. The primary integration comparison is image.

## Measured changes

The five frozen retained stages were interleaved for every input, with version
order rotated per file and reversed in round two. Each paired timing uses five
batches of at least 60 ms. The focused figures below come from those complete
105-file paired runs, comparing each change with its immediate predecessor.

| Change | Focus | Million cycles, round 1: before → after | Million cycles, round 2: before → after | Fewer cycles |
| --- | --- | ---: | ---: | ---: |
| CMYK/YCCK chunks | 4 CMYK photos | 103.21 → 59.52 | 106.10 → 64.15 | 39.5–42.3% |
| Direct vertical interpolation | 2 real 4:4:0 artworks | 141.10 → 49.98 | 143.84 → 51.93 | 63.9–64.6% |
| Huffman prefix and AC reuse | 24 progressive photos | 514.93 → 465.98 | 543.10 → 492.09 | 9.4–9.5% |
| Direct progressive traversal | 24 progressive photos | 465.98 → 440.32 | 492.09 → 459.37 | 5.5–6.6% |

- [CMYK/YCCK conversion](../src/reconstruct.rs) uses 64-pixel chunks and exact
  8-bit rounded multiplication. This removes per-pixel general color dispatch
  and wide division. Exhaustive channel/black pairs verify the original
  rounding under both ink polarities; YCCK retains the existing YCbCr transform.
- [Vertical interpolation](../src/raster.rs) directly blends contiguous rows
  when horizontal sampling already has full width. It removes redundant
  horizontal coordinates and weights, with constant denominators and one final
  rounding step. Nearest/full-row copies use the direct path.
- [Huffman decoding](../src/entropy.rs) reuses its fetched nine-bit prefix and
  skips the first nine impossible lengths after a lookup miss.
  [Progressive initial AC scans](../src/progressive.rs) reuse the existing
  combined run/magnitude lookup without changing lookahead or range checks.
- [Progressive scan traversal](../src/decode.rs) selects the component and tables
  once and visits visible blocks directly within each padded storage row. This
  removes per-block component/sampling loops, coordinate division and coding
  dispatch. Restart, differential prediction and final scan checks are preserved.

No input regressed by more than 5% against the starting native decoder in
**both** paired rounds. Native allocation calls and requested bytes are
unchanged for every input after every retained stage.

Three candidates were measured and removed: MCU coordinate caching improved
the progressive subset by only 0.27–0.43%; a left-aligned bit buffer increased
progressive cycles by 6.5%; packing the canonical and AC lookup tables increased
them by 4.3–4.6% in two paired runs. The packed table was restored to the
qualified source, and all 35 recorded source/harness hashes were rechecked.
Their measurements and cumulative trial patches remain in the archive.

## Corpus groups and individual exceptions

Ratios are native/image process cycles; below 1 favors native. Inputs were
selected before timing. Nineteen runtime artworks are local to the measured
host; the separate 86-input repository/public subset is identified in the
archive and must not be presented as the same workload.

| Group | Inputs | Native/image, run 1 | Native/image, run 2 |
| --- | ---: | ---: | ---: |
| Shipped assets | 5 | 0.817 | 0.814 |
| User artwork | 19 | 0.645 | 0.662 |
| Kodak baseline 4:2:0 | 24 | 0.829 | 0.815 |
| Kodak progressive 4:2:0 | 24 | 0.995 | 0.993 |
| Kodak CMYK | 4 | 0.756 | 0.763 |
| Kodak grayscale | 4 | 0.251 | 0.255 |
| Kodak encoded RGB | 4 | 0.822 | 0.809 |
| Small compatibility fixtures | 21 | 0.556 | 0.559 |

Progressive photographs moved from 17.0% more cycles than image in the initial
baseline run to 0.5–0.7% fewer in the final runs. That small final margin should
be treated as parity. Native is faster on **96/105** inputs in run one and
**93/105** in run two. These eight inputs are numerically slower in both:

| Input | Native/image, run 1 | Native/image, run 2 |
| --- | ---: | ---: |
| Wendy font texture | 1.0960 | 1.0644 |
| kodim01 progressive | 1.0223 | 1.0285 |
| kodim13 progressive | 1.0222 | 1.0208 |
| kodim05 progressive | 1.0198 | 1.0286 |
| kodim08 progressive | 1.0151 | 1.0243 |
| kodim14 progressive | 1.0144 | 1.0150 |
| kodim24 progressive | 1.0141 | 1.0194 |
| kodim20 progressive | 1.0341 | 1.0000 |

The font texture is the only input more than 5% slower in both reference runs
(6.4–9.6%). The progressive-photo differences are small; kodim20 is effectively
tied in the second run. Every per-file value, including losses that occurred
in only one run, is retained in the raw TSVs.

The workload contains five shipped images, nineteen stratified user artworks,
twenty-one small compatibility fixtures and sixty Kodak JPEG derivatives.
All 24 Kodak photographs appear as baseline and progressive 4:2:0 at quality 85;
photos 01, 06, 13 and 23 also appear as grayscale, CMYK and encoded RGB.
Six 4:1:0/4:1:1/multiscan diagnostics remain outside performance totals because
of known reference interpolation/scan differences, but are included in the
exact native behavior checks. Higher-precision and uncommon JPEG processes
are correctness coverage here, not part of these throughput claims.

## Allocations

One decode per input after warmup, using the separate calibrated instrumented
binary. Requests include allocation, zeroed allocation and replacement sizes
for reallocation; bytes are cumulative requests, not peak live memory or RSS.

| Decoder | Requests across 105 decodes | Requested bytes |
| --- | ---: | ---: |
| Native, optimized | 575 | 886,668,015 |
| image 0.25.10 | 2,238 | 914,963,997 |
| zune-jpeg 0.5.15 adapter | 1,816 | 893,284,211 |
| jpeg-decoder 0.3.2 adapter | 244,519 | 1,903,489,315 |

The four optimizations change CPU work, not allocation behavior. Native's
575 requests and 886,668,015 requested bytes match the starting implementation
on every file.

## Exact behavior and checks

The original native binary captured **1,641 input cases**: 675 fixture, asset
and photo inputs, plus 966 deterministic truncation/entropy-mutation cases.
Every candidate matched the original serialized results under ten API/policy
combinations per input. Comparisons include pixels, precision/sample variants,
coefficients, quantizers, ordered metadata, scan/consumed counts and exact
errors, using both input policies and both pixel upsamplers.

The snapshot helper also passed nine positive/negative checks, including
changed-input and corrupt/truncated/extended-snapshot rejection. Public default
resource limits are captured; stricter limits and stateful abbreviated
decoding remain covered by integration tests. Reference maximum-difference
and RGBA RMSE diagnostics are unchanged for all 315 input/reference pairs;
exact native equality is established independently of reference JPEG rounding.

Validation on the retained source:

- **190 tests passed**, no failures or ignored tests, with normal dispatch.
- **190 passed** with forced portable dispatch.
- **55 focused tests passed** on i686, including color, entropy, compatibility,
  progressive scans, raster output and padded component grids.
- Strict Clippy passed for the crate and benchmark workspace; all changed Rust
  sources passed rustfmt.
- New coverage includes exhaustive CMYK rounding, fractional vertical sampling,
  and 8/12-bit noninterleaved progressive scans with right/bottom block padding
  and restart intervals 3, 4 and 7.

No new production unsafe code or dependencies were introduced. Performance
was measured on this one AVX2 host; portable/i686 checks establish correctness,
not performance on other physical CPUs.

The historical 31-input raw-output measurements and earlier sanitizer campaigns
remain under `results/`. They use different scopes and are not substitutes
for the current 105-input RGBA8 measurements.
