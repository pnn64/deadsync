# Standards coverage and outstanding work

This crate is under development. No complete-conformance or competitive-speed
claim is made. The complete user scope is retained in PLAN.md.

| Requirement | Current evidence / remaining work |
| --- | --- |
| Independent package, workspace registration | Cargo manifest; no application dependency |
| Native bounded marker parsing | Zero-copy segments/scans; stuffing/restart/fill boundaries; every truncated prefix tested |
| Sequential Huffman SOF0/SOF1, 8/12-bit | Implemented; 25 libjpeg-turbo cases, exact coefficients/tables |
| Progressive Huffman SOF2, 8/12-bit | Implemented spectral selection and DC/AC refinement; 25 oracle cases exact; band order, partial progression, EOB/restart and Al=13 tests |
| Arithmetic SOF9/SOF10 | Pending: Annex D coder and contexts |
| Predictive lossless SOF3/SOF11, 2–16-bit | Pending: predictors, point transform, entropy |
| Hierarchical SOF5/6/7/13/14/15, DHP/EXP | Pending: differential frames and expansion |
| DNL, abbreviated tables/image streams | Pending |
| Gray, YCbCr, RGB, CMYK, YCCK | Native planes, native CMYK/polarity, all five shared output conversions; 486 color comparisons plus 192 CMYK source/transform encoder cases; broader corpus pending |
| All legal sampling factors and scan layouts | Sequential interleaved/non-interleaved implemented; common/fractional encoding verified through C coefficients; full scan-layout corpus pending |
| Full component-count range | Encoder currently accepts 1–4 components; complete sequential component-count coverage pending |
| Restart intervals and sequence validation | Sequential/progressive implemented; external 8/12-bit fixtures, corruption and EOB boundary tests |
| Original coefficients, tables, metadata | Coefficients and quantizers retained; APPn/COM bytes borrowed with scan positions |
| JFIF/JFXX, Exif, Adobe, ICC assembly | Typed bounded views, Exif orientation/IFDs/thumbnails, zero-copy ICC assembly and profile writing through 255 chunks; 114 external matches plus 6 malformed ICC cases; caller uses original profile for color management |
| IDCT, chroma upsampling, color numerics | Q20 IDCT: error <=1 on initial 50 cases; exact rational centered interpolation; 486 RGB/CMYK comparisons have observed RGB error <=3; broader numerical/independent corpus gates pending |
| Native configurable encoder | Native pixel/quality/precision/sampling controls and integer FDCT/quantization; 172 external coefficient matches; 132 float-FDCT/size/quality comparisons; broader validation/performance pending |
| Optimized Huffman, progressive encoding | Annex K length-limited Huffman tables and spectral progressive output implemented; successive approximation encoder pending |
| Streaming and allocation limits | Storage/pixel/metadata/scan budgets implemented; streaming and allocator accounting pending |
| Independent corpus and three C oracles | Pinned libjpeg-turbo fixtures and 772 encoder cases; IJG libjpeg, mozjpeg and independent conformance corpus pending |
| Fuzzing and malformed corpus | Every sequential/progressive fixture truncation; 4,000 mutation smoke cases; invalid progression and quantizer changes; coverage-guided fuzzing pending |
| Throughput/cycles/allocations/memory | Pending |
| Independent out-of-workspace build | `python tests/independent.py`: isolated package with only shared primitives dependency; current gates recorded in PLAN.md |
