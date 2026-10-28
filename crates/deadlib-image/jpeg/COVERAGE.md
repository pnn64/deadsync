# Standards coverage and outstanding work

This crate is under development. No complete-conformance or competitive-speed
claim is made. The complete user scope is retained in PLAN.md.

| Requirement | Current evidence / remaining work |
| --- | --- |
| Independent package, workspace registration | Cargo manifest; no application dependency |
| Native bounded marker parsing | Zero-copy segments/scans; stuffing/restart/fill boundaries; every truncated prefix tested |
| Sequential Huffman SOF0/SOF1, 8/12-bit | Implemented; 25 libjpeg-turbo cases, exact coefficients/tables |
| Progressive Huffman SOF2, 8/12-bit | Implemented spectral selection and DC/AC refinement; 25 oracle cases exact; band order, partial progression, EOB/restart and Al=13 tests |
| Arithmetic SOF9/SOF10 | Sequential/progressive 8/12-bit decoding/encoding implemented: native Annex D coder, full 16-bit Annex F/G models, DAC/defaults, DC/AC refinement, custom bands/scripts, band history and restarts; 32 pinned decoder fixtures and external coefficient gates. Broader conformance/performance pending; see ARITHMETIC-VALIDATION.md and PROGRESSIVE-ENCODER-VALIDATION.md |
| Predictive lossless SOF3/SOF11, 2–16-bit | Native Huffman/arithmetic decoding and encoding: all seven predictors, Pt, row restarts, component sampling/scripts and bounded two-dimensional arithmetic contexts. Turbo gates: 4,628 decoder/4,448 encoder cases. JPEG XT: 3,240 arithmetic cases (857 direct streams; 2,383 with documented Pt header normalization), 45 additional pinned fixtures, all 4,448 Huffman output hashes unchanged. Reference setup/output limitations and remaining conformance work are in LOSSLESS-VALIDATION.md |
| Hierarchical SOF5/6/7/13/14/15, DHP/EXP | Decoder, direct frame writer and automatic native/shared source pyramids implemented: matching IDCT references, signed FDCT/Pt, partial progressive precision, rational sampling, K.5 reduction, final spatial corrections, metadata/ICC and bounded storage/output. Prior 5,200 exact C/native encoder comparisons plus 560 source-pyramid cases checked against C frame observations and independent source/filter/float-FDCT models; C-input workarounds are explicit. Full DCT component counts and broader conformance remain pending. See HIERARCHICAL-VALIDATION.md |
| DNL, initially unknown SOF/DHP heights | Whole-input decoding/encoding for all 13 frame processes: first-scan resolution before allocation, zero/provisional/redefined heights, completed DHP inference, strict adjacency/range/length checks and bounded output. 2,124 native DNL files and 336 independent C-authored input checks; original-header C matches are separated from known-header entropy comparisons. 26 pinned all-process regressions; see DNL-VALIDATION.md. Incremental I/O remains pending |
| Abbreviated tables/image streams | Pending |
| Gray, YCbCr, RGB, CMYK, YCCK | Native planes, native CMYK/polarity, all five shared output conversions; 486 color comparisons plus 192 CMYK source/transform encoder cases; broader corpus pending |
| All legal sampling factors and scan layouts | Explicit sequential/progressive scripts, per-scan MCU bounds, common/fractional factors and separate scans up to 4x4 factors (including YCCK 4:1:0); 5,214 C/native scan-script comparisons and 128 randomized legal scripts. Independent conformance corpus still pending |
| Full component-count range | Lossless decoding/encoding tested through 255 components; DCT encoder currently accepts 1–4; complete DCT component-count coverage pending |
| Restart intervals and sequence validation | Sequential/progressive implemented; external 8/12-bit fixtures, corruption and EOB boundary tests |
| Original coefficients, tables, metadata | Coefficients and quantizers retained; APPn/COM bytes borrowed with scan positions |
| JFIF/JFXX, Exif, Adobe, ICC assembly | Typed bounded views, Exif orientation/IFDs/thumbnails, zero-copy ICC assembly and profile writing through 255 chunks; 114 external matches plus 6 malformed ICC cases; caller uses original profile for color management |
| IDCT, chroma upsampling, color numerics | Q20 IDCT: error <=1 on initial 50 cases; exact rational centered interpolation; 486 RGB/CMYK comparisons have observed RGB error <=3; broader numerical/independent corpus gates pending |
| Native configurable encoder | Native pixel/quality/precision/sampling controls and integer FDCT/quantization; 172 external coefficient matches; 132 float-FDCT/size/quality comparisons; broader validation/performance pending |
| Optimized Huffman, progressive encoding | Annex K length-limited tables; generated/custom spectral and DC/AC successive approximation scans for both entropy modes; partial progressions and Al=13. EOB aggregation, per-scan table optimization and measured performance remain open |
| Streaming and allocation limits | Storage/pixel/metadata/scan budgets implemented; streaming and allocator accounting pending |
| Independent corpus and three C oracles | Pinned libjpeg-turbo fixtures and per-feature coefficient/color/metadata/encoder gates; IJG libjpeg, mozjpeg and independent conformance corpus pending |
| Fuzzing and malformed corpus | Every Huffman/arithmetic fixture truncation; 4,000 Huffman and 7,000 arithmetic mutation smoke cases; invalid progression, quantizer/DAC changes, SOS order and restart tests; 128 valid randomized scan-script comparisons. Coverage-guided fuzzing pending |
| Throughput/cycles/allocations/memory | Pending |
| Independent out-of-workspace build | `python tests/independent.py`: isolated package with only shared primitives dependency; current gates recorded in PLAN.md |
