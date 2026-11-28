# H.264 CABAC and reconstruction revision comparison

Windows x64, Xeon E5-2696 v4, Rust 1.100.0; pinned logical CPU 42 unless otherwise specified. The direct release build uses opt-level=3, 16 decoder codegen units, default CPU, std and surface, no LTO. Full-decode timings use QueryThreadCycleTime and separate allocation-meter builds. The dedicated decoder executables have no trace or replay code. The external sample corpus and hashes are inherited from the preceding entropy pass; no media or captured coefficient data is committed.

The isolated harness replays every 512th CABAC arithmetic decision, every 128th CABAC residual block, all slice initialization arguments, and every 64th reconstruction operation, using captured input state from actual video decoding. Chroma replay uses the actual reconstruction fragment extracted from each source revision and every 64th non-skipped chroma block recorded before the transform, including the decoder's existing has_ac flag. Replay includes operation setup and result consumption. Isolated replay is timed without allocation instrumentation; the functions under test do not allocate. Full-decode allocation calls include alloc, alloc_zeroed and realloc; requested bytes count traffic rather than peak memory.

Cycle and throughput figures are individual medians; paired percentage changes use medians of before/after ratios. The absolute cycle totals below cover the entire measured batch (256 trace repetitions by default, 4096 for initialization, one stream for full decoding); dividing by repeats gives cycles per trace replay. Throughput is operations or frames per billion CPU cycles. Negative cycle and allocation changes are improvements. Ranges include all samples, including outliers. Small changes with substantial variation are inconclusive. The full pipeline includes framing, RBSP extraction, parameter sets, decoding, output ordering, consumption, draining and destruction. The surface path uses SPS reorder depth; packed output uses the default depth.

Source revisions: cab0 = `be50d3a00`; cab1 = `f9fefd1d8`; cab2 = `00062295e`; cab3 = `6b43d4d07`. cab0fresh is a fresh direct build of the same H.264 source as cab0, which reused the preceding pass's baseline executable.

The trialnorm, trialmask and trialselect experiments were screened with actual residual/bin replay and rejected before full decoding. trialseq/trialseqfull specialized consecutive CABAC significance contexts; it had modest isolated gains but reproduced a roughly 4% SRPG10 surface regression and was reverted. trialinit/trialinitfull narrowed initialization arithmetic to i16 and substantially accelerated isolated initialization, but repeatedly increased SRPG9 packed decode cycles, including on logical CPU 2; it was reverted. trialidct4 tested generic coefficient scanning for DC-only blocks and lost cycles on multiple real traces, so it was screened out. The same-source cab0/cab0fresh comparison exposed significant run-to-run and build-to-build variation (including a +5.31% SRPG9 packed paired median); these experiments do not establish tight equivalence bounds. Accepted reconstruction candidates use the freshly built baseline. No full-game rendering or LTO build was measured.

Flat 8x8 factor precomputation had a large repeatable isolated gain and a smaller gain on the custom matrix trace. Flat 4x4 factor precomputation had repeatable isolated gains on ten flat-matrix traces but an isolated regression on the custom matrix trace; the custom-matrix full surface and packed decodes did not reproduce that slowdown. All accepted changes preserve the measured decoder allocation traffic.

The chroma candidate uses the existing has_ac flag to avoid the inverse transform when only DC is present. Sampled non-skipped chroma blocks were DC-only in 81.1% to 99.1% of cases depending on the input. Isolated replay improved on all eleven inputs. The first surface batch was noisy and several medians were adverse. Repeat42 resolved Envivio and custom-matrix concerns; repeat2 on CPU 2 gave +0.66% on native Bunny, -4.29% on Bunny CAVLC and -0.55% on game CAVLC. The cab2/cab2control batch compares byte-identical executables on CPU 2, yielding +1.63% on native Bunny. The adverse medians therefore do not establish a reproducible regression beyond observed noise; this is not a tight equivalence proof. All raw samples are retained.

The ten original inputs come from the [preceding entropy comparison](../windows-x86_64-20261128-entropy/README.md). bunny-custom is a new 640x360, 60-frame x264 High-profile transcode of the external Big Buck Bunny source with cqm=jvt and 8x8dct=1. All media and traces are external to Git.

Cumulative surface and packed comparisons cover all eleven inputs. CPU 42 had adverse paired medians on SRPG10 surface (+2.80%), nature CAVLC surface (+9.16%), Africa surface (+20.60%) and SRPG9 packed (+16.89%), with large bidirectional outliers. The independent 11-pair CPU 2 batches gave -1.81%, -3.30%, -0.62% and -2.70%, respectively. Neither the adverse medians nor a tight equivalence margin are established as reproducible; most improvements are modest. All cumulative allocation counts and requested bytes matched exactly.

## Input identity

| File | Bytes | SHA-256 |
|---|---:|---|
| africa-excerpt.avcc | 351163 | `b5705b7892c79c23e4833df440f676f1e666b5c88e6f8380967f6b38849b07c4` |
| africa-excerpt.chroma4 | 37950 | `7b2c60739ed6efcc7ace2095f16e49aa74726de85ee1fdf44d59dc5347290543` |
| africa-excerpt.dq4 | 326088 | `2d18a8be44793258bb3cd2df3879291d44014b39cd22ada185d6b2071eaf6a19` |
| africa-excerpt.h264 | 350873 | `5a3d75cc95113422c9f4f5e6275f5ebff60dc79f1f55a8499e68fdffddf90e98` |
| africa-excerpt.idct4 | 1321792 | `263401364f81a0d1d27344b7673270807b37a71d74c4df5b0df9a0e8ed25c141` |
| bunny-cavlc.avcc | 627969 | `863b379a3c660614b6ab7b168a0d9ee86669f3f71bab4e61ae960cdd362d1513` |
| bunny-cavlc.chroma4 | 92322 | `7912139141286a64b3710cf9e5992667e5a83eec7d6e0f7405077318d0135157` |
| bunny-cavlc.dq4 | 601692 | `ba25deb09792798acd78d436cf0a2ae8f44279ba71682d19f3d819a28ee4443a` |
| bunny-cavlc.h264 | 627967 | `a3e25a74469cf568bea7bb060441c73c2deb0a499c70ff7b1b90a3488a44c7c4` |
| bunny-cavlc.idct4 | 528960 | `656a49e5ebf859febd2db50df5fb9104efdc3fa7246d5c5a2065826e122f4b56` |
| bunny-custom.avcc | 385105 | `a3c57f1ee8db5fb35d87551581b7b19eaa978d6aa730d6695a3dfaf50487bd6f` |
| bunny-custom.chroma4 | 69207 | `f653d8ee631399e1e2acaa2f3eaec0a68e71301492e2e5670a1f2df1be02e404` |
| bunny-custom.dq4 | 95424 | `0a74d5dbf7d8333647fc0def54b584895cf16fa62ddd9398db344d755aad57f9` |
| bunny-custom.dq8 | 254664 | `477b836d8edfaeb1499699ec72f92772a30a5ac75b9350f6c6b4d2b9beab4886` |
| bunny-custom.h264 | 385103 | `55d191b8bd1002bb7fb48f09d978b63e86f8b0b3beb7510c19eb41826a14e35c` |
| bunny-custom.idct4 | 128256 | `05f4aa3ff5d90815d167a9ee2580462d6900ecc14fc26b072000b6be6051902c` |
| bunny-custom.idct8 | 201216 | `def02ab33390a2f440854e911ceb10d56a4d0642e47d23557e42b13e68ffa3f7` |
| bunny-excerpt.avcc | 229571 | `470aae0d87f57aa644ae5ac1bfa8098f23b75495ccf78fa371efdb7314472f53` |
| bunny-excerpt.bins | 111904 | `5f1487a1f480941a175034eec5972bfc098196b01eb86522784a841c3edc45bc` |
| bunny-excerpt.chroma4 | 103845 | `5661685512af2b9f24f074074efb0e01840601d5b5a2aa9447ce21d09d42526f` |
| bunny-excerpt.dq4 | 12348 | `903101a495abf2b658453a7b90c4422912d704de92bf9b2a419e5ea87a0dc112` |
| bunny-excerpt.dq8 | 201528 | `42113c6f8b77c9e6d463e2367658aad1b9dd46d2024b524e709974c67149852b` |
| bunny-excerpt.h264 | 229570 | `aa2b085e1b7f444159ee32ac1dbbe32ad3c8c845bdd95c069560050c073a9cc8` |
| bunny-excerpt.idct4 | 550016 | `8526a1e6378b3175a95040c99e80f0762ba050bdbc1453b731eb9c40646e7e68` |
| bunny-excerpt.idct8 | 159232 | `3b8e6f3324540a437884f0ddc268725aa92aa7526802656e713f7110eaa1edee` |
| bunny-excerpt.init | 324 | `36ab34c1e2ec7a1945cb68ddaa0d25e03b3f816bdcb19e69020f6c715e5a9431` |
| bunny-excerpt.residual | 957593 | `3c94f1c5de64fda51748076cf98ff32e0b40891bc31cd3d635b7d6e323133ec4` |
| cavlc.avcc | 162593 | `68ea7bc1e55a39512493261594262a266bfd5f005486891e21faaa5003b2fd64` |
| cavlc.chroma4 | 36363 | `e2b833009916647d732066876f06fcc778fff2ecd3b3ed997498e122f242af19` |
| cavlc.dq4 | 116508 | `585355b3d38cc327b356362a547acdf0248a204d760f15c06aa2190f843f3db0` |
| cavlc.h264 | 162591 | `1a5cee1db2ef746fa4b9ee304cd6760a202a859b5f911f056f493ae44c449da0` |
| cavlc.idct4 | 120640 | `4d90fba2539d229252d6a9cec412210c4a82d934aa29b2b8f945b09b8a1471df` |
| envivio-excerpt.avcc | 138194 | `22f1b7213a04adc60bcf882c1d34a4f9fb81927f07d3883733da224cceb0ecdb` |
| envivio-excerpt.bins | 43040 | `3bbb9e36aa66b3ed39f59046ecc5538e9afa938efdb59823510b22cd323f8517` |
| envivio-excerpt.chroma4 | 11592 | `03ae1236025283ea70f1c7813df7e53459344f09899a511160920a87c8d96c4d` |
| envivio-excerpt.dq4 | 101976 | `43a155971c52e500b94add875c8ce7da870044fed9823ce69f260132982769a1` |
| envivio-excerpt.h264 | 138012 | `a23b4399693ac110f425f4ebf3bb32a65c22e2f3536b6a1af9dc35187d851c62` |
| envivio-excerpt.idct4 | 94464 | `57537943c7ba6825b88e89ef28cb802a41ad750ccd3a7ec51c2fa86b3fe4f8f6` |
| envivio-excerpt.init | 2160 | `009a8438dd329c044bdf1f90cba76856bf395310208711f8c061243965b2a107` |
| envivio-excerpt.residual | 968422 | `a5c5b2bc5953092459377a6af9dd3862e98795f685dd3300a382c8fd26bc76fa` |
| interlaced-excerpt.avcc | 173762 | `edfb103fbdc970cf772fc253a310220b51839037e2d2420e7c34bbf9ee8693c7` |
| interlaced-excerpt.bins | 58944 | `1b484a3ce0a231aaa7335689156c52a8144ad75f303cbc01749ccdf7fb3b6f1b` |
| interlaced-excerpt.chroma4 | 28773 | `ee8a403785b22560094558de46fa7e739cd3c4cc6ce6755e834113702bc77193` |
| interlaced-excerpt.dq4 | 76104 | `b7e7dcb88faea39521db279148c20aefa335e80532cbc8acfe46b71645e3a730` |
| interlaced-excerpt.h264 | 173636 | `82650ce23215c163321a0a825755e84a48fa19ab5314d71ffae89849ec44c62d` |
| interlaced-excerpt.idct4 | 103616 | `f2cb56d3ce63ea6b6ab3be402a75671477b8e60f2bb6e27c515bfdf2c8a64b85` |
| interlaced-excerpt.init | 540 | `2d4696ecbdc7c19c14a7ee8cb9f218a413874947b7747c4adc1ff3887891cb4e` |
| interlaced-excerpt.residual | 756483 | `9bad3ed2160f9a129fc96d848c5f4f9f4f9ad9e04fdb68f757e76585d5f21e72` |
| nature-cavlc.avcc | 129746 | `4577010eaf5b23e31e2fb76fd17d4c627e0b9b6607cb59e72df3615f811f700b` |
| nature-cavlc.chroma4 | 95910 | `765d8ff9b4c72bdf1e3073e5817cf05eea735c95bfc1dd7b4d96b61002ae2128` |
| nature-cavlc.dq4 | 58044 | `cfdf8e4a653361611f5f62a2f8c5271dd0ef89b3ceab0daceb5c8a662f227584` |
| nature-cavlc.h264 | 129744 | `c290ecd66604ff377fa6d417490be7438de08e0cadc8255012e819a73988010e` |
| nature-cavlc.idct4 | 181184 | `74808951ccfbd379e834e1afb891860e347513cee8904cc21d99c7d9f0fa1868` |
| nature-excerpt.avcc | 432106 | `fbc06367a54253a33a832ca45b7e84ad341d0e760fa5c4220cfb090f78ae1d90` |
| nature-excerpt.bins | 130144 | `d7681fffd6a7b3b09840aa298c9f874ed5cc12a53991cbadbf5b03dccb26f972` |
| nature-excerpt.chroma4 | 166290 | `c3ab4c7efab4f8d6d6d177a78443c442701c57e519e531a1f05c722f89b626f7` |
| nature-excerpt.dq4 | 362796 | `a84592cbef7d7dc0d091deb962437ff872d542b4c625dc3052c269076ec3fff7` |
| nature-excerpt.h264 | 432105 | `fae94e432597dfb474c78ee5f4154e5b5dc08d0dbc1ea449aa4efc7dc88b00d8` |
| nature-excerpt.idct4 | 518656 | `ad0be5841e012e5109b774a2b71b7edf459ff27ffc5bac7e1ddd3a9bb223d80a` |
| nature-excerpt.init | 540 | `47f50a3ad95ff2aa8f778fff9af3fbe5b62b67d45d6128a22f41a9c031321297` |
| nature-excerpt.residual | 3290469 | `817e53ae8aefe11a82efd847e9b1921b14806fc7f22f334cd0fc11fcde9add0e` |
| srpg10.avcc | 608716 | `0d6b46fbe33e50f2e90e00b8e6731243a05c480689aeef5f828e6fe139667811` |
| srpg10.bins | 199984 | `3d70a0405f244b988e8349f74704f26e8b68663d793ad46ed20098cc69974601` |
| srpg10.chroma4 | 111504 | `0ba65608dce6593920558475a5c4fb6981d192bcc17a1516355e4f33d66eacf4` |
| srpg10.dq4 | 141120 | `cc7915b741aa5c35a2dbc74bf9491f6b17b8ef8da75a4baf5a979a242184bca2` |
| srpg10.dq8 | 313308 | `ade29e09708c546470c1d4f1280b7fc9e1bdeff4e023bf1f943a23a47921d6a9` |
| srpg10.h264 | 608670 | `6deb4073b70632e32f4237f39b0b535982a49f2afaddef8b49b0b55a78b69e83` |
| srpg10.idct4 | 223296 | `f21601948006ac83aa43a2378bf0b022ff7f87ae24b9addff251c47c086098d3` |
| srpg10.idct8 | 247552 | `b7d89898f5752e473951cdc8eb637ad2975e8e1c5d691d351cdd82a6fe237d77` |
| srpg10.init | 405 | `111bd159fd2e814c5a42cd60bbacd7ca441c362725387f12607a066bbada9f17` |
| srpg10.residual | 2400944 | `06f5625d9c95014ea76bdb1a2c1abdc8f76a1d4c417add41ca9862b18178f5fd` |
| srpg9.avcc | 762600 | `eb96cc1664b14e180ed0c7d79933d506aec68b7aea3cbc02292acdd41a669d99` |
| srpg9.bins | 245760 | `712ec28ae87f2887e57effaa5f13c5b1f5996c7f549592d27fbe2a7286befed4` |
| srpg9.chroma4 | 195822 | `9498c907e851e0ad8089e6af1cfe07c81fc01c0629284cf7809197beb398ca02` |
| srpg9.dq4 | 171108 | `ff8575b6f72b0a1aefdeb4fd5ae08c56f9b996ebc2259edf108c2861d35f016d` |
| srpg9.dq8 | 261792 | `4d8bb63f2c2b12e7f0d470107298c56e29f5ed479251d9e9cb48b36232ab0005` |
| srpg9.h264 | 762599 | `da749bf97d34eca0a907433ba55bc19e55ca6a2ee72603636402fe65ed659a4e` |
| srpg9.idct4 | 394368 | `ba7c424f1d5859bfaf9e33c79525328d6e4275c6cf4b8f6fb9b3b2b2648869eb` |
| srpg9.idct8 | 206848 | `fedaeb1f32fb31fc8f4842dec3f5709db3ca822e0714a7ddb5e131bdc8f4e5ee` |
| srpg9.init | 405 | `11daf9a8f578386a84d7546dfc849b473ccdc86a9023e4e277c5aaa59690b79c` |
| srpg9.residual | 2824822 | `425d2c0c65e5fd162528790d7a85db575d58bb2df4bb42fed8168742f5871ec2` |

## cab0-cab0fresh-packed-avcc

[Raw paired samples](cab0-cab0fresh-packed-avcc.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | packed-avcc | 7 | 2,207,837,234 / 2,602,650,355 | +5.31% (-3.72%..+27.44%) | 20.4 / 17.3 (-5.04%) | 580 / 580 (+0.00%) | 185,246,984 / 185,246,984 (+0.00%) |

## cab0-cab0fresh-surface-annex

[Raw paired samples](cab0-cab0fresh-surface-annex.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 7 | 2,032,907,793 / 2,064,097,010 | +1.17% (-32.69%..+52.56%) | 22.1 / 21.8 (-1.15%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,367,792,089 / 1,369,017,765 | -3.52% (-34.29%..+33.02%) | 32.9 / 32.9 (+3.64%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| bunny-excerpt | surface-annex | 7 | 1,466,844,537 / 1,537,927,419 | +0.90% (-25.86%..+31.20%) | 24.5 / 23.4 (-0.89%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |

## cab0-trialinit-init

[Raw paired samples](cab0-trialinit-init.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | init | 7 | 865,917,851 / 513,741,724 | -40.83% (-41.28%..-38.90%) | 212,860.8 / 358,779.5 (+68.99%) | not instrumented | not instrumented |
| srpg10 | init | 7 | 865,675,076 / 506,855,789 | -40.70% (-42.37%..-38.17%) | 212,920.5 / 363,653.7 (+68.64%) | not instrumented | not instrumented |
| bunny-excerpt | init | 7 | 685,364,823 / 417,423,812 | -40.75% (-50.05%..-37.60%) | 215,149.6 / 353,252.5 (+68.76%) | not instrumented | not instrumented |
| nature-excerpt | init | 7 | 1,167,857,877 / 697,188,845 | -40.88% (-41.27%..-30.29%) | 210,436.6 / 352,501.3 (+69.14%) | not instrumented | not instrumented |
| envivio-excerpt | init | 7 | 4,643,426,792 / 2,734,706,358 | -41.11% (-48.20%..-38.98%) | 211,705.7 / 359,468.2 (+69.80%) | not instrumented | not instrumented |
| interlaced-excerpt | init | 7 | 1,158,559,418 / 693,926,203 | -39.91% (-42.28%..-38.92%) | 212,125.5 / 354,158.7 (+66.42%) | not instrumented | not instrumented |

## cab0-trialinitfull-init

[Raw paired samples](cab0-trialinitfull-init.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | init | 7 | 874,135,707 / 509,747,051 | -41.85% (-48.17%..-40.28%) | 210,859.7 / 361,591.1 (+71.96%) | not instrumented | not instrumented |
| srpg10 | init | 7 | 853,629,572 / 517,747,828 | -40.28% (-54.88%..+5.60%) | 215,925.0 / 356,003.4 (+67.45%) | not instrumented | not instrumented |
| bunny-excerpt | init | 7 | 688,695,744 / 415,554,547 | -39.66% (-41.91%..-27.46%) | 214,109.1 / 354,841.5 (+65.73%) | not instrumented | not instrumented |
| nature-excerpt | init | 7 | 1,166,415,530 / 692,299,048 | -40.49% (-43.28%..-38.03%) | 210,696.8 / 354,991.1 (+68.04%) | not instrumented | not instrumented |
| envivio-excerpt | init | 7 | 4,637,004,435 / 2,764,765,595 | -40.24% (-41.77%..-25.01%) | 211,998.9 / 355,560.0 (+67.34%) | not instrumented | not instrumented |
| interlaced-excerpt | init | 7 | 1,171,394,011 / 694,749,109 | -40.04% (-45.73%..-39.82%) | 209,801.3 / 353,739.2 (+66.77%) | not instrumented | not instrumented |

## cab0-trialinitfull-packed-avcc-repeat2-srpg9-interlaced-excerpt

[Raw paired samples](cab0-trialinitfull-packed-avcc-repeat2-srpg9-interlaced-excerpt.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | packed-avcc | 11 | 1,804,498,970 / 1,850,461,839 | +2.32% (-2.47%..+18.18%) | 24.9 / 24.3 (-2.26%) | 580 / 580 (+0.00%) | 185,246,984 / 185,246,984 (+0.00%) |
| interlaced-excerpt | packed-avcc | 11 | 561,644,749 / 553,383,427 | -2.13% (-40.08%..+15.00%) | 106.8 / 108.4 (+2.18%) | 724 / 724 (+0.00%) | 25,838,097 / 25,838,097 (+0.00%) |

## cab0-trialinitfull-packed-avcc-repeat42-srpg9-srpg9

[Raw paired samples](cab0-trialinitfull-packed-avcc-repeat42-srpg9-srpg9.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | packed-avcc | 11 | 1,893,840,938 / 2,031,110,329 | +8.08% (-13.14%..+46.44%) | 23.8 / 22.2 (-7.48%) | 580 / 580 (+0.00%) | 185,246,984 / 185,246,984 (+0.00%) |

## cab0-trialinitfull-packed-avcc

[Raw paired samples](cab0-trialinitfull-packed-avcc.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | packed-avcc | 7 | 1,954,358,862 / 1,911,350,897 | +2.51% (-6.69%..+55.46%) | 23.0 / 23.5 (-2.45%) | 580 / 580 (+0.00%) | 185,246,984 / 185,246,984 (+0.00%) |
| srpg10 | packed-avcc | 7 | 1,454,862,023 / 1,397,234,036 | -2.58% (-9.52%..+1.05%) | 30.9 / 32.2 (+2.65%) | 475 / 475 (+0.00%) | 167,458,926 / 167,458,926 (+0.00%) |
| bunny-excerpt | packed-avcc | 7 | 1,256,252,505 / 1,275,582,772 | +0.92% (-41.88%..+21.14%) | 28.7 / 28.2 (-0.92%) | 508 / 508 (+0.00%) | 139,297,262 / 139,297,262 (+0.00%) |
| nature-excerpt | packed-avcc | 7 | 546,265,040 / 550,279,267 | +0.11% (-1.61%..+1.47%) | 109.8 / 109.0 (-0.11%) | 474 / 474 (+0.00%) | 40,805,728 / 40,805,728 (+0.00%) |
| envivio-excerpt | packed-avcc | 7 | 163,426,969 / 164,289,606 | +0.34% (-2.41%..+20.25%) | 367.1 / 365.2 (-0.34%) | 637 / 637 (+0.00%) | 10,251,776 / 10,251,776 (+0.00%) |
| interlaced-excerpt | packed-avcc | 7 | 559,130,159 / 600,602,486 | +2.33% (-1.85%..+33.43%) | 107.3 / 99.9 (-2.27%) | 724 / 724 (+0.00%) | 25,838,097 / 25,838,097 (+0.00%) |

## cab0-trialinitfull-surface-annex

[Raw paired samples](cab0-trialinitfull-surface-annex.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 7 | 1,548,653,729 / 1,579,741,942 | +1.28% (-2.73%..+69.29%) | 29.1 / 28.5 (-1.26%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,040,607,357 / 1,042,993,771 | +1.32% (-7.22%..+10.61%) | 43.2 / 43.1 (-1.30%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| bunny-excerpt | surface-annex | 7 | 1,101,846,319 / 1,078,268,289 | -0.97% (-5.45%..+0.61%) | 32.7 / 33.4 (+0.98%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 548,680,861 / 545,348,631 | +0.49% (-4.41%..+1.84%) | 109.4 / 110.0 (-0.49%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 165,861,447 / 164,505,372 | -0.39% (-10.74%..+3.56%) | 361.7 / 364.7 (+0.39%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 542,061,395 / 543,865,904 | -0.54% (-1.79%..+0.74%) | 110.7 / 110.3 (+0.55%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |
| cavlc | surface-annex | 7 | 205,312,400 / 198,235,935 | -0.42% (-21.07%..+5.18%) | 438.4 / 454.0 (+0.43%) | 415 / 415 (+0.00%) | 2,033,213 / 2,033,213 (+0.00%) |
| bunny-cavlc | surface-annex | 7 | 593,193,255 / 584,026,061 | -0.72% (-3.41%..-0.24%) | 151.7 / 154.1 (+0.72%) | 416 / 416 (+0.00%) | 2,093,271 / 2,093,271 (+0.00%) |
| nature-cavlc | surface-annex | 7 | 278,928,522 / 282,089,544 | +0.31% (-1.93%..+3.54%) | 215.1 / 212.7 (-0.31%) | 294 / 294 (+0.00%) | 3,430,654 / 3,430,654 (+0.00%) |
| africa-excerpt | surface-annex | 7 | 924,445,313 / 933,425,941 | +1.11% (-0.93%..+2.31%) | 48.7 / 48.2 (-1.10%) | 6,090 / 6,090 (+0.00%) | 39,597,276 / 39,597,276 (+0.00%) |

## cab0-trialmask-cabres

[Raw paired samples](cab0-trialmask-cabres.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | cabres | 7 | 239,148,764 / 234,128,796 | -2.14% (-4.06%..+1.88%) | 1,954,666.2 / 1,996,576.3 (+2.19%) | not instrumented | not instrumented |
| srpg10 | cabres | 7 | 226,010,591 / 231,921,505 | +2.62% (-2.21%..+55.98%) | 1,757,935.3 / 1,713,131.3 (-2.55%) | not instrumented | not instrumented |
| bunny-excerpt | cabres | 7 | 88,282,463 / 89,316,745 | +2.22% (-4.64%..+6.01%) | 1,794,965.8 / 1,774,180.2 (-2.17%) | not instrumented | not instrumented |
| nature-excerpt | cabres | 7 | 176,132,510 / 180,132,675 | +2.82% (-1.80%..+43.51%) | 3,091,490.6 / 3,022,838.6 (-2.75%) | not instrumented | not instrumented |
| envivio-excerpt | cabres | 7 | 58,917,971 / 58,468,215 | -1.99% (-3.34%..+8.28%) | 2,719,985.0 / 2,740,908.0 (+2.03%) | not instrumented | not instrumented |
| interlaced-excerpt | cabres | 7 | 75,421,954 / 77,521,473 | +4.81% (-0.54%..+67.96%) | 1,659,782.0 / 1,614,830.0 (-4.59%) | not instrumented | not instrumented |

## cab0-trialnorm-bin-cabres

[Raw paired samples](cab0-trialnorm-bin-cabres.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | bin | 7 | 54,876,317 / 54,234,501 | -1.64% (-7.36%..+19.62%) | 62,516,148.8 / 63,255,970.6 (+1.67%) | not instrumented | not instrumented |
| srpg9 | cabres | 7 | 239,829,214 / 247,247,433 | +1.93% (+0.94%..+7.18%) | 1,949,120.3 / 1,890,640.5 (-1.89%) | not instrumented | not instrumented |
| srpg10 | bin | 7 | 46,167,654 / 44,602,179 | -2.28% (-6.23%..+4.20%) | 60,789,920.1 / 62,923,562.5 (+2.33%) | not instrumented | not instrumented |
| srpg10 | cabres | 7 | 223,106,163 / 234,094,116 | +4.24% (+2.88%..+6.38%) | 1,780,820.4 / 1,697,231.9 (-4.07%) | not instrumented | not instrumented |
| bunny-excerpt | bin | 7 | 25,798,711 / 24,999,951 | -2.87% (-8.29%..-1.49%) | 65,680,180.7 / 67,778,692.8 (+2.95%) | not instrumented | not instrumented |
| bunny-excerpt | cabres | 7 | 51,051,747 / 52,393,772 | +2.70% (-3.48%..+3.84%) | 3,103,987.8 / 3,024,481.6 (-2.62%) | not instrumented | not instrumented |
| nature-excerpt | bin | 7 | 28,699,844 / 28,065,270 | -1.65% (-4.21%..-0.77%) | 63,697,071.0 / 65,137,303.2 (+1.67%) | not instrumented | not instrumented |
| nature-excerpt | cabres | 7 | 172,337,793 / 184,766,978 | +7.12% (+3.40%..+9.16%) | 3,159,562.3 / 2,947,020.1 (-6.65%) | not instrumented | not instrumented |
| envivio-excerpt | bin | 7 | 11,294,592 / 10,664,846 | -0.50% (-6.83%..+11.91%) | 53,287,095.3 / 56,433,632.5 (+0.50%) | not instrumented | not instrumented |
| envivio-excerpt | cabres | 7 | 58,949,358 / 62,730,248 | +6.59% (+3.99%..+11.45%) | 2,718,536.8 / 2,554,684.6 (-6.18%) | not instrumented | not instrumented |
| interlaced-excerpt | bin | 7 | 12,857,870 / 12,276,417 | -4.52% (-12.84%..+6.03%) | 63,831,412.2 / 66,854,685.7 (+4.74%) | not instrumented | not instrumented |
| interlaced-excerpt | cabres | 7 | 43,961,240 / 47,187,014 | +7.78% (-1.18%..+10.09%) | 2,847,599.4 / 2,652,933.3 (-7.22%) | not instrumented | not instrumented |

## cab0-trialselect-bin-cabres

[Raw paired samples](cab0-trialselect-bin-cabres.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | bin | 7 | 57,930,660 / 54,478,145 | -5.24% (-16.99%..-3.17%) | 59,220,040.0 / 62,973,069.3 (+5.52%) | not instrumented | not instrumented |
| srpg9 | cabres | 7 | 240,539,073 / 245,864,805 | +2.19% (-0.52%..+5.29%) | 1,943,368.3 / 1,901,272.5 (-2.14%) | not instrumented | not instrumented |
| srpg10 | bin | 7 | 47,412,438 / 44,574,087 | -4.84% (-21.53%..+3.91%) | 59,193,918.7 / 62,963,219.0 (+5.09%) | not instrumented | not instrumented |
| srpg10 | cabres | 7 | 224,243,837 / 228,037,891 | +2.82% (+0.34%..+8.23%) | 1,771,785.6 / 1,742,306.9 (-2.74%) | not instrumented | not instrumented |
| bunny-excerpt | bin | 7 | 27,627,369 / 26,438,554 | -4.36% (-9.05%..+10.07%) | 61,332,803.7 / 64,090,645.8 (+4.56%) | not instrumented | not instrumented |
| bunny-excerpt | cabres | 7 | 52,046,083 / 54,534,114 | +4.85% (+1.06%..+7.88%) | 3,044,686.4 / 2,905,777.5 (-4.63%) | not instrumented | not instrumented |
| nature-excerpt | bin | 7 | 28,949,855 / 29,504,312 | -1.18% (-3.47%..+4.56%) | 63,146,982.9 / 61,960,299.2 (+1.20%) | not instrumented | not instrumented |
| nature-excerpt | cabres | 7 | 178,359,995 / 183,792,179 | +1.43% (-9.31%..+7.76%) | 3,052,881.9 / 2,962,650.5 (-1.41%) | not instrumented | not instrumented |
| envivio-excerpt | bin | 7 | 11,248,497 / 9,613,442 | -5.37% (-18.47%..+0.33%) | 53,505,459.4 / 62,605,672.3 (+5.67%) | not instrumented | not instrumented |
| envivio-excerpt | cabres | 7 | 59,380,674 / 61,290,327 | +3.31% (-3.01%..+6.50%) | 2,698,790.5 / 2,614,703.0 (-3.20%) | not instrumented | not instrumented |
| interlaced-excerpt | bin | 7 | 12,556,716 / 12,189,055 | -2.56% (-6.19%..+3.33%) | 65,362,312.9 / 67,333,849.9 (+2.63%) | not instrumented | not instrumented |
| interlaced-excerpt | cabres | 7 | 44,988,938 / 46,087,539 | +4.13% (+1.66%..+6.84%) | 2,782,550.7 / 2,716,222.3 (-3.96%) | not instrumented | not instrumented |

## cab0-trialseq-cabres

[Raw paired samples](cab0-trialseq-cabres.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | cabres | 7 | 240,709,188 / 234,492,289 | -3.61% (-6.78%..-1.23%) | 1,941,994.8 / 1,993,481.3 (+3.74%) | not instrumented | not instrumented |
| srpg10 | cabres | 7 | 228,602,887 / 227,114,457 | -1.29% (-3.18%..-0.11%) | 1,738,000.8 / 1,749,391.1 (+1.31%) | not instrumented | not instrumented |
| bunny-excerpt | cabres | 7 | 51,654,059 / 50,917,416 | -2.13% (-5.34%..+1.46%) | 3,067,793.8 / 3,112,176.8 (+2.17%) | not instrumented | not instrumented |
| nature-excerpt | cabres | 7 | 175,694,386 / 170,405,313 | -2.93% (-4.42%..+1.52%) | 3,099,199.8 / 3,195,393.3 (+3.02%) | not instrumented | not instrumented |
| envivio-excerpt | cabres | 7 | 60,741,795 / 58,081,017 | -3.85% (-6.71%..-0.72%) | 2,638,315.2 / 2,759,180.3 (+4.00%) | not instrumented | not instrumented |
| interlaced-excerpt | cabres | 7 | 45,454,720 / 44,086,576 | -1.20% (-15.01%..+3.45%) | 2,754,037.4 / 2,839,503.8 (+1.21%) | not instrumented | not instrumented |

## cab0-trialseqfull-cabres

[Raw paired samples](cab0-trialseqfull-cabres.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | cabres | 7 | 232,833,304 / 227,029,289 | -1.85% (-4.30%..+40.11%) | 2,007,685.3 / 2,059,011.9 (+1.88%) | not instrumented | not instrumented |
| srpg10 | cabres | 7 | 221,465,626 / 218,528,935 | -1.47% (-2.12%..-0.01%) | 1,794,012.0 / 1,818,120.8 (+1.49%) | not instrumented | not instrumented |
| bunny-excerpt | cabres | 7 | 68,837,615 / 85,821,210 | +1.59% (-1.94%..+25.85%) | 2,301,997.2 / 1,846,443.3 (-1.56%) | not instrumented | not instrumented |
| nature-excerpt | cabres | 7 | 171,816,259 / 168,630,658 | -1.19% (-16.01%..+7.15%) | 3,169,152.9 / 3,229,021.4 (+1.21%) | not instrumented | not instrumented |
| envivio-excerpt | cabres | 7 | 59,424,576 / 56,570,856 | -4.96% (-10.07%..-3.89%) | 2,696,796.7 / 2,832,836.8 (+5.21%) | not instrumented | not instrumented |
| interlaced-excerpt | cabres | 7 | 43,682,038 / 43,077,753 | -2.29% (-3.88%..+4.49%) | 2,865,800.4 / 2,906,001.2 (+2.34%) | not instrumented | not instrumented |

## cab0-trialseqfull-packed-avcc

[Raw paired samples](cab0-trialseqfull-packed-avcc.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | packed-avcc | 7 | 1,775,051,494 / 1,749,429,714 | -1.44% (-13.78%..+3.59%) | 25.4 / 25.7 (+1.46%) | 580 / 580 (+0.00%) | 185,246,984 / 185,246,984 (+0.00%) |
| srpg10 | packed-avcc | 7 | 1,296,772,631 / 1,296,721,495 | +0.95% (-3.34%..+5.13%) | 34.7 / 34.7 (-0.94%) | 475 / 475 (+0.00%) | 167,458,926 / 167,458,926 (+0.00%) |
| bunny-excerpt | packed-avcc | 7 | 1,253,034,841 / 1,240,689,512 | -0.63% (-5.13%..+0.84%) | 28.7 / 29.0 (+0.63%) | 508 / 508 (+0.00%) | 139,297,262 / 139,297,262 (+0.00%) |
| nature-excerpt | packed-avcc | 7 | 547,772,562 / 557,828,735 | +2.62% (+0.23%..+50.73%) | 109.5 / 107.6 (-2.56%) | 474 / 474 (+0.00%) | 40,805,728 / 40,805,728 (+0.00%) |
| envivio-excerpt | packed-avcc | 7 | 164,506,250 / 164,060,665 | -0.08% (-2.84%..+25.36%) | 364.7 / 365.7 (+0.08%) | 637 / 637 (+0.00%) | 10,251,776 / 10,251,776 (+0.00%) |
| interlaced-excerpt | packed-avcc | 7 | 556,475,301 / 555,668,851 | -0.18% (-28.11%..+1.85%) | 107.8 / 108.0 (+0.18%) | 724 / 724 (+0.00%) | 25,838,097 / 25,838,097 (+0.00%) |

## cab0-trialseqfull-surface-annex-repeat42-srpg10-srpg10

[Raw paired samples](cab0-trialseqfull-surface-annex-repeat42-srpg10-srpg10.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg10 | surface-annex | 11 | 1,162,899,810 / 1,208,557,127 | +4.03% (-15.91%..+20.28%) | 38.7 / 37.2 (-3.88%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |

## cab0-trialseqfull-surface-annex

[Raw paired samples](cab0-trialseqfull-surface-annex.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 7 | 1,823,898,803 / 1,740,887,653 | -3.99% (-31.08%..+43.49%) | 24.7 / 25.8 (+4.16%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,249,852,757 / 1,225,490,453 | +3.49% (-11.17%..+18.69%) | 36.0 / 36.7 (-3.37%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| bunny-excerpt | surface-annex | 7 | 1,196,238,122 / 1,186,834,514 | -3.63% (-23.14%..+55.27%) | 30.1 / 30.3 (+3.77%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 558,355,538 / 553,896,402 | -0.70% (-36.31%..+2.12%) | 107.5 / 108.3 (+0.70%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 166,067,992 / 163,951,358 | -0.27% (-40.61%..+54.56%) | 361.3 / 366.0 (+0.27%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 542,066,230 / 535,337,904 | -1.49% (-7.30%..+0.71%) | 110.7 / 112.1 (+1.51%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |
| cavlc | surface-annex | 7 | 198,971,703 / 197,479,102 | -0.89% (-8.11%..+1.99%) | 452.3 / 455.7 (+0.89%) | 415 / 415 (+0.00%) | 2,033,213 / 2,033,213 (+0.00%) |
| bunny-cavlc | surface-annex | 7 | 597,553,190 / 591,656,099 | -1.44% (-2.54%..-0.69%) | 150.6 / 152.1 (+1.46%) | 416 / 416 (+0.00%) | 2,093,271 / 2,093,271 (+0.00%) |
| nature-cavlc | surface-annex | 7 | 287,667,476 / 288,019,339 | -1.21% (-4.26%..+2.64%) | 208.6 / 208.3 (+1.22%) | 294 / 294 (+0.00%) | 3,430,654 / 3,430,654 (+0.00%) |
| africa-excerpt | surface-annex | 7 | 936,009,239 / 957,333,234 | +1.69% (+0.02%..+7.00%) | 48.1 / 47.0 (-1.66%) | 6,090 / 6,090 (+0.00%) | 39,597,276 / 39,597,276 (+0.00%) |

## cab0fresh-cab1-dq8-custom

[Raw paired samples](cab0fresh-cab1-dq8-custom.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| bunny-custom | dq8 | 9 | 65,320,347 / 61,296,910 | -6.39% (-9.48%..-2.94%) | 3,080,449.0 / 3,282,645.1 (+6.82%) | not instrumented | not instrumented |

## cab0fresh-cab1-dq8

[Raw paired samples](cab0fresh-cab1-dq8.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | dq8 | 9 | 64,707,501 / 21,150,143 | -67.40% (-68.26%..-66.08%) | 3,196,661.9 / 9,779,981.2 (+206.76%) | not instrumented | not instrumented |
| srpg10 | dq8 | 9 | 77,274,756 / 25,514,241 | -67.13% (-67.42%..-66.09%) | 3,203,530.0 / 9,702,503.0 (+204.20%) | not instrumented | not instrumented |
| bunny-excerpt | dq8 | 9 | 51,198,813 / 16,403,235 | -67.84% (-69.42%..-66.86%) | 3,110,072.1 / 9,707,353.5 (+210.98%) | not instrumented | not instrumented |

## cab0fresh-cab1-packed-avcc

[Raw paired samples](cab0fresh-cab1-packed-avcc.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | packed-avcc | 7 | 1,744,136,246 / 1,734,029,361 | -0.24% (-7.44%..+4.30%) | 25.8 / 26.0 (+0.24%) | 580 / 580 (+0.00%) | 185,246,984 / 185,246,984 (+0.00%) |
| srpg10 | packed-avcc | 7 | 1,284,985,051 / 1,280,787,989 | -0.60% (-2.78%..+5.50%) | 35.0 / 35.1 (+0.60%) | 475 / 475 (+0.00%) | 167,458,926 / 167,458,926 (+0.00%) |
| bunny-excerpt | packed-avcc | 7 | 1,232,322,606 / 1,222,780,718 | -0.84% (-4.84%..+2.18%) | 29.2 / 29.4 (+0.84%) | 508 / 508 (+0.00%) | 139,297,262 / 139,297,262 (+0.00%) |

## cab0fresh-cab1-surface-annex-packed-avcc

[Raw paired samples](cab0fresh-cab1-surface-annex-packed-avcc.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| bunny-custom | surface-annex | 9 | 443,946,431 / 447,246,180 | +0.99% (-0.78%..+3.37%) | 135.2 / 134.2 (-0.98%) | 372 / 372 (+0.00%) | 3,025,741 / 3,025,741 (+0.00%) |
| bunny-custom | packed-avcc | 9 | 459,467,276 / 463,923,345 | +0.92% (-0.93%..+2.53%) | 130.6 / 129.3 (-0.91%) | 584 / 584 (+0.00%) | 24,242,197 / 24,242,197 (+0.00%) |

## cab0fresh-cab1-surface-annex-repeat42-srpg9-srpg9

[Raw paired samples](cab0fresh-cab1-surface-annex-repeat42-srpg9-srpg9.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 11 | 1,580,612,491 / 1,552,786,903 | -1.30% (-5.65%..+2.94%) | 28.5 / 29.0 (+1.32%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |

## cab0fresh-cab1-surface-annex

[Raw paired samples](cab0fresh-cab1-surface-annex.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 7 | 1,682,150,313 / 1,766,388,061 | +2.57% (-8.45%..+39.08%) | 26.8 / 25.5 (-2.50%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,154,573,506 / 1,159,393,080 | -0.10% (-7.96%..+5.46%) | 39.0 / 38.8 (+0.10%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| bunny-excerpt | surface-annex | 7 | 1,185,638,684 / 1,147,074,505 | -0.84% (-4.94%..+34.39%) | 30.4 / 31.4 (+0.85%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 548,094,787 / 551,846,486 | +0.68% (-8.12%..+23.48%) | 109.5 / 108.7 (-0.68%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 169,905,953 / 168,371,423 | -1.20% (-4.73%..+3.57%) | 353.1 / 356.4 (+1.21%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 558,535,300 / 578,558,757 | +1.07% (-8.43%..+4.35%) | 107.4 / 103.7 (-1.06%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |
| cavlc | surface-annex | 7 | 195,980,793 / 196,938,035 | +0.31% (-1.12%..+3.81%) | 459.2 / 457.0 (-0.31%) | 415 / 415 (+0.00%) | 2,033,213 / 2,033,213 (+0.00%) |
| bunny-cavlc | surface-annex | 7 | 614,865,153 / 597,310,870 | -2.85% (-41.16%..+1.75%) | 146.4 / 150.7 (+2.94%) | 416 / 416 (+0.00%) | 2,093,271 / 2,093,271 (+0.00%) |
| nature-cavlc | surface-annex | 7 | 281,709,375 / 292,740,565 | +0.68% (-0.89%..+7.82%) | 213.0 / 205.0 (-0.68%) | 294 / 294 (+0.00%) | 3,430,654 / 3,430,654 (+0.00%) |
| africa-excerpt | surface-annex | 7 | 939,580,718 / 942,698,721 | +0.01% (-0.68%..+1.54%) | 47.9 / 47.7 (-0.01%) | 6,090 / 6,090 (+0.00%) | 39,597,276 / 39,597,276 (+0.00%) |

## cab0fresh-cab3-packed-avcc-repeat2-srpg9-srpg9

[Raw paired samples](cab0fresh-cab3-packed-avcc-repeat2-srpg9-srpg9.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | packed-avcc | 11 | 1,858,138,184 / 1,813,059,905 | -2.70% (-5.43%..+4.32%) | 24.2 / 24.8 (+2.77%) | 580 / 580 (+0.00%) | 185,246,984 / 185,246,984 (+0.00%) |

## cab0fresh-cab3-packed-avcc

[Raw paired samples](cab0fresh-cab3-packed-avcc.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | packed-avcc | 7 | 1,797,088,646 / 2,100,588,884 | +16.89% (-18.21%..+38.74%) | 25.0 / 21.4 (-14.45%) | 580 / 580 (+0.00%) | 185,246,984 / 185,246,984 (+0.00%) |
| srpg10 | packed-avcc | 7 | 1,319,725,976 / 1,371,089,406 | -0.50% (-22.21%..+34.43%) | 34.1 / 32.8 (+0.50%) | 475 / 475 (+0.00%) | 167,458,926 / 167,458,926 (+0.00%) |
| bunny-excerpt | packed-avcc | 7 | 1,232,820,650 / 1,246,678,576 | -0.64% (-5.84%..+25.05%) | 29.2 / 28.9 (+0.64%) | 508 / 508 (+0.00%) | 139,297,262 / 139,297,262 (+0.00%) |
| nature-excerpt | packed-avcc | 7 | 559,294,562 / 543,808,617 | -2.58% (-19.91%..+4.63%) | 107.3 / 110.3 (+2.65%) | 474 / 474 (+0.00%) | 40,805,728 / 40,805,728 (+0.00%) |
| envivio-excerpt | packed-avcc | 7 | 163,386,799 / 160,891,303 | -1.43% (-2.42%..+2.70%) | 367.2 / 372.9 (+1.45%) | 637 / 637 (+0.00%) | 10,251,776 / 10,251,776 (+0.00%) |
| interlaced-excerpt | packed-avcc | 7 | 554,083,406 / 557,778,257 | -0.11% (-0.86%..+66.01%) | 108.3 / 107.6 (+0.11%) | 724 / 724 (+0.00%) | 25,838,097 / 25,838,097 (+0.00%) |
| cavlc | packed-avcc | 7 | 215,294,600 / 209,959,872 | -2.50% (-5.56%..+62.09%) | 418.0 / 428.7 (+2.57%) | 747 / 747 (+0.00%) | 33,861,829 / 33,861,829 (+0.00%) |
| bunny-cavlc | packed-avcc | 7 | 612,649,075 / 589,980,218 | -3.72% (-8.53%..-1.28%) | 146.9 / 152.5 (+3.86%) | 748 / 748 (+0.00%) | 33,921,887 / 33,921,887 (+0.00%) |
| nature-cavlc | packed-avcc | 7 | 306,589,041 / 308,074,618 | -0.90% (-3.32%..+34.29%) | 195.7 / 194.8 (+0.91%) | 506 / 506 (+0.00%) | 39,946,950 / 39,946,950 (+0.00%) |
| africa-excerpt | packed-avcc | 7 | 1,006,831,576 / 981,184,103 | -2.14% (-35.34%..+11.02%) | 44.7 / 45.9 (+2.19%) | 6,150 / 6,150 (+0.00%) | 74,800,092 / 74,800,092 (+0.00%) |
| bunny-custom | packed-avcc | 7 | 451,173,251 / 452,365,798 | +0.60% (-1.51%..+2.53%) | 133.0 / 132.6 (-0.60%) | 584 / 584 (+0.00%) | 24,242,197 / 24,242,197 (+0.00%) |

## cab0fresh-cab3-surface-annex-repeat2-srpg10-africa-excerpt

[Raw paired samples](cab0fresh-cab3-surface-annex-repeat2-srpg10-africa-excerpt.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg10 | surface-annex | 11 | 1,148,743,597 / 1,124,016,700 | -1.81% (-11.78%..+22.71%) | 39.2 / 40.0 (+1.84%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| nature-cavlc | surface-annex | 11 | 298,288,644 / 286,452,985 | -3.30% (-9.04%..+2.31%) | 201.1 / 209.5 (+3.41%) | 294 / 294 (+0.00%) | 3,430,654 / 3,430,654 (+0.00%) |
| africa-excerpt | surface-annex | 11 | 948,075,376 / 935,735,967 | -0.62% (-3.19%..+4.83%) | 47.5 / 48.1 (+0.63%) | 6,090 / 6,090 (+0.00%) | 39,597,276 / 39,597,276 (+0.00%) |

## cab0fresh-cab3-surface-annex

[Raw paired samples](cab0fresh-cab3-surface-annex.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 7 | 1,740,471,476 / 1,816,097,107 | -1.21% (-20.87%..+25.55%) | 25.9 / 24.8 (+1.22%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,160,361,063 / 1,199,832,659 | +2.80% (-7.01%..+9.90%) | 38.8 / 37.5 (-2.73%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| bunny-excerpt | surface-annex | 7 | 1,170,642,451 / 1,104,427,196 | -4.17% (-49.33%..+3.91%) | 30.8 / 32.6 (+4.35%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 547,669,621 / 538,903,878 | -2.32% (-3.08%..-0.10%) | 109.6 / 111.3 (+2.37%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 166,790,371 / 161,544,098 | -2.55% (-5.28%..+0.63%) | 359.7 / 371.4 (+2.61%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 535,812,230 / 531,702,535 | -1.32% (-4.82%..+0.27%) | 112.0 / 112.8 (+1.34%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |
| cavlc | surface-annex | 7 | 198,631,480 / 194,476,557 | -2.10% (-44.92%..-0.81%) | 453.1 / 462.8 (+2.14%) | 415 / 415 (+0.00%) | 2,033,213 / 2,033,213 (+0.00%) |
| bunny-cavlc | surface-annex | 7 | 593,936,493 / 583,806,570 | -1.60% (-2.38%..+60.43%) | 151.5 / 154.2 (+1.63%) | 416 / 416 (+0.00%) | 2,093,271 / 2,093,271 (+0.00%) |
| nature-cavlc | surface-annex | 7 | 282,390,919 / 327,459,542 | +9.16% (-45.29%..+57.61%) | 212.5 / 183.2 (-8.39%) | 294 / 294 (+0.00%) | 3,430,654 / 3,430,654 (+0.00%) |
| africa-excerpt | surface-annex | 7 | 1,010,005,548 / 1,420,585,123 | +20.60% (-26.54%..+55.39%) | 44.6 / 31.7 (-17.08%) | 6,090 / 6,090 (+0.00%) | 39,597,276 / 39,597,276 (+0.00%) |
| bunny-custom | surface-annex | 7 | 459,808,161 / 454,839,120 | +0.68% (-28.74%..+38.16%) | 130.5 / 131.9 (-0.68%) | 372 / 372 (+0.00%) | 3,025,741 / 3,025,741 (+0.00%) |

## cab0fresh-trialdq8-dq8

[Raw paired samples](cab0fresh-trialdq8-dq8.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | dq8 | 9 | 79,712,743 / 24,500,808 | -68.22% (-72.02%..-41.70%) | 2,594,917.6 / 8,442,497.1 (+214.66%) | not instrumented | not instrumented |
| srpg10 | dq8 | 9 | 95,084,330 / 28,671,089 | -69.54% (-70.43%..-57.96%) | 2,603,499.4 / 8,634,202.9 (+228.32%) | not instrumented | not instrumented |
| bunny-excerpt | dq8 | 9 | 63,094,836 / 18,971,165 | -70.21% (-75.60%..-60.20%) | 2,523,693.1 / 8,393,369.6 (+235.71%) | not instrumented | not instrumented |

## cab0fresh-trialidct4-idct4

[Raw paired samples](cab0fresh-trialidct4-idct4.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | idct4 | 7 | 45,955,618 / 48,976,814 | +5.91% (-4.44%..+8.50%) | 34,325,988.2 / 32,208,546.7 (-5.58%) | not instrumented | not instrumented |
| srpg10 | idct4 | 7 | 27,966,933 / 29,126,773 | +5.18% (+2.48%..+7.57%) | 31,937,145.2 / 30,665,395.0 (-4.93%) | not instrumented | not instrumented |
| bunny-excerpt | idct4 | 7 | 69,385,269 / 51,836,242 | -27.07% (-32.39%..-20.57%) | 31,707,940.8 / 42,442,582.9 (+37.12%) | not instrumented | not instrumented |
| nature-excerpt | idct4 | 7 | 73,818,071 / 75,845,372 | +7.32% (-22.89%..+14.27%) | 28,104,554.5 / 27,353,336.7 (-6.82%) | not instrumented | not instrumented |
| envivio-excerpt | idct4 | 7 | 12,081,280 / 15,258,105 | +33.86% (+20.96%..+42.69%) | 31,276,156.2 / 24,764,281.0 (-25.30%) | not instrumented | not instrumented |
| interlaced-excerpt | idct4 | 7 | 13,762,651 / 14,433,002 | +2.12% (-0.43%..+61.95%) | 30,115,128.3 / 28,716,409.8 (-2.08%) | not instrumented | not instrumented |
| cavlc | idct4 | 7 | 15,719,272 / 14,896,806 | -1.53% (-7.99%..+5.28%) | 30,698,622.7 / 32,393,521.1 (+1.56%) | not instrumented | not instrumented |
| bunny-cavlc | idct4 | 7 | 70,853,941 / 90,981,436 | +24.65% (+18.28%..+102.31%) | 29,861,994.6 / 23,255,733.2 (-19.78%) | not instrumented | not instrumented |
| nature-cavlc | idct4 | 7 | 34,110,299 / 25,637,600 | -25.79% (-36.67%..-18.88%) | 21,246,838.1 / 28,268,480.7 (+34.75%) | not instrumented | not instrumented |
| africa-excerpt | idct4 | 7 | 188,481,798 / 174,879,601 | -18.70% (-23.87%..+9.20%) | 28,051,345.3 / 30,233,188.8 (+23.00%) | not instrumented | not instrumented |

## cab1-cab2-dq4

[Raw paired samples](cab1-cab2-dq4.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | dq4 | 7 | 25,264,890 / 17,555,611 | -30.42% (-33.71%..-24.83%) | 20,640,184.9 / 29,704,007.5 (+43.71%) | not instrumented | not instrumented |
| srpg10 | dq4 | 7 | 20,954,568 / 14,911,951 | -29.68% (-35.42%..-26.52%) | 20,524,403.1 / 28,841,296.5 (+42.21%) | not instrumented | not instrumented |
| bunny-excerpt | dq4 | 7 | 1,801,437 / 1,401,727 | -19.62% (-25.26%..-13.36%) | 20,889,989.5 / 26,846,882.5 (+24.42%) | not instrumented | not instrumented |
| nature-excerpt | dq4 | 7 | 55,873,726 / 39,518,560 | -29.29% (-33.34%..-23.36%) | 19,788,621.2 / 27,978,347.4 (+41.43%) | not instrumented | not instrumented |
| envivio-excerpt | dq4 | 7 | 13,921,128 / 10,591,972 | -27.16% (-29.12%..-21.34%) | 22,324,627.7 / 29,341,467.3 (+37.29%) | not instrumented | not instrumented |
| interlaced-excerpt | dq4 | 7 | 10,577,706 / 7,475,952 | -29.32% (-34.37%..-25.67%) | 21,926,871.5 / 31,024,276.2 (+41.49%) | not instrumented | not instrumented |
| cavlc | dq4 | 7 | 16,561,935 / 11,583,235 | -28.11% (-32.69%..-26.96%) | 21,439,040.8 / 30,653,958.1 (+39.10%) | not instrumented | not instrumented |
| bunny-cavlc | dq4 | 7 | 83,543,894 / 59,461,452 | -28.63% (-29.87%..-27.71%) | 21,949,276.1 / 30,838,937.5 (+40.12%) | not instrumented | not instrumented |
| nature-cavlc | dq4 | 7 | 8,083,527 / 5,681,977 | -29.76% (-30.51%..-27.04%) | 21,883,516.9 / 31,132,825.8 (+42.37%) | not instrumented | not instrumented |
| africa-excerpt | dq4 | 7 | 47,264,059 / 32,144,019 | -32.05% (-36.71%..-31.94%) | 21,026,378.6 / 30,916,855.8 (+47.18%) | not instrumented | not instrumented |
| bunny-custom | dq4 | 7 | 12,795,750 / 16,518,254 | +28.32% (+24.19%..+31.44%) | 22,727,546.3 / 17,605,734.8 (-22.07%) | not instrumented | not instrumented |

## cab1-cab2-packed-avcc

[Raw paired samples](cab1-cab2-packed-avcc.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | packed-avcc | 7 | 1,741,390,302 / 1,742,748,774 | -0.01% (-2.97%..+7.23%) | 25.8 / 25.8 (+0.01%) | 580 / 580 (+0.00%) | 185,246,984 / 185,246,984 (+0.00%) |
| srpg10 | packed-avcc | 7 | 1,275,789,535 / 1,276,042,383 | +0.01% (-2.41%..+1.35%) | 35.3 / 35.3 (-0.01%) | 475 / 475 (+0.00%) | 167,458,926 / 167,458,926 (+0.00%) |
| bunny-cavlc | packed-avcc | 7 | 614,608,122 / 609,564,015 | -1.10% (-1.36%..+2.64%) | 146.4 / 147.6 (+1.12%) | 748 / 748 (+0.00%) | 33,921,887 / 33,921,887 (+0.00%) |
| africa-excerpt | packed-avcc | 7 | 985,474,216 / 970,499,493 | -0.81% (-3.26%..+0.37%) | 45.7 / 46.4 (+0.82%) | 6,150 / 6,150 (+0.00%) | 74,800,092 / 74,800,092 (+0.00%) |
| bunny-custom | packed-avcc | 7 | 461,132,847 / 462,032,136 | -0.17% (-1.69%..+1.56%) | 130.1 / 129.9 (+0.17%) | 584 / 584 (+0.00%) | 24,242,197 / 24,242,197 (+0.00%) |

## cab1-cab2-surface-annex

[Raw paired samples](cab1-cab2-surface-annex.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 7 | 1,593,744,726 / 1,608,521,680 | +0.06% (-1.59%..+4.36%) | 28.2 / 28.0 (-0.06%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,102,359,079 / 1,108,286,668 | +1.38% (-2.52%..+7.09%) | 40.8 / 40.6 (-1.36%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| bunny-excerpt | surface-annex | 7 | 1,101,166,321 / 1,108,043,901 | +1.20% (-7.51%..+3.83%) | 32.7 / 32.5 (-1.18%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 547,766,195 / 542,882,975 | -0.49% (-3.71%..+2.82%) | 109.5 / 110.5 (+0.49%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 169,524,898 / 166,188,722 | -0.77% (-5.20%..+2.35%) | 353.9 / 361.0 (+0.78%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 536,169,794 / 532,315,381 | +0.23% (-2.37%..+1.49%) | 111.9 / 112.7 (-0.23%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |
| cavlc | surface-annex | 7 | 199,875,599 / 202,925,557 | +1.53% (-5.84%..+12.47%) | 450.3 / 443.5 (-1.50%) | 415 / 415 (+0.00%) | 2,033,213 / 2,033,213 (+0.00%) |
| bunny-cavlc | surface-annex | 7 | 595,379,921 / 577,639,492 | -2.13% (-4.81%..-1.06%) | 151.2 / 155.8 (+2.17%) | 416 / 416 (+0.00%) | 2,093,271 / 2,093,271 (+0.00%) |
| nature-cavlc | surface-annex | 7 | 290,779,553 / 290,573,001 | -1.43% (-6.29%..+2.74%) | 206.3 / 206.5 (+1.45%) | 294 / 294 (+0.00%) | 3,430,654 / 3,430,654 (+0.00%) |
| africa-excerpt | surface-annex | 7 | 931,285,371 / 924,907,153 | -0.38% (-7.48%..+2.27%) | 48.3 / 48.7 (+0.38%) | 6,090 / 6,090 (+0.00%) | 39,597,276 / 39,597,276 (+0.00%) |
| bunny-custom | surface-annex | 7 | 433,744,070 / 434,335,407 | +0.07% (-8.95%..+1.87%) | 138.3 / 138.1 (-0.07%) | 372 / 372 (+0.00%) | 3,025,741 / 3,025,741 (+0.00%) |

## cab2-cab2control-surface-annex-aa2-bunny-excerpt-bunny-excerpt

[Raw paired samples](cab2-cab2control-surface-annex-aa2-bunny-excerpt-bunny-excerpt.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| bunny-excerpt | surface-annex | 11 | 1,491,110,251 / 1,519,965,501 | +1.63% (-24.46%..+12.71%) | 24.1 / 23.7 (-1.60%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |

## cab2-cab3-chroma4

[Raw paired samples](cab2-cab3-chroma4.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | chroma4 | 7 | 52,411,551 / 20,897,716 | -59.09% (-66.54%..-51.45%) | 13,861,982.4 / 34,765,904.6 (+144.43%) | not instrumented | not instrumented |
| srpg10 | chroma4 | 7 | 24,990,075 / 10,843,081 | -57.57% (-63.22%..-49.35%) | 16,554,412.1 / 38,152,993.6 (+135.69%) | not instrumented | not instrumented |
| bunny-excerpt | chroma4 | 7 | 21,908,735 / 9,060,521 | -68.97% (-78.66%..-53.37%) | 17,585,679.9 / 42,522,941.0 (+222.29%) | not instrumented | not instrumented |
| nature-excerpt | chroma4 | 7 | 38,621,902 / 13,187,122 | -61.33% (-70.41%..-56.59%) | 15,974,355.7 / 46,785,037.7 (+158.61%) | not instrumented | not instrumented |
| envivio-excerpt | chroma4 | 7 | 2,935,374 / 1,197,153 | -62.04% (-67.24%..-56.40%) | 14,651,625.3 / 35,925,232.6 (+163.44%) | not instrumented | not instrumented |
| interlaced-excerpt | chroma4 | 7 | 7,549,923 / 2,901,789 | -53.71% (-70.97%..-47.66%) | 14,139,482.0 / 36,788,339.9 (+116.03%) | not instrumented | not instrumented |
| cavlc | chroma4 | 7 | 7,543,996 / 3,535,486 | -57.54% (-67.10%..-45.42%) | 17,883,360.5 / 38,159,393.1 (+135.49%) | not instrumented | not instrumented |
| bunny-cavlc | chroma4 | 7 | 17,913,175 / 7,232,086 | -60.71% (-62.11%..-58.94%) | 19,121,568.3 / 47,362,268.6 (+154.53%) | not instrumented | not instrumented |
| nature-cavlc | chroma4 | 7 | 17,233,603 / 4,937,653 | -71.07% (-73.44%..-55.29%) | 20,648,032.8 / 72,066,627.6 (+245.69%) | not instrumented | not instrumented |
| africa-excerpt | chroma4 | 7 | 8,288,539 / 2,348,650 | -68.90% (-74.31%..-55.24%) | 16,987,312.2 / 59,949,332.6 (+221.51%) | not instrumented | not instrumented |
| bunny-custom | chroma4 | 7 | 12,766,120 / 4,781,149 | -61.39% (-63.43%..-46.15%) | 20,113,237.2 / 53,704,245.6 (+158.98%) | not instrumented | not instrumented |

## cab2-cab3-packed-avcc

[Raw paired samples](cab2-cab3-packed-avcc.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | packed-avcc | 7 | 1,736,258,160 / 1,720,017,811 | -1.39% (-6.64%..+1.98%) | 25.9 / 26.2 (+1.41%) | 580 / 580 (+0.00%) | 185,246,984 / 185,246,984 (+0.00%) |
| srpg10 | packed-avcc | 7 | 1,273,048,850 / 1,279,929,744 | -1.31% (-6.07%..+2.22%) | 35.3 / 35.2 (+1.33%) | 475 / 475 (+0.00%) | 167,458,926 / 167,458,926 (+0.00%) |
| bunny-cavlc | packed-avcc | 7 | 589,413,687 / 587,376,065 | -0.23% (-3.45%..+0.65%) | 152.7 / 153.2 (+0.23%) | 748 / 748 (+0.00%) | 33,921,887 / 33,921,887 (+0.00%) |
| africa-excerpt | packed-avcc | 7 | 992,650,560 / 962,857,820 | -1.62% (-4.18%..+40.09%) | 45.3 / 46.7 (+1.65%) | 6,150 / 6,150 (+0.00%) | 74,800,092 / 74,800,092 (+0.00%) |
| bunny-custom | packed-avcc | 7 | 512,875,141 / 518,364,393 | -0.55% (-14.73%..+51.67%) | 117.0 / 115.7 (+0.55%) | 584 / 584 (+0.00%) | 24,242,197 / 24,242,197 (+0.00%) |

## cab2-cab3-surface-annex-repeat2-bunny-excerpt-cavlc

[Raw paired samples](cab2-cab3-surface-annex-repeat2-bunny-excerpt-cavlc.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| bunny-excerpt | surface-annex | 11 | 1,472,575,246 / 1,474,859,134 | +0.66% (-12.48%..+19.77%) | 24.4 / 24.4 (-0.66%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| bunny-cavlc | surface-annex | 11 | 882,536,632 / 836,266,246 | -4.29% (-22.44%..+17.06%) | 102.0 / 107.6 (+4.49%) | 416 / 416 (+0.00%) | 2,093,271 / 2,093,271 (+0.00%) |
| cavlc | surface-annex | 11 | 266,845,932 / 249,805,269 | -0.55% (-21.06%..+28.94%) | 337.3 / 360.3 (+0.55%) | 415 / 415 (+0.00%) | 2,033,213 / 2,033,213 (+0.00%) |

## cab2-cab3-surface-annex-repeat42-envivio-excerpt-bunny-custom

[Raw paired samples](cab2-cab3-surface-annex-repeat42-envivio-excerpt-bunny-custom.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| envivio-excerpt | surface-annex | 11 | 241,847,076 / 223,054,142 | +0.96% (-17.34%..+37.01%) | 248.1 / 269.0 (-0.95%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| bunny-excerpt | surface-annex | 11 | 1,385,521,088 / 1,393,574,110 | +2.77% (-32.61%..+20.41%) | 26.0 / 25.8 (-2.69%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| bunny-cavlc | surface-annex | 11 | 812,713,459 / 803,348,040 | +1.53% (-11.11%..+24.83%) | 110.7 / 112.0 (-1.51%) | 416 / 416 (+0.00%) | 2,093,271 / 2,093,271 (+0.00%) |
| cavlc | surface-annex | 11 | 241,405,883 / 264,191,296 | +1.48% (-32.93%..+50.83%) | 372.8 / 340.7 (-1.46%) | 415 / 415 (+0.00%) | 2,033,213 / 2,033,213 (+0.00%) |
| bunny-custom | surface-annex | 11 | 635,138,618 / 591,793,068 | -0.19% (-36.37%..+13.49%) | 94.5 / 101.4 (+0.19%) | 372 / 372 (+0.00%) | 3,025,741 / 3,025,741 (+0.00%) |

## cab2-cab3-surface-annex

[Raw paired samples](cab2-cab3-surface-annex.csv)

| Input | Mode | Pairs | Cycles before / after | Paired cycles Δ (min..max) | Throughput before / after (paired Δ) | Allocation calls before / after | Requested bytes before / after |
|---|---|---:|---:|---:|---:|---:|---:|
| srpg9 | surface-annex | 7 | 2,154,581,485 / 2,213,241,551 | -8.58% (-34.59%..+16.89%) | 20.9 / 20.3 (+9.39%) | 404 / 404 (+0.00%) | 44,221,088 / 44,221,088 (+0.00%) |
| srpg10 | surface-annex | 7 | 1,487,415,627 / 1,291,153,001 | -5.35% (-20.83%..+1.68%) | 30.3 / 34.9 (+5.65%) | 304 / 304 (+0.00%) | 26,433,606 / 26,433,606 (+0.00%) |
| bunny-excerpt | surface-annex | 7 | 1,225,009,303 / 1,259,242,087 | +2.79% (-28.62%..+22.44%) | 29.4 / 28.6 (-2.72%) | 391 / 391 (+0.00%) | 26,483,430 / 26,483,430 (+0.00%) |
| nature-excerpt | surface-annex | 7 | 903,195,519 / 857,350,766 | -8.03% (-32.12%..+0.92%) | 66.4 / 70.0 (+8.74%) | 285 / 285 (+0.00%) | 12,965,064 / 12,965,064 (+0.00%) |
| envivio-excerpt | surface-annex | 7 | 239,239,640 / 254,627,024 | +6.45% (+3.17%..+15.52%) | 250.8 / 235.6 (-6.06%) | 569 / 569 (+0.00%) | 4,374,088 / 4,374,088 (+0.00%) |
| interlaced-excerpt | surface-annex | 7 | 963,148,226 / 942,696,092 | +0.66% (-12.34%..+8.09%) | 62.3 / 63.6 (-0.65%) | 482 / 482 (+0.00%) | 3,690,441 / 3,690,441 (+0.00%) |
| cavlc | surface-annex | 7 | 350,357,777 / 349,142,849 | +2.88% (-10.71%..+9.66%) | 256.9 / 257.8 (-2.80%) | 415 / 415 (+0.00%) | 2,033,213 / 2,033,213 (+0.00%) |
| bunny-cavlc | surface-annex | 7 | 710,477,824 / 729,714,363 | +4.23% (-19.99%..+7.67%) | 126.7 / 123.3 (-4.06%) | 416 / 416 (+0.00%) | 2,093,271 / 2,093,271 (+0.00%) |
| nature-cavlc | surface-annex | 7 | 284,535,654 / 282,544,131 | -1.23% (-2.06%..-0.48%) | 210.9 / 212.4 (+1.24%) | 294 / 294 (+0.00%) | 3,430,654 / 3,430,654 (+0.00%) |
| africa-excerpt | surface-annex | 7 | 922,871,286 / 917,203,583 | +0.11% (-2.35%..+3.09%) | 48.8 / 49.1 (-0.11%) | 6,090 / 6,090 (+0.00%) | 39,597,276 / 39,597,276 (+0.00%) |
| bunny-custom | surface-annex | 7 | 431,003,833 / 441,911,883 | +1.70% (-0.49%..+3.26%) | 139.2 / 135.8 (-1.67%) | 372 / 372 (+0.00%) | 3,025,741 / 3,025,741 (+0.00%) |

## Final validation

All-features cargo test: 100 unit tests, 4 integration tests and 7 doctests passed. No-default-features cargo check and cargo fmt check passed. The two new dequantization tests cover QP 0 through 63, flat and custom matrices, and extreme i32 coefficients. The DC shortcut uses the existing has_ac invariant and wrapping rounding; both call sites initialize the AC block to zero when has_ac is false.

Final baseline/final comparison: all eleven inputs in each of surface Annex B and packed AVCC modes (22 comparisons) matched FFmpeg's visible YUV420p plane hashes in display order. Adjacent revision checks also matched. The reference hashes are:

| Input | SHA-256 |
|---|---|
| SRPG9 | `f9301a596ac487ba19393bc4f0fe4848d4a59fbd877a100063952633a88fed90` |
| SRPG10 | `fb59c6f413e15e17cccff1e43125a6758799cafd205d8d3b6a91154f15c8703e` |
| Bunny | `3511282e16c625404196da6a8e3600e1c8eef7d8a4687a33694658a3e5e97056` |
| Nature | `7c1208d0339e0c1749b05ecd5a30a7fe2fdb586b21d108026f8e37933246e11f` |
| Envivio | `71c6c9fa9ae757032a33cc9c440318f2a4f5d9d9f78baa57b872997b091206d7` |
| Interlaced | `b87aac1ea34e23a61871b2c789853d579aa5d3e2b5b44ab15655a5ba2d9270dd` |
| Game CAVLC | `2ae5a960e68994f390d01884047a3f62743c6e9e8ccf287b397b7cac9f69e374` |
| Bunny CAVLC | `6bec3d68d60d251a5dcc205a123a7700f6470760a0d6c7989a294f96158a1c62` |
| Nature CAVLC | `5f86e1ce4609dd4da56e1d396cae38b6c6806e5e5cabc7b2e5ac5573cb4b77a0` |
| Africa | `e8b847769b792e306eab67b2c5dc12489fca347a661daf62879843779754e72c` |
| Bunny custom | `685261255808a496edf3767951fa80a67d564279334ef2edc798e0bffaa53537` |
