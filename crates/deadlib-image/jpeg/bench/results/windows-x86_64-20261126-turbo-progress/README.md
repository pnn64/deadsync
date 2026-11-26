# JPEG decode optimization against libjpeg-turbo

## Progressive AC refinement

Deadlib tracks nonzero AC positions in zigzag order across successive
approximation scans. A refinement scan uses the mask to find the next zero
coefficient and to visit only existing coefficients that carry correction
bits. The scratch costs eight bytes per block where the decode memory budget
can accommodate it. The direct refinement path covers tight budgets and
hierarchical frames.

The pinned libjpeg-turbo decoder selects a separate routine per progressive
scan type and reads all progressive scans before supplying output rows. Its AC
refinement routine walks the remaining coefficient band after end-of-band.
Separate phase measurements on three Kodak progressive files put Turbo's
preload at 12.4–15.8 million cycles and scanline delivery at 2.8–3.3 million
cycles. This identified entropy decoding as the larger optimization target.
[Turbo progressive entropy decoder](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/5b2e0e9269f1fff760d867be97b7ba5f79188227/src/jdphuff.c)
· [Turbo preload and scanline API](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/5b2e0e9269f1fff760d867be97b7ba5f79188227/src/jdapistd.c)

| Input set | Files | Deadlib / Turbo before | Deadlib / Turbo with occupancy | Change in Deadlib cycles |
| --- | ---: | ---: | ---: | ---: |
| Common images | 53 | 1.188× | 1.122× | −4.6% |
| Kodak progressive 4:2:0 | 24 | 1.224× | 1.106× | −7.8% |
| Kodak baseline 4:2:0 | 24 | 1.139× | 1.129× | +0.2% |
| Shipped game assets | 5 | 1.148× | 1.156× | −1.0% |

The common-image ratio was 1.125× and 1.120× in the two candidate runs.
Measurements use the default Turbo profile, packed RGB output, and CPU 2 on the
[original AVX2 comparison host](../windows-x86_64-20261126-contenders/README.md).
The sequence was before, candidate, candidate, before. Each run took the
median of five batches of at least 8 ms per file and codec. Per-file medians
across runs are summed before calculating the group ratio. All 583
verification rows agreed across the four runs.

The JPEG release suite passed, including the new tight-memory fallback test.
The candidate reproduced all 1,641 frozen native snapshots exactly, including
coefficients, pixels and errors under both input policies.

- [Common input manifest](decode-common.tsv) and [cycle summary](summary.json)
- [Before run 1](before-common1.tsv.gz) and [before run 2](before-common2.tsv.gz)
- [Candidate run 1](occupancy-common1.tsv.gz) and [candidate run 2](occupancy-common2.tsv.gz)

## 8-bit transform, marker search and entropy

Turbo's accurate AVX2 IDCT works with packed integer lanes. Deadlib preserves
its floating-point output and uses a shorter AAN transform when the nonzero
coefficients fit in the upper-left 4×4. About 36% of the sampled Kodak blocks
have nonzero AC confined to that region, in addition to 8% DC-only blocks.
The AVX2 output packs and reorders groups of four rows. Its marker search
examines 32 bytes at a time and uses the scalar search for the tail. Huffman
word refills use one alignment shift. Progressive corrections process
the set correction bits from a single buffered word when they fit.
[Turbo accurate AVX2 IDCT](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/5b2e0e9269f1fff760d867be97b7ba5f79188227/simd/x86_64/jidctint-avx2.asm)

| Input set | Files | Deadlib / Turbo before | Deadlib / Turbo after |
| --- | ---: | ---: | ---: |
| Common images | 53 | 1.103× | 1.036× |
| Kodak progressive 4:2:0 | 24 | 1.054× | 0.975× |
| Kodak baseline 4:2:0 | 24 | 1.149× | 1.097× |
| Shipped game assets | 5 | 1.186× | 1.134× |

Both builds used the same `--turbo-only` harness, at the same CPU affinity and
with the same batch size as above. The sequence was before, candidate,
candidate, before. The per-run candidate ratios were 1.037× and 1.035×
overall. Across the four runs, Deadlib used 1.6% fewer raw cycles while the
Turbo control used 4.8% more cycles in the candidate runs. On game assets the
raw Deadlib cycles increased 3.2% and the control increased 8.0%. These
variations make small inter-build raw changes uncertain.

Output and acceptance matched on all 412 Deadlib/Turbo verification rows in
the full corpus. The candidate reproduced all 1,641 exact native snapshots.
All 189 release unit and integration tests passed.

- [Paired cycle summary](pair-summary.json)
- [Before pair 1](pair-occupancy1.tsv.gz) and [before pair 2](pair-occupancy2.tsv.gz)
- [Candidate pair 1](pair-quarter1.tsv.gz) and [candidate pair 2](pair-quarter2.tsv.gz)

## Sequential refill, row stores and nearest sampling

The sequential entropy path refills to the Huffman lookup width and requests
larger lookahead for long codes or separate magnitudes. Turbo likewise keeps
Huffman lookup and magnitude refill checks separate. The AVX2 inverse transform
writes eight rows after one span check. Nearest-neighbor sampling advances the
source coordinate by a constant step and uses a shift for power-of-two widths.
[Turbo sequential entropy decoder](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/5b2e0e9269f1fff760d867be97b7ba5f79188227/src/jdhuff.c)

| Input set | Paired files | Deadlib / Turbo before | Deadlib / Turbo after |
| --- | ---: | ---: | ---: |
| All comparable rasters | 157 | 1.027× | 0.996× |
| Common images | 53 | 1.054× | 1.038× |
| Kodak progressive 4:2:0 | 24 | 1.009× | 0.981× |
| Kodak baseline 4:2:0 | 24 | 1.099× | 1.095× |
| Shipped game assets | 5 | 1.127× | 1.134× |
| Kodak 4:1:1 nearest | 1 | 2.164× | 1.317× |
| Kodak 4:1:0 nearest | 1 | 2.537× | 1.475× |

The two candidate full-corpus runs were 0.986× and 1.006× across the 157
paired rasters. Deadlib used 3.7% fewer raw cycles across those pairs.
The common-image measurements varied from 1.026× to 1.050×. All 412
verification rows agreed across the four runs. All 1,641 exact snapshots
matched and all 189 release tests passed. An explicit AVX2 RGB conversion
trial increased the common-image ratio and was reverted.

- [Full-corpus cycle summary](direct-summary.json)
- [Before run 1](direct-qualify-quarter1.tsv.gz) and [before run 2](direct-qualify-quarter2.tsv.gz)
- [Candidate run 1](direct-qualify-direct1.tsv.gz) and [candidate run 2](direct-qualify-direct2.tsv.gz)

## Fused chroma interpolation and RGB output

The AVX2 row kernel interpolates half-width chroma and writes packed RGB
directly to the output vector. It reuses the native integer formulas and
single-rounding rule for bilinear interpolation. The existing scalar row
kernel covers the final partial group and portable dispatch.

| Input set | Paired files | Deadlib / Turbo before | Deadlib / Turbo after |
| --- | ---: | ---: | ---: |
| Common images | 53 | 1.035× | **0.985×** |
| Kodak baseline 4:2:0 | 24 | 1.073× | 0.970× |
| Kodak progressive 4:2:0 | 24 | 0.989× | 0.977× |
| Shipped game assets | 5 | 1.118× | 1.024× |
| All comparable rasters | 157 | — | **0.934×** |

The two common-image runs were 0.9848× and 0.9856×. The two full-corpus
runs were 0.9357× and 0.9332× on the 157 paired rasters. Each file and
codec used five batches of at least 12 ms with each benchmark process pinned
to CPU 2. The common-image cycle ratio uses the same input and Turbo build in every
run. Deadlib used 4.8% fewer raw cycles than its parent on the common set.
The common-image and full-corpus aggregates both passed the 1% cycle target.

All 1,641 native snapshots matched exactly. Decode acceptance, output shape
and pixel agreement matched the previous milestone across all 412 full-corpus
verification rows. All 190 release tests, 50 forced-portable tests and strict
Clippy checks passed. The boundary test covers packed-group edges and scalar
tails.

- [Cycle summary and binary hashes](fused-summary.json)
- [Before common run 1](fused-trial-common-direct1.tsv.gz) and [before common run 2](fused-trial-common-direct2.tsv.gz)
- [Fused common run 1](fused-trial-common-fused1.tsv.gz) and [fused common run 2](fused-trial-common-fused2.tsv.gz)
- [Fused full run 1](fused-trial-inputs-fused1.tsv.gz) and [fused full run 2](fused-trial-inputs-fused2.tsv.gz)
