# Standards coverage and outstanding work

This crate is under development. No complete-conformance or competitive-speed
claim is made. The complete user scope is retained in PLAN.md.

| Requirement | Current evidence / remaining work |
| --- | --- |
| Independent package, workspace registration | Cargo manifest; no application dependency |
| Native bounded marker parsing | Zero-copy segments/scans; stuffing/restart/fill boundaries; every truncated prefix tested |
| Sequential Huffman SOF0/SOF1, 8/12-bit | Implemented; 25 libjpeg-turbo cases, exact coefficients/tables |
| Progressive Huffman SOF2, 8/12-bit | Pending |
| Arithmetic SOF9/SOF10 | Pending: Annex D coder and contexts |
| Predictive lossless SOF3/SOF11, 2–16-bit | Pending: predictors, point transform, entropy |
| Hierarchical SOF5/6/7/13/14/15, DHP/EXP | Pending: differential frames and expansion |
| DNL, abbreviated tables/image streams | Pending |
| Gray, YCbCr, RGB, CMYK, YCCK | Native component planes; Gray/RGB/YCbCr shared conversion; CMYK/YCCK conversion pending |
| All legal sampling factors and scan layouts | Sequential interleaved/non-interleaved implemented; fractional geometry test; broad corpus pending |
| Restart intervals and sequence validation | Sequential implemented; external 8/12-bit fixtures and corruption tests |
| Original coefficients, tables, metadata | Coefficients and quantizers retained; APPn/COM bytes borrowed with scan positions |
| JFIF/JFXX, Exif, Adobe, ICC assembly | Pending |
| IDCT, chroma upsampling, color numerics | Q20 integer IDCT: error <=1 vs reference float on 25 cases; centered linear/nearest; broader numerical and color gates pending |
| Native configurable encoder | Pending |
| Optimized Huffman, progressive encoding | Pending |
| Streaming and allocation limits | Storage/pixel/metadata/scan budgets implemented; streaming and allocator accounting pending |
| Independent corpus and three C oracles | Pending |
| Fuzzing and malformed corpus | Every sequential fixture truncation; 4,000 mutation smoke cases; coverage-guided fuzzing pending |
| Throughput/cycles/allocations/memory | Pending |
| Independent out-of-workspace build | `python tests/independent.py`: isolated package and all 12 tests pass; only shared primitives dependency |
