# H.264 entropy decoding revision comparison (2026-11-28)

Windows x64, Xeon E5-2696 v4, logical CPU 42 (CPU 2 in repeat2), Rust 1.100.0. All measured binaries use identical direct rustc settings: opt-level=3, 16 codegen units for the decoder, default target CPU, std and surface, no LTO. QueryThreadCycleTime measures the pinned serial decoding thread; allocation counting runs separately. Each sample begins with fresh decoder and lazy tables. Allocation calls include alloc, alloc_zeroed and realloc; requested bytes count traffic rather than peak memory. Other processes were active on this host.

The ten video inputs include the three previous game samples, five independent native internet streams and two CAVLC transcodes of internet footage. The files, excerpts and sampled entropy traces are temporary and outside the repository. No sample bytes are included in this report or committed.

The pipeline includes NAL parsing, RBSP extraction, SPS parsing, decoding, display ordering, output consumption, draining and destruction. The surface mode uses SPS reorder depth; packed mode uses the default depth. Isolated benchmarks replay every 32nd call of each operation from an instrumented **baseline** decoder, with original alignment, context and up to 256 subsequent RBSP bytes. Replay setup is outside the measurement. Each batch repeats the trace 64 times (once in the cold batch). Both stages consume identical traces. This is a sampled operation replay, not an estimate of the percentage of total decode time.

Source revisions: pass0 = `a20d4ac56`; pass1 = `aa27ce695`; pass2 = `6c2ac0fc4`; pass3 = `0e4a1c4bc`.
The first pass0/pass1 full-decode comparison (without decodeonly in the filename) linked the replay code into the executable. It was retained as exploratory evidence. Repeat batches on two logical CPUs suggested a small SRPG9 regression; the dedicated decode-only harness excludes all replay code and is used in the decodeonly42 batches and every later comparison. The dedicated comparisons put CABAC control medians within the observed noise, with no established reproducible regression. This harness distinction is material; the initial samples are not deleted or silently substituted. trialbits is a rejected fixed-width bit-read prototype, reverted before the next change.

Cycle and throughput figures below are individual medians; percentage changes use medians of paired ratios and may differ from ratios of individual medians. Negative cycle and allocation changes are improvements. Paired ranges expose variance. Allocation counts and requested bytes are the absolute totals per batch, including lazy table initialization. Separate fresh-process allocation runs must agree. Timing and allocation signatures and operation counts must agree between stages. FFmpeg 7.1 (Gyan full build) provides independent output hashes.

The stack-residual change removes one heap allocation per nonempty residual block. The Exp-Golomb fast path consumes short codewords from a word and retains the original scalar fallback. The static LUT change packs coefficient entries into two bytes and eliminates the runtime table builders and OnceLock access. Its cold isolated cycle results are mixed, including regressions on Bunny and Africa, while the warm replay is effectively unchanged. The full pipeline sees reduced heap traffic and no reproducible cycle regression in the dedicated harness. The static tables increase the decode cycle executable from 1,656,832 to 1,835,008 bytes (+178,176 bytes). This is an allocation/code-simplicity tradeoff, not a claim that the isolated cold path is faster.

The cumulative pass0/pass3 surface batch was noisy on Bunny CAVLC (+14.85% paired median, range -10.64%..+46.13%); a separate 11-pair repeat was consistently faster (-11.04%, range -12.99%..-7.83%). Both are included. Packed output independently confirmed the improvement (-11.41%). The interlaced control repeat was +0.42% with mixed signs. Native CABAC control performance is considered inconclusive rather than claiming a speedup or a proven equivalence margin.

## External media provenance

Big Buck Bunny: (c) copyright 2008, Blender Foundation / www.bigbuckbunny.org, CC BY 3.0. Original internet files were used locally for testing only.
FFmpeg emitted truncated SEI diagnostics on the BBC Africa source, but the full visible-plane hashes matched on every accepted revision.

| Source file | Download URL | Bytes | SHA-256 |
|---|---|---:|---|
| bunny.mp4 | https://raw.githubusercontent.com/chthomos/video-media-samples/master/big-buck-bunny-1080p-30sec.mp4 | 22718509 | `07b756a4c7b481829776645c153167ca14c9df802ddbea7dffda3d817aa5261a` |
| africa.mov | https://samples.ffmpeg.org/V-codecs/h264/bbc-africa_m720p.mov | 8200479 | `2b700b90b727da228c4bf4ce76141a12c963593baa9987f8b02064d319dcb5b3` |
| nature.h264 | https://samples.ffmpeg.org/V-codecs/h264/nature_704x576_25Hz_1500kbits.h264 | 13339932 | `9688d400da159b7ef2d5b4fe31223502bc9f4505c5f4eeb4969da3e0efe763ea` |
| envivio.mp4 | https://samples.ffmpeg.org/V-codecs/h264/envivio-h264.mp4 | 519221 | `3f6a22ffe790d1570d09ad2e8f73fc432b8aa5486d59a956b98ae0452e543e31` |
| interlaced.mp4 | https://samples.ffmpeg.org/V-codecs/h264/interlaced_crop.mp4 | 347569 | `49559668e71f3a9ef5f1dbe092980e58262857d625a26bfc3016336d1aaf6c11` |

## Input identity

| Input | Bytes | SHA-256 |
|---|---:|---|
| africa-excerpt.avcc | 351163 | `b5705b7892c79c23e4833df440f676f1e666b5c88e6f8380967f6b38849b07c4` |
| africa-excerpt.h264 | 350873 | `5a3d75cc95113422c9f4f5e6275f5ebff60dc79f1f55a8499e68fdffddf90e98` |
| bunny-cavlc.avcc | 627969 | `863b379a3c660614b6ab7b168a0d9ee86669f3f71bab4e61ae960cdd362d1513` |
| bunny-cavlc.h264 | 627967 | `a3e25a74469cf568bea7bb060441c73c2deb0a499c70ff7b1b90a3488a44c7c4` |
| bunny-excerpt.avcc | 229571 | `470aae0d87f57aa644ae5ac1bfa8098f23b75495ccf78fa371efdb7314472f53` |
| bunny-excerpt.h264 | 229570 | `aa2b085e1b7f444159ee32ac1dbbe32ad3c8c845bdd95c069560050c073a9cc8` |
| cavlc.avcc | 162593 | `68ea7bc1e55a39512493261594262a266bfd5f005486891e21faaa5003b2fd64` |
| cavlc.h264 | 162591 | `1a5cee1db2ef746fa4b9ee304cd6760a202a859b5f911f056f493ae44c449da0` |
| envivio-excerpt.avcc | 138194 | `22f1b7213a04adc60bcf882c1d34a4f9fb81927f07d3883733da224cceb0ecdb` |
| envivio-excerpt.h264 | 138012 | `a23b4399693ac110f425f4ebf3bb32a65c22e2f3536b6a1af9dc35187d851c62` |
| interlaced-excerpt.avcc | 173762 | `edfb103fbdc970cf772fc253a310220b51839037e2d2420e7c34bbf9ee8693c7` |
| interlaced-excerpt.h264 | 173636 | `82650ce23215c163321a0a825755e84a48fa19ab5314d71ffae89849ec44c62d` |
| nature-cavlc.avcc | 129746 | `4577010eaf5b23e31e2fb76fd17d4c627e0b9b6607cb59e72df3615f811f700b` |
| nature-cavlc.h264 | 129744 | `c290ecd66604ff377fa6d417490be7438de08e0cadc8255012e819a73988010e` |
| nature-excerpt.avcc | 432106 | `fbc06367a54253a33a832ca45b7e84ad341d0e760fa5c4220cfb090f78ae1d90` |
| nature-excerpt.h264 | 432105 | `fae94e432597dfb474c78ee5f4154e5b5dc08d0dbc1ea449aa4efc7dc88b00d8` |
| srpg10.avcc | 608716 | `0d6b46fbe33e50f2e90e00b8e6731243a05c480689aeef5f828e6fe139667811` |
| srpg10.h264 | 608670 | `6deb4073b70632e32f4237f39b0b535982a49f2afaddef8b49b0b55a78b69e83` |
| srpg9.avcc | 762600 | `eb96cc1664b14e180ed0c7d79933d506aec68b7aea3cbc02292acdd41a669d99` |
| srpg9.h264 | 762599 | `da749bf97d34eca0a907433ba55bc19e55ca6a2ee72603636402fe65ed659a4e` |

## Isolated trace identity

| Input | Residual calls | UE calls | Fixed-width reads | SHA-256 |
|---|---:|---:|---:|---|
| africa-excerpt.trace | 10396 | 11274 | 6699 | `50c4d503662dd23ab9b223146d68000fe0484b090db7e7fca7d0459ead289884` |
| bunny-cavlc.trace | 16534 | 15343 | 8911 | `f6e7c80177af7c2ba0d3afdd2d3b4210d33e00e9552637a7c749ea81d0dafd3b` |
| bunny-excerpt.trace | 0 | 38 | 19 | `39d7a2580473158406b05d6c0193f387b79e07705544152026c5a561769d7eed` |
| cavlc.trace | 3614 | 4487 | 3408 | `6746c1a910608a99c17f8c7c6c5464dad35e76d352f327f28f9503bef2eab736` |
| envivio-excerpt.trace | 0 | 122 | 77 | `79f5f8080ccc2a911e47b9a258d5ff62a23a22eeb1472937caf3ab4178a35fff` |
| interlaced-excerpt.trace | 0 | 43 | 26 | `adc8c377c6f2a5faa8f8ab555cd90a8fc9062529fbfb6706c20149ee199ebfc1` |
| nature-cavlc.trace | 2764 | 7748 | 3264 | `96b0258a1b22278f981c03164f25e5f00f073021788c7e9f49c749f8dbe1e58c` |
| nature-excerpt.trace | 0 | 26 | 10 | `49a287d7c02e42e2232550e9ddca5ab85122fbcd032caa4a51bbfe11d29df752` |

## pass0-pass1-packed-avcc-decodeonly42-cavlc-africa-excerpt

[Raw paired samples](pass0-pass1-packed-avcc-decodeonly42-cavlc-africa-excerpt.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | packed-avcc | 7 | 238,243,987 / 224,601,400 | -5.89% (-8.78%..-0.21%) | 377.8 / 400.7 (+6.26%) | 60,899 / 776 (-98.73%) | 34,750,585 / 34,117,697 (-1.82%) |
| bunny-cavlc | packed-avcc | 7 | 708,889,294 / 647,812,984 | -9.01% (-11.09%..-6.56%) | 127.0 / 138.9 (+9.91%) | 295,189 / 777 (-99.74%) | 36,836,391 / 34,177,755 (-7.22%) |
| nature-cavlc | packed-avcc | 7 | 328,576,571 / 315,043,308 | -4.25% (-7.62%..+4.58%) | 182.6 / 190.5 (+4.44%) | 50,744 / 534 (-98.95%) | 40,464,242 / 40,202,810 (-0.65%) |
| africa-excerpt | packed-avcc | 7 | 1,037,306,291 / 980,455,127 | -5.74% (-39.77%..-3.66%) | 43.4 / 45.9 (+6.08%) | 145,521 / 6,179 (-95.75%) | 75,987,776 / 75,055,960 (-1.23%) |

## pass0-pass1-packed-avcc

[Raw paired samples](pass0-pass1-packed-avcc.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | packed-avcc | 7 | 236,355,625 / 221,525,545 | -5.06% (-14.08%..-4.08%) | 380.8 / 406.3 (+5.33%) | 60,899 / 776 (-98.73%) | 34,750,585 / 34,117,697 (-1.82%) |
| bunny-cavlc | packed-avcc | 7 | 691,227,669 / 631,615,644 | -8.37% (-11.28%..-5.86%) | 130.2 / 142.5 (+9.13%) | 295,189 / 777 (-99.74%) | 36,836,391 / 34,177,755 (-7.22%) |
| nature-cavlc | packed-avcc | 7 | 333,510,939 / 314,143,786 | -4.84% (-6.85%..-3.43%) | 179.9 / 191.0 (+5.09%) | 50,744 / 534 (-98.95%) | 40,464,242 / 40,202,810 (-0.65%) |
| africa-excerpt | packed-avcc | 7 | 1,027,160,128 / 995,475,956 | -3.25% (-5.78%..-0.18%) | 43.8 / 45.2 (+3.36%) | 145,521 / 6,179 (-95.75%) | 75,987,776 / 75,055,960 (-1.23%) |

## pass0-pass1-residual

[Raw paired samples](pass0-pass1-residual.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | residual | 7 | 74,598,613 / 50,521,876 | -31.33% (-39.76%..-29.48%) | 3,100,540.2 / 4,578,135.6 (+45.61%) | 119,069 / 29 (-99.98%) | 1,507,708 / 255,868 (-83.03%) |
| bunny-cavlc | residual | 7 | 335,673,008 / 226,035,170 | -32.11% (-33.94%..-30.82%) | 3,152,401.2 / 4,681,466.2 (+47.30%) | 587,805 / 29 (-100.00%) | 5,602,428 / 255,868 (-95.43%) |
| nature-cavlc | residual | 7 | 46,729,795 / 27,492,156 | -41.40% (-44.13%..-37.43%) | 3,785,507.7 / 6,434,417.1 (+70.66%) | 102,422 / 22 (-99.98%) | 799,584 / 255,584 (-68.04%) |
| africa-excerpt | residual | 7 | 167,676,710 / 110,424,526 | -33.08% (-36.25%..-29.02%) | 3,968,016.8 / 6,025,328.1 (+49.44%) | 278,492 / 28 (-99.99%) | 2,156,664 / 255,864 (-88.14%) |

## pass0-pass1-surface-annex-decodeonly42-cavlc-interlaced-excerpt

[Raw paired samples](pass0-pass1-surface-annex-decodeonly42-cavlc-interlaced-excerpt.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | surface-annex | 7 | 216,257,768 / 203,938,992 | -5.38% (-6.05%..-4.45%) | 416.2 / 441.3 (+5.68%) | 60,567 / 444 (-99.27%) | 2,921,969 / 2,289,081 (-21.66%) |
| bunny-cavlc | surface-annex | 7 | 658,963,806 / 599,092,108 | -8.76% (-9.60%..-7.79%) | 136.6 / 150.2 (+9.60%) | 294,857 / 445 (-99.85%) | 5,007,775 / 2,349,139 (-53.09%) |
| nature-cavlc | surface-annex | 7 | 298,368,774 / 285,712,838 | -3.98% (-5.70%..-3.02%) | 201.1 / 210.0 (+4.15%) | 50,532 / 322 (-99.36%) | 3,947,946 / 3,686,514 (-6.62%) |
| africa-excerpt | surface-annex | 7 | 980,135,326 / 960,045,590 | -1.45% (-6.40%..+3.52%) | 45.9 / 46.9 (+1.48%) | 145,461 / 6,119 (-95.79%) | 40,784,960 / 39,853,144 (-2.28%) |
| bunny-excerpt | surface-annex | 7 | 1,103,580,147 / 1,094,030,361 | -0.88% (-5.45%..+7.70%) | 32.6 / 32.9 (+0.89%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 538,543,254 / 544,721,301 | +1.13% (-1.53%..+1.64%) | 111.4 / 110.1 (-1.11%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 162,019,534 / 162,102,727 | +0.13% (-3.08%..+0.88%) | 370.3 / 370.1 (-0.13%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 528,682,435 / 530,373,904 | +0.27% (-0.52%..+1.54%) | 113.5 / 113.1 (-0.27%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |

## pass0-pass1-surface-annex-decodeonly42

[Raw paired samples](pass0-pass1-surface-annex-decodeonly42.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 11 | 1,618,579,387 / 1,633,578,481 | +0.43% (-3.15%..+44.48%) | 27.8 / 27.5 (-0.43%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 11 | 1,142,748,392 / 1,160,168,794 | +1.38% (-4.29%..+4.78%) | 39.4 / 38.8 (-1.36%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |

## pass0-pass1-surface-annex-repeat2

[Raw paired samples](pass0-pass1-surface-annex-repeat2.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 9 | 1,574,236,649 / 1,603,971,012 | +2.56% (-6.02%..+4.22%) | 28.6 / 28.1 (-2.50%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 9 | 1,114,514,093 / 1,126,587,913 | +0.43% (-3.56%..+5.53%) | 40.4 / 39.9 (-0.43%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |

## pass0-pass1-surface-annex-repeat42

[Raw paired samples](pass0-pass1-surface-annex-repeat42.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 11 | 1,564,281,879 / 1,600,773,111 | +1.56% (-4.48%..+5.15%) | 28.8 / 28.1 (-1.54%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 11 | 1,108,955,484 / 1,107,076,349 | -0.19% (-3.60%..+8.56%) | 40.6 / 40.6 (+0.19%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |

## pass0-pass1-surface-annex

[Raw paired samples](pass0-pass1-surface-annex.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 7 | 1,618,432,325 / 1,756,913,983 | +4.46% (-2.68%..+9.00%) | 27.8 / 25.6 (-4.27%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,093,529,027 / 1,138,984,837 | +4.45% (-1.10%..+6.02%) | 41.2 / 39.5 (-4.26%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| cavlc | surface-annex | 7 | 217,185,595 / 206,595,596 | -4.29% (-9.68%..-2.74%) | 414.4 / 435.6 (+4.48%) | 60,567 / 444 (-99.27%) | 2,921,969 / 2,289,081 (-21.66%) |
| bunny-cavlc | surface-annex | 7 | 674,089,110 / 620,455,822 | -7.53% (-10.69%..+47.87%) | 133.5 / 145.1 (+8.14%) | 294,857 / 445 (-99.85%) | 5,007,775 / 2,349,139 (-53.09%) |
| nature-cavlc | surface-annex | 7 | 310,939,092 / 299,017,173 | -3.83% (-6.45%..-0.41%) | 193.0 / 200.7 (+3.99%) | 50,532 / 322 (-99.36%) | 3,947,946 / 3,686,514 (-6.62%) |
| africa-excerpt | surface-annex | 7 | 975,513,099 / 944,007,376 | -3.31% (-7.28%..-1.58%) | 46.1 / 47.7 (+3.42%) | 145,461 / 6,119 (-95.79%) | 40,784,960 / 39,853,144 (-2.28%) |
| bunny-excerpt | surface-annex | 7 | 1,113,955,256 / 1,109,554,947 | +0.99% (-1.42%..+9.24%) | 32.3 / 32.4 (-0.98%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 550,631,558 / 552,710,437 | -0.03% (-1.84%..+2.05%) | 109.0 / 108.6 (+0.03%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 164,333,503 / 166,352,466 | +0.77% (-1.08%..+3.86%) | 365.1 / 360.7 (-0.77%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 545,578,665 / 544,257,267 | -1.18% (-2.13%..+0.17%) | 110.0 / 110.2 (+1.19%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |

## pass0-pass3-packed-avcc

[Raw paired samples](pass0-pass3-packed-avcc.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | packed-avcc | 7 | 234,414,805 / 214,262,949 | -8.66% (-10.49%..-3.92%) | 383.9 / 420.0 (+9.49%) | 60,899 / 747 (-98.77%) | 34,750,585 / 33,861,829 (-2.56%) |
| bunny-cavlc | packed-avcc | 7 | 683,544,289 / 610,356,414 | -11.41% (-13.73%..-9.78%) | 131.7 / 147.5 (+12.88%) | 295,189 / 748 (-99.75%) | 36,836,391 / 33,921,887 (-7.91%) |
| nature-cavlc | packed-avcc | 7 | 325,996,794 / 308,682,192 | -5.76% (-7.31%..-4.23%) | 184.1 / 194.4 (+6.11%) | 50,744 / 506 (-99.00%) | 40,464,242 / 39,946,950 (-1.28%) |
| africa-excerpt | packed-avcc | 7 | 1,027,583,544 / 980,283,047 | -4.75% (-7.98%..-2.00%) | 43.8 / 45.9 (+4.98%) | 145,521 / 6,150 (-95.77%) | 75,987,776 / 74,800,092 (-1.56%) |

## pass0-pass3-residual-ue

[Raw paired samples](pass0-pass3-residual-ue.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | residual | 7 | 72,908,899 / 50,544,922 | -30.67% (-32.43%..-28.80%) | 3,172,397.4 / 4,576,048.2 (+44.25%) | 119,069 / 0 (-100.00%) | 1,507,708 / 0 (-100.00%) |
| cavlc | ue | 7 | 16,327,288 / 12,100,376 | -25.44% (-27.16%..-24.27%) | 17,588,224.1 / 23,732,155.1 (+34.12%) | 0 / 0 (0%) | 0 / 0 (0%) |
| bunny-cavlc | residual | 7 | 337,461,936 / 229,322,405 | -32.16% (-36.22%..-30.22%) | 3,135,689.9 / 4,614,359.4 (+47.40%) | 587,805 / 0 (-100.00%) | 5,602,428 / 0 (-100.00%) |
| bunny-cavlc | ue | 7 | 57,860,857 / 43,397,564 | -24.97% (-25.65%..-22.95%) | 16,970,920.4 / 22,626,892.1 (+33.29%) | 0 / 0 (0%) | 0 / 0 (0%) |
| nature-cavlc | residual | 7 | 44,842,533 / 27,599,490 | -38.12% (-39.47%..-37.15%) | 3,944,826.2 / 6,409,393.8 (+61.61%) | 102,422 / 0 (-100.00%) | 799,584 / 0 (-100.00%) |
| nature-cavlc | ue | 7 | 28,097,318 / 20,418,329 | -27.42% (-27.85%..-26.32%) | 17,648,374.8 / 24,285,630.8 (+37.78%) | 0 / 0 (0%) | 0 / 0 (0%) |
| africa-excerpt | residual | 7 | 165,607,480 / 117,050,350 | -29.46% (-32.44%..-25.99%) | 4,017,596.3 / 5,684,254.7 (+41.76%) | 278,492 / 0 (-100.00%) | 2,156,664 / 0 (-100.00%) |
| africa-excerpt | ue | 7 | 43,534,752 / 31,507,471 | -29.53% (-30.36%..-23.52%) | 16,573,793.7 / 22,900,473.4 (+41.90%) | 0 / 0 (0%) | 0 / 0 (0%) |

## pass0-pass3-surface-annex-repeat42-bunny-cavlc-interlaced-excerpt

[Raw paired samples](pass0-pass3-surface-annex-repeat42-bunny-cavlc-interlaced-excerpt.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| bunny-cavlc | surface-annex | 11 | 665,861,155 / 593,060,462 | -11.04% (-12.99%..-7.83%) | 135.2 / 151.8 (+12.41%) | 294,857 / 416 (-99.86%) | 5,007,775 / 2,093,271 (-58.20%) |
| interlaced-excerpt | surface-annex | 11 | 527,791,697 / 529,423,247 | +0.42% (-1.69%..+4.14%) | 113.7 / 113.3 (-0.41%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |

## pass0-pass3-surface-annex

[Raw paired samples](pass0-pass3-surface-annex.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | surface-annex | 7 | 219,234,844 / 196,507,162 | -9.12% (-10.73%..+47.25%) | 410.5 / 458.0 (+10.03%) | 60,567 / 415 (-99.31%) | 2,921,969 / 2,033,213 (-30.42%) |
| bunny-cavlc | surface-annex | 7 | 654,588,291 / 750,542,057 | +14.85% (-10.64%..+46.13%) | 137.5 / 119.9 (-12.93%) | 294,857 / 416 (-99.86%) | 5,007,775 / 2,093,271 (-58.20%) |
| nature-cavlc | surface-annex | 7 | 302,253,479 / 280,031,082 | -6.54% (-11.88%..+26.84%) | 198.5 / 214.3 (+7.00%) | 50,532 / 294 (-99.42%) | 3,947,946 / 3,430,654 (-13.10%) |
| africa-excerpt | surface-annex | 7 | 962,092,211 / 926,261,248 | -3.82% (-6.11%..-3.26%) | 46.8 / 48.6 (+3.98%) | 145,461 / 6,090 (-95.81%) | 40,784,960 / 39,597,276 (-2.91%) |
| srpg9 | surface-annex | 7 | 1,773,239,318 / 1,658,765,673 | -4.30% (-29.07%..+20.90%) | 25.4 / 27.1 (+4.49%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,330,239,789 / 1,256,541,803 | -4.54% (-15.14%..+0.63%) | 33.8 / 35.8 (+4.75%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| bunny-excerpt | surface-annex | 7 | 1,125,534,102 / 1,123,576,168 | +0.77% (-25.06%..+5.44%) | 32.0 / 32.0 (-0.77%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 547,820,422 / 546,415,390 | -0.26% (-1.97%..+1.76%) | 109.5 / 109.8 (+0.26%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 164,574,513 / 166,462,651 | -0.05% (-1.27%..+2.32%) | 364.6 / 360.4 (+0.05%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 543,569,144 / 545,909,227 | +1.28% (-1.51%..+30.40%) | 110.4 / 109.9 (-1.27%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |

## pass1-pass2-packed-avcc-repeat42-cavlc-cavlc

[Raw paired samples](pass1-pass2-packed-avcc-repeat42-cavlc-cavlc.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | packed-avcc | 11 | 221,841,628 / 218,112,540 | -1.63% (-35.55%..+0.64%) | 405.7 / 412.6 (+1.66%) | 776 / 776 (+0.00%) | 34,117,697 / 34,117,697 (+0.00%) |

## pass1-pass2-packed-avcc

[Raw paired samples](pass1-pass2-packed-avcc.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | packed-avcc | 7 | 222,036,320 / 230,149,044 | +3.65% (-2.39%..+22.68%) | 405.3 / 391.1 (-3.52%) | 776 / 776 (+0.00%) | 34,117,697 / 34,117,697 (+0.00%) |
| bunny-cavlc | packed-avcc | 7 | 626,769,310 / 609,260,448 | -1.78% (-4.13%..-0.19%) | 143.6 / 147.7 (+1.81%) | 777 / 777 (+0.00%) | 34,177,755 / 34,177,755 (+0.00%) |
| nature-cavlc | packed-avcc | 7 | 316,555,430 / 309,450,437 | -2.92% (-4.95%..+1.93%) | 189.5 / 193.9 (+3.01%) | 534 / 534 (+0.00%) | 40,202,810 / 40,202,810 (+0.00%) |
| africa-excerpt | packed-avcc | 7 | 989,795,963 / 985,646,089 | -0.60% (-1.67%..+57.29%) | 45.5 / 45.7 (+0.60%) | 6,179 / 6,179 (+0.00%) | 75,055,960 / 75,055,960 (+0.00%) |

## pass1-pass2-surface-annex

[Raw paired samples](pass1-pass2-surface-annex.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | surface-annex | 7 | 210,820,089 / 203,110,153 | -3.72% (-6.73%..+0.10%) | 426.9 / 443.1 (+3.86%) | 444 / 444 (+0.00%) | 2,289,081 / 2,289,081 (+0.00%) |
| bunny-cavlc | surface-annex | 7 | 612,638,549 / 605,402,732 | -1.42% (-3.02%..+0.24%) | 146.9 / 148.7 (+1.44%) | 445 / 445 (+0.00%) | 2,349,139 / 2,349,139 (+0.00%) |
| nature-cavlc | surface-annex | 7 | 295,341,636 / 286,780,040 | -2.93% (-4.66%..-0.78%) | 203.2 / 209.2 (+3.02%) | 322 / 322 (+0.00%) | 3,686,514 / 3,686,514 (+0.00%) |
| africa-excerpt | surface-annex | 7 | 918,123,294 / 917,314,858 | -0.23% (-3.31%..+3.24%) | 49.0 / 49.1 (+0.23%) | 6,119 / 6,119 (+0.00%) | 39,853,144 / 39,853,144 (+0.00%) |
| srpg9 | surface-annex | 7 | 1,611,020,913 / 1,609,734,858 | -0.23% (-1.82%..+1.14%) | 27.9 / 28.0 (+0.23%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,185,911,304 / 1,170,629,721 | +0.02% (-4.40%..+29.59%) | 37.9 / 38.4 (-0.02%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| bunny-excerpt | surface-annex | 7 | 1,158,152,019 / 1,135,411,379 | -1.68% (-12.42%..+3.27%) | 31.1 / 31.7 (+1.71%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 551,303,226 / 553,008,307 | +0.13% (-42.79%..+18.98%) | 108.8 / 108.5 (-0.13%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 279,540,495 / 281,961,142 | +1.37% (-13.91%..+3.13%) | 214.6 / 212.8 (-1.36%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 554,037,099 / 556,714,118 | +2.02% (-32.22%..+82.20%) | 108.3 / 107.8 (-1.98%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |

## pass1-pass2-ue

[Raw paired samples](pass1-pass2-ue.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | ue | 7 | 16,098,789 / 12,544,425 | -24.08% (-28.87%..-18.29%) | 17,837,863.5 / 22,892,081.5 (+31.72%) | 0 / 0 (0%) | 0 / 0 (0%) |
| bunny-cavlc | ue | 7 | 61,145,898 / 45,598,711 | -25.30% (-27.14%..-23.85%) | 16,059,163.9 / 21,534,643.8 (+33.88%) | 0 / 0 (0%) | 0 / 0 (0%) |
| nature-cavlc | ue | 7 | 29,845,635 / 22,239,740 | -25.37% (-28.05%..-17.83%) | 16,614,556.9 / 22,296,663.5 (+33.99%) | 0 / 0 (0%) | 0 / 0 (0%) |
| africa-excerpt | ue | 7 | 43,694,766 / 32,142,920 | -25.96% (-29.43%..-23.32%) | 16,513,099.1 / 22,447,742.8 (+35.06%) | 0 / 0 (0%) | 0 / 0 (0%) |

## pass1-trialbits-bits

[Raw paired samples](pass1-trialbits-bits.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | bits | 7 | 6,104,075 / 6,424,545 | +3.79% (+1.90%..+9.00%) | 35,732,195.3 / 33,949,797.2 (-3.65%) | 0 / 0 (0%) | 0 / 0 (0%) |
| bunny-cavlc | bits | 7 | 17,840,303 / 19,530,231 | +9.23% (+8.34%..+9.91%) | 31,967,170.1 / 29,201,088.3 (-8.45%) | 0 / 0 (0%) | 0 / 0 (0%) |
| nature-cavlc | bits | 7 | 5,872,723 / 5,989,496 | +2.01% (-2.45%..+4.14%) | 35,570,552.2 / 34,877,058.1 (-1.97%) | 0 / 0 (0%) | 0 / 0 (0%) |
| africa-excerpt | bits | 7 | 13,637,315 / 13,315,090 | -2.54% (-6.04%..-0.69%) | 31,438,446.6 / 32,199,256.6 (+2.60%) | 0 / 0 (0%) | 0 / 0 (0%) |

## pass1-trialbits-packed-avcc

[Raw paired samples](pass1-trialbits-packed-avcc.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | packed-avcc | 7 | 254,221,164 / 254,205,363 | -1.49% (-7.41%..+1.58%) | 354.0 / 354.0 (+1.51%) | 776 / 776 (+0.00%) | 34,117,697 / 34,117,697 (+0.00%) |
| bunny-cavlc | packed-avcc | 7 | 714,536,378 / 722,324,463 | +0.21% (-5.51%..+45.07%) | 126.0 / 124.6 (-0.21%) | 777 / 777 (+0.00%) | 34,177,755 / 34,177,755 (+0.00%) |
| nature-cavlc | packed-avcc | 7 | 353,717,225 / 356,940,145 | +0.60% (-35.14%..+14.95%) | 169.6 / 168.1 (-0.59%) | 534 / 534 (+0.00%) | 40,202,810 / 40,202,810 (+0.00%) |
| africa-excerpt | packed-avcc | 7 | 1,018,468,806 / 1,031,304,948 | -1.62% (-6.67%..+5.14%) | 44.2 / 43.6 (+1.65%) | 6,179 / 6,179 (+0.00%) | 75,055,960 / 75,055,960 (+0.00%) |

## pass1-trialbits-surface-annex

[Raw paired samples](pass1-trialbits-surface-annex.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 7 | 1,852,732,333 / 1,726,263,017 | -8.52% (-38.20%..+10.16%) | 24.3 / 26.1 (+9.31%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,312,149,933 / 1,486,994,413 | +8.07% (-7.97%..+73.98%) | 34.3 / 30.3 (-7.47%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| cavlc | surface-annex | 7 | 229,781,158 / 405,788,333 | +55.08% (-7.48%..+100.00%) | 391.7 / 221.8 (-35.52%) | 444 / 444 (+0.00%) | 2,289,081 / 2,289,081 (+0.00%) |
| bunny-cavlc | surface-annex | 7 | 750,593,638 / 706,516,722 | -13.55% (-35.57%..+18.96%) | 119.9 / 127.4 (+15.67%) | 445 / 445 (+0.00%) | 2,349,139 / 2,349,139 (+0.00%) |
| nature-cavlc | surface-annex | 7 | 301,764,872 / 301,905,125 | +1.75% (-3.65%..+79.38%) | 198.8 / 198.7 (-1.72%) | 322 / 322 (+0.00%) | 3,686,514 / 3,686,514 (+0.00%) |
| africa-excerpt | surface-annex | 7 | 974,513,715 / 990,630,937 | +1.35% (-12.81%..+53.17%) | 46.2 / 45.4 (-1.34%) | 6,119 / 6,119 (+0.00%) | 39,853,144 / 39,853,144 (+0.00%) |
| bunny-excerpt | surface-annex | 7 | 1,311,501,743 / 1,607,661,689 | +14.51% (-7.32%..+41.03%) | 27.4 / 22.4 (-12.67%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 871,539,452 / 951,385,878 | +9.60% (-42.60%..+50.82%) | 68.8 / 63.1 (-8.76%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 216,519,622 / 201,940,881 | -7.99% (-38.29%..+19.12%) | 277.1 / 297.1 (+8.68%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 790,865,738 / 689,213,101 | -13.11% (-29.71%..+29.46%) | 75.9 / 87.1 (+15.09%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |

## pass2-pass3-packed-avcc

[Raw paired samples](pass2-pass3-packed-avcc.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | packed-avcc | 7 | 215,502,464 / 213,432,803 | -1.15% (-1.86%..+0.39%) | 417.6 / 421.7 (+1.16%) | 776 / 747 (-3.74%) | 34,117,697 / 33,861,829 (-0.75%) |
| bunny-cavlc | packed-avcc | 7 | 616,570,683 / 614,538,316 | -0.06% (-2.77%..+5.11%) | 146.0 / 146.5 (+0.06%) | 777 / 748 (-3.73%) | 34,177,755 / 33,921,887 (-0.75%) |
| nature-cavlc | packed-avcc | 7 | 310,250,519 / 306,418,708 | -0.97% (-2.56%..+0.81%) | 193.4 / 195.8 (+0.98%) | 534 / 506 (-5.24%) | 40,202,810 / 39,946,950 (-0.64%) |
| africa-excerpt | packed-avcc | 7 | 978,170,797 / 986,777,178 | -1.06% (-13.94%..+39.26%) | 46.0 / 45.6 (+1.07%) | 6,179 / 6,150 (-0.47%) | 75,055,960 / 74,800,092 (-0.34%) |

## pass2-pass3-residual-cold-cavlc-africa-excerpt

[Raw paired samples](pass2-pass3-residual-cold-cavlc-africa-excerpt.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | residual | 11 | 1,293,074 / 1,366,607 | +5.99% (-2.60%..+16.96%) | 2,794,890.3 / 2,644,505.7 (-5.65%) | 29 / 0 (-100.00%) | 255,868 / 0 (-100.00%) |
| bunny-cavlc | residual | 11 | 3,872,859 / 4,352,466 | +11.96% (+8.68%..+16.48%) | 4,269,197.5 / 3,798,766.0 (-10.69%) | 29 / 0 (-100.00%) | 255,868 / 0 (-100.00%) |
| nature-cavlc | residual | 11 | 955,483 / 874,708 | -8.35% (-10.30%..-4.95%) | 2,892,777.8 / 3,159,911.7 (+9.12%) | 22 / 0 (-100.00%) | 255,584 / 0 (-100.00%) |
| africa-excerpt | residual | 11 | 2,097,323 / 2,483,861 | +18.83% (+14.93%..+110.61%) | 4,956,794.9 / 4,185,419.4 (-15.84%) | 28 / 0 (-100.00%) | 255,864 / 0 (-100.00%) |

## pass2-pass3-residual

[Raw paired samples](pass2-pass3-residual.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | residual | 7 | 52,179,760 / 52,760,339 | -0.31% (-5.56%..+7.02%) | 4,432,676.6 / 4,383,899.0 (+0.31%) | 29 / 0 (-100.00%) | 255,868 / 0 (-100.00%) |
| bunny-cavlc | residual | 7 | 231,263,442 / 229,967,952 | +0.14% (-1.68%..+2.90%) | 4,575,630.2 / 4,601,406.4 (-0.14%) | 29 / 0 (-100.00%) | 255,868 / 0 (-100.00%) |
| nature-cavlc | residual | 7 | 28,875,444 / 28,151,754 | -0.10% (-4.26%..+6.59%) | 6,126,174.2 / 6,283,658.2 (+0.10%) | 22 / 0 (-100.00%) | 255,584 / 0 (-100.00%) |
| africa-excerpt | residual | 7 | 116,269,588 / 119,004,999 | +2.49% (-3.25%..+21.98%) | 5,722,425.0 / 5,590,891.2 (-2.43%) | 28 / 0 (-100.00%) | 255,864 / 0 (-100.00%) |

## pass2-pass3-surface-annex-repeat42-interlaced-excerpt-bunny-excerpt

[Raw paired samples](pass2-pass3-surface-annex-repeat42-interlaced-excerpt-bunny-excerpt.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| interlaced-excerpt | surface-annex | 11 | 529,257,746 / 529,022,875 | +0.04% (-1.16%..+50.95%) | 113.4 / 113.4 (-0.04%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |
| bunny-excerpt | surface-annex | 11 | 1,115,460,590 / 1,115,009,958 | +0.33% (-1.61%..+22.61%) | 32.3 / 32.3 (-0.33%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |

## pass2-pass3-surface-annex

[Raw paired samples](pass2-pass3-surface-annex.csv)

Throughput: sampled operations or decoded frames per billion CPU cycles.

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after (Δ) | Requested bytes before / after (Δ) |
|---|---|---:|---:|---:|---:|---:|---:|
| cavlc | surface-annex | 7 | 204,490,585 / 203,242,735 | -0.60% (-1.58%..+0.77%) | 440.1 / 442.8 (+0.60%) | 444 / 415 (-6.53%) | 2,289,081 / 2,033,213 (-11.18%) |
| bunny-cavlc | surface-annex | 7 | 606,783,167 / 593,282,159 | -1.75% (-3.22%..-0.70%) | 148.3 / 151.7 (+1.78%) | 445 / 416 (-6.52%) | 2,349,139 / 2,093,271 (-10.89%) |
| nature-cavlc | surface-annex | 7 | 287,550,928 / 284,844,268 | -0.42% (-3.32%..+0.81%) | 208.7 / 210.6 (+0.42%) | 322 / 294 (-8.70%) | 3,686,514 / 3,430,654 (-6.94%) |
| africa-excerpt | surface-annex | 7 | 941,352,533 / 927,620,845 | -0.40% (-3.68%..+5.17%) | 47.8 / 48.5 (+0.40%) | 6,119 / 6,090 (-0.47%) | 39,853,144 / 39,597,276 (-0.64%) |
| srpg9 | surface-annex | 7 | 1,573,132,569 / 1,542,952,197 | +0.07% (-4.41%..+2.77%) | 28.6 / 29.2 (-0.07%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,086,997,147 / 1,088,349,476 | +0.11% (-5.72%..+5.32%) | 41.4 / 41.3 (-0.11%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| bunny-excerpt | surface-annex | 7 | 1,087,142,892 / 1,100,603,720 | +1.47% (-3.45%..+2.63%) | 33.1 / 32.7 (-1.45%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 547,584,895 / 537,509,408 | -0.53% (-3.39%..+0.58%) | 109.6 / 111.6 (+0.53%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 161,936,779 / 162,358,884 | +0.22% (-0.82%..+2.04%) | 370.5 / 369.6 (-0.22%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 533,942,311 / 536,181,431 | +0.75% (+0.01%..+10.34%) | 112.4 / 111.9 (-0.75%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |

## Bit-exact output identity

All accepted adjacent revision pairs and the baseline/final comparison are checked against FFmpeg's SHA-256 of visible YUV420p planes in display order. The surface harness covers all ten inputs; the packed harness covers the four CAVLC inputs.

| Input | Frames | SHA-256 |
|---|---:|---|
| SRPG9 | 45 | `f9301a596ac487ba19393bc4f0fe4848d4a59fbd877a100063952633a88fed90` |
| SRPG10 | 45 | `fb59c6f413e15e17cccff1e43125a6758799cafd205d8d3b6a91154f15c8703e` |
| SRPG10 CAVLC | 90 | `2ae5a960e68994f390d01884047a3f62743c6e9e8ccf287b397b7cac9f69e374` |
| Bunny CAVLC | 90 | `6bec3d68d60d251a5dcc205a123a7700f6470760a0d6c7989a294f96158a1c62` |
| Nature CAVLC | 60 | `5f86e1ce4609dd4da56e1d396cae38b6c6806e5e5cabc7b2e5ac5573cb4b77a0` |
| BBC Africa | 45 | `e8b847769b792e306eab67b2c5dc12489fca347a661daf62879843779754e72c` |
| Big Buck Bunny | 36 | `3511282e16c625404196da6a8e3600e1c8eef7d8a4687a33694658a3e5e97056` |
| Nature | 60 | `7c1208d0339e0c1749b05ecd5a30a7fe2fdb586b21d108026f8e37933246e11f` |
| Envivio | 60 | `71c6c9fa9ae757032a33cc9c440318f2a4f5d9d9f78baa57b872997b091206d7` |
| Interlaced crop | 60 | `b87aac1ea34e23a61871b2c789853d579aa5d3e2b5b44ab15655a5ba2d9270dd` |

## Final validation

- 98 unit tests, 4 integration tests, and 7 doctests passed (all features).
- No-default-features cargo check and cargo fmt check passed.
- All 14 final decoder/output-mode comparisons matched reference frame hashes.
- No downloaded media, extracted samples, or binary traces are included in this repository.
