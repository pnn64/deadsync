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
