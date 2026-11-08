# Indexed textures and color keying

Three safe Rust changes reduce texture preparation CPU work. Baseline:
`f5a7daa09`. Workspace version: `0.5.1874` to `0.5.1875`.

1. **Indexed PNG expansion.** Eight-bit indices go directly from decoder bytes
   through the palette into RGBA pixels. This removes coordinate iteration,
   row-offset arithmetic, and bit unpacking from that path. Packed one-, two-,
   and four-bit indices use row slices, with the mask computed once. Palette
   alpha and the first-matching-entry color-key rule are unchanged.
2. **Indexed BMP row writes.** Select the destination row once, then expand
   indices directly into its four-byte pixel slots. This removes coordinate
   construction and `put_pixel` from the inner loop. One-, four-, and eight-bit
   unpacking, padded rows, top-down/bottom-up orientation, palette bounds errors,
   and OS/2 header handling are unchanged.
3. **Color-key blocks.** Compare native-endian RGBA words in blocks of sixteen
   pixels, skipping writes when a block has no matching key. Matching blocks
   use whole-word masks instead of conditional per-pixel stores. This preserves
   exact alpha matching, top/bottom-edge selection of off-pink, partial blocks,
   empty images, and backing bytes outside the logical image dimensions.

The execution paths follow `M-HOTPATH` and `M-THROUGHPUT` in
[rust-performance.md](../../rust-performance.md). No production helper,
dependency, cache, or `unsafe` block was added. The touched production module
had no existing `unsafe` to replace. The added dispatch and block scan remove
repeated work; this is an execution simplification, with a net addition of
26 production lines. Four further lines register the test module.

The production release assembly confirms SSE2 comparisons (`pcmpeqd`) and
masked pixel writes (`pandn`), with unaligned vector loads/stores (`movdqu`).
Both the block scan and its write loop vectorize from safe Rust. This codegen
check uses the production `panic=abort` profile with LTO disabled.

## Measurements

The table pools three fresh confirmation processes, giving 27 samples per
implementation per workload. CPU cycles and images/second are pooled medians
per complete operation; cycle reduction is the median of the three runs'
paired reductions. Throughput is the median ratio of the 27 matched sample
pairs. These summaries need not equal ratios of pooled medians. Allocation
counts and requested bytes are unchanged for every workload; the final column
applies to both implementations. Reallocations are also unchanged: one to eight
per PNG decode, depending on the workload, and zero for BMP and color keying.

| Workload | Cycles before | After | Paired reduction | Images/s before | After | Paired throughput | Allocations / bytes (both) |
|---|---:|---:|---:|---:|---:|---:|---:|
| png_1bit_64 | 387,637 | 379,757 | 2.2% | 5,631.9 | 5,769.2 | 1.02x | 13 / 53,852 |
| png_1bit_512 | 2,992,926 | 2,390,794 | 18.7% | 731.5 | 914.2 | 1.23x | 15 / 1,236,188 |
| png_1bit_1024 | 8,390,398 | 6,652,145 | 23.9% | 260.8 | 328.8 | 1.31x | 14 / 4,869,220 |
| png_2bit_64 | 284,450 | 288,105 | 2.6% | 7,676.7 | 7,599.4 | 1.03x | 13 / 54,892 |
| png_2bit_512 | 2,837,125 | 2,212,747 | 20.1% | 770.2 | 987.0 | 1.25x | 14 / 1,391,732 |
| png_2bit_1024 | 8,973,621 | 7,071,752 | 21.9% | 243.9 | 309.0 | 1.28x | 15 / 4,979,948 |
| png_4bit_64 | 265,167 | 258,867 | 3.5% | 8,235.5 | 8,468.5 | 1.03x | 13 / 57,036 |
| png_4bit_512 | 3,282,655 | 2,744,957 | 17.3% | 665.3 | 796.5 | 1.20x | 15 / 1,703,244 |
| png_4bit_1024 | 10,028,527 | 8,144,218 | 18.1% | 218.2 | 268.4 | 1.23x | 14 / 5,207,364 |
| png_8bit_64 | 324,893 | 310,823 | 7.2% | 6,737.6 | 7,055.4 | 1.07x | 13 / 65,356 |
| png_8bit_512 | 3,579,628 | 2,587,883 | 28.0% | 611.5 | 844.6 | 1.39x | 14 / 1,803,204 |
| png_8bit_1024 | 12,322,840 | 8,638,104 | 31.1% | 177.4 | 253.2 | 1.47x | 14 / 5,668,092 |
| bmp_1bit_64 | 229,268 | 205,408 | 10.9% | 9,562.5 | 10,671.8 | 1.12x | 4 / 24,796 |
| bmp_1bit_512 | 3,740,917 | 2,183,059 | 40.1% | 584.8 | 999.8 | 1.68x | 4 / 1,057,044 |
| bmp_1bit_1024 | 13,739,515 | 7,659,507 | 44.2% | 159.2 | 285.6 | 1.79x | 4 / 4,202,836 |
| bmp_4bit_64 | 233,153 | 207,428 | 10.7% | 9,403.4 | 10,587.1 | 1.12x | 4 / 24,820 |
| bmp_4bit_512 | 3,710,977 | 2,344,820 | 38.1% | 588.6 | 931.2 | 1.59x | 4 / 1,057,236 |
| bmp_4bit_1024 | 14,338,881 | 8,434,496 | 40.2% | 152.5 | 259.1 | 1.72x | 4 / 4,203,220 |
| bmp_8bit_64 | 222,858 | 199,284 | 12.2% | 9,833.3 | 10,947.5 | 1.11x | 4 / 24,852 |
| bmp_8bit_512 | 3,375,724 | 2,109,955 | 37.9% | 648.0 | 1,036.3 | 1.61x | 4 / 1,057,492 |
| bmp_8bit_1024 | 12,511,094 | 7,196,648 | 41.7% | 174.7 | 303.6 | 1.72x | 4 / 4,203,732 |
| key_random_64 | 3,753 | 2,535 | 25.8% | 600,600.6 | 896,861.0 | 1.36x | 0 / 0 |
| key_mixed_64 | 4,697 | 3,633 | 22.4% | 476,190.5 | 621,118.0 | 1.29x | 0 / 0 |
| key_off_pink_64 | 4,500 | 3,457 | 23.1% | 500,000.0 | 653,594.8 | 1.31x | 0 / 0 |
| key_solid_64 | 5,059 | 3,655 | 25.0% | 442,477.9 | 619,195.0 | 1.34x | 0 / 0 |
| key_random_512 | 420,342 | 366,576 | 13.4% | 5,206.7 | 5,972.1 | 1.14x | 0 / 0 |
| key_mixed_512 | 2,270,804 | 432,174 | 81.3% | 960.7 | 5,001.6 | 5.26x | 0 / 0 |
| key_off_pink_512 | 2,515,163 | 444,564 | 81.8% | 871.2 | 4,905.4 | 5.36x | 0 / 0 |
| key_solid_512 | 494,797 | 414,943 | 13.3% | 4,431.6 | 5,284.2 | 1.17x | 0 / 0 |
| key_random_1024 | 1,954,648 | 1,695,166 | 11.4% | 1,120.8 | 1,288.5 | 1.13x | 0 / 0 |
| key_mixed_1024 | 9,595,311 | 2,151,440 | 76.0% | 228.1 | 1,016.5 | 4.20x | 0 / 0 |
| key_off_pink_1024 | 8,925,572 | 2,197,382 | 75.6% | 245.1 | 995.1 | 4.10x | 0 / 0 |
| key_solid_1024 | 2,685,056 | 2,346,521 | 9.8% | 811.7 | 933.5 | 1.12x | 0 / 0 |

The initial unchanged benchmark ran before any production edit. PNG was then
changed and measured on its own. Fresh unchanged BMP and color-key measurements
preceded their respective edits. The first color-key candidate wrote every
pixel with a mask; it was rejected after the 1024-square no-match control used
11% more cycles. The retained block scan avoids those unnecessary writes.
The [JSON evidence](2026-10-23-indexed-pixels.json) contains every campaign,
including that rejected candidate, individual samples, source hashes, frozen
reference verification hashes, final binary hashes, and confirmation ranges.

Environment and measurement scope:

- Intel Xeon E5-2696 v4, Windows x86_64, Rust 1.99.1; release optimization level
  three, LTO disabled, static MSVC runtime, default target CPU. The benchmark
  worker is pinned to logical CPU 2. Builds and benchmarks run sequentially.
- The existing benchmark helper records `QueryThreadCycleTime` scheduled thread
  cycles, including kernel work. These are not retired-instruction PMU counts.
  Rates use elapsed time. Each process alternates original/current order for
  ten rounds and discards the first. Every batch performs twenty operations.
- PNG/BMP timings include warm file reads, decoding, output construction, and
  destruction. File generation is excluded. PNG inputs use deterministic packed
  patterns and a palette with duplicate pink entries; BMP inputs use deterministic
  packed patterns with padded row support. BMP performance inputs are bottom-up.
  Keying inputs cover random pixels without matches, pseudorandom half-key
  mixtures, off-pink mixtures, and solid key colors.
- Keying input clones are prepared outside measurement. Each batch holds twenty
  independent images, so the larger cases include substantial memory traffic.
  Timings include output destruction. Allocation counters cover current-thread
  Rust `System` traffic, excluding prepared inputs, native allocations, and RSS.
  Both variants use the same counter instrumentation and paths.
- Desktop activity and host scheduling are uncontrolled. Unchanged comparisons
  show several-percent variation; the smallest mixed-key cases vary much more
  despite identical source, so their absolute gains should be treated cautiously.
  These are texture-processing measurements, with no whole-game FPS claim.

## Correctness and reproduction

All three frozen function bodies match `f5a7daa09` after renaming references and
applying identical rustfmt formatting. New differential comparisons cover twenty
generated PNGs across all indexed bit depths, thirty BMPs across all supported
indexed depths and both row orientations, and eighteen committed PNG/BMP files,
each with color keying enabled and disabled. They also compare truncated BMPs
and invalid palette-index errors. Forty-five color-key cases cover dimensions,
patterns, empty images, one-pixel axes, and partial blocks; explicit expectations
check alpha-sensitive matching and extra backing bytes.

The full asset/version suites pass **105 tests**, with five opt-in benchmarks
ignored by the normal run. The new benchmark is run explicitly. Existing asset
tests also compare native bitmap captures, PNG/GIF reference output, fixed-point
resize rounding, and texture hint combinations. Clippy completes with four
existing production warnings and three warnings in frozen references, including
one copied into the new reference. New test files pass rustfmt, and the complete
diff passes whitespace checks.

```powershell
cargo test -p deadlib-assets -p deadsync-version --release --locked --offline -j 2 --config profile.release.lto=false -- --test-threads=1
cargo clippy -p deadlib-assets --release --locked --offline -j 2 --config profile.release.lto=false --no-deps --all-targets
cargo test -p deadlib-assets --release --locked --offline -j 2 --config profile.release.lto=false --lib benchmark_indexed_pixels -- --ignored --nocapture --test-threads=1
cargo rustc -p deadlib-assets --release --locked --offline -j 2 --config profile.release.lto=false --lib -- --emit=asm
```

Run the benchmark three times to reproduce the confirmation sample count. Each
workload emits one `PIXEL_BENCH` JSON line. Set `DEADSYNC_INDEXED_BENCH` to `png`,
`bmp`, or `key` to isolate a change. Full local logs and intermediate binaries
are under `target/indexed-pixels`.
